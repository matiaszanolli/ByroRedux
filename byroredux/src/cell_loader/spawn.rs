//! Spawn ECS entities for one cached NIF placement.
//!
//! Per REFR placement the loader calls `spawn_placed_instances` to
//! create the actual ECS entities (mesh draw, lights from NiLight
//! blocks, particle emitters, collision shapes) under a single
//! `placement_root` parent. Driven by `load_references`; called once
//! per placement at cell load time.

use byroredux_core::ecs::{
    GlobalTransform, LightSource, Material, MeshHandle, ParticleEmitter, TextureHandle, Transform,
    World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_renderer::VulkanContext;

use crate::asset_provider::{
    resolve_texture, resolve_texture_with_clamp, TextureProvider,
};
use crate::components::{AlphaBlend, DarkMapHandle, ExtraTextureMaps, NormalMapHandle, TwoSided};

use super::nif_import_registry::CachedNifImport;
use super::pack_effect_shader_flags;
use super::refr::RefrTextureOverlay;

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
/// `4096.0` matches the cell-scale fallback already used at the
/// NIF-direct spawn site for ambient / directional placeholders
/// without an authored radius. Authored Bethesda XCLL radii are
/// 256–4096 units, so this default is a "covers the cell" net,
/// not a typical value — a malformed LIGH record that ships
/// `radius=0` becomes visible rather than silently invisible.
#[inline]
pub(crate) fn light_radius_or_default(radius: f32) -> f32 {
    if radius > 0.0 {
        radius
    } else {
        4096.0
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
    refr_overlay: Option<&RefrTextureOverlay>,
    clip_handle: Option<u32>,
    // #renderlayer — base content-class derived from the REFR's base
    // record type via `RecordType::render_layer()`. Per-mesh
    // `is_decal` / `alpha_test_func` escalate this to
    // `RenderLayer::Decal` at the spawn site below; the caller passes
    // the unescalated base layer.
    base_layer: byroredux_core::ecs::components::RenderLayer,
    mesh_cache_key: Option<&str>,
) -> usize {
    use byroredux_core::ecs::{Name, Parent};
    use byroredux_renderer::Vertex;

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
    let placement_root = world.spawn();
    world.insert(placement_root, Transform::new(ref_pos, ref_rot, ref_scale));
    world.insert(
        placement_root,
        GlobalTransform::new(ref_pos, ref_rot, ref_scale),
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
        let final_pos = ref_rot * (ref_scale * nif_pos) + ref_pos;
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
            LightSource {
                radius,
                color: light.color,
                flags: 0,
                ..Default::default()
            },
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
                let mut pool =
                    world.resource_mut::<byroredux_core::string::StringPool>();
                pool.intern(nif_name)
            };
            world.insert(entity, Name(interned));
        }
    }

    // Spawn particle emitter entities (#401). One ECS entity per
    // detected NiParticleSystem, positioned at the composed REFR + NIF-
    // local transform. The heuristic preset is picked from the nearest
    // named ancestor in the NIF (host_name):
    //   spark/ember/cinder → embers (small, bright, additive — checked
    //                                FIRST so "FireSparks" doesn't fall
    //                                into the larger flame body)
    //   torch/fire/flame/brazier/candle → torch_flame
    //   smoke/steam/ash      → smoke
    //   magic/enchant/sparkle/glow → magic_sparkles
    //   fallback             → torch_flame so the audit's "every torch
    //                          invisible" failure is resolved end-to-
    //                          end even when the host node carries no
    //                          descriptive name.
    // Mirrored in `byroredux/src/scene.rs` — keep both lists in lockstep.
    // The proper data-driven fix (NIF-authored colour curves via
    // `NiPSysColorModifier` → `NiColorData`) stays open at #707; this
    // is the heuristic band-aid that landed first.
    for em in &cached.particle_emitters {
        let nif_pos = Vec3::new(
            em.local_position[0],
            em.local_position[1],
            em.local_position[2],
        );
        let world_pos = ref_rot * (ref_scale * nif_pos) + ref_pos;
        let host = em.host_name.as_deref().unwrap_or("").to_ascii_lowercase();
        let mut preset =
            if host.contains("spark") || host.contains("ember") || host.contains("cinder") {
                ParticleEmitter::embers()
            } else if host.contains("torch")
                || host.contains("fire")
                || host.contains("flame")
                || host.contains("brazier")
                || host.contains("candle")
            {
                ParticleEmitter::torch_flame()
            } else if host.contains("smoke") || host.contains("steam") || host.contains("ash") {
                ParticleEmitter::smoke()
            } else if host.contains("magic")
                || host.contains("enchant")
                || host.contains("sparkle")
                || host.contains("glow")
            {
                ParticleEmitter::magic_sparkles()
            } else {
                ParticleEmitter::torch_flame()
            };
        // #707 / FX-2 — override preset start/end colour from the
        // authored `NiPSysColorModifier -> NiColorData` keyframe stream
        // when the NIF carries one. See the parallel block in scene.rs
        // for the rationale; same field origin.
        if let Some(curve) = em.color_curve {
            preset.start_color = curve.start;
            preset.end_color = curve.end;
        }
        // #984 / NIF-D5-ORPHAN-A2 — carry authored force-field
        // modifiers onto the spawned `ParticleEmitter` so the
        // simulator can integrate gravity / vortex / drag /
        // turbulence / air / radial alongside the preset's `gravity`
        // scalar. NIF Z-up axes are converted to engine Y-up via
        // `convert_force_fields_zup_to_yup`. Empty for emitters whose
        // source NIF authored no field modifiers — preset behaviour
        // is unchanged in that case.
        preset.force_fields =
            crate::systems::convert_force_fields_zup_to_yup(&em.force_fields);
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(world_pos));
        world.insert(entity, GlobalTransform::new(world_pos, Quat::IDENTITY, 1.0));
        world.insert(entity, preset);
    }

    // Spawn collision entities from NiNode collision data.
    // Guard against parry3d panics from nested composite shapes — some
    // Bethesda NIFs have deeply nested bhkCompressedMeshShape hierarchies
    // that parry3d's Compound shape rejects. Skip those shapes gracefully.
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
        let final_pos = ref_rot * (ref_scale * nif_pos) + ref_pos;
        let final_rot = ref_rot * nif_quat;
        let final_scale = ref_scale * coll.scale;

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
    }

    // #882 / CELL-PERF-05 — single StringPool lock acquisition for the
    // whole spawn loop. Pre-fix the loop took one read lock per mesh
    // (10 path-slot resolves) AND one write lock per mesh (the
    // `mesh.name` intern), so Megaton's hundreds of placements paid
    // hundreds of `RwLock` CAS pairs on the cell-load critical path.
    // The borrow-checker forbids hoisting the guard across the spawn
    // loop (`world.spawn()` / `world.insert()` need `&mut world` while
    // `resource_mut` borrows `&world`), so we resolve every path slot
    // + intern every name in this single pre-pass and let the spawn
    // loop below read the pre-computed values. Mirrors the #523
    // batched-commit pattern already in use one level up at
    // `load_references`.
    //
    // Local `fn resolve_to_owned` (not a closure) so the inline
    // resolves don't capture `&pool` for longer than a statement and
    // the trailing `pool.intern(...)` can re-borrow as `&mut`.
    struct ResolvedMeshPaths {
        texture_path: Option<String>,
        normal_map: Option<String>,
        glow_map: Option<String>,
        gloss_map: Option<String>,
        parallax_map: Option<String>,
        env_map: Option<String>,
        env_mask: Option<String>,
        material_path: Option<String>,
        detail_map: Option<String>,
        dark_map: Option<String>,
        name_sym: Option<byroredux_core::string::FixedString>,
    }
    fn resolve_to_owned(
        pool: &byroredux_core::string::StringPool,
        sym: Option<byroredux_core::string::FixedString>,
    ) -> Option<String> {
        sym.and_then(|s| pool.resolve(s)).map(|s| s.to_string())
    }
    let resolved_paths: Vec<ResolvedMeshPaths> = {
        let ov = refr_overlay;
        let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
        imported
            .iter()
            .map(|mesh| {
                // Effective texture slot paths. REFR overlay
                // (XATO/XTNM/XTXR) wins over the NIF-authored paths
                // when present; for slots the overlay left empty the
                // cached NIF's texture rides through. `None` on both
                // sides means the slot has no texture. See #584.
                let texture_path =
                    resolve_to_owned(&pool, ov.and_then(|o| o.diffuse).or(mesh.texture_path));
                let normal_map =
                    resolve_to_owned(&pool, ov.and_then(|o| o.normal).or(mesh.normal_map));
                let glow_map = resolve_to_owned(&pool, ov.and_then(|o| o.glow).or(mesh.glow_map));
                let gloss_map =
                    resolve_to_owned(&pool, ov.and_then(|o| o.specular).or(mesh.gloss_map));
                let parallax_map =
                    resolve_to_owned(&pool, ov.and_then(|o| o.height).or(mesh.parallax_map));
                let env_map = resolve_to_owned(&pool, ov.and_then(|o| o.env).or(mesh.env_map));
                let env_mask =
                    resolve_to_owned(&pool, ov.and_then(|o| o.env_mask).or(mesh.env_mask));
                let material_path = resolve_to_owned(
                    &pool,
                    ov.and_then(|o| o.material_path).or(mesh.material_path),
                );
                // Detail/dark slots come straight from the NIF
                // (no REFR-overlay path for these today).
                let detail_map = resolve_to_owned(&pool, mesh.detail_map);
                let dark_map = resolve_to_owned(&pool, mesh.dark_map);
                // Intern the mesh name in the same lock — see #882's
                // second hotspot. `mesh.name: Option<Arc<str>>`. The
                // `pool.intern` call must follow the resolves so the
                // `&pool` borrows from `resolve_to_owned` end before
                // the `&mut pool` re-borrow.
                let name_sym = mesh.name.as_deref().map(|n| pool.intern(n));
                ResolvedMeshPaths {
                    texture_path,
                    normal_map,
                    glow_map,
                    gloss_map,
                    parallax_map,
                    env_map,
                    env_mask,
                    material_path,
                    detail_map,
                    dark_map,
                    name_sym,
                }
            })
            .collect()
        // pool guard dropped here at end of block.
    };

    let mut blas_specs: Vec<(u32, u32, u32)> = Vec::new();
    for (sub_mesh_index, mesh) in imported.iter().enumerate() {
        let paths = &resolved_paths[sub_mesh_index];
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
        let cache_hit_handle = mesh_cache_key
            .and_then(|key| ctx.mesh_registry.acquire_cached(key, sub_mesh_index_u32));

        let mesh_handle = if let Some(handle) = cache_hit_handle {
            // Cached: skip the CPU vertex-build, the GPU upload, AND
            // the BLAS batch entry. The existing BLAS for this handle
            // is already attached to live placements in earlier cells
            // (or earlier in this same cell).
            handle
        } else {
            let vertices: Vec<Vertex> = (0..num_verts)
                .map(|i| {
                    // Drop alpha — current `Vertex` color is 3-channel; the
                    // alpha lane lives on `ImportedMesh::colors[i][3]` for
                    // when the renderer extends to a 4-channel vertex (#618).
                    let color3 = if i < mesh.colors.len() {
                        let c = mesh.colors[i];
                        [c[0], c[1], c[2]]
                    } else {
                        [1.0, 1.0, 1.0]
                    };
                    let mut v = Vertex::new(
                        mesh.positions[i],
                        color3,
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
            let upload_result = match mesh_cache_key {
                Some(key) => ctx.mesh_registry.register_scene_mesh_keyed(
                    &ctx.device,
                    alloc,
                    &ctx.graphics_queue,
                    ctx.transfer_pool,
                    &vertices,
                    &mesh.indices,
                    ctx.device_caps.ray_query_supported,
                    None,
                    key,
                    sub_mesh_index_u32,
                ),
                None => ctx.mesh_registry.upload_scene_mesh(
                    &ctx.device,
                    alloc,
                    &ctx.graphics_queue,
                    ctx.transfer_pool,
                    &vertices,
                    &mesh.indices,
                    ctx.device_caps.ray_query_supported,
                    None,
                ),
            };
            let handle = match upload_result {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("Failed to upload mesh: {}", e);
                    continue;
                }
            };

            // Fresh upload — this handle needs a BLAS. Subsequent
            // cache hits for the same `(path, sub_mesh_index)` reuse
            // this BLAS entry without re-submitting.
            blas_specs.push((handle, num_verts as u32, mesh.indices.len() as u32));
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
        let eff_texture_path = paths.texture_path.clone();
        let eff_normal_map = paths.normal_map.clone();
        let eff_glow_map = paths.glow_map.clone();
        let eff_gloss_map = paths.gloss_map.clone();
        let eff_parallax_map = paths.parallax_map.clone();
        let eff_env_map = paths.env_map.clone();
        let eff_env_mask = paths.env_mask.clone();
        let eff_material_path = paths.material_path.clone();
        let eff_detail_map = paths.detail_map.clone();
        let eff_dark_map = paths.dark_map.clone();

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
            mesh.texture_clamp_mode,
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

        // World-space placement (parent_rot * (parent_scale *
        // child_pos) + parent_pos) — used only to seed the initial
        // `GlobalTransform`. `Transform` itself stays NIF-local so
        // the propagation pass produces the same value next tick.
        let final_pos = ref_rot * (ref_scale * nif_pos) + ref_pos;
        let final_rot = ref_rot * nif_quat;
        let final_scale = ref_scale * mesh.scale;

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
        world.insert(
            entity,
            Material {
                emissive_color: mesh.emissive_color,
                emissive_mult: mesh.emissive_mult,
                specular_color: mesh.specular_color,
                specular_strength: mesh.specular_strength,
                diffuse_color: mesh.diffuse_color,
                ambient_color: mesh.ambient_color,
                glossiness: mesh.glossiness,
                uv_offset: mesh.uv_offset,
                uv_scale: mesh.uv_scale,
                alpha: mesh.mat_alpha,
                env_map_scale: mesh.env_map_scale,
                normal_map: eff_normal_map.clone(),
                texture_path: eff_texture_path.clone(),
                material_path: eff_material_path.clone(),
                glow_map: eff_glow_map.clone(),
                detail_map: eff_detail_map.clone(),
                gloss_map: eff_gloss_map.clone(),
                dark_map: eff_dark_map.clone(),
                vertex_color_mode: mesh.vertex_color_mode,
                alpha_test: mesh.alpha_test,
                alpha_threshold: mesh.alpha_threshold,
                alpha_test_func: mesh.alpha_test_func,
                material_kind: mesh.material_kind,
                z_test: mesh.z_test,
                z_write: mesh.z_write,
                z_function: mesh.z_function,
                shader_type_fields: if mesh.shader_type_fields.is_empty() {
                    None
                } else {
                    Some(Box::new(mesh.shader_type_fields.to_core()))
                },
                // #620 — BSEffectShaderProperty falloff cone (Skyrim+)
                // OR BSShaderNoLightingProperty falloff cone (FO3/FNV
                // SIBLING per #451). See scene.rs for the full
                // explanation; this site mirrors the same plumbing.
                effect_falloff: mesh
                    .effect_shader
                    .as_ref()
                    .map(
                        |es| byroredux_core::ecs::components::material::EffectFalloff {
                            start_angle: es.falloff_start_angle,
                            stop_angle: es.falloff_stop_angle,
                            start_opacity: es.falloff_start_opacity,
                            stop_opacity: es.falloff_stop_opacity,
                            soft_falloff_depth: es.soft_falloff_depth,
                        },
                    )
                    .or_else(|| {
                        mesh.no_lighting_falloff.as_ref().map(|nl| {
                            byroredux_core::ecs::components::material::EffectFalloff {
                                start_angle: nl.start_angle,
                                stop_angle: nl.stop_angle,
                                start_opacity: nl.start_opacity,
                                stop_opacity: nl.stop_opacity,
                                soft_falloff_depth: 0.0,
                            }
                        })
                    }),
                // #890 Stage 2 — pack the four BSEffect flag bits into
                // a GpuMaterial-format u32 so the renderer can OR them
                // straight into `GpuMaterial.material_flags` without
                // per-bit re-encoding. Zero on the FO3/FNV
                // `BSShaderNoLightingProperty` path (which shares the
                // `effect_falloff` slot but has no SLSF1/SLSF2 bits).
                effect_shader_flags: pack_effect_shader_flags(mesh.effect_shader.as_ref()),
            },
        );
        // Load and attach normal map if the material specifies one.
        if let Some(ref nmap_path) = eff_normal_map {
            let h = resolve_texture(ctx, tex_provider, Some(nmap_path.as_str()));
            if h != ctx.texture_registry.fallback() {
                world.insert(entity, NormalMapHandle(h));
            }
        }
        // Load and attach dark/lightmap if the material specifies one (#264).
        if let Some(ref dark_path) = eff_dark_map {
            let h = resolve_texture(ctx, tex_provider, Some(dark_path.as_str()));
            if h != ctx.texture_registry.fallback() {
                world.insert(entity, DarkMapHandle(h));
            }
        }
        // #399 — Resolve glow / detail / gloss texture handles. All three
        // default to 0 (no map; shader falls through to inline material
        // constants). The component is only attached when at least one
        // path resolved to a real handle, keeping the SparseSet small
        // for the bulk of meshes that have no extra maps.
        let mut resolve = |path: &Option<String>| -> u32 {
            path.as_deref()
                .map(|p| resolve_texture(ctx, tex_provider, Some(p)))
                .filter(|&h| h != ctx.texture_registry.fallback())
                .unwrap_or(0)
        };
        let glow_h = resolve(&eff_glow_map);
        let detail_h = resolve(&eff_detail_map);
        let gloss_h = resolve(&eff_gloss_map);
        let parallax_h = resolve(&eff_parallax_map);
        let env_h = resolve(&eff_env_map);
        let env_mask_h = resolve(&eff_env_mask);
        if glow_h != 0
            || detail_h != 0
            || gloss_h != 0
            || parallax_h != 0
            || env_h != 0
            || env_mask_h != 0
        {
            world.insert(
                entity,
                ExtraTextureMaps {
                    glow: glow_h,
                    detail: detail_h,
                    gloss: gloss_h,
                    parallax: parallax_h,
                    env: env_h,
                    env_mask: env_mask_h,
                    parallax_height_scale: mesh.parallax_height_scale.unwrap_or(0.04),
                    parallax_max_passes: mesh.parallax_max_passes.unwrap_or(4.0),
                },
            );
        }
        if mesh.has_alpha {
            world.insert(
                entity,
                AlphaBlend {
                    src_blend: mesh.src_blend_mode,
                    dst_blend: mesh.dst_blend_mode,
                },
            );
        }
        if mesh.two_sided {
            world.insert(entity, TwoSided);
        }
        // #renderlayer — derive the per-entity content-class layer.
        // Base layer comes from the REFR's record type
        // (`stat.record_type.render_layer()`); the per-mesh
        // `mesh.is_decal` (NIF-flagged decals — blood splats, scorch
        // marks) and `mesh.alpha_test_func != 0` (alpha-tested rugs /
        // posters / fences / cutout foliage) escalate to
        // [`RenderLayer::Decal`] regardless of the base, so any
        // coplanar overlay wins its z-fight against the surface
        // beneath. Architecture (zero bias) is the safe default for
        // the rare "neither base nor mesh hints decal" path.
        //
        // Pre-#renderlayer this site also inserted a `Decal` marker
        // component when `mesh.is_decal` — that marker is retired now
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
            let layer =
                escalate_small_static_to_clutter(base_layer, mesh.local_bound_radius * ref_scale);
            let layer = render_layer_with_decal_escalation(layer, mesh.is_decal, mesh.alpha_test);
            world.insert(entity, layer);
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
                world.insert(
                    entity,
                    LightSource {
                        radius: light_radius_or_default(ld.radius),
                        color: ld.color,
                        flags: ld.flags,
                        ..Default::default()
                    },
                );
            }
        }
        count += 1;
    }

    // Batched BLAS build: single GPU submission for all meshes in this cell.
    if !blas_specs.is_empty() {
        let built = ctx.build_blas_batched(&blas_specs);
        log::info!("Cell BLAS batch: {built}/{} meshes", blas_specs.len());
    }

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
        let player_entity = world.spawn();
        let mut player = byroredux_core::animation::AnimationPlayer::new(handle);
        player.root_entity = Some(placement_root);
        world.insert(player_entity, player);
    }

    count
}
