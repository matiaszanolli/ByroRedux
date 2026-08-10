//! Spawn ECS entities for one cached NIF placement.
//!
//! Per REFR placement the loader calls `spawn_placed_instances` to
//! create the actual ECS entities (mesh draw, lights from NiLight
//! blocks, particle emitters, collision shapes) under a single
//! `placement_root` parent. Driven by `load_references`; called once
//! per placement at cell load time.

use byroredux_core::ecs::components::FormIdComponent;
use byroredux_core::ecs::{
    BSXFlags, Billboard, BillboardMode, GlobalTransform, LightSource, LocalBound, MeshHandle,
    SceneFlags, TextureHandle, Transform, World, WorldBound,
};
use byroredux_core::form_id::{FormIdPair, FormIdPool};
use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::VulkanContext;
use std::time::{Duration, Instant};

use crate::asset_provider::{
    derive_normal_map_path, resolve_material_texture_handles_with_clamp, resolve_texture,
    resolve_texture_with_clamp, TextureProvider,
};
use crate::components::{
    decal_uses_implicit_alpha_blend, texture_path_is_fx_mesh, AlphaBlend, DoorTeleport,
    IsDecalMesh, IsFxMesh, MaterialTextureHandles, TwoSided,
};

use super::nif_import_registry::CachedNifImport;
use super::references::attach_light_flicker_if_needed;
use super::refr::RefrTextureOverlay;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PlacementSpawnTimings {
    pub cpu_upload: Duration,
    pub blas: Duration,
    /// This placement authored an opaque FO4+ packed-Havok object and
    /// received a render-geometry compatibility collider.
    pub packed_collision_fallbacks: u32,
    /// This placement authored opaque packed collision but had no safe render
    /// geometry from which to build a compatibility collider.
    pub unresolved_packed_collision: u32,
}

/// #2355 / SF-D8-04 — before `PackedAabbProxy` existed, this function only
/// ever returned `ArchitectureTriMesh` or `None`, so every Starfield
/// Clutter/Actor placement whose collision routes through the undecoded
/// `BhkSystemBinary` blob (see `crates/nif/src/import/collision/mod.rs`)
/// spawned with **no collider at all** — not even an approximate one.
/// `PackedAabbProxy` (below) closes that: any layer with
/// `authoring.needs_packed_havok_fallback()` now gets a conservative
/// AABB proxy instead of silently dropping collision. Bethesda containers
/// built into the level (footlockers, vending machines) are classified
/// `RenderLayer::Architecture` at spawn, so they already hit the more
/// precise `ArchitectureTriMesh` arm — the "container" gap in #2355's
/// title was Architecture-adjacent Clutter/Actor content, which this arm
/// covers. See `references/mod.rs`'s `packed_collision_fallbacks` /
/// `unresolved_packed_collision` per-cell log line for the measurable
/// count of placements this fallback catches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissingCollisionFallback {
    None,
    /// Precise per-submesh static triangles for structural content.
    ArchitectureTriMesh,
    /// One conservative placement-following cuboid for opaque packed Havok.
    PackedAabbProxy,
}

fn missing_collision_fallback(
    collisions_empty: bool,
    authoring: byroredux_nif::import::collision::CollisionAuthoringSummary,
    base_layer: byroredux_core::ecs::components::RenderLayer,
) -> MissingCollisionFallback {
    use byroredux_core::ecs::components::RenderLayer;

    if !collisions_empty {
        return MissingCollisionFallback::None;
    }
    if base_layer == RenderLayer::Architecture {
        return MissingCollisionFallback::ArchitectureTriMesh;
    }
    if authoring.needs_packed_havok_fallback()
        && matches!(base_layer, RenderLayer::Clutter | RenderLayer::Actor)
    {
        return MissingCollisionFallback::PackedAabbProxy;
    }
    MissingCollisionFallback::None
}

#[derive(Clone, Copy)]
struct ProxyMeshGeometry<'a> {
    positions: &'a [[f32; 3]],
    translation: Vec3,
    rotation: Quat,
    scale: f32,
}

/// Union mesh geometry in placement-local space. Mesh-local TRS is applied,
/// while the outer REFR transform is deliberately left for the proxy entity's
/// parent relationship. Keeping the center unscaled lets transform propagation
/// follow a moved/rotated placement; only the cuboid half-extents need the REFR
/// scale baked because physics ignores `GlobalTransform::scale`.
fn transformed_mesh_aabb<'a>(
    meshes: impl IntoIterator<Item = ProxyMeshGeometry<'a>>,
) -> Option<(Vec3, Vec3)> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut points = 0usize;
    for mesh in meshes {
        if !mesh.translation.is_finite()
            || !mesh.rotation.is_finite()
            || mesh.rotation.length_squared() <= f32::EPSILON
            || !mesh.scale.is_finite()
        {
            continue;
        }
        let rotation = mesh.rotation.normalize();
        for point in mesh.positions {
            let point = Vec3::from_array(*point);
            if !point.is_finite() {
                continue;
            }
            let point = mesh.translation + rotation * (point * mesh.scale);
            if point.is_finite() {
                min = min.min(point);
                max = max.max(point);
                points += 1;
            }
        }
    }
    (points > 0).then_some((min, max))
}

fn synthesize_packed_havok_proxy(
    meshes: &[byroredux_nif::import::ImportedMesh],
    ref_scale: f32,
) -> Option<(Vec3, byroredux_core::ecs::components::CollisionShape)> {
    use byroredux_core::ecs::components::CollisionShape;

    if !ref_scale.is_finite() {
        return None;
    }

    // #2531 / NIFAL-D6-NEW-01 — `mesh.positions` on a skinned mesh is
    // bind-pose (rest-pose) local geometry, the exact array GPU skinning
    // deforms at render time — NOT a runtime-posed shape. A T-pose
    // skeleton's splayed limbs would union into a substantially
    // oversized, permanently-wrong box (this proxy is `Keyframed` and
    // parented to `placement_root`, never re-derived per-pose). Mirrors
    // the Architecture-trimesh fallback's `mesh.skin.is_none()` gate —
    // but unlike that per-submesh loop (where skipping one skinned
    // submesh still leaves colliders from the rest), creature content is
    // commonly skinned end-to-end, so an outright skip here would
    // silently regress back to #2355's "no proxy at all" for exactly the
    // population most likely to need one. Skinned submeshes instead
    // contribute their own pose-independent local bounding sphere
    // (already computed at import time from bind-pose extent, but
    // consumed as a SPHERE rather than raw extremity positions, so
    // rotation can't amplify it into something larger than the mesh
    // actually occupies).
    let rigid_geometry = meshes
        .iter()
        .filter(|mesh| {
            mesh.skin.is_none()
                && !mesh.material.is_decal
                && !mesh.material.alpha_test
                && mesh.material.material_kind != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
                && !mesh.positions.is_empty()
        })
        .map(|mesh| ProxyMeshGeometry {
            positions: &mesh.positions,
            translation: Vec3::from_array(mesh.translation),
            rotation: Quat::from_array(mesh.rotation),
            scale: mesh.scale,
        });

    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut any = false;
    if let Some((rigid_min, rigid_max)) = transformed_mesh_aabb(rigid_geometry) {
        min = min.min(rigid_min);
        max = max.max(rigid_max);
        any = true;
    }

    for mesh in meshes.iter().filter(|m| m.skin.is_some()) {
        let translation = Vec3::from_array(mesh.translation);
        let rotation = Quat::from_array(mesh.rotation);
        if !translation.is_finite()
            || !rotation.is_finite()
            || rotation.length_squared() <= f32::EPSILON
            || !mesh.scale.is_finite()
        {
            continue;
        }
        let local_center = Vec3::from_array(mesh.local_bound_center);
        if !local_center.is_finite() || !mesh.local_bound_radius.is_finite() {
            continue;
        }
        // A sphere's shape is rotation-invariant, so its world AABB
        // contribution is exact (not an approximation the way unioning
        // rotated cube corners would need multiple sample points to
        // bound) — transform the center through the mesh's local TRS and
        // scale the radius by the (uniform) mesh scale only.
        let rotation = rotation.normalize();
        let world_center = translation + rotation * (local_center * mesh.scale);
        let world_radius = mesh.local_bound_radius * mesh.scale.abs();
        if world_radius <= 0.0 {
            continue;
        }
        let r = Vec3::splat(world_radius);
        min = min.min(world_center - r);
        max = max.max(world_center + r);
        any = true;
    }

    if !any {
        return None;
    }
    let center = (min + max) * 0.5;
    // Thin render cards still need a non-zero physical thickness. Half a
    // Gamebryo unit is small relative to clutter and actor bounds, while
    // avoiding a degenerate parry cuboid.
    let half_extents = ((max - min) * 0.5 * ref_scale.abs()).max(Vec3::splat(0.5));
    // #2543 — `ref_scale` is an unclamped raw REFR `XSCL` off disk; the
    // `ref_scale.is_finite()` check above runs on the *input*, not this
    // product, so a large-but-finite scale (or an f32 overflow that still
    // slipped past that check as literal `Infinity`) can reach here
    // uncaught. Reject non-finite outright (mirrors the `finite_vec`
    // pattern every other `CollisionShape` producer uses, e.g.
    // `BhkBoxShape` at `crates/nif/src/import/collision/shape.rs`), then
    // clamp to the same "corrupt content" ceiling the exterior-cell RT
    // precision guard already treats as the hard upper bound for a sane
    // spatial magnitude (`RT_ABSOLUTE_PRECISION_CEILING`, #1495) — a
    // finite-but-extreme scale still shouldn't hand Rapier a collider
    // that dwarfs the world and corrupts its broad-phase for every other
    // body in the scene.
    if !half_extents.is_finite() {
        return None;
    }
    let half_extents = half_extents.min(Vec3::splat(
        super::references::RT_ABSOLUTE_PRECISION_CEILING,
    ));
    Some((center, CollisionShape::Cuboid { half_extents }))
}

#[allow(clippy::too_many_arguments)]
fn spawn_packed_havok_proxy(
    world: &mut World,
    cached: &CachedNifImport,
    placement_root: byroredux_core::ecs::EntityId,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    placement_fid: Option<byroredux_core::form_id::FormId>,
    base_layer: byroredux_core::ecs::components::RenderLayer,
) -> bool {
    use byroredux_core::ecs::components::{MotionType, PhysicsSourceForm, RigidBodyData};
    use byroredux_core::ecs::Parent;

    let Some((local_center, shape)) = synthesize_packed_havok_proxy(&cached.meshes, ref_scale)
    else {
        return false;
    };
    let (world_center, world_rot, _) = GlobalTransform::compose_trs(
        ref_pos,
        ref_rot,
        ref_scale,
        local_center,
        Quat::IDENTITY,
        1.0,
    );
    let ghost = world.spawn();
    world.insert(ghost, Transform::new(local_center, Quat::IDENTITY, 1.0));
    world.insert(ghost, GlobalTransform::new(world_center, world_rot, 1.0));
    world.insert(ghost, shape);
    world.insert(
        ghost,
        RigidBodyData {
            // The packed blob's mass and motion type are unknown. Kinematic is
            // conservative and follows any script/animation that moves the
            // placement root instead of drifting away from the visual model.
            motion_type: MotionType::Keyframed,
            ..RigidBodyData::STATIC
        },
    );
    world.insert(ghost, Parent(placement_root));
    crate::helpers::add_child(world, placement_root, ghost);
    if let Some(fid) = placement_fid {
        world.insert(ghost, PhysicsSourceForm(fid));
    }
    world.insert(ghost, base_layer);
    true
}

