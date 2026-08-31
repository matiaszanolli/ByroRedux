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
use byroredux_renderer::{MorphSlot, SceneMeshUpload, VulkanContext};
use std::time::{Duration, Instant};

use crate::asset_provider::{
    derive_normal_map_path, derive_present_normal_map_path,
    resolve_material_texture_handles_with_clamp, resolve_texture, resolve_texture_with_clamp,
    MaterialProvider, TextureProvider,
};
use crate::components::{
    texture_path_is_fx_mesh, DoorTeleport, IsFxMesh, Locked, MaterialTextureDebugInfo,
    MaterialTextureHandles, MaterialTextureSource,
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
/// `RigidBodyData` from a render mesh's canonical local geometry.
/// Placement scale stays on `GlobalTransform`; the shared PHYSAL
/// converter applies it exactly once when registering the collider.
///
/// Used as a fallback when the source NIF authored no bhk collision —
/// the FO4+ Havok-content-system case. Returns `None` when the mesh
/// has no usable triangle data (degenerate index count). Vertices are
/// in NIF-local Y-up space, matching the entity's local `Transform`,
/// so the physics body's world placement composes correctly.
fn synthesize_static_trimesh(
    positions: &[[f32; 3]],
    mesh_indices: &[u32],
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
        .map(|p| Vec3::new(p[0], p[1], p[2]))
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
    let Some((shape, body)) = synthesize_static_trimesh(positions, mesh_indices) else {
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
/// Has a Papyrus `Disable()` been recorded against this placement?
///
/// #3278 — extracted from [`spawn_placed_instances`] so the decision is
/// unit-testable: that function needs a live `VulkanContext`, so the gate
/// itself would otherwise be reachable only on a machine with a GPU and real
/// game data. Same posture as [`count_spawnable_nif_lights`] above.
///
/// Answers `false` for every placement with no form id (the precombined and
/// loose-NIF spawn paths pass `None` — bake artifacts have no placement-level
/// identity to disable), and for a world with no `ReferenceEnableState` or no
/// `FormIdPool` registered. `ReferenceEnableState` is keyed by *local* form
/// id, matching `byroredux_scripting`'s own writers.
pub(crate) fn placement_is_disabled(
    world: &World,
    placement_fid: Option<byroredux_core::form_id::FormId>,
) -> bool {
    let Some(fid) = placement_fid else {
        return false;
    };
    let Some(local) = world
        .try_resource::<FormIdPool>()
        .and_then(|pool| pool.resolve(fid).map(|pair| pair.local.0))
    else {
        return false;
    };
    world
        .try_resource::<byroredux_scripting::ReferenceEnableState>()
        .is_some_and(|state| !state.is_enabled(local))
}

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
    // #3098 — XLOC lock payload from `PlacedRef.lock`. `None` on the
    // vast majority of REFRs (unlocked); `Some` on locked doors and
    // containers. Threaded through the same way as `teleport` above.
    lock: Option<esm::cell::LockData>,
    // #973 / FO4-D4-NEW-08-followup — same provider `build_refr_texture_overlay`
    // already consumed to build `refr_overlay`. Re-borrowed here (not moved)
    // so `resolve_mesh_paths`'s per-shape MSWP consumer can walk a swapped
    // shape's BGSM/BGEM chain. `None` on the precombined path (no REFR
    // overlay, so the per-shape swap is always a no-op there too).
    mat_provider: Option<&mut MaterialProvider>,
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
        lock,
    );

    // #3278 (SCR-D5-2026-08-24-01) — the runtime consumer for a Papyrus
    // `Disable()`. Before this, `ReferenceEnableState` recorded intent that
    // nothing ever read: a disabled reference stayed fully visible,
    // collidable and interactive.
    //
    // Gating here — after the placement root, before any mesh, collider or
    // light — is what makes one check cover all three at once. An unspawned
    // mesh cannot render, an unspawned collider cannot block, an unspawned
    // light cannot contribute. Hooking a render-side visibility flag instead
    // would have covered only the first: `AnimatedVisibility` is honoured in
    // `render/static_meshes.rs` but *not* in `render/skinned.rs`, and nothing
    // on the physics side reads it at all.
    //
    // The placement root itself still spawns, and deliberately so: it carries
    // the REFR's `FormIdComponent`, teleport and lock payloads, so a disabled
    // door is still addressable by `prid <fid>` / `World::find_by_form_id`
    // and still rides the normal `CellRootIndex` teardown. What it has is no
    // renderable or collidable content.
    //
    // KNOWN LIMITATION: this is consulted at spawn, so a `Disable()` on an
    // already-resident reference takes effect on that cell's next load rather
    // than immediately. Applying it live means despawning mid-frame, which
    // has to go through `unload_cell`'s GPU-handle release path or it leaks
    // mesh/texture refcounts — a separate piece of work, not a widening of
    // this one.
    if placement_is_disabled(world, placement_fid) {
        log::debug!(
            "REFR {:?} is disabled (ReferenceEnableState) — placement root spawned \
             without renderable or collidable content (#3278)",
            placement_form_id_pair,
        );
        return (placement_root, 0, PlacementSpawnTimings::default());
    }

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

    let resolved_paths = resolve_mesh_paths(
        world,
        imported,
        refr_overlay,
        mat_provider,
        Some(tex_provider),
    );
    let mut blas_specs: Vec<(u32, u32, u32)> = Vec::new();
    let pc = PlacementCtx {
        tex_provider,
        geometry_dedup: &cached.geometry_dedup,
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
    let prepared_meshes = prepare_mesh_uploads(ctx, &pc, imported, &resolved_paths);
    for (sub_mesh_index, (mesh, prepared)) in imported.iter().zip(prepared_meshes).enumerate() {
        if spawn_mesh_instance(
            world,
            ctx,
            &pc,
            cached,
            mesh,
            &resolved_paths[sub_mesh_index],
            count,
            prepared,
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
                    clip.texture_flip_channels.clone(),
                )
            })
        };
        if let Some((bools, floats, colors, texture_flips)) = channels {
            crate::anim_convert::attach_animation_sinks(
                world,
                &bools,
                &floats,
                &colors,
                &texture_flips,
                Some(ctx),
                Some(tex_provider),
                placement_root,
            );
        }

        let player_entity = world.spawn();
        // #3345 — start at the clip's authored phase offset.
        let phase = world
            .resource::<byroredux_core::animation::AnimationClipRegistry>()
            .get(handle)
            .map(|c| c.phase)
            .unwrap_or(0.0);
        let mut player = byroredux_core::animation::AnimationPlayer::new(handle).with_phase(phase);
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
#[allow(clippy::too_many_arguments)] // Mirrors the placement record boundary.
fn spawn_placement_root(
    world: &mut World,
    cached: &CachedNifImport,
    ref_pos: Vec3,
    ref_rot: Quat,
    ref_scale: f32,
    placement_form_id_pair: Option<FormIdPair>,
    teleport: Option<esm::cell::TeleportDest>,
    // #3098 — XLOC lock payload from `PlacedRef.lock`. `Some` on any
    // locked door or container; `None` (the common case) leaves the
    // placement root without a `Locked` component.
    lock: Option<esm::cell::LockData>,
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
    // #994 — seam for a future NiBillboardNode-rooted NIF producer that
    // flags a billboard mode on the cache entry instead of on the mesh.
    // No producer sets `placement_root_billboard` today — #3076 moved
    // the SpeedTree billboard onto the renderable mesh itself
    // (`mesh_instance.rs` attaches `Billboard` alongside the mesh), so
    // this branch is currently unreachable.
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
    // #3098 — XLOC lock plumbing. Stamped on the placement root for any
    // locked door OR container (this fn is generic across REFR base
    // types, so this is the shared spawn site for both) so the
    // interaction system's activation gate has something to consult.
    // See `Locked`'s doc for what's deferred.
    if let Some(l) = lock {
        world.insert(
            placement_root,
            Locked {
                lock_level: l.lock_level,
                key_form_id: l.key_form_id,
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
        let world_direction = (ref_rot
            * Vec3::new(light.direction[0], light.direction[1], light.direction[2]))
        .to_array();
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
                world_direction,
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
            em.max_particles,
        );
        // #2610 — see the sibling site in `scene/nif_loader.rs`: the authored
        // BGEM effect payload is packed into the canonical
        // `material_flag::EFFECT_*` word at this importer boundary, not
        // re-derived in the renderer.
        preset.effect_shader_flags =
            crate::cell_loader::pack_effect_shader_flags(em.effect_shader.as_ref());

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
            let now_seconds = { world.resource::<byroredux_core::ecs::TotalTime>().0 };
            let combustion_state =
                crate::fog::combustion_state_from_particle(fog_volume, &preset, now_seconds);
            world.insert(entity, fog_volume);
            if let Some(state) = combustion_state {
                world.insert(entity, state);
            }
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

        // `coll.shape` is the canonical `CollisionShape` enum — a plain
        // data structure (#[derive(Clone)] on Vec/Box/glam types) — so
        // cloning it here cannot panic regardless of nesting depth. The
        // nested-Compound parry3d panic this used to guard against was
        // fixed structurally by #373, which made the Rapier conversion
        // (`crates/physics/src/convert.rs`) flatten any Compound-of-
        // Compound into a `Vec<(Isometry3, SharedShape)>` before it ever
        // reaches parry3d — a step that happens downstream of this spawn
        // site, not here.
        let shape = coll.shape.clone();

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

// Per-sub-mesh instance spawn (#2410 / TD1-007).
pub(super) mod mesh_instance;
use mesh_instance::{prepare_mesh_uploads, resolve_mesh_paths, spawn_mesh_instance, PlacementCtx};

#[cfg(test)]
mod synthesize_trimesh_tests;