/// `true` when an `ImportedLight` has a non-trivial diffuse colour
/// contribution and therefore would actually spawn a `LightSource`
/// entity. Authored-off placeholder lights (FNV light-bulb meshes
/// park a zero-colour `NiPointLight` to mark intent without baking
/// the colour; the ESM LIGH base record carries the real value)
/// fail this predicate so the ESM-fallback gate in
/// `spawn_placed_instances` can attach the authoritative LightSource
/// instead.
///
/// Threshold of `1e-4` matches the in-loop check exactly — kept as
/// a free function so #632's regression tests can pin the predicate
/// without standing up a full Vulkan context.
pub(crate) fn is_spawnable_nif_light(light: &byroredux_nif::import::ImportedLight) -> bool {
    light.color[0] + light.color[1] + light.color[2] >= 1e-4
}

/// F3 (2026-05-27) — build a static `CollisionShape::TriMesh` +
/// `RigidBodyData` from a render mesh's geometry, baking `world_scale`
/// (the composed `ref_scale × mesh.scale`) into the vertices so the
/// collider matches the rendered geometry (the physics sync places
/// bodies by translation+rotation only, ignoring `GlobalTransform`
/// scale).
///
/// Used as a fallback when the source NIF authored no bhk collision —
/// the FO4+ Havok-content-system case. Returns `None` when the mesh
/// has no usable triangle data (degenerate index count). Vertices are
/// in NIF-local Y-up space, matching the entity's local `Transform`,
/// so the physics body's world placement composes correctly.
fn synthesize_static_trimesh(
    positions: &[[f32; 3]],
    mesh_indices: &[u32],
    world_scale: f32,
) -> Option<(
    byroredux_core::ecs::components::CollisionShape,
    byroredux_core::ecs::components::RigidBodyData,
)> {
    use byroredux_core::ecs::components::{CollisionShape, RigidBodyData};

    let tri_count = mesh_indices.len() / 3;
    if tri_count == 0 {
        return None;
    }
    let vertices: Vec<Vec3> = positions
        .iter()
        .map(|p| Vec3::new(p[0] * world_scale, p[1] * world_scale, p[2] * world_scale))
        .collect();
    let vert_count = vertices.len() as u32;
    let mut indices: Vec<[u32; 3]> = Vec::with_capacity(tri_count);
    for tri in mesh_indices.chunks_exact(3) {
        // Defensive: skip any triangle that references a vertex out of
        // range (corrupt index buffer) so parry3d's trimesh builder
        // doesn't panic mid-cell-load.
        if tri[0] < vert_count && tri[1] < vert_count && tri[2] < vert_count {
            indices.push([tri[0], tri[1], tri[2]]);
        }
    }
    if indices.is_empty() {
        return None;
    }

    let shape = CollisionShape::TriMesh { vertices, indices };
    // Static architecture — `RigidBodyData::STATIC` (zero mass,
    // friction 0.5, restitution 0.3). Same default the bhk extract
    // path uses for a `motion_type == Static` body.
    Some((shape, RigidBodyData::STATIC))
}

/// Synthesise a static trimesh collider from render geometry and attach it
/// to a physics-only ghost entity.
///
/// This is the single collider-synthesis path for *all* renderable static
/// geometry, whatever kind of cell it came from. Gamebryo/Creation split
/// collision by cell type — interiors got authored `bhk` bodies while
/// exterior landscape was a separate engine-side heightfield subsystem —
/// but that split is an artifact of their architecture, not a property of
/// the data: a triangle is a triangle. ByroRedux keeps one path so terrain
/// and architecture cannot drift apart (pre-fix, exterior LAND tiles
/// rendered with no collider at all and the player fell through the world).
///
/// The ghost carries no `MeshHandle`, so it takes no BLAS entry, no TLAS
/// instance, and no render cost — Rapier only needs `CollisionShape` +
/// `RigidBodyData` + `GlobalTransform`. Returns `true` when a collider was
/// actually spawned. See R6a-stale-14-collider-partial.
pub(crate) fn spawn_trimesh_collider_ghost(
    world: &mut World,
    positions: &[[f32; 3]],
    mesh_indices: &[u32],
    pos: Vec3,
    rot: Quat,
    scale: f32,
    source_form: Option<byroredux_core::form_id::FormId>,
) -> bool {
    let Some((shape, body)) = synthesize_static_trimesh(positions, mesh_indices, scale) else {
        return false;
    };
    let ghost = world.spawn();
    world.insert(ghost, Transform::new(pos, rot, scale));
    world.insert(ghost, GlobalTransform::new(pos, rot, scale));
    world.insert(ghost, shape);
    world.insert(ghost, body);
    if let Some(form_id) = source_form {
        world.insert(
            ghost,
            byroredux_core::ecs::components::PhysicsSourceForm(form_id),
        );
    }
    true
}

/// Count NIF lights that would survive `is_spawnable_nif_light`. The
/// ESM-fallback gate uses this instead of `nif_lights.is_empty()` so
/// a NIF carrying only zero-colour placeholders still receives the
/// ESM LIGH-authored `LightSource` (#632).
pub(crate) fn count_spawnable_nif_lights(
    nif_lights: &[byroredux_nif::import::ImportedLight],
) -> usize {
    nif_lights
        .iter()
        .filter(|l| is_spawnable_nif_light(l))
        .count()
}

/// Sanitise a placement-time light radius before it reaches the GPU
/// `position_radius.w` slot. A non-positive value would zero the
/// shader's `effectiveRange = radius * 4.0` attenuation window
/// (light contributes nothing) AND collapse the shadow-ray jitter
/// disk to the dead 1.5u floor (RT-9 / #672 — penumbra degenerates
/// to a hard point shadow if the light ever crosses the
/// `contribution >= 0.001` gate).
///
/// `EXTERIOR_CELL_UNITS` (4096) matches the cell-scale fallback
/// already used at the NIF-direct spawn site for ambient / directional
/// placeholders without an authored radius. Authored Bethesda XCLL
/// radii are 256–4096 units, so this default is a "covers the cell"
/// net, not a typical value — a malformed LIGH record that ships
/// `radius=0` becomes visible rather than silently invisible.
#[inline]
pub(crate) fn light_radius_or_default(radius: f32) -> f32 {
    if radius > 0.0 {
        radius
    } else {
        EXTERIOR_CELL_UNITS
    }
}

/// Spawn entities for every mesh / light / collision in a pre-parsed NIF
/// with a parent REFR transform applied. Each NIF sub-mesh has its own
/// local transform from the scene graph which composes on top of the
/// REFR placement transform. `cached` is produced by
/// `parse_and_import_nif` and shared across all placements of the same
/// model via `Arc`.
///
/// `mesh_cache_key` is the lowercased model path used to dedup GPU
/// uploads across REFR placements (#879). When `Some`, the mesh
/// uploader first asks `MeshRegistry::acquire_cached` for an existing
/// handle (refcount-bumped) and only falls through to a fresh upload
/// on a miss. `None` keeps the legacy fresh-upload-per-call path —
/// callers that don't share placements (terrain-tile / single-NIF CLI
/// view) keep the old shape.
/// Stamp the FO4+ weapon-mod attach graph (`AttachPoints` /
/// `ChildAttachConnections`) from a cache entry onto a placement-root
/// entity. A no-op for the dominant non-modular case (both `None`). Split
/// out so the materialization is unit-testable without a Vulkan device.
/// See #985 / #1594.
pub(super) fn stamp_attach_components(
    world: &mut World,
    root: byroredux_core::ecs::storage::EntityId,
    cached: &CachedNifImport,
) {
    if let Some(ap) = &cached.attach_points {
        world.insert(root, ap.clone());
    }
    if let Some(cac) = &cached.child_attach_connections {
        world.insert(root, cac.clone());
    }
    // M41.5 Phase B — stamp furniture sit/sleep/lean markers. `None` for
    // the dominant non-furniture case; furniture still renders as its
    // static mesh, this only surfaces the entry positions to the runtime.
    if let Some(furn) = &cached.furniture {
        world.insert(root, furn.clone());
    }
}

#[tracing::instrument(
    name = "spawn_placed_instances",
    skip_all,
    fields(ref_scale = ref_scale, mesh_count = cached.meshes.len()),
)]
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_placed_instances(
    world: &mut World,
    ctx: &mut VulkanContext,
    cached: &CachedNifImport,
    tex_provider: &TextureProvider,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    light_data: Option<&esm::cell::LightData>,
    // Shared light-animation behavior already decoded from the active
    // game's raw LIGH flag layout. Kept separate from `light_data.flags`,
    // whose non-animation bits retain their source-game meanings.
    light_animation_flags: u32,
    // #2250 (REN-D22-01) — canonical shadow-projection type, decoded the
    // same way as `light_animation_flags` above (`canonical_light_shadow_flags`).
    light_shadow_flags: u32,
    // #2439 (NIFAL-D2-01) — geometry half of the same translation
    // boundary (`crate::systems::translate_light`), decoded by the
    // caller (which has `game` and `ref_rot` both available) the same
    // way `light_animation_flags`/`light_shadow_flags` above already are.
    // `LightKind::Point` / `[0.0; 3]` / `0.0` when `light_data` is `None`.
    light_kind: byroredux_core::ecs::LightKind,
    light_direction: [f32; 3],
    light_outer_angle: f32,
    refr_overlay: Option<&RefrTextureOverlay>,
    clip_handle: Option<u32>,
    // #renderlayer — base content-class derived from the REFR's base
    // record type via `RecordType::render_layer()`. Per-mesh
    // `is_decal` / `alpha_test_func` escalate this to
    // `RenderLayer::Decal` at the spawn site below; the caller passes
    // the unescalated base layer.
    base_layer: byroredux_core::ecs::components::RenderLayer,
    mesh_cache_key: Option<&str>,
    // #1212 / D1-NEW-01 — placement form-id (placement-level identity,
    // distinct from `base_form_id` of the referenced base record). When
    // `Some`, the spawn site interns via `FormIdPool` and attaches a
    // `FormIdComponent` to the placement root so `World::find_by_form_id`
    // / debug-server `prid <fid>` / Papyrus `ObjectReference` lookups
    // resolve to the entity. Pre-#1212 every cell-loaded REFR was
    // invisible to those code paths.
    //
    // `None` is the precombined-spawn path (`precombined.rs`) — bake
    // artifacts have no placement-level identity. Loose-NIF (single-NIF
    // CLI view) also passes `None`.
    placement_form_id_pair: Option<FormIdPair>,
    // M40 Phase 2 Stage 1 — XTEL teleport payload from `PlacedRef.teleport`.
    // When `Some`, the placement root carries a `DoorTeleport` component
    // that the `door.teleport` console command (and the future F-key
    // activate system) reads to drive cell-swap orchestration. `None` on
    // every non-door REFR + on the precombined / loose-NIF spawn paths.
    teleport: Option<esm::cell::TeleportDest>,
) -> (byroredux_core::ecs::EntityId, usize, PlacementSpawnTimings) {
    let total_started = Instant::now();
    let imported = &cached.meshes;
    let collisions = &cached.collisions;
    let nif_lights = &cached.lights;
    let mut count = 0;

    // #544 — per-REFR placement root entity. Mesh entities spawned
    // below become its children with NIF-local transforms; the
    // transform-propagation system composes the REFR transform onto
    // them each frame. Pre-#544 every mesh was anchored independently
    // at the world-space-composed transform, which prevented the
    // embedded animation clip's subtree walk from finding the spawned
    // entities (no `Parent` / `Children` edges, no `Name` to bind
    // node-keyed channels against). The placement root carries the
    // composed REFR transform AND the world-space `GlobalTransform`
    // up front so any read that hits the entity before the next
    // propagation tick still sees the right placement (e.g. BLAS
    // build during `build_blas_batched` later in the function).

    let (placement_root, placement_fid) = spawn_placement_root(
        world,
        cached,
        ref_pos,
        ref_rot,
        ref_scale,
        placement_form_id_pair,
        teleport,
    );

    // Pre-compute how many NIF lights will actually spawn. The
    // ESM-fallback gate at the bottom of this function uses this
    // count instead of `nif_lights.is_empty()` so a NIF that
    // authored only zero-colour placeholders (FNV light-bulb
    // meshes are the audit's example) still receives the ESM
    // LIGH-authored LightSource. Pre-#632 the gate checked the
    // raw array length, so placeholders prevented the fallback
    // and the cell rendered dark even when both NIF intent and
    // ESM authority agreed it should be lit.
    let spawned_nif_lights = count_spawnable_nif_lights(nif_lights);

    spawn_nif_lights(world, nif_lights, ref_pos, ref_rot, ref_scale, light_data);

    spawn_particle_emitters(
        world,
        ctx,
        tex_provider,
        cached,
        ref_pos,
        ref_rot,
        ref_scale,
    );

    spawn_collision_shapes(
        world,
        collisions,
        ref_pos,
        ref_rot,
        ref_scale,
        placement_fid,
        base_layer,
    );

    let collision_fallback = missing_collision_fallback(
        collisions.is_empty(),
        cached.collision_authoring,
        base_layer,
    );
    let mut synthesized_collision_proxy = collision_fallback
        == MissingCollisionFallback::PackedAabbProxy
        && spawn_packed_havok_proxy(
            world,
            cached,
            placement_root,
            ref_pos,
            ref_rot,
            ref_scale,
            placement_fid,
            base_layer,
        );

    let resolved_paths = resolve_mesh_paths(world, imported, refr_overlay);
    let mut blas_specs: Vec<(u32, u32, u32)> = Vec::new();
    let pc = PlacementCtx {
        tex_provider,
        ref_pos,
        ref_rot,
        ref_scale,
        base_layer,
        mesh_cache_key,
        refr_overlay,
        light_data,
        light_animation_flags,
        light_shadow_flags,
        light_kind,
        light_direction,
        light_outer_angle,
        placement_root,
        collision_fallback,
        spawned_nif_lights,
    };
    for (sub_mesh_index, mesh) in imported.iter().enumerate() {
        if spawn_fog_mesh_instance(world, &pc, mesh, &resolved_paths[sub_mesh_index]) {
            count += 1;
            continue;
        }
        if spawn_mesh_instance(
            world,
            ctx,
            &pc,
            cached,
            mesh,
            &resolved_paths[sub_mesh_index],
            sub_mesh_index,
            count,
            &mut blas_specs,
            &mut synthesized_collision_proxy,
        ) {
            count += 1;
        }
    }

    // Batched BLAS build: single GPU submission for all meshes in this cell.
    let blas_started = Instant::now();
    if !blas_specs.is_empty() {
        let built = ctx.build_blas_batched(&blas_specs);
        log::info!("Cell BLAS batch: {built}/{} meshes", blas_specs.len());
    }
    let blas_elapsed = blas_started.elapsed();

    // #544 — bind the embedded animation clip to this REFR. Mirrors
    // the loose-NIF path in `scene.rs::load_nif_bytes`. The clip
    // registration itself happens once per unique parsed NIF in
    // `load_references` (cached on `NifImportRegistry`); here we
    // just spawn one `AnimationPlayer` per placement so the
    // animation system's subtree walk finds this REFR's mesh
    // children. Without this insert, water UV scrolls / lava
    // emissive pulses / torch visibility flickers / fade-in alphas
    // all stay frozen on cell-rendered REFRs, while loose-NIF
    // imports of the same models animate correctly.
    if let Some(handle) = clip_handle {
        // #2221 — attach the `Animated*` sinks this clip's non-transform
        // channels write into, BEFORE the player starts ticking.
        // `animation_system` can only write into components that already
        // exist (it holds `&World`), so without this the UV scrolls /
        // emissive pulses / visibility flickers described above resolve
        // their target entity and then discard every sampled value.
        // Channels are cloned out so the registry guard drops before
        // `attach_animation_sinks` takes `&mut World`.
        let channels = {
            let registry = world.resource::<byroredux_core::animation::AnimationClipRegistry>();
            registry.get(handle).map(|clip| {
                (
                    clip.bool_channels.clone(),
                    clip.float_channels.clone(),
                    clip.color_channels.clone(),
                )
            })
        };
        if let Some((bools, floats, colors)) = channels {
            crate::anim_convert::attach_animation_sinks(
                world,
                &bools,
                &floats,
                &colors,
                placement_root,
            );
        }

        let player_entity = world.spawn();
        let mut player = byroredux_core::animation::AnimationPlayer::new(handle);
        player.root_entity = Some(placement_root);
        world.insert(player_entity, player);
    }

    // M47.0 Phase 3b — return the placement_root alongside the
    // entity count so the caller (cell_loader/references.rs) can
    // attach script-state components keyed on the REFR's base
    // record `script_form_id`. Pre-Phase-3b the function returned
    // only the count. Precombined streaming also consumes the bounded
    // CPU-upload / batched-BLAS timing split to identify atomic hash tails;
    // ordinary REFR callers discard it.
    let total_elapsed = total_started.elapsed();
    let packed_collision_authored =
        collisions.is_empty() && cached.collision_authoring.needs_packed_havok_fallback();
    (
        placement_root,
        count,
        PlacementSpawnTimings {
            cpu_upload: total_elapsed.saturating_sub(blas_elapsed),
            blas: blas_elapsed,
            packed_collision_fallbacks: u32::from(
                packed_collision_authored && synthesized_collision_proxy,
            ),
            unresolved_packed_collision: u32::from(
                packed_collision_authored && !synthesized_collision_proxy,
            ),
        },
    )
}

/// Spawn the per-REFR placement root entity and stamp every
/// placement-level component (transforms, bounds, billboard, attach
/// graph, form id, teleport, BSX / scene flags). Returns the root
/// plus the interned placement `FormId` (when the REFR carried one) so
/// the standalone collision entities can share it. Split out of
/// `spawn_placed_instances` (#2057).
fn spawn_placement_root(
    world: &mut World,
    cached: &CachedNifImport,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    placement_form_id_pair: Option<FormIdPair>,
    teleport: Option<esm::cell::TeleportDest>,
) -> (
    byroredux_core::ecs::EntityId,
    Option<byroredux_core::form_id::FormId>,
) {
    let placement_root = world.spawn();
    world.insert(placement_root, Transform::new(ref_pos, ref_rot, ref_scale));
    world.insert(
        placement_root,
        GlobalTransform::new(ref_pos, ref_rot, ref_scale),
    );
    // Bounds-propagation system (Pass 2) folds child WorldBounds into
    // their parent — but only writes to entities that already have a
    // `WorldBound` row. Without this seed insert, every REFR placement
    // root would be invisible to ray-cast picking, culling, and any
    // future RT-budget-by-bounding-sphere consumer. See bounds.rs:161.
    world.insert(placement_root, WorldBound::ZERO);
    // #994 — SpeedTree placeholders (and any future NiBillboardNode-
    // rooted scene) flag a billboard mode on the cache entry. Attaching
    // the `Billboard` component on the placement root makes
    // `billboard_system` yaw-track the spawned entity each frame.
    // Without this insertion `.spt` REFRs render as static quads.
    if let Some(mode) = cached.placement_root_billboard {
        world.insert(placement_root, Billboard::new(mode));
    }
    // #985 / #1594 — stamp the FO4+ weapon-mod attach graph onto the
    // placement root so it reaches the ECS (it dead-ended at the import
    // boundary before this). Visible attachment of mod parts at the named
    // connect points is the #973 OMOD consumer's job; this lands the data.
    stamp_attach_components(world, placement_root, cached);
    // #1212 / D1-NEW-01 — attach FormIdComponent so console / Papyrus /
    // debug-server can locate this REFR by its placement form-id. The
    // FormIdPool intern is a single write-lock per REFR; for cell loads
    // measuring at 800–1000 REFRs (Megaton / Diamond City), the cost is
    // dominated by the StringPool intern (#882) one level above this.
    //
    // #1698 — cached here (not just inserted) so the standalone bhk
    // collision entities spawned below can carry the same form id. Those
    // entities are NOT children of `placement_root`'s render mesh (they're
    // spawned bare from `cached.collisions`, see the loop's own comment),
    // so without this they're otherwise unresolvable to their source REFR
    // — the runtime `dump_awake_fallers` diagnostic could only ever print
    // `form=? layer=?` for any awake dynamic clutter body.
    let placement_fid = placement_form_id_pair.map(|pair| {
        let fid = world.resource_mut::<FormIdPool>().intern(pair);
        world.insert(placement_root, FormIdComponent(fid));
        fid
    });
    // M40 Phase 2 Stage 1 — XTEL portal plumbing. When the REFR carries
    // a teleport destination, stamp a `DoorTeleport` component on the
    // placement root so the console command + future F-key activate
    // system can resolve "this door leads to <cell>, materialise at
    // <position> with <rotation>". Pre-Phase-2 every XTEL parsed at
    // the ESM layer landed on the floor — `TeleportDest` rode along on
    // `PlacedRef` since #412 but no consumer existed.
    if let Some(t) = teleport {
        world.insert(
            placement_root,
            DoorTeleport {
                destination_form_id: t.destination,
                position_zup: t.position,
                rotation_zup: t.rotation,
            },
        );
    }
    // #1214 / D1-NEW-03 — attach BSXFlags on the placement root when
    // the NIF authored them. Editor-marker bit (0x20) is filtered at
    // parse time (`references.rs:840`), so any cached entry reaching
    // here either has the bit clear OR comes from a `.spt` /
    // generated path with `bsx_flags = 0`.
    if cached.bsx_flags != 0 {
        world.insert(placement_root, BSXFlags(cached.bsx_flags));
    }
    // #1235 / LC-D1-NEW-01 — attach SceneFlags on the placement root for
    // parity with the loose-NIF loader (`scene/nif_loader.rs:450-452`).
    // APP_CULLED (bit 0) is filtered import-side in `walk/mod.rs`, so
    // any cached entry reaching here has the bit clear; the remaining
    // bits (SELECTIVE_UPDATE / DISABLE_SORTING / DISPLAY_OBJECT /
    // IS_NODE) ride through for downstream consumers (future
    // visibility-toggle systems, alpha-sort draw order, animation-cost
    // gating).
    if cached.root_flags != 0 {
        world.insert(placement_root, SceneFlags::from_nif(cached.root_flags));
    }
    (placement_root, placement_fid)
}

/// Spawn a `LightSource` entity per authored NIF light with a
/// non-trivial diffuse contribution. Split out of
/// `spawn_placed_instances` (#2057). Widened to `pub(crate)` by #2530 /
/// NIFAL-D3-NEW-01 so `scene::nif_loader::load_nif_bytes_with_skeleton`
/// (the loose-NIF / NPC-part load path, which has no REFR and therefore
/// no `esm::cell::LightData` — pass `None`) can spawn lights through the
/// exact same construction + sanitization the cell loader uses, instead
/// of re-deriving it a third time.
pub(crate) fn spawn_nif_lights(
    world: &mut World,
    nif_lights: &[byroredux_nif::import::ImportedLight],
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    light_data: Option<&esm::cell::LightData>,
) {
    use byroredux_core::ecs::Name;
    // Spawn per-mesh NiLight blocks as LightSource entities. Parented
    // through the reference transform so torches/candles inside cell
    // refs contribute to the live GpuLight buffer. See issue #156.
    // When the ESM LIGH record provides an authored radius, prefer it
    // over the NIF-computed attenuation_radius (which often returns 2048
    // for NiPointLights with constant-only attenuation coefficients).
    let esm_radius = light_data.as_ref().map(|ld| ld.radius);
    for light in nif_lights {
        // Skip lights whose diffuse contribution is effectively zero —
        // these are usually authored-off placeholders. The audit's
        // FNV Prospector Saloon evidence: light-bulb meshes ship a
        // disabled NiPointLight to mark intent without baking colour;
        // the ESM LIGH base record carries the real authored colour.
        // Predicate kept in lockstep with `is_spawnable_nif_light`.
        if !is_spawnable_nif_light(light) {
            continue;
        }
        let nif_pos = Vec3::new(
            light.translation[0],
            light.translation[1],
            light.translation[2],
        );
        let final_pos = GlobalTransform::compose_translation(ref_pos, ref_rot, ref_scale, nif_pos);
        // Pick the authored radius source, then sanitise. Pre-#672
        // an `esm_radius == Some(0.0)` slipped through as a real
        // `0 * ref_scale = 0` and the light became invisible at
        // the shader (zero attenuation, dead-floor jitter disk).
        // Falling through to `light_radius_or_default` keeps the
        // 4096u cell-scale fallback that previously only fired on
        // the NIF-side `else` branch.
        let raw_radius = match esm_radius {
            Some(r) if r > 0.0 => r * ref_scale,
            _ if light.radius > 0.0 => light.radius * ref_scale,
            _ => 0.0,
        };
        let radius = light_radius_or_default(raw_radius);
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(final_pos));
        world.insert(entity, GlobalTransform::new(final_pos, Quat::IDENTITY, 1.0));
        world.insert(
            entity,
            LightSource::from_legacy_world_units(
                radius,
                light.color,
                // A direct NiLight has no ESM LIGH DATA flags. Preserve its
                // authored physical visibility explicitly at this boundary.
                byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
                0.0,
                light.kind,
                light.direction,
                light.outer_angle,
                byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
            ),
        );
        // #983 — attach the NIF light's own block name so the
        // animation system can resolve `NiLight*Controller` channels
        // keyed by this name. Anonymous lights (`name.is_none()`)
        // can't be animated by anything but transform-derived
        // ancestor controllers, which fall through this path.
        //
        // Inline `world.resource_mut::<StringPool>()` intern site —
        // lights per cell typically number 1-50 (Skyrim Riften ~25,
        // FNV Goodsprings ~30), so the short write-lock cost is
        // bounded. Pre-fix the mesh path pre-interned via a
        // separate pre-pass (#882); a parallel pre-pass for light
        // names is a deferred optimisation if a light-heavy cell
        // surfaces a measurable cost.
        if let Some(ref nif_name) = light.name {
            let interned = {
                let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
                pool.intern(nif_name)
            };
            world.insert(entity, Name(interned));
        }
    }
}

/// Spawn one particle-emitter entity per detected NiParticleSystem,
/// overlaying authored emitter params onto the name-derived preset.
/// Split out of `spawn_placed_instances` (#2057).
fn spawn_particle_emitters(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    cached: &CachedNifImport,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
) {
    for em in &cached.particle_emitters {
        let nif_pos = Vec3::new(
            em.local_position[0],
            em.local_position[1],
            em.local_position[2],
        );
        let world_pos = GlobalTransform::compose_translation(ref_pos, ref_rot, ref_scale, nif_pos);
        let host = em.host_name.as_deref().unwrap_or("").to_ascii_lowercase();
        let mut preset = crate::fog::particle_preset(&host, em.texture_path.as_deref());
        // NIFAL particles slice (#1513) — overlay every authored emitter
        // override (colour curve #707, NiPSysEmitter base params, birth
        // rate, force fields #984, texture/blend #2300) onto the heuristic
        // preset through the single shared boundary. Parallel to the
        // loose-NIF site in scene/nif_loader.rs — both call the same helper
        // so the two load paths can't diverge.
        crate::systems::apply_emitter_overlays(
            &mut preset,
            &em.color_curve,
            &em.emitter_params,
            em.emitter_rate,
            &em.force_fields,
            &em.texture_path,
            em.src_blend,
            em.dst_blend,
        );

        // Alpha-over fog/smoke and additive flame/ember are both participating
        // media, not transparent geometry. Replace the billboard system at the
        // translation boundary so it cannot fight froxel history or FSR later.
        // The local ellipsoid retains the authored emitter's swept extent;
        // smoke seeds optical density from authored alpha, fire from its
        // blackbody temperature. Lighting visibility remains the shared
        // BLAS-backed TLAS query in the volumetric inject pass.
        if let Some(fog_volume) = crate::fog::medium_from_particle(&host, &preset) {
            log::debug!(
                target: "byroredux::fog",
                "replaced particle emitter with local {} volume: \
                 type={:?} host={:?} texture={:?}",
                if fog_volume.is_emissive() { "emissive" } else { "fog" },
                em.original_type,
                em.host_name,
                preset.texture_path,
            );
            let entity = world.spawn();
            world.insert(
                entity,
                Transform::new(world_pos, ref_rot, ref_scale.abs().max(1.0e-4)),
            );
            world.insert(
                entity,
                GlobalTransform::new(world_pos, ref_rot, ref_scale.abs().max(1.0e-4)),
            );
            world.insert(entity, fog_volume);
            continue;
        }
        if preset.dst_blend == 7 {
            log::debug!(
                target: "byroredux::fog",
                "alpha-over particle emitter kept on billboard path: \
                 type={:?} host={:?} texture={:?} alpha={:.3}->{:.3} \
                 rate={:.3} life={:.3} max_particles={} size={:.3}->{:.3}",
                em.original_type,
                em.host_name,
                preset.texture_path,
                preset.start_color[3],
                preset.end_color[3],
                preset.rate,
                preset.life,
                preset.max_particles,
                preset.start_size,
                preset.end_size,
            );
        }

        // A particle without a real sprite is not a valid white quad. The
        // old renderer hardcoded bindless slot zero, turning every such
        // billboard into the giant white streaks seen in Railroad HQ.
        let texture_handle = resolve_texture(ctx, tex_provider, preset.texture_path.as_deref());
        if texture_handle == ctx.texture_registry.fallback()
            || texture_handle == ctx.texture_registry.neutral_fallback()
        {
            log::debug!(
                "skipping particle emitter {:?}: no resolvable sprite texture {:?}",
                em.original_type,
                preset.texture_path,
            );
            continue;
        }
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(world_pos));
        world.insert(entity, GlobalTransform::new(world_pos, Quat::IDENTITY, 1.0));
        world.insert(entity, TextureHandle(texture_handle));
        world.insert(entity, preset);
    }
}

/// Spawn standalone collision entities from the NIF's bhk shapes.
/// Split out of `spawn_placed_instances` (#2057).
fn spawn_collision_shapes(
    world: &mut World,
    collisions: &[byroredux_nif::import::ImportedCollision],
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    placement_fid: Option<byroredux_core::form_id::FormId>,
    base_layer: byroredux_core::ecs::components::RenderLayer,
) {
    for coll in collisions {
        let nif_pos = Vec3::new(
            coll.translation[0],
            coll.translation[1],
            coll.translation[2],
        );
        let nif_quat = Quat::from_xyzw(
            coll.rotation[0],
            coll.rotation[1],
            coll.rotation[2],
            coll.rotation[3],
        );
        let (final_pos, final_rot, final_scale) = GlobalTransform::compose_trs(
            ref_pos, ref_rot, ref_scale, nif_pos, nif_quat, coll.scale,
        );

        // parry3d panics on nested Compound shapes. Clone inside
        // catch_unwind so a bad shape doesn't kill the entire load.
        let shape_result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| coll.shape.clone()));
        let shape = match shape_result {
            Ok(s) => s,
            Err(_) => {
                log::warn!(
                    "Skipping collision shape (nested composite) at ({:.0},{:.0},{:.0})",
                    final_pos.x,
                    final_pos.y,
                    final_pos.z
                );
                continue;
            }
        };

        let entity = world.spawn();
        world.insert(entity, Transform::new(final_pos, final_rot, final_scale));
        world.insert(
            entity,
            GlobalTransform::new(final_pos, final_rot, final_scale),
        );
        world.insert(entity, shape);
        world.insert(entity, coll.body.clone());
        // #1698 — tag this collision entity with its source REFR's form id
        // and content-class so a runtime diagnostic (`dump_awake_fallers`)
        // can resolve WHICH placement an awake/free-falling dynamic body
        // belongs to. Pre-fix these entities carried neither — structurally
        // unresolvable to anything a user could act on.
        //
        // Deliberately `PhysicsSourceForm`, NOT `FormIdComponent`: this
        // entity isn't parented to `placement_root` (its `Transform` is
        // already world-composed above, not NIF-local — adding `Parent`
        // would either double-transform it under propagation, or (without
        // the matching `Children` bookkeeping `add_child` provides) orphan
        // it from propagation entirely and freeze its `GlobalTransform`).
        // Reusing `FormIdComponent` here would also make
        // `World::find_by_form_id` ambiguous: a compound bhk shape can
        // spawn several of these per REFR, all sharing the placement's
        // form id, and that lookup returns the first match — it must stay
        // resolvable to the actual placement root, not an arbitrary
        // collision proxy.
        if let Some(fid) = placement_fid {
            world.insert(
                entity,
                byroredux_core::ecs::components::PhysicsSourceForm(fid),
            );
        }
        world.insert(entity, base_layer);
    }
}

/// Effective per-mesh texture-slot paths, resolved in one StringPool
/// lock (#882). Promoted to module scope from `spawn_placed_instances`
/// so `resolve_mesh_paths` + `spawn_mesh_instance` can share it (#2057).
struct ResolvedMeshPaths {
    textures: byroredux_nif::import::MaterialTextureSet<Option<String>>,
    material_path: Option<String>,
    name_sym: Option<byroredux_core::string::FixedString>,
}
fn resolve_to_owned(
    pool: &byroredux_core::string::StringPool,
    sym: Option<byroredux_core::string::FixedString>,
) -> Option<String> {
    sym.and_then(|s| pool.resolve(s)).map(|s| s.to_string())
}

/// Resolve every mesh's effective texture-slot paths + interned name
/// under a single StringPool lock (#882). Split out of
/// `spawn_placed_instances` (#2057).
fn resolve_mesh_paths(
    world: &mut World,
    imported: &[byroredux_nif::import::ImportedMesh],
    refr_overlay: Option<&RefrTextureOverlay>,
) -> Vec<ResolvedMeshPaths> {
    let ov = refr_overlay;
    let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
    imported
        .iter()
        .map(|mesh| {
            let mut textures = mesh
                .material
                .textures
                .map_ref(|path| resolve_to_owned(&pool, *path));
            // Effective texture slot paths. REFR overlay
            // (XATO/XTNM/XTXR) wins over the NIF-authored paths
            // when present; for slots the overlay left empty the
            // cached NIF's texture rides through. `None` on both
            // sides means the slot has no texture. See #584.
            textures.base_color = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.diffuse)
                    .or(mesh.material.textures.base_color),
            );
            // Oblivion/FO3 ship normal maps via the `<base>_n.dds`
            // load-time convention, not an explicit NIF slot. When the
            // mesh left both normal/bump slots empty, derive the sibling
            // from the (effective) diffuse path; it resolves like any
            // texture and fails soft if absent (#1303 / OBL-D4-NEW-01).
            textures.normal = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.normal).or(mesh.material.textures.normal),
            )
            .or_else(|| textures.base_color.as_deref().map(derive_normal_map_path));
            textures.emissive = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.glow).or(mesh.material.textures.emissive),
            );
            textures.smooth_spec = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.specular)
                    .or(mesh.material.textures.smooth_spec),
            );
            textures.height = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.height).or(mesh.material.textures.height),
            );
            textures.environment = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.env)
                    .or(mesh.material.textures.environment),
            );
            textures.environment_mask = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.env_mask)
                    .or(mesh.material.textures.environment_mask),
            );
            let material_path = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.material_path)
                    .or(mesh.material.material_path),
            );
            // Intern the mesh name in the same lock — see #882's
            // second hotspot. `mesh.name: Option<Arc<str>>`. The
            // `pool.intern` call must follow the resolves so the
            // `&pool` borrows from `resolve_to_owned` end before
            // the `&mut pool` re-borrow.
            let name_sym = mesh.name.as_deref().map(|n| pool.intern(n));
            ResolvedMeshPaths {
                textures,
                material_path,
                name_sym,
            }
        })
        .collect()
    // pool guard dropped here at end of block.
}

/// Immutable per-placement context shared by `spawn_mesh_instance` —
/// bundles the REFR transform + render/overlay inputs so the helper's
/// signature stays legible. Split out of `spawn_placed_instances`
/// (#2057). All fields are `Copy`.
#[derive(Clone, Copy)]
struct PlacementCtx<'a> {
    tex_provider: &'a TextureProvider,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    base_layer: byroredux_core::ecs::components::RenderLayer,
    mesh_cache_key: Option<&'a str>,
    refr_overlay: Option<&'a RefrTextureOverlay>,
    light_data: Option<&'a esm::cell::LightData>,
    light_animation_flags: u32,
    light_shadow_flags: u32,
    // #2439 (NIFAL-D2-01) — see `spawn_placed_instances`'s matching params.
    light_kind: byroredux_core::ecs::LightKind,
    light_direction: [f32; 3],
    light_outer_angle: f32,
    placement_root: byroredux_core::ecs::EntityId,
    collision_fallback: MissingCollisionFallback,
    spawned_nif_lights: usize,
}

/// Replace one authored alpha-over fog/smoke mesh with an analytic local
/// medium before any texture upload, raster entity, or BLAS work occurs.
fn spawn_fog_mesh_instance(
    world: &mut World,
    pc: &PlacementCtx,
    mesh: &byroredux_nif::import::ImportedMesh,
    paths: &ResolvedMeshPaths,
) -> bool {
    use byroredux_core::ecs::{Name, Parent};

    let texture_path = paths.textures.base_color.as_deref();
    let fog_semantics = pc.mesh_cache_key.is_some_and(crate::fog::has_fog_token)
        || texture_path.is_some_and(crate::fog::has_fog_token)
        || mesh.name.as_deref().is_some_and(crate::fog::has_fog_token);
    let Some(fog_volume) = crate::fog::fog_volume_from_mesh(pc.mesh_cache_key, texture_path, mesh)
    else {
        if fog_semantics {
            log::debug!(
                target: "byroredux::fog",
                "authored fog mesh candidate kept on legacy path: model={:?} texture={:?} \
                 name={:?} has_alpha={} dst_blend={} material_kind={}",
                pc.mesh_cache_key,
                texture_path,
                mesh.name,
                mesh.material.has_alpha,
                mesh.material.dst_blend_mode,
                mesh.material.material_kind,
            );
        }
        return false;
    };
    log::debug!(
        target: "byroredux::fog",
        "replaced authored fog mesh with local volume: model={:?} texture={:?} name={:?}",
        pc.mesh_cache_key,
        texture_path,
        mesh.name,
    );

    let nif_rotation = Quat::from_xyzw(
        mesh.rotation[0],
        mesh.rotation[1],
        mesh.rotation[2],
        mesh.rotation[3],
    );
    let nif_position = Vec3::from_array(mesh.translation);
    let (world_position, world_rotation, world_scale) = GlobalTransform::compose_trs(
        pc.ref_pos,
        pc.ref_rot,
        pc.ref_scale,
        nif_position,
        nif_rotation,
        mesh.scale,
    );

    let entity = world.spawn();
    world.insert(
        entity,
        Transform::new(nif_position, nif_rotation, mesh.scale),
    );
    world.insert(
        entity,
        GlobalTransform::new(world_position, world_rotation, world_scale),
    );
    world.insert(entity, fog_volume);
    world.insert(entity, Parent(pc.placement_root));
    crate::helpers::add_child(world, pc.placement_root, entity);
    if let Some(symbol) = paths.name_sym {
        world.insert(entity, Name(symbol));
    }
    true
}

/// Spawn the render entity (+ optional physics ghost + ESM light
/// fallback) for one imported sub-mesh. Returns `true` when an entity
/// was spawned (the caller then increments its placement count); a
/// failed GPU upload returns `false`, mirroring the pre-split `continue`.
/// Split out of `spawn_placed_instances` (#2057).
#[allow(clippy::too_many_arguments)]
fn spawn_mesh_instance(
    world: &mut World,
    ctx: &mut VulkanContext,
    pc: &PlacementCtx,
    cached: &CachedNifImport,
    mesh: &byroredux_nif::import::ImportedMesh,
    paths: &ResolvedMeshPaths,
    sub_mesh_index: usize,
    count: usize,
    blas_specs: &mut Vec<(u32, u32, u32)>,
    synthesized_collision_proxy: &mut bool,
) -> bool {
    use byroredux_core::ecs::{Name, Parent};
    use byroredux_renderer::Vertex;
    let PlacementCtx {
        tex_provider,
        ref_pos,
        ref_rot,
        ref_scale,
        base_layer,
        mesh_cache_key,
        refr_overlay,
        light_data,
        light_animation_flags,
        light_shadow_flags,
        light_kind,
        light_direction,
        light_outer_angle,
        placement_root,
        collision_fallback,
        spawned_nif_lights,
    } = *pc;
    let num_verts = mesh.positions.len();
    let sub_mesh_index_u32 = sub_mesh_index as u32;

    // #879 / CELL-PERF-01 — refcounted GPU mesh dedup. First
    // placement of `chair.nif` uploads the vertex/index pair and
    // registers it under `(model_path, sub_mesh_index)`; the next
    // 39 chair placements bump the entry's refcount and reuse
    // the same `mesh_handle` (and the same BLAS — skipping the
    // batched BLAS build entry for the cached hit). Without
    // `mesh_cache_key` (terrain / single-NIF CLI view) the cache
    // is bypassed and we keep the legacy fresh-upload-per-call
    // shape.
    let cache_hit_handle =
        mesh_cache_key.and_then(|key| ctx.mesh_registry.acquire_cached(key, sub_mesh_index_u32));

    let mesh_handle = if let Some(handle) = cache_hit_handle {
        // Cached: skip the CPU vertex-build, the GPU upload, AND
        // the BLAS batch entry. The existing BLAS for this handle
        // is already attached to live placements in earlier cells
        // (or earlier in this same cell).
        handle
    } else {
        let vertices: Vec<Vertex> = (0..num_verts)
            .map(|i| {
                // Preserve authored vertex opacity. Large additive effect
                // proxies (light beams, soft glow volumes) use this lane to
                // fade their boundary to zero; treating them as opaque makes
                // the proxy geometry visible and floods bloom/TAA.
                let color = if i < mesh.colors.len() {
                    mesh.colors[i]
                } else {
                    [1.0, 1.0, 1.0, 1.0]
                };
                let mut v = Vertex::new_rgba(
                    mesh.positions[i],
                    color,
                    if i < mesh.normals.len() {
                        mesh.normals[i]
                    } else {
                        [0.0, 1.0, 0.0]
                    },
                    if i < mesh.uvs.len() {
                        mesh.uvs[i]
                    } else {
                        [0.0, 0.0]
                    },
                );
                // #783 / M-NORMALS — propagate authored tangent
                // (NiBinaryExtraData "Tangent space ..." for Oblivion
                // / FO3 / FNV cell-loader content). Empty mesh.tangents
                // → zero, which the fragment shader's perturbNormal
                // detects and routes to its screen-space derivative
                // fallback.
                if i < mesh.tangents.len() {
                    v.tangent = mesh.tangents[i];
                }
                v
            })
            .collect();

        let alloc = ctx.allocator.as_ref().unwrap();
        let upload_ctx = GpuUploadCtx {
            device: &ctx.device,
            allocator: alloc,
            queue: &ctx.graphics_queue,
            command_pool: ctx.transfer_pool,
        };
        // BSEffectShader meshes are raster proxy volumes (light shafts,
        // soft glows, particles), not physical surfaces. Building BLAS for
        // them both wastes memory/load time and lets shadow/GI rays hit the
        // proxy hull as if it were solid geometry.
        let for_rt = ctx.device_caps.ray_query_supported
            && mesh.material.material_kind != byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER
            && mesh.material.material_kind != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
            && !mesh.material.is_decal;
        let upload_result = match mesh_cache_key {
            Some(key) => ctx.mesh_registry.register_scene_mesh_keyed(
                upload_ctx,
                &vertices,
                &mesh.indices,
                for_rt,
                None,
                (key, sub_mesh_index_u32),
            ),
            None => ctx.mesh_registry.upload_scene_mesh(
                upload_ctx,
                &vertices,
                &mesh.indices,
                for_rt,
                None,
            ),
        };
        let handle = match upload_result {
            Ok(h) => h,
            Err(e) => {
                log::warn!("Failed to upload mesh: {}", e);
                return false;
            }
        };

        // Fresh physical-surface upload — this handle needs a BLAS. Subsequent
        // cache hits for the same `(path, sub_mesh_index)` reuse
        // this BLAS entry without re-submitting.
        if for_rt {
            blas_specs.push((handle, num_verts as u32, mesh.indices.len() as u32));
        }
        handle
    };

    // Pre-resolved texture slot paths from the single-lock
    // pre-pass above (#882). Cloned per-mesh because the Material
    // ECS component owns its `Option<String>` fields and the
    // resolved-paths Vec stays alive across this iteration; the
    // alternative — moving paths out of `resolved_paths[i]` — would
    // need a swap-with-default to keep the Vec indexable for the
    // texture-handle resolves below. Per-slot clone is one
    // allocation per populated slot per mesh, same as the pre-fix
    // `resolve_owned(...).clone()` pattern at the Material struct
    // construction site.
    let eff_textures = paths.textures.clone();
    let eff_texture_path = eff_textures.base_color.clone();
    let eff_material_path = paths.material_path.clone();

    // Load texture (shared resolve: cache → BSA → fallback).
    // #610 — pass the diffuse-slot `TexClampMode` so the bindless
    // descriptor's sampler picks the matching `VkSamplerAddressMode`
    // pair. CLAMP-authored decals / scope reticles / Oblivion
    // architecture trim no longer render with the legacy
    // REPEAT/REPEAT bleed.
    let tex_handle = resolve_texture_with_clamp(
        ctx,
        tex_provider,
        eff_texture_path.as_deref(),
        mesh.material.texture_clamp_mode,
    );

    // #544 — mesh entities now sit in the NIF-local frame and
    // descend from the placement root. The transform-propagation
    // system composes `placement_root` (the REFR transform) onto
    // them each frame to produce the world-space `GlobalTransform`
    // the renderer / BLAS / lighting consume. Pre-#544 every mesh
    // pre-baked the REFR composition into its own `Transform`,
    // which left it anchored to nothing the embedded animation
    // clip could walk to.
    //
    // The composed `final_*` values are still computed up front
    // because the `GlobalTransform` we seed on the mesh has to
    // match what the propagation pass will compute on the first
    // tick — anything that reads `GlobalTransform` before then
    // (renderer's per-frame data collection, BLAS build below)
    // gets a correctly-placed value in the meantime.
    let nif_quat = Quat::from_xyzw(
        mesh.rotation[0],
        mesh.rotation[1],
        mesh.rotation[2],
        mesh.rotation[3],
    );
    let nif_pos = Vec3::new(
        mesh.translation[0],
        mesh.translation[1],
        mesh.translation[2],
    );

    // World-space placement — used only to seed the initial
    // `GlobalTransform`. `Transform` itself stays NIF-local so
    // the propagation pass produces the same value next tick. The
    // parent→child composition order lives in `compose_trs`.
    let (final_pos, final_rot, final_scale) =
        GlobalTransform::compose_trs(ref_pos, ref_rot, ref_scale, nif_pos, nif_quat, mesh.scale);

    // Diagnostic: log meshes with significant NIF-internal offsets
    // (these are wall/structural pieces most likely to show positioning issues)
    let nif_offset_len = nif_pos.length();
    if nif_offset_len > 50.0 {
        log::debug!(
            "  NIF offset {:.0} for mesh {:?}: nif_pos=({:.0},{:.0},{:.0}) \
                 final=({:.0},{:.0},{:.0})",
            nif_offset_len,
            mesh.name,
            nif_pos.x,
            nif_pos.y,
            nif_pos.z,
            final_pos.x,
            final_pos.y,
            final_pos.z,
        );
    }

    let entity = world.spawn();
    // NIF-local Transform for hierarchy propagation; world-space
    // GlobalTransform for first-tick consumers. See #544.
    world.insert(entity, Transform::new(nif_pos, nif_quat, mesh.scale));
    world.insert(
        entity,
        GlobalTransform::new(final_pos, final_rot, final_scale),
    );
    // #1213 / D1-NEW-02 — seed LocalBound from the mesh-local
    // bounding sphere (`ImportedMesh.local_bound_center`,
    // `.local_bound_radius`, both extracted by the NIF importer
    // from `NiTriShapeData.center` / `BsTriShape.center` or
    // computed from vertex positions). The bounds-propagation
    // system at `byroredux/src/systems/bounds.rs:43-66` reads
    // this row and produces a world-space `WorldBound` each
    // frame; pre-#1213 no LocalBound row was ever inserted, so
    // every WorldBound stayed at the component default (zero
    // sphere) and downstream culling / RT-budget / cell-bounds
    // consumers fell through to coarser approximations.
    world.insert(
        entity,
        LocalBound::new(
            Vec3::new(
                mesh.local_bound_center[0],
                mesh.local_bound_center[1],
                mesh.local_bound_center[2],
            ),
            mesh.local_bound_radius,
        ),
    );
    // Sibling to the LocalBound insert above. `bounds.rs` Pass 1 at
    // line 61-63 only *updates* a pre-existing `WorldBound` row —
    // it does not insert one — so a missing seed row means the
    // entity stays at `WorldBound::default()` (zero sphere) and is
    // invisible to ray-cast picking, frustum culling, and the
    // skinned-LRU bounds heuristic. The propagation pass overwrites
    // this `ZERO` with the real value on the next tick.
    world.insert(entity, WorldBound::ZERO);
    // #1235 / LC-D1-NEW-01 — attach SceneFlags for parity with the
    // loose-NIF loader (`scene/nif_loader.rs:789-791`). APP_CULLED
    // shapes never reach this point (filtered import-side in
    // `walk/mod.rs`); the remaining NiAVObject bits ride through
    // for downstream consumers.
    if mesh.flags != 0 {
        world.insert(entity, SceneFlags::from_nif(mesh.flags));
    }
    // #2206 / NIFAL-D4-02 — per-mesh parity with the loose-NIF loader
    // (`scene/nif_loader.rs`'s `node.billboard_mode` attach). The flat
    // cell-loader walk spawns one entity per mesh with no node entities
    // at all, so the nearest-ancestor `NiBillboardNode` mode set by
    // `walk_node_flat` rides on the mesh itself instead of a node.
    if let Some(raw) = mesh.billboard_mode {
        world.insert(entity, Billboard::new(BillboardMode::from_nif(raw)));
    }
    // Parent/Children edge → embedded animation clip's subtree
    // walk discovers this mesh through `placement_root`.
    world.insert(entity, Parent(placement_root));
    crate::helpers::add_child(world, placement_root, entity);
    // Name from `ImportedMesh.name` so the clip's node-keyed
    // channels (`FixedString` interned at parse time, #340)
    // resolve through `build_subtree_name_map` to this entity.
    // Pre-#544 the cell-loader path skipped this insert, so even
    // if `Parent` had been wired the channels would have failed
    // their name lookup and silently no-op'd.
    //
    // Pre-#882 this site re-acquired a `world.resource_mut::<
    // StringPool>()` write lock per mesh. The intern is now done
    // in the pre-pass above; this site only consumes the cached
    // `FixedString`.
    if let Some(sym) = paths.name_sym {
        world.insert(entity, Name(sym));
    }
    world.insert(entity, MeshHandle(mesh_handle));
    world.insert(entity, TextureHandle(tex_handle));
    // Canonical material translation — the single boundary that
    // resolves a raw `ImportedMesh` into the engine `Material`
    // (PBR resolved, glass classified once, flag union packed).
    // The cell path contributes the REFR-overlay model-space-normals
    // bit as `extra_material_flags`; everything else is shared with
    // the loose-NIF path. See `material_translate.rs`.
    let extra_material_flags = refr_overlay
        .filter(|o| o.model_space_normals)
        .map(|_| byroredux_renderer::vulkan::material::material_flag::MODEL_SPACE_NORMALS)
        .unwrap_or(0);
    let material = crate::material_translate::translate_material(
        &mesh.material,
        mesh.name.as_deref(),
        crate::material_translate::ResolvedPaths {
            textures: eff_textures.clone(),
            material_path: eff_material_path.clone(),
        },
        extra_material_flags,
    );
    let material_kind = material.material_kind;
    world.insert(entity, material);
    // PERF-D3-NEW-02 / #1136 — classify FX-decoration meshes at spawn
    // time so build_render_data can skip them via a component query
    // instead of running 6 substring scans per draw per frame.
    if let Some(ref tp) = eff_texture_path {
        if texture_path_is_fx_mesh(tp, material_kind) {
            world.insert(entity, IsFxMesh);
        }
    }
    // Resolve every secondary semantic role with the SAME authored clamp mode
    // as base colour. The shared helper is also used by loose-NIF spawning so
    // structures, clutter, actors, and exterior statics cannot drift.
    let texture_handles = resolve_material_texture_handles_with_clamp(
        ctx,
        tex_provider,
        &eff_textures,
        tex_handle,
        mesh.material.texture_clamp_mode,
    );
    let normal_has_alpha = texture_handles.normal != 0
        && ctx
            .texture_registry
            .handle_has_alpha(texture_handles.normal);
    world.insert(
        entity,
        MaterialTextureHandles {
            textures: texture_handles,
            normal_has_alpha,
            parallax_height_scale: mesh.material.parallax_height_scale.unwrap_or(0.04),
            parallax_max_passes: mesh.material.parallax_max_passes.unwrap_or(4.0),
        },
    );
    // #1480 / REN-D22-NEW-01 — resolve the normal-alpha-as-spec roughness
    // ONCE into the canonical Material now that MaterialTextureHandles is
    // attached, instead of recomputing it per
    // draw in the render path. Reads the same components the renderer
    // reads, so the value is identical — only canonical + tooling-visible.
    crate::material_translate::resolve_normal_alpha_spec_roughness(world, entity);
    let implicit_decal_blend = decal_uses_implicit_alpha_blend(
        mesh.material.is_decal,
        mesh.material.has_alpha,
        mesh.material.alpha_test,
        mesh.material.alpha_threshold,
    );
    if mesh.material.has_alpha || implicit_decal_blend {
        // FO4's dedicated decal pass composites texture alpha even when the
        // BGSM generic blend function is `None` for low-threshold soft
        // decals. Preserve explicit factors; otherwise use alpha-over.
        let (src_blend, dst_blend) = if mesh.material.has_alpha {
            (mesh.material.src_blend_mode, mesh.material.dst_blend_mode)
        } else {
            (6, 7)
        };
        world.insert(
            entity,
            AlphaBlend {
                src_blend,
                dst_blend,
            },
        );
    }
    if mesh.material.is_decal {
        world.insert(entity, IsDecalMesh);
    }
    if mesh.material.two_sided {
        world.insert(entity, TwoSided);
    }
    // #renderlayer — derive the per-entity content-class layer.
    // Base layer comes from the REFR's record type
    // (`stat.record_type.render_layer()`); the per-mesh
    // `mesh.material.is_decal` (NIF-flagged decals — blood splats, scorch
    // marks) and `mesh.material.alpha_test_func != 0` (alpha-tested rugs /
    // posters / fences / cutout foliage) escalate to
    // [`RenderLayer::Decal`] regardless of the base, so any
    // coplanar overlay wins its z-fight against the surface
    // beneath. Architecture (zero bias) is the safe default for
    // the rare "neither base nor mesh hints decal" path.
    //
    // Pre-#renderlayer this site also inserted a `Decal` marker
    // component when `mesh.material.is_decal` — that marker is retired now
    // that `RenderLayer::Decal` carries the same signal end-to-end.
    {
        use byroredux_core::ecs::components::{
            escalate_small_static_to_clutter, render_layer_with_decal_escalation,
        };
        // Small-STAT escalation runs first so decorative clutter
        // authored as STAT (paper piles, folders, clipboards on
        // desks — Bethesda's record-type classifier can't tell
        // these from architectural STATs without spatial extent)
        // gets the Clutter bias before the decal gate sees it.
        // Decal escalation still wins for alpha-tested overlays
        // and NIF-flagged decals regardless of size.
        //
        // The post-escalation layer is the RENDER-z-bias signal
        // (`RenderLayer` ECS component). #1294 moved the collision
        // trimesh-fallback gate below off this post-escalation
        // layer onto `base_layer` so SF sub-decomposed architecture
        // (per-LOD per-material sub-meshes < 50 units each, but
        // composing into a 1000-unit wall) doesn't get its
        // collider stripped on a render-side optimization.
        let layer =
            escalate_small_static_to_clutter(base_layer, mesh.local_bound_radius * ref_scale);
        let layer = render_layer_with_decal_escalation(
            layer,
            mesh.material.is_decal,
            mesh.material.alpha_test,
        );
        world.insert(entity, layer);
    }

    // F3 (2026-05-27) — synthesize a static TriMesh collider from
    // the render geometry when the NIF authored NO bhk collision.
    // This is the FO4+ case: those games moved static architecture
    // collision into the Havok content-system blob
    // (`bhkNPCollisionObject` → `bhkPhysicsSystem`), which our
    // `extract_collision` doesn't deserialize yet (a multi-day
    // project — see docs/audits/FALLOUT_SYMPTOMS F3). Without any
    // static collider the M28.5 character controller has nothing
    // to ground against and the player falls through the floor.
    //
    // The render mesh is a coarse but serviceable stand-in for the
    // authored collision hull on structural architecture (floors,
    // walls, ramps). Gated tightly so we don't turn clutter, decals,
    // or skinned actors into expensive trimesh colliders:
    //   - `collisions_empty` — the NIF gave us no bhk shape, so
    //     we're not double-covering FNV/FO3/Skyrim (which parse bhk).
    //   - `RenderLayer::Architecture` — structural only; clutter and
    //     decals are escalated away from this layer above.
    //   - `!mesh.skinned` — never synthesize for animated bodies.
    //   - `!mesh.material.is_decal && !mesh.material.alpha_test` — skip overlay planes.
    //   - ≥ 1 triangle of geometry.
    // Scale: the physics sync places bodies by GlobalTransform
    // translation+rotation only (it ignores scale — bhk shapes bake
    // havok_scale into their verts at extract time). So we bake the
    // composed `final_scale` into the trimesh verts here to match
    // the rendered geometry.
    // #1294 — gate on `base_layer` (pre-escalation REFR record-type
    // classification), NOT `final_layer` (post-escalation render
    // layer). The small-STAT-to-Clutter escalation
    // (`escalate_small_static_to_clutter`) is a RENDER z-bias
    // optimization that demotes architecturally-classified meshes
    // with a small bounding-sphere radius (< 50 units) to the
    // Clutter render layer so decorative STATs (papers / folders
    // / clipboards) win the coplanar z-fight against desks. It was
    // never intended to gate collision generation.
    //
    // For Starfield content the gate-on-`final_layer` site rejected
    // every wall / floor / ramp on Cydonia because SF NIFs are
    // heavily decomposed into per-material per-LOD sub-meshes
    // (an industrial platform = 6 BSGeometry blocks each with 4
    // LOD slots = 24 sub-meshes), each individual sub-mesh smaller
    // than the 50-unit threshold. Even though the COMPOSITE REFR
    // is a giant wall, the per-mesh radius escalates to Clutter
    // and the trimesh fallback skips it → zero static colliders →
    // character free-falls indefinitely from frame 0 (`rapier_bodies=1`
    // diagnostic warn at `character.rs:290`).
    //
    // `base_layer` reflects the REFR's base record type
    // (STAT/MSTT/FURN/DOOR/… → Architecture; NPC_ → Actor; etc.).
    // That's the correct "should this be a static collider?" signal
    // — independent of per-mesh sub-decomposition. NPC actors (Actor
    // base) and small clutter (record-type-classified Clutter) both
    // skip the fallback as before; only the misclassified
    // sub-decomposed architecture changes behaviour.
    if collision_fallback == MissingCollisionFallback::ArchitectureTriMesh
        && mesh.skin.is_none()
        && !mesh.material.is_decal
        && !mesh.material.alpha_test
        && mesh.material.material_kind != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
        && mesh.positions.len() >= 3
        && mesh.indices.len() >= 3
    {
        // Shared with the exterior LAND path — see
        // `spawn_trimesh_collider_ghost`. The render `entity` keeps its
        // MeshHandle and enters BLAS+TLAS normally (RT shadows/GI on
        // FO4/Starfield architecture); the ghost is physics-only.
        let source_form = world
            .get::<FormIdComponent>(placement_root)
            .map(|form_id| form_id.0);
        *synthesized_collision_proxy |= spawn_trimesh_collider_ghost(
            world,
            &mesh.positions,
            &mesh.indices,
            final_pos,
            final_rot,
            final_scale,
            source_form,
        );
    }
    // Attach ESM light_data ONLY if the NIF didn't actually spawn
    // any lights (avoids duplicates) and only on the first mesh
    // (avoids N copies when a lamp NIF has multiple sub-meshes).
    //
    // Pre-#632 this gated on `nif_lights.is_empty()` — wrong
    // because zero-colour placeholders take a slot in the array
    // but get filtered out at the spawn loop above. Cells with
    // light-bulb meshes (Prospector Saloon) rendered dark even
    // though both the NIF placeholder and the ESM LIGH record
    // agreed there should be a light. Track real spawns instead.
    if let Some(ld) = light_data {
        if spawned_nif_lights == 0 && count == 0 {
            // Phase 18 (reverted in Phase 19.7) — the
            // flame-node offset spawn lived here, but the
            // substring-based pattern match
            // (`flame` / `fire` / `attachlight`) hit
            // false-positives on at least one Skyrim candle
            // NIF (upper shelf in Sleeping Giant Inn — visible
            // as "no light emitted at all" from that REFR's
            // light placement). Restoring the pre-Phase-18
            // attach-to-mesh-entity-at-ref_pos behaviour.
            //
            // The Phase 18 *capture* path stays — every cached
            // NIF still records `flame_attach_offset` at parse
            // time. A future re-enable with tighter pattern
            // matching (e.g. `^Flame[0-9]+$` regex, or
            // requiring an `AttachFire` block specifically)
            // can consume the captured offset without
            // re-walking the NIF.
            let _ = cached.flame_attach_offset;

            world.insert(
                entity,
                LightSource::from_legacy_world_units(
                    light_radius_or_default(ld.radius),
                    ld.color,
                    ld.flags,
                    ld.falloff_exponent,
                    light_kind,
                    light_direction,
                    light_outer_angle,
                    light_shadow_flags,
                ),
            );
            // Phase 17 — animation companion at the placement root,
            // same position as the mesh entity. The caller has already
            // decoded source-game LIGH flags into shared behavior.
            attach_light_flicker_if_needed(world, entity, ld, ref_pos, light_animation_flags);
        }
    }
    true
}

#[cfg(test)]
mod synthesize_trimesh_tests {
    use super::{
        missing_collision_fallback, spawn_packed_havok_proxy, spawn_trimesh_collider_ghost,
        synthesize_packed_havok_proxy, synthesize_static_trimesh, transformed_mesh_aabb,
        MissingCollisionFallback, ProxyMeshGeometry,
    };
    use byroredux_core::{
        ecs::{
            components::{
                CollisionShape, MeshHandle, MotionType, PhysicsSourceForm, RenderLayer,
                RigidBodyData,
            },
            World,
        },
        form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId},
        math::{Quat, Vec3},
    };
    use byroredux_nif::import::collision::CollisionAuthoringSummary;

    /// A single unit triangle synthesizes into a 1-triangle TriMesh
    /// with a Static body. Baseline that the geometry round-trips.
    #[test]
    fn single_triangle_round_trips() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices = [0u32, 1, 2];
        let (shape, body) =
            synthesize_static_trimesh(&positions, &indices, 1.0).expect("one triangle");
        match shape {
            CollisionShape::TriMesh { vertices, indices } => {
                assert_eq!(vertices.len(), 3);
                assert_eq!(indices, vec![[0, 1, 2]]);
            }
            other => panic!("expected TriMesh, got {other:?}"),
        }
        assert_eq!(body.motion_type, MotionType::Static);
    }

    /// LAND terrain and NIF architecture both call this helper for missing
    /// authored collision. Pin the shared floor contract: one static physics
    /// proxy, with no render mesh/BLAS payload of its own.
    #[test]
    fn floor_collider_ghost_is_static_and_renderer_free() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let indices = [0u32, 1, 2];
        let mut world = World::new();

        assert!(spawn_trimesh_collider_ghost(
            &mut world,
            &positions,
            &indices,
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
            None,
        ));

        let shape_q = world
            .query::<CollisionShape>()
            .expect("ghost must carry CollisionShape");
        let (entity, _) = shape_q.iter().next().expect("one collider ghost");
        assert_eq!(shape_q.iter().count(), 1);

        let body_q = world
            .query::<RigidBodyData>()
            .expect("ghost must carry RigidBodyData");
        assert_eq!(
            body_q
                .get(entity)
                .expect("same ghost owns the body")
                .motion_type,
            MotionType::Static
        );

        let mesh_q = world.query::<MeshHandle>();
        assert!(
            mesh_q.as_ref().is_none_or(|q| !q.contains(entity)),
            "physics floor proxy must not create an extra raster/TLAS instance"
        );
    }

    #[test]
    fn placement_trimesh_ghost_keeps_reference_ownership_backlink() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let indices = [0u32, 1, 2];
        let mut world = World::new();
        let mut pool = FormIdPool::new();
        let source_form = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(0x1234),
        });

        assert!(spawn_trimesh_collider_ghost(
            &mut world,
            &positions,
            &indices,
            Vec3::ZERO,
            Quat::IDENTITY,
            1.0,
            Some(source_form),
        ));

        let backlink = world
            .query::<PhysicsSourceForm>()
            .and_then(|query| query.iter().next().map(|(_, source)| *source));
        assert_eq!(backlink, Some(PhysicsSourceForm(source_form)));
    }

    /// `world_scale` bakes into the vertex positions — the physics sync
    /// ignores `GlobalTransform` scale, so the collider must carry it.
    #[test]
    fn world_scale_bakes_into_vertices() {
        let positions = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
        let indices = [0u32, 1, 2];
        let (shape, _) =
            synthesize_static_trimesh(&positions, &indices, 2.0).expect("scaled triangle");
        match shape {
            CollisionShape::TriMesh { vertices, .. } => {
                assert_eq!(vertices[0].to_array(), [2.0, 4.0, 6.0]);
                assert_eq!(vertices[2].to_array(), [14.0, 16.0, 18.0]);
            }
            other => panic!("expected TriMesh, got {other:?}"),
        }
    }

    /// Fewer than 3 indices → no triangle → `None`.
    #[test]
    fn degenerate_index_count_returns_none() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        assert!(synthesize_static_trimesh(&positions, &[0, 1], 1.0).is_none());
        assert!(synthesize_static_trimesh(&positions, &[], 1.0).is_none());
    }

    /// Triangles that reference out-of-range vertices are dropped (a
    /// corrupt index buffer must not reach parry3d's trimesh builder,
    /// which would panic). When every triangle is out of range the
    /// result is `None`.
    #[test]
    fn out_of_range_indices_are_dropped() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        // Second triangle references vertex 9 (out of range, only 3
        // verts). First triangle is valid.
        let indices = [0u32, 1, 2, 0, 1, 9];
        let (shape, _) =
            synthesize_static_trimesh(&positions, &indices, 1.0).expect("one valid triangle");
        match shape {
            CollisionShape::TriMesh { indices, .. } => {
                assert_eq!(indices, vec![[0, 1, 2]], "out-of-range triangle dropped");
            }
            other => panic!("expected TriMesh, got {other:?}"),
        }
        // All-out-of-range → None.
        let all_bad = [9u32, 10, 11];
        assert!(synthesize_static_trimesh(&positions, &all_bad, 1.0).is_none());
    }

    #[test]
    fn collision_authoring_selects_packed_proxy_only_for_safe_layers() {
        let packed = CollisionAuthoringSummary {
            new_physics: 1,
            ..Default::default()
        };
        assert_eq!(
            missing_collision_fallback(true, packed, RenderLayer::Clutter),
            MissingCollisionFallback::PackedAabbProxy,
        );
        assert_eq!(
            missing_collision_fallback(true, packed, RenderLayer::Actor),
            MissingCollisionFallback::PackedAabbProxy,
        );
        assert_eq!(
            missing_collision_fallback(true, packed, RenderLayer::Architecture),
            MissingCollisionFallback::ArchitectureTriMesh,
        );
        assert_eq!(
            missing_collision_fallback(true, packed, RenderLayer::Decal),
            MissingCollisionFallback::None,
        );
        assert_eq!(
            missing_collision_fallback(
                true,
                CollisionAuthoringSummary::default(),
                RenderLayer::Clutter,
            ),
            MissingCollisionFallback::None,
            "clutter with no authored packed collision must remain non-colliding",
        );
        assert_eq!(
            missing_collision_fallback(false, packed, RenderLayer::Clutter),
            MissingCollisionFallback::None,
            "decoded collision always wins over a compatibility proxy",
        );
    }

    #[test]
    fn packed_proxy_aabb_unions_mesh_local_transforms() {
        let a = [[-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]];
        let b = [[0.0, 0.0, 0.0], [2.0, 4.0, 6.0]];
        let (min, max) = transformed_mesh_aabb([
            ProxyMeshGeometry {
                positions: &a,
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: 1.0,
            },
            ProxyMeshGeometry {
                positions: &b,
                translation: Vec3::new(10.0, 0.0, 0.0),
                rotation: Quat::IDENTITY,
                scale: 0.5,
            },
        ])
        .expect("finite mesh geometry must produce an AABB");

        assert_eq!(min, Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(max, Vec3::new(11.0, 2.0, 3.0));
    }

    #[test]
    fn packed_proxy_bakes_outer_scale_into_cuboid_extent() {
        let mut mesh = byroredux_nif::import::ImportedMesh::from_geometry(
            vec![[-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        mesh.translation = [4.0, 5.0, 6.0];

        let (center, shape) = synthesize_packed_havok_proxy(&[mesh], 2.0)
            .expect("finite render geometry must produce a packed-Havok proxy");
        assert_eq!(center, Vec3::new(4.0, 5.0, 6.0));
        match shape {
            CollisionShape::Cuboid { half_extents } => {
                assert_eq!(half_extents, Vec3::new(2.0, 4.0, 6.0));
            }
            other => panic!("expected Cuboid, got {other:?}"),
        }
    }

    /// Regression for #2543: an extreme-but-finite `ref_scale` (unclamped
    /// REFR `XSCL` read straight off disk) must not hand back a
    /// `Cuboid` whose half-extents dwarf the world — it must clamp to
    /// `RT_ABSOLUTE_PRECISION_CEILING` instead of multiplying straight
    /// through.
    #[test]
    fn packed_proxy_clamps_extreme_finite_scale() {
        let mut mesh = byroredux_nif::import::ImportedMesh::from_geometry(
            vec![[-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        mesh.translation = [4.0, 5.0, 6.0];

        // Large enough to blow well past `RT_ABSOLUTE_PRECISION_CEILING`
        // (~1e6) but nowhere near `f32::MAX` (~3.4e38), so the product
        // stays finite — this is the "corrupt-but-finite" case the debug
        // assert alone can't catch in release builds.
        let (_, shape) = synthesize_packed_havok_proxy(&[mesh], 1.0e30)
            .expect("a finite (if extreme) scale must still produce a clamped proxy");
        match shape {
            CollisionShape::Cuboid { half_extents } => {
                assert!(
                    half_extents.is_finite(),
                    "clamped half_extents must stay finite: {half_extents:?}"
                );
                assert!(
                    half_extents.x <= super::super::references::RT_ABSOLUTE_PRECISION_CEILING
                        && half_extents.y
                            <= super::super::references::RT_ABSOLUTE_PRECISION_CEILING
                        && half_extents.z
                            <= super::super::references::RT_ABSOLUTE_PRECISION_CEILING,
                    "half_extents {half_extents:?} must be clamped to the sane-magnitude ceiling"
                );
            }
            other => panic!("expected Cuboid, got {other:?}"),
        }
    }

    /// Regression for #2543: a non-finite product (e.g. an `f32` overflow
    /// during the scale multiply) must reject the proxy outright rather
    /// than propagate `Infinity`/`NaN` half-extents into the ECS.
    #[test]
    fn packed_proxy_rejects_non_finite_half_extents() {
        let mut mesh = byroredux_nif::import::ImportedMesh::from_geometry(
            vec![[-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        mesh.translation = [4.0, 5.0, 6.0];
        // Finite input scale, but `f32::MAX * f32::MAX` overflows to
        // `Infinity` inside `synthesize_packed_havok_proxy`'s multiply —
        // this must not slip through as a literal-infinite collider.
        mesh.scale = f32::MAX;

        assert!(
            synthesize_packed_havok_proxy(&[mesh], f32::MAX).is_none(),
            "an overflowing (non-finite) half-extents product must reject the proxy"
        );
    }

    /// Regression for #2531 / NIFAL-D6-NEW-01: a skinned mesh's bind-pose
    /// `positions` (splayed T-pose-wide) must NOT be unioned directly into
    /// the proxy AABB — the mesh's own pose-independent `local_bound_*`
    /// (deliberately tight here) must be used instead. Proves the fix by
    /// using positions wide enough that, if they leaked through unfiltered,
    /// the resulting cuboid would be far larger than the tight bound allows.
    #[test]
    fn packed_proxy_uses_local_bound_not_bind_pose_positions_for_skinned_mesh() {
        let mut mesh = byroredux_nif::import::ImportedMesh::from_geometry(
            // T-pose-wide bind-pose positions — splayed limbs, ±50 units.
            vec![[-50.0, 0.0, 0.0], [50.0, 0.0, 0.0]],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        mesh.skin = Some(byroredux_nif::import::ImportedSkin::default());
        // Tight, pose-independent local bound — what the fix must use instead.
        mesh.local_bound_center = [0.0, 0.0, 0.0];
        mesh.local_bound_radius = 2.0;

        let (center, shape) = synthesize_packed_havok_proxy(&[mesh], 1.0)
            .expect("a skinned-only mesh set must still produce a proxy (not #2355's regression)");
        assert_eq!(center, Vec3::ZERO);
        match shape {
            CollisionShape::Cuboid { half_extents } => {
                assert!(
                    half_extents.x <= 2.0 && half_extents.y <= 2.0 && half_extents.z <= 2.0,
                    "half_extents {half_extents:?} must come from the tight local_bound_radius \
                     (2.0), not the ±50 bind-pose positions — a T-pose leaking through would \
                     produce half_extents around 50.0"
                );
            }
            other => panic!("expected Cuboid, got {other:?}"),
        }
    }

    /// Companion: a rigid (non-skinned) mesh and a skinned mesh together
    /// must union BOTH contributions — the rigid mesh's real positions and
    /// the skinned mesh's local bound sphere — into one proxy, proving the
    /// skin filter doesn't silently drop the rigid geometry it sits
    /// alongside (e.g. a creature's non-skinned prop/weapon submesh).
    #[test]
    fn packed_proxy_unions_rigid_positions_and_skinned_local_bound() {
        let rigid = byroredux_nif::import::ImportedMesh::from_geometry(
            vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let mut skinned = byroredux_nif::import::ImportedMesh::from_geometry(
            vec![[-50.0, -50.0, -50.0], [50.0, 50.0, 50.0]],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        skinned.skin = Some(byroredux_nif::import::ImportedSkin::default());
        skinned.translation = [10.0, 0.0, 0.0];
        skinned.local_bound_center = [0.0, 0.0, 0.0];
        skinned.local_bound_radius = 1.0;

        let (_, shape) = synthesize_packed_havok_proxy(&[rigid, skinned], 1.0)
            .expect("mixed rigid + skinned mesh set must produce a proxy");
        match shape {
            CollisionShape::Cuboid { half_extents } => {
                // Rigid mesh spans x in [0, 1]; skinned sphere (radius 1,
                // translated +10) spans x in [9, 11] — union half-extent
                // on X must reach at least (11 - 0) / 2 = 5.5, and must NOT
                // reach anywhere near the skinned mesh's raw ±50 extent
                // (which would push it to ~30).
                assert!(
                    half_extents.x >= 5.0,
                    "union must include both meshes' contributions: {half_extents:?}"
                );
                assert!(
                    half_extents.x < 15.0,
                    "the skinned mesh's raw ±50 bind-pose positions must not leak into \
                     the union: {half_extents:?}"
                );
            }
            other => panic!("expected Cuboid, got {other:?}"),
        }
    }

    #[test]
    fn packed_proxy_is_keyframed_and_parented_to_visual_placement() {
        use crate::cell_loader::nif_import_registry::CachedNifImport;
        use byroredux_core::ecs::{GlobalTransform, Parent, Transform};

        let mesh = byroredux_nif::import::ImportedMesh::from_geometry(
            vec![[-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let cached = CachedNifImport {
            meshes: vec![mesh],
            collisions: Vec::new(),
            collision_authoring: CollisionAuthoringSummary {
                new_physics: 1,
                ..Default::default()
            },
            lights: Vec::new(),
            particle_emitters: Vec::new(),
            embedded_clip: None,
            placement_root_billboard: None,
            bsx_flags: 0,
            root_flags: 0,
            flame_attach_offset: None,
            attach_points: None,
            child_attach_connections: None,
            furniture: None,
        };
        let mut world = World::new();
        let root = world.spawn();
        world.insert(root, Transform::default());
        world.insert(root, GlobalTransform::default());

        assert!(spawn_packed_havok_proxy(
            &mut world,
            &cached,
            root,
            Vec3::new(10.0, 20.0, 30.0),
            Quat::IDENTITY,
            1.0,
            None,
            RenderLayer::Clutter,
        ));

        let bodies = world
            .query::<RigidBodyData>()
            .expect("proxy must carry a rigid body");
        let (proxy, body) = bodies.iter().next().expect("one proxy body");
        assert_eq!(body.motion_type, MotionType::Keyframed);
        let parents = world.query::<Parent>().expect("proxy must be parented");
        assert_eq!(parents.get(proxy).map(|p| p.0), Some(root));
        let meshes = world.query::<MeshHandle>();
        assert!(meshes.as_ref().is_none_or(|q| !q.contains(proxy)));
    }
}
