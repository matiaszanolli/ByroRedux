//! Static mesh main loop — extracted from `build_render_data` per #1115.
//!
//! The bulk of the pre-sort render-data assembly: walks every entity
//! with `(GlobalTransform, MeshHandle)`, applies the ~13 optional
//! per-entity component modifiers (texture, alpha, two-sided, decal,
//! visibility, material, normal map, world bound, render layer,
//! animated UV, dark/extra maps, terrain tile slot, FX-mesh marker),
//! frustum-culls via `FrustumPlanes::contains_sphere`, builds a
//! `DrawCommand`, interns its material payload into the deduplicated
//! `MaterialTable`, and appends to `draw_commands`.
//!
//! Held read locks (≈15 ECS storages) live for the full body of this
//! function — no concurrent writer runs against any of them because
//! the render pass executes outside the scheduler under
//! `winit::WindowEvent::RedrawRequested`. When M40 turns the scheduler
//! parallel, this loop converts to the Bevy-style "extract stage"
//! shape (snapshot to `Vec<RenderInstance>` resource once per frame,
//! iterate it here with zero locks held).

use rustc_hash::FxHashMap;

use byroredux_core::ecs::{
    AnimatedAlpha, AnimatedAmbientColor, AnimatedDiffuseColor, AnimatedEmissiveColor,
    AnimatedShaderColor, AnimatedShaderFloat, AnimatedSpecularColor, AnimatedTextureFlip,
    AnimatedUvTransform, AnimatedVisibility, EntityId, GlobalTransform, Material, MeshHandle,
    RenderLayer, TextureHandle, World, WorldBound,
};
use byroredux_core::math::{Mat4, Vec3};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::MaterialTable;

use crate::components::{
    AlphaBlend, IsDecalMesh, IsFxMesh, IsLodTerrain, MaterialTextureHandles, TerrainTileSlot,
    TwoSided,
};

use super::camera::FrustumPlanes;
use super::{f32_sortable_u32, quantize_fade};

/// Include an LOD sphere when any part of it can intersect the conservative
/// receiver-fade + directional-trace reach around the camera.
fn lod_shadow_caster_in_range(center: Vec3, radius: f32, cam_pos: Vec3) -> bool {
    center.distance(cam_pos)
        <= byroredux_renderer::shader_constants::LOD_SHADOW_CASTER_DISTANCE + radius.max(0.0)
}

/// Walk every (GlobalTransform, MeshHandle) entity, apply per-entity
/// optional-component overrides, frustum-cull, intern materials, and
/// append the resulting `DrawCommand`s to `draw_commands`.
///
/// Must run AFTER `build_skinned_palettes` (uses `skin_offsets` to
/// stamp the per-mesh bone offset onto each draw) and BEFORE the
/// `draw_commands` sort.
pub(super) fn collect_static_mesh_draws(
    world: &World,
    frustum: &FrustumPlanes,
    vp_mat: Mat4,
    cam_pos: Vec3,
    skin_offsets: &FxHashMap<EntityId, u32>,
    draw_commands: &mut Vec<DrawCommand>,
    material_table: &mut MaterialTable,
) {
    // ── Render-data query bundle (#246) ──────────────────────────────
    //
    // Collect draw commands from entities with (GlobalTransform,
    // MeshHandle). Everything here is read-only, so each query is an
    // independent `QueryRead`. Two observations:
    //
    //   1. The ECS has no `query_n_mut!` macro for acquiring N optional
    //      components in one call, so we acquire each component
    //      separately. That's ~13 RwLock read acquisitions per frame; all
    //      reads can coexist (no deadlock risk), so no TypeId-sorted
    //      bundling is needed.
    //
    //   2. The bundle is held across the full `for (entity, mesh) in
    //      mq.iter()` loop. No system that writes these components
    //      runs concurrently (render runs outside the scheduler in
    //      `RedrawRequested`), so read contention is theoretical.
    //
    //   3. #501 / M40 — when the scheduler goes parallel (per CLAUDE.md
    //      architecture invariants), any concurrent writer to one of
    //      these ~13 storages will stall for the full build window
    //      (~1.5–2 ms). Fix at that point by introducing a
    //      `RenderExtract` stage that snapshots the per-entity data
    //      into a `Vec<RenderInstance>` resource in one pass and
    //      iterates it here with zero locks held (Bevy's extract-stage
    //      pattern). Deferred deliberately — implementing before M40
    //      lands would lock in a design without the constraints of the
    //      actual parallel scheduler to inform it, and would add
    //      ~0.5 ms/frame for zero benefit today.
    //
    // `GlobalTransform` and `MeshHandle` are required — if either is
    // absent there are no meshes to emit, so the whole collection path
    // is skipped. The other eight components are optional per-entity
    // modifiers (texture, alpha, two-sided, decal, visibility,
    // material, normal map, world bound) and stay as `Option<QueryRead>`
    // so entities without them fall through to the fallback path inside
    // the loop.
    let tq = world.query::<GlobalTransform>();
    let mq = world.query::<MeshHandle>();
    let tex_q = world.query::<TextureHandle>();
    let alpha_q = world.query::<AlphaBlend>();
    let two_sided_q = world.query::<TwoSided>();
    let vis_q = world.query::<AnimatedVisibility>();
    let mat_q = world.query::<Material>();
    // #525 — `AnimatedUvTransform` overrides the static
    // `Material::uv_offset` / `uv_scale` when an entity has an active
    // UV-scrolling controller (water, lava, conveyor belts, flickering
    // HUD backdrops). The component lands the per-axis values
    // independently so a single channel can drive offset.x while the
    // material's authored offset.y stays at 0 — the renderer reads the
    // full Vec2 transform here. Identity defaults (0, 0) / (1, 1)
    // mean the override is a no-op until the animation system writes
    // a non-identity slot.
    let anim_uv_q = world.query::<AnimatedUvTransform>();
    // #2221 — animated material scalars/colors. Each starts absent and is
    // inserted (at t=0, then kept live) by `anim_convert::attach_animation_sinks`
    // only for entities whose clip actually drives that channel type — see
    // that function's doc for why the sink can't just be blanket-attached.
    // Absent means "no active controller for this role"; the static
    // `Material` value is the fallback, matching every other optional
    // modifier in this loop. Mirrors `AnimatedUvTransform`'s "REPLACES,
    // doesn't blend" semantic (#525) — an active controller fully owns
    // the role it animates.
    let anim_alpha_q = world.query::<AnimatedAlpha>();
    let anim_diffuse_q = world.query::<AnimatedDiffuseColor>();
    let anim_ambient_q = world.query::<AnimatedAmbientColor>();
    let anim_specular_q = world.query::<AnimatedSpecularColor>();
    let anim_emissive_q = world.query::<AnimatedEmissiveColor>();
    let anim_shader_color_q = world.query::<AnimatedShaderColor>();
    let anim_shader_float_q = world.query::<AnimatedShaderFloat>();
    // #2221 — `NiFlipController` flipbook. Only the base-color slot
    // (`TexType::BASE_MAP == 0`, the overwhelmingly common vanilla case —
    // TV static, computer terminal screens) is wired here; a flip
    // targeting a different slot needs the same shader-type-aware
    // `slot_to_role` dispatch `cell_loader/spawn/mesh_instance.rs` uses
    // for XTXR overrides, which this loop doesn't have a mesh-material
    // handle to run (deliberately deferred rather than guessed).
    let anim_texture_flip_q = world.query::<AnimatedTextureFlip>();
    // #renderlayer — per-entity content-class for the depth-bias
    // ladder (Architecture / Clutter / Actor / Decal). Attached at
    // cell-load time from the REFR's base-record `RecordType` (see
    // `RecordType::render_layer`). Absent component falls back to
    // `Architecture` (zero bias) — identical to pre-fix behaviour.
    // The overlay escalation is applied at spawn time, not here, so this
    // query reads the final per-entity layer directly: `mesh.is_decal` →
    // `Decal`, `mesh.alpha_test` → `Clutter`. (#2446 — previously written
    // here as `mesh.is_decal || alpha_test_func != 0` → Decal, wrong in
    // both the gating field and the resulting layer.)
    let render_layer_q = world.query::<RenderLayer>();
    let texture_maps_q = world.query::<MaterialTextureHandles>();
    let terrain_tile_q = world.query::<TerrainTileSlot>();
    let wb_q = world.query::<WorldBound>();
    // PERF-D3-NEW-02 / #1136 — query once instead of 6 substring scans
    // per draw per frame. Entities tagged at spawn by `cell_loader::spawn`
    // + `scene::nif_loader` when the texture path matches an FX needle.
    let fx_q = world.query::<IsFxMesh>();
    // Authored decal surfaces need alpha-over compositing but must not become
    // coplanar depth/TLAS occluders. This is deliberately separate from the
    // RenderLayer::Decal class, which also includes ordinary alpha cutouts.
    let decal_mesh_q = world.query::<IsDecalMesh>();
    // Distant-terrain/object LOD blocks (#view-dist). They flow through this
    // same loop; a conservative camera-local subset enters the TLAS as
    // structure shadow casters while farther blocks remain raster-only.
    let lod_q = world.query::<IsLodTerrain>();
    // DEBUG BISECT (#markarth-fragments) — `BYRO_NO_CULL=1` forces every
    // static visible (skips the frustum cull) to test whether the
    // "polygons come and go as I move" fragmentation is the under-counted
    // per-sub-mesh `WorldBound.radius` (#1294 trap) culling composite
    // architecture at frustum edges. Off by default.
    // PERF-D1-NEW-02 / #1802 — cached via `OnceLock` so the hot path
    // doesn't `getenv` per frame, mirroring `apply_fog_overrides`. Env
    // vars can't change mid-process, so caching is semantics-preserving.
    let no_cull = {
        use std::sync::OnceLock;
        static NO_CULL: OnceLock<bool> = OnceLock::new();
        *NO_CULL.get_or_init(|| std::env::var_os("BYRO_NO_CULL").is_some())
    };
    if let (Some(tq), Some(mq)) = (tq, mq) {
        for (entity, mesh) in mq.iter() {
            // #1377 / D2-NEW-04 (#1805): single lookup instead of a
            // presence probe here plus a second `tq.get(entity)` later —
            // entities without a GT (recently spawned, partially loaded,
            // or missing a Transform component) are rare but previously
            // paid two SparseSet gets (vis_q + wb_q) before reaching this
            // check. Probing GT first short-circuits the expensive sibling
            // lookups for the skip case; binding `transform` here removes
            // the redundant re-fetch below.
            let Some(transform) = tq.get(entity) else {
                continue;
            };

            // Skip entities hidden by animation.
            let visible = vis_q
                .as_ref()
                .and_then(|q| q.get(entity))
                .map(|v| v.0)
                .unwrap_or(true);
            if !visible {
                continue;
            }

            // FX-decoration skip — PERF-D3-NEW-02 / #1136. Hoisted to
            // immediately after the visibility gate (D2-NEW-04 / #1805):
            // pre-fix this fired only after the frustum test below and
            // ~12 optional-component gets in the block that follows, all
            // wasted work for FX entities (crossed glow quads, god rays —
            // sprite-billboard bloom-halo fakes) that are always skipped.
            // The classification (texture-path substring scan over 6
            // needles) is precomputed at spawn time and stored as an
            // `IsFxMesh` marker so this hot path is one component-lookup
            // instead of 6 byte-windowed substring scans per draw per frame.
            if fx_q.as_ref().is_some_and(|q| q.get(entity).is_some()) {
                continue;
            }

            // Frustum cull: flag entities whose WorldBound is entirely
            // outside the view frustum with `in_raster = false`. The
            // draw loop skips rasterization for them but they still
            // reach the TLAS so on-screen fragments can hit their
            // occluder/reflector geometry via ray queries. Entities
            // without a WorldBound (or radius 0, i.e. not yet computed)
            // pass through as visible. See #237 (original cull) +
            // #516 (split raster / TLAS predicate).
            let world_bound = wb_q.as_ref().and_then(|q| q.get(entity));
            let in_raster = no_cull
                || match world_bound {
                    Some(wb) if wb.radius > 0.0 => frustum.contains_sphere(wb.center, wb.radius),
                    _ => true,
                };

            // Resolve the two consumption predicates before touching the
            // remaining optional components or hashing a material. A draw
            // outside the frustum that is also excluded from the TLAS cannot
            // be consumed by any renderer path. Effect-shader surfaces are
            // deliberately retained: they occupy VISIBILITY_LAYER_EFFECT so
            // optical/GI rays can see them while opaque shadow masks cannot.
            let is_decal_mesh = decal_mesh_q
                .as_ref()
                .is_some_and(|q| q.get(entity).is_some());
            let is_lod = lod_q.as_ref().is_some_and(|q| q.get(entity).is_some());
            let lod_shadow_caster = is_lod
                && world_bound
                    .is_some_and(|wb| lod_shadow_caster_in_range(wb.center, wb.radius, cam_pos));
            let mat = mat_q.as_ref().and_then(|q| q.get(entity));
            let material_kind = mat.map(|m| m.material_kind).unwrap_or(0);
            let in_tlas = (!is_lod || lod_shadow_caster)
                && !is_decal_mesh
                && material_kind != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION;
            if !in_raster && !in_tlas {
                continue;
            }

            {
                // #2221 — an active base-color flipbook REPLACES the
                // spawn-time-resolved `TextureHandle`, same "controller
                // fully owns the role" semantic as every other animated
                // sink in this loop.
                let tex_handle = anim_texture_flip_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .and_then(|f| f.handle_for_slot(0))
                    .or_else(|| tex_q.as_ref().and_then(|q| q.get(entity)).map(|t| t.0))
                    .unwrap_or(0);
                let alpha_comp = alpha_q.as_ref().and_then(|q| q.get(entity));
                let alpha_blend = alpha_comp.is_some();
                let (src_blend, dst_blend) = alpha_comp
                    .map(|a| (a.src_blend, a.dst_blend))
                    .unwrap_or((6, 7)); // SRC_ALPHA / INV_SRC_ALPHA defaults
                let two_sided = two_sided_q
                    .as_ref()
                    .map(|q| q.get(entity).is_some())
                    .unwrap_or(false);
                // #renderlayer — `is_decal` is now derived from
                // `RenderLayer::Decal`, not a separate `Decal` marker.
                // The shader / GpuInstance flag paths still want a
                // bool, but the ECS source-of-truth is the layer enum.
                let render_layer_for_entity = render_layer_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .copied()
                    .unwrap_or_default();
                let is_decal = render_layer_for_entity == RenderLayer::Decal;
                let bone_offset = skin_offsets.get(&entity).copied().unwrap_or(0);
                let material_texture_handles =
                    texture_maps_q.as_ref().and_then(|q| q.get(entity)).copied();
                let texture_indices = material_texture_handles
                    .map(|handles| handles.textures)
                    .unwrap_or_default();
                let normal_map_index = texture_indices.normal;
                let normal_has_alpha = material_texture_handles
                    .map(|handles| handles.normal_has_alpha)
                    .unwrap_or(false);
                let dark_map_index = texture_indices.dark;
                let glow_map_index = texture_indices.emissive;
                let detail_map_index = texture_indices.detail;
                let mut gloss_map_index = texture_indices.smooth_spec;
                // #3530 — Oblivion's `APPLY_HILIGHT2` materials bind the
                // NORMAL map into the height slot and carry their height in
                // its alpha, because that game ships no separate height
                // texture. The per-game rule was resolved at the NIFAL
                // boundary; this only flags the channel for the shader, and
                // only when an actual texture is bound (a bare bit on index 0
                // would make the shader's "is a height map bound" test pass
                // and sample texture 0).
                //
                // #3562 — and only when that texture actually HAS an alpha
                // channel. `dds::format_has_alpha` is false for every
                // BC1/BC4/BC5 variant, and the sampler returns `A = 1.0` for
                // them by format. A constant height of 1.0 makes
                // `parallaxDisplaceUV`'s `currentDepth >= sampledHeight`
                // guard unreachable, so the marcher runs every step and
                // returns `uv - planarSlide` — the FULL slide (≈0.8 UV units
                // at grazing incidence), at every fragment, view-dependent
                // per frame. And `sampleUV` feeds every later fetch (base,
                // normal, detail, glow, gloss, dark, the eight terrain splat
                // layers), so the whole material swims rather than just the
                // height read. Mixed-block BC1 is worse than either extreme:
                // 3-colour blocks decode `A = 0` (instant break) while
                // 4-colour blocks decode `A = 1` (full slide), tearing the
                // surface along block boundaries.
                //
                // This is the same gate `NORMAL_ALPHA_SPEC_BIT` already uses
                // (`normal_alpha_spec_binding_applies`), which #3530's own
                // comments cite as the pattern being reused — the alpha
                // check was the half that didn't come across. It belongs
                // here rather than at the NIFAL boundary because the DDS
                // format is not known there; that is why `normal_has_alpha`
                // is a render-side `MaterialTextureHandles` field and not a
                // `Material` field.
                let mut parallax_map_index = texture_indices.height;
                if parallax_map_index != 0
                    && normal_has_alpha
                    && mat.is_some_and(|material| material.parallax_height_in_alpha)
                {
                    parallax_map_index |= crate::material_translate::PARALLAX_ALPHA_HEIGHT_BIT;
                }
                let env_map_index = texture_indices.environment;
                let env_mask_index = texture_indices.environment_mask;
                let greyscale_lut_index = texture_indices.greyscale_lut;
                // #3073 (NIFAL-D1) — `MaterialTextureHandles.parallax_*`
                // is itself already the canonical resolved value (from
                // `Material::parallax_height_scale`/`parallax_max_passes`,
                // #3073); this `.unwrap_or` only covers draws with no
                // `MaterialTextureHandles` at all (terrain / particles /
                // water, none of which read `parallax_map_index`), so it
                // shares the same named default rather than its own
                // independently-typed magic number.
                let parallax_height_scale = material_texture_handles
                    .map(|handles| handles.parallax_height_scale)
                    .unwrap_or(
                        byroredux_core::ecs::components::material::DEFAULT_PARALLAX_HEIGHT_SCALE,
                    );
                let parallax_max_passes = material_texture_handles
                    .map(|handles| handles.parallax_max_passes)
                    .unwrap_or(
                        byroredux_core::ecs::components::material::DEFAULT_PARALLAX_MAX_PASSES,
                    );

                // Terrain splat tile index (#470). Only LAND terrain
                // entities carry the component; statics pass `None`.
                let terrain_tile_index = terrain_tile_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|s| s.0);

                let (
                    // #1480 — roughness is the canonical resolve-once value
                    // (incl. the normal-alpha-as-spec derivation, now applied
                    // at spawn). The render path no longer mutates it.
                    roughness,
                    metalness,
                    ior,
                    emissive_mult,
                    emissive_color,
                    specular_strength,
                    specular_color,
                    diffuse_color,
                    ambient_color,
                    alpha_threshold,
                    alpha_test_func,
                ) = if let Some(m) = mat {
                    // Canonical PBR is resolved once at `translate_material`
                    // (`material.{metalness,roughness}`) — read it directly,
                    // no per-draw keyword scan / classify_pbr fallback.
                    let thresh = if m.alpha_test { m.alpha_threshold } else { 0.0 };
                    let func = if m.alpha_test {
                        m.alpha_test_func as u32
                    } else {
                        0
                    };
                    (
                        m.roughness,
                        m.metalness,
                        m.ior,
                        m.emissive_mult,
                        m.emissive_color,
                        m.specular_strength,
                        m.specular_color,
                        m.diffuse_color,
                        m.ambient_color,
                        thresh,
                        func,
                    )
                } else {
                    // No Material → identity tint, identity ambient.
                    (
                        0.5,
                        0.0,
                        byroredux_core::ecs::components::material::DEFAULT_DIELECTRIC_IOR,
                        0.0,
                        [0.0; 3],
                        1.0,
                        [1.0; 3],
                        [1.0; 3],
                        [1.0; 3],
                        0.0,
                        0u32,
                    )
                };

                // #398 — depth state from NiZBufferProperty (Material).
                // Defaults match the Gamebryo runtime defaults the
                // pre-#398 hardcoded pipeline state used: depth test+
                // write on, LESSEQUAL.
                let (z_test, mut z_write, z_function) = mat
                    .map(|m| (m.z_test, m.z_write, m.z_function))
                    .unwrap_or((true, true, 3));
                if is_decal_mesh && alpha_blend {
                    z_write = false;
                }

                // Geometry SSBO offsets for RT reflection UV lookups.
                let (v_off, i_off, v_count) = {
                    // SAFETY: mesh_registry is accessed immutably through the
                    // VulkanContext ref, not through the ECS.
                    // We can't access it here directly; pass zeros and let draw.rs fill from mesh_registry.
                    (0u32, 0u32, 0u32)
                };

                // Camera-space depth for draw order sorting. Transform
                // the model position through the VP matrix and use the
                // clip-space W (≈ linear depth) for sorting.
                let model_mat = transform.to_matrix();
                let pos = model_mat.col(3); // translation column
                let clip = vp_mat * pos;
                let sort_depth = f32_sortable_u32(clip.w);

                // Material kind for shader dispatch. The full glass /
                // effect-shader / variant classification happens at
                // spawn time:
                //   - BSLightingShaderProperty.shader_type values 0..=19
                //     are forwarded verbatim by the importer (#344).
                //   - Engine-synthesized kinds live at >= 100
                //     (MATERIAL_KIND_GLASS = 100, MATERIAL_KIND_EFFECT_SHADER
                //     = 101, …) and are set by `helpers::classify_glass_into_material`
                //     and the importer's BSEffectShader / NoLighting arms.
                //
                // #1280 sub-step 3c — the render-side glass-heuristic
                // chain that used to live here is gone. Audit pre-deletion
                // confirmed it was provably dead code: spawn-time
                // `classify_glass_into_material` is a strict superset of
                // the render-side gate (same `is_glass_keyword_path`
                // predicate, additional mesh-name + BGEM-glass triggers,
                // forces roughness to 0.10 so any future render-side
                // roughness gate would also fire), and a Material-creation-
                // site audit confirmed both spawn sites
                // (`cell_loader/spawn/mesh_instance.rs` and
                // `scene/nif_loader.rs`, each at their
                // `material_translate::translate_material` call) route
                // through the classifier before `world.insert`. #3465 —
                // named by symbol rather than line, per #1114: the cell-path
                // site moved out of `cell_loader/spawn.rs` under #2057 and
                // neither original line number resolved any more.
                // Pre-deletion the heuristic also required the texture-
                // path keyword and the same alpha/metal/!decal gates,
                // gated by `roughness < 0.4` — every entity that would
                // have hit those gates was already spawn-classified GLASS
                // with roughness 0.10, so the heuristic never changed
                // material_kind on any draw. Verified via the
                // translation-completeness harness (zero drift in
                // per-game m_kind% / mat_path% pre/post deletion).
                // Step 2 — normal-alpha-as-spec gloss-flag BINDING (Skyrim/
                // Gamebryo convention). When a lit Skyrim-era surface
                // (env_map_scale ~ 0 — the matte-default population) ships no
                // dedicated gloss map but its normal carries an alpha channel,
                // that alpha IS the per-pixel specular-intensity mask: point
                // the gloss slot at the normal with the high-bit "sample .a"
                // flag so black suppresses the specular/environment response
                // and white retains the authored intensity.
                //
                // The alpha-less high-specular fallback roughness is resolved
                // once at spawn; alpha-bearing normals leave canonical
                // roughness untouched. What stays here is only the per-draw
                // texture binding (transient, not canonical state), gated by
                // the SAME shared predicate the spawn write-back uses so the
                // two cannot diverge.
                //
                // #2445 (MAT-D3-03) — gated on `mat.is_some()`, which is what
                // makes the "cannot diverge" claim above actually true. The
                // shared predicate was necessary but not sufficient: the
                // spawn-side write-back early-returns on any entity with no
                // `Material`, while this side had no such guard and fed it
                // the no-`Material` fallback scalars — which still pass the
                // gate. So a `Material`-less draw bound the gloss slot here
                // with nothing having resolved the paired roughness at spawn.
                // MAT-D3-02 (#2444) removed the only population in that shape
                // (exterior terrain / LOD); this keeps the invariant enforced
                // structurally rather than by the absence of such a
                // population, so the next one to appear can't silently fall
                // into the gap.
                if crate::material_translate::normal_alpha_spec_binding_applies(
                    mat,
                    normal_has_alpha,
                    material_kind,
                    metalness,
                    normal_map_index,
                    gloss_map_index,
                ) {
                    gloss_map_index =
                        normal_map_index | crate::material_translate::NORMAL_ALPHA_SPEC_BIT;
                }

                // Glass single-sided override — Bethesda authors many
                // glass meshes (drinking glasses, pitchers, bottles)
                // with `TRIANGLE_FACING_CULL_DISABLE` so both inside
                // and outside walls render. With alpha blending and
                // no intra-mesh per-triangle depth sort, the back
                // walls composite over the front walls in arbitrary
                // mesh-vertex order, producing the visible "wireframe
                // through the glass" artifact on Prospector cups.
                //
                // The inter-mesh depth sort in `draw_sort_key` only
                // orders ENTIRE meshes back-to-front; per-triangle
                // ordering within one mesh would need OIT or per-
                // triangle CPU sort (impractical real-time). Effect-
                // shader (material_kind ≥ 100) and other two-sided
                // alpha — fire planes, foliage, banner cloth — keep
                // their authored two-sided behavior because they
                // typically aren't volumetric closed meshes.
                //
                // Trade-off: glass cups no longer render their
                // interior walls. For Bethesda content this is
                // fine — the alpha-blended exterior plus the IOR
                // refraction path in triangle.frag's glassIOR
                // branch already shows the scene through the cup.
                //
                // PERF-D2-02 / #2691 — this override is also what makes
                // `needs_two_sided_blend_split` (`vulkan::context::draw`)
                // structurally dead for engine-classified glass: it clears
                // `two_sided` before the `DrawCommand` exists, so that
                // predicate's `b.two_sided && order_dependent_glass` limb can
                // never be satisfied through the `MATERIAL_KIND_GLASS` arm of
                // `is_refractive_glass`. The two are independent mitigations
                // for the same #1804/#2237 artifact and only this one is live;
                // see that predicate's doc before treating the split as the
                // active fix. Keep the cross-reference in both directions —
                // the dormancy has been rediscovered empirically more than
                // once because neither site pointed at the other.
                let two_sided = if material_kind == byroredux_renderer::MATERIAL_KIND_GLASS {
                    false
                } else {
                    two_sided
                };

                // #562 / #619 — Skyrim+ BSLightingShaderProperty variant
                // payload. Each field group is gated on the matching
                // `material_kind` so the pack runs only for materials
                // whose shader branch reads it. Pre-#619 every chain
                // ran on every draw — wasted work on the vast majority
                // of materials (every non-Skyrim mesh + every Skyrim
                // static, ~99% of a typical cell). These slots live on
                // `GpuMaterial` (skin_tint_*/hair_tint_*/sparkle_*/eye_*/
                // multi_layer_*, R1 Phase 6), and `GpuMaterial::default`
                // already zeroes them so non-active kinds emit neutral
                // output identical to pre-fix.
                //
                // Variant ↔ field mapping (must mirror the
                // `materialKind == N` ladder in triangle.frag:769-796):
                //   5  SkinTint            → `skin_tint_*`        (live)
                //   6  HairTint            → `hair_tint_*`        (live)
                //   11 MultiLayerParallax  → `multi_layer_*`      (stub)
                //   14 SparkleSnow         → `sparkle_*`          (live)
                //   16 EyeEnvmap           → `eye_*`              (stub)
                //
                // Variants 11 + 16 are shader stubs today (#619); the
                // pack still runs on those kinds so the data is already
                // plumbed when the shader branches land.
                let stf = mat.and_then(|m| m.shader_type_fields.as_deref());
                let skin_tint_rgba = if material_kind == 5 {
                    stf.and_then(|f| {
                        f.skin_tint_color
                            .map(|c| [c[0], c[1], c[2], f.skin_tint_alpha.unwrap_or(1.0)])
                    })
                    .unwrap_or([0.0; 4])
                } else {
                    [0.0; 4]
                };
                // #2602 — the missing-colour fallback must be the identity
                // for the operation the shader performs, not a bare zero.
                // SkinTint's `mix(albedo, albedo * tint, skinTintA)` is
                // already inert at the all-zero default (alpha 0 selects the
                // untinted term), but HairTint's `albedo *= hairTint`
                // multiplies, so `[0,0,0]` renders pitch-black hair. `[1,1,1]`
                // is the multiplicative identity, so an unauthored
                // `hair_tint_color` passes the albedo through untouched.
                // Vanilla FO4/Skyrim hair always authors the field; this only
                // guards modded / future content that omits it.
                let hair_tint_rgb = if material_kind == 6 {
                    stf.and_then(|f| f.hair_tint_color).unwrap_or([1.0; 3])
                } else {
                    // Non-HairTint kinds never reach the shader's
                    // `materialKind == 6u` branch, so the slot stays zeroed
                    // exactly as `GpuMaterial::default` leaves it (`hair_tint_*`
                    // is a `GpuMaterial` field, not `GpuInstance`).
                    [0.0; 3]
                };
                let sparkle_rgba = if material_kind == 14 {
                    stf.and_then(|f| f.sparkle_parameters).unwrap_or([0.0; 4])
                } else {
                    [0.0; 4]
                };
                // The existing shared scalar is semantically an environment
                // reflection strength for both the ordinary Envmap variant
                // and MultiLayerParallax. Reuse it for the canonical field so
                // the authored `Material::env_map_scale` reaches the cubemap
                // shader without expanding the pinned GpuMaterial layout.
                let env_map_scale = mat.map(|m| m.env_map_scale).unwrap_or(0.0);
                let (
                    multi_layer_envmap_strength,
                    multi_layer_inner_thickness,
                    multi_layer_refraction_scale,
                    multi_layer_inner_scale,
                ) = if material_kind == 11 {
                    (
                        stf.and_then(|f| f.multi_layer_envmap_strength)
                            .unwrap_or(env_map_scale),
                        stf.and_then(|f| f.multi_layer_inner_thickness)
                            .unwrap_or(0.0),
                        stf.and_then(|f| f.multi_layer_refraction_scale)
                            .unwrap_or(0.0),
                        stf.and_then(|f| f.multi_layer_inner_layer_scale)
                            .unwrap_or([1.0, 1.0]),
                    )
                } else {
                    (env_map_scale, 0.0, 0.0, [1.0, 1.0])
                };
                let (eye_left_center, eye_cubemap_scale, eye_right_center) = if material_kind == 16
                {
                    (
                        stf.and_then(|f| f.eye_left_reflection_center)
                            .unwrap_or([0.0; 3]),
                        stf.and_then(|f| f.eye_cubemap_scale).unwrap_or(0.0),
                        stf.and_then(|f| f.eye_right_reflection_center)
                            .unwrap_or([0.0; 3]),
                    )
                } else {
                    ([0.0; 3], 0.0, [0.0; 3])
                };
                // #620 / SK-D4-01 — BSEffectShaderProperty falloff cone
                // pulled from `MaterialInfo.effect_shader` (Skyrim+
                // BSEffectShaderProperty path) or `no_lighting_falloff`
                // (FO3/FNV BSShaderNoLightingProperty SIBLING path,
                // #451). Both populate the same `[start_angle,
                // stop_angle, start_opacity, stop_opacity, soft_depth]`
                // tuple; the FO3/FNV path leaves `soft_depth = 0.0` since
                // BSShaderNoLightingProperty has no soft-depth field. The
                // fragment shader gates the read on `material_kind == 101`,
                // so non-effect materials emit the identity-pass-through
                // tuple `[1.0, 1.0, 1.0, 1.0, 0.0]` (no view-angle fade,
                // no soft-depth fade).
                let effect_falloff =
                    if material_kind == byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER {
                        mat.and_then(|m| m.effect_falloff)
                            .map(|f| {
                                [
                                    f.start_angle,
                                    f.stop_angle,
                                    f.start_opacity,
                                    f.stop_opacity,
                                    f.soft_falloff_depth,
                                ]
                            })
                            .unwrap_or([1.0, 1.0, 1.0, 1.0, 0.0])
                    } else {
                        [1.0, 1.0, 1.0, 1.0, 0.0]
                    };

                // #2697 — indexed by `supplemental_texture_slot::*` rather
                // than built positionally. A bare `[a, b, c, ...]` literal
                // has no compiler-enforced link to the constants
                // `DrawCommand::to_gpu_material` reads it back out
                // through (`crates/renderer/src/vulkan/context/mod.rs`):
                // inserting a role mid-list there would silently shift
                // every following slot by one with no compile error and
                // no failing test. Index assignment makes the two orders
                // the same source of truth.
                use byroredux_renderer::vulkan::material::supplemental_texture_slot as slot;
                let mut supplemental_texture_indices = [0u32; slot::COUNT];
                supplemental_texture_indices[slot::TINT] = texture_indices.tint;
                supplemental_texture_indices[slot::INNER_LAYER] = texture_indices.inner_layer;
                supplemental_texture_indices[slot::SPECULAR] = texture_indices.specular;
                supplemental_texture_indices[slot::LIGHTING] = texture_indices.lighting;
                supplemental_texture_indices[slot::FLOW] = texture_indices.flow;
                supplemental_texture_indices[slot::WRINKLE] = texture_indices.wrinkle;
                supplemental_texture_indices[slot::REFLECTANCE] = texture_indices.reflectance;
                supplemental_texture_indices[slot::EMITTANCE_GRADIENT] =
                    texture_indices.emittance_gradient;
                supplemental_texture_indices[slot::DECAL_0] = texture_indices.decals[0];
                supplemental_texture_indices[slot::DECAL_1] = texture_indices.decals[1];
                supplemental_texture_indices[slot::DECAL_2] = texture_indices.decals[2];
                supplemental_texture_indices[slot::DECAL_3] = texture_indices.decals[3];
                supplemental_texture_indices[slot::GLASS_ROUGHNESS_SCRATCH] =
                    texture_indices.glass_roughness_scratch;
                supplemental_texture_indices[slot::GLASS_DIRT_OVERLAY] =
                    texture_indices.glass_dirt_overlay;
                supplemental_texture_indices[slot::LIGHTING_MASK] = texture_indices.lighting_mask;
                supplemental_texture_indices[slot::BACK_LIGHTING] = texture_indices.back_lighting;
                let mut cmd = DrawCommand {
                    mesh_handle: mesh.0,
                    texture_handle: tex_handle,
                    model_matrix: model_mat.to_cols_array(),
                    alpha_blend,
                    src_blend,
                    dst_blend,
                    two_sided,
                    // #869 — NiWireframeProperty routes to the
                    // `vk::PolygonMode::LINE` pipeline variant. Falls
                    // back to FILL silently when the device lacks
                    // `fillModeNonSolid`.
                    wireframe: mat.map(|m| m.wireframe).unwrap_or(false),
                    // #869 — NiShadeProperty.flags==0: sets the
                    // `INSTANCE_FLAG_FLAT_SHADING` bit so the fragment
                    // shader uses the per-face derivative for normals.
                    flat_shading: mat.map(|m| m.flat_shading).unwrap_or(false),
                    is_decal,
                    // #renderlayer — final per-entity layer (already
                    // computed above as `render_layer_for_entity`,
                    // includes the spawn-time `Decal` escalation for
                    // alpha-tested overlays).
                    render_layer: render_layer_for_entity,
                    bone_offset,
                    normal_map_index,
                    dark_map_index,
                    glow_map_index,
                    detail_map_index,
                    gloss_map_index,
                    parallax_map_index,
                    parallax_height_scale,
                    parallax_max_passes,
                    env_map_index,
                    env_mask_index,
                    alpha_threshold,
                    alpha_test_func,
                    roughness,
                    metalness,
                    // Source-format-independent optical behavior. Generic
                    // dielectrics retain 1.5; FNV NIF glass and FO4 BGEM
                    // glass both arrive with the canonical 1.45 behavior.
                    // Texture handles above remain source-authored overlays.
                    ior,
                    glass_fresnel_color: mat.map(|m| m.glass_fresnel_color).unwrap_or([1.0; 3]),
                    glass_refraction_scale: mat.map(|m| m.glass_refraction_scale).unwrap_or(0.05),
                    glass_blur_scale: mat.map(|m| m.glass_blur_scale).unwrap_or(0.4),
                    glass_blur_scale_factor: mat.map(|m| m.glass_blur_scale_factor).unwrap_or(1.0),
                    lighting_effect_1: mat.map(|m| m.lighting_effect_1).unwrap_or(0.0),
                    lighting_effect_2: mat.map(|m| m.lighting_effect_2).unwrap_or(0.0),
                    subsurface_rolloff: mat.map(|m| m.subsurface_rolloff).unwrap_or(0.0),
                    rimlight_power: mat.map(|m| m.rimlight_power).unwrap_or(0.0),
                    backlight_power: mat.map(|m| m.backlight_power).unwrap_or(0.0),
                    fresnel_power: mat.map(|m| m.fresnel_power).unwrap_or(5.0),
                    grayscale_to_palette_scale: mat
                        .map(|m| m.grayscale_to_palette_scale)
                        .unwrap_or(1.0),
                    // #1249 — Disney diffuse defaults zero so the
                    // shader-side Lambert/Disney branch picks Lambert
                    // (every NIF without MAT_FLAG_BGSM_PBR). No source
                    // format authors these — reachable only via `mat.set`
                    // (Cornell harness). #2514 / REN-D21-2026-08-07-02.
                    subsurface: mat.map(|m| m.subsurface).unwrap_or(0.0),
                    sheen: mat.map(|m| m.sheen).unwrap_or(0.0),
                    sheen_tint: mat.map(|m| m.sheen_tint).unwrap_or(0.0),
                    // #1250 — isotropic GGX by default. Hair / brushed-metal
                    // anisotropy has no source-format equivalent either
                    // (BGSM authors no anisotropy metadata); `mat.set`-only.
                    anisotropic: mat.map(|m| m.anisotropic).unwrap_or(0.0),
                    emissive_mult,
                    // #2221 — `AnimatedEmissiveColor` etc. REPLACE the
                    // static `Material` value, same "controller fully
                    // owns the role" semantic `AnimatedUvTransform` (#525)
                    // established below. Each is driven independently
                    // (`NiMaterialColorController.target_color`), so a
                    // mesh with only an animated emissive keeps its
                    // static diffuse/ambient/specular untouched.
                    // #3246 / D7-01 — `quantize_fade` on the *animated*
                    // output only (never on the `unwrap_or` static
                    // fallback): entities of the same clip attaching on
                    // different streaming-spawn frames now collapse onto
                    // ≤32 `MaterialTable` slots instead of one per phase
                    // offset. See `super::quantize_fade`'s doc.
                    emissive_color: anim_emissive_q
                        .as_ref()
                        .and_then(|q| q.get(entity))
                        .map(|c| c.0.to_array().map(quantize_fade))
                        .unwrap_or(emissive_color),
                    specular_strength,
                    specular_color: anim_specular_q
                        .as_ref()
                        .and_then(|q| q.get(entity))
                        .map(|c| c.0.to_array().map(quantize_fade))
                        .unwrap_or(specular_color),
                    diffuse_color: anim_diffuse_q
                        .as_ref()
                        .and_then(|q| q.get(entity))
                        .map(|c| c.0.to_array().map(quantize_fade))
                        .unwrap_or(diffuse_color),
                    ambient_color: anim_ambient_q
                        .as_ref()
                        .and_then(|q| q.get(entity))
                        .map(|c| c.0.to_array().map(quantize_fade))
                        .unwrap_or(ambient_color),
                    vertex_offset: v_off,
                    index_offset: i_off,
                    vertex_count: v_count,
                    sort_depth,
                    in_tlas,
                    in_raster,
                    entity_id: entity,
                    // #492 — UV transform + material alpha pulled from
                    // the `Material` component (already populated by
                    // the NIF importer and/or the FO4 BGSM resolver).
                    // Identity defaults when the mesh has no Material.
                    //
                    // #525 — `AnimatedUvTransform`, when present, REPLACES
                    // the static authored values entirely (rather than
                    // adds / multiplies). The component starts at
                    // identity (0, 0) / (1, 1) on insertion and the
                    // animation system writes per-channel slots over
                    // time; the static `Material` values are the
                    // baseline only for entities WITHOUT a controller.
                    // This matches `NiTextureTransformController`'s
                    // legacy semantic — the controller authored over
                    // the material's UV transform, not on top of it.
                    uv_offset: anim_uv_q
                        .as_ref()
                        .and_then(|q| q.get(entity))
                        .map(|t| [t.offset.x, t.offset.y])
                        .or_else(|| mat.map(|m| m.uv_offset))
                        .unwrap_or([0.0, 0.0]),
                    uv_scale: anim_uv_q
                        .as_ref()
                        .and_then(|q| q.get(entity))
                        .map(|t| [t.scale.x, t.scale.y])
                        .or_else(|| mat.map(|m| m.uv_scale))
                        .unwrap_or([1.0, 1.0]),
                    // #2221 — `AnimatedAlpha` REPLACES the static
                    // `Material.alpha`, same semantic as the color sinks
                    // above (`NiAlphaController` fully owns the role
                    // while active — fades, pulsing FX, VATS highlight).
                    // #3246 / D7-01 — quantized like the color sinks
                    // above; the static `mat.alpha` fallback stays at
                    // full precision.
                    material_alpha: anim_alpha_q
                        .as_ref()
                        .and_then(|q| q.get(entity))
                        .map(|a| quantize_fade(a.0))
                        .or_else(|| mat.map(|m| m.alpha))
                        .unwrap_or(1.0),
                    // #2221 — animated BSShaderProperty color/scalar.
                    // Forwarded to `GpuMaterial.shader_color_*` /
                    // `.shader_float`, both currently unsampled by any
                    // shader (see their doc in
                    // `crates/renderer/src/vulkan/material.rs` for why —
                    // the sink is deliberately generic with no single
                    // settled shader-uniform target yet). Absent when no
                    // `BSLightingShaderPropertyColorController` /
                    // `BSEffectShaderProperty*Controller` targets this
                    // entity.
                    //
                    // #3246 / D7-01 — quantized like the other animated
                    // sinks (see `super::quantize_fade`'s doc).
                    shader_color: anim_shader_color_q
                        .as_ref()
                        .and_then(|q| q.get(entity))
                        .map(|c| c.0.to_array().map(quantize_fade))
                        .unwrap_or([0.0; 3]),
                    shader_float: anim_shader_float_q
                        .as_ref()
                        .and_then(|q| q.get(entity))
                        .map(|f| quantize_fade(f.0))
                        .unwrap_or(0.0),
                    // Material tint for the one-bounce GI bounce colour
                    // (read at the ray hit as `hitInst.avgAlbedo`). Carries
                    // the material's diffuse_color: exact for untextured /
                    // vertex-coloured surfaces (Cornell walls bounce red /
                    // green). For textured content the renderer multiplies
                    // this tint by the diffuse texture's cached texel-mean
                    // when it builds the GpuInstance (#1628, draw.rs), so the
                    // bounce carries the true surface mean, not just the tint.
                    // Without this the hardcoded 0.5 grey made colour-bleeding
                    // impossible.
                    avg_albedo: mat.map(|m| m.diffuse_color).unwrap_or([0.5, 0.5, 0.5]),
                    z_test,
                    z_write,
                    z_function,
                    material_kind,
                    terrain_tile_index,
                    skin_tint_rgba,
                    hair_tint_rgb,
                    multi_layer_envmap_strength,
                    eye_left_center,
                    eye_cubemap_scale,
                    eye_right_center,
                    multi_layer_inner_thickness,
                    multi_layer_refraction_scale,
                    multi_layer_inner_scale,
                    sparkle_rgba,
                    effect_falloff,
                    material_id: 0,
                    // O4-03 / #695 — `NiVertexColorProperty.vertex_mode
                    // == SOURCE_EMISSIVE` (encoded as `1` per
                    // `Material::vertex_color_mode`). Routes the
                    // per-vertex `fragColor` payload to the fragment
                    // shader's emissive accumulator instead of the
                    // albedo modulation. False on every mesh without a
                    // Material component (defaults to AmbientDiffuse) or
                    // when the property explicitly disables vertex
                    // colors (`Ignore`).
                    vertex_color_emissive: mat.is_some_and(|m| m.vertex_color_mode == 1),
                    // #890 Stage 2 — packed BSEffect flag bits captured
                    // at importer ingestion (see
                    // `cell_loader::pack_effect_shader_flags`). Layout
                    // matches `GpuMaterial::material_flags` so
                    // `to_gpu_material` ORs the word straight in.
                    effect_shader_flags: mat.map(|m| m.effect_shader_flags).unwrap_or(0),
                    greyscale_lut_index,
                    supplemental_texture_indices,
                    // #1147 Phase 2b — BGSM v>=8 translucency suite.
                    // Gated at the shader by `MAT_FLAG_BGSM_TRANSLUCENCY`
                    // (packed via `pack_imported_material_flags`).
                    translucency_subsurface_color: mat
                        .map(|m| m.translucency_subsurface_color)
                        .unwrap_or([0.0; 3]),
                    translucency_transmissive_scale: mat
                        .map(|m| m.translucency_transmissive_scale)
                        .unwrap_or(0.0),
                    translucency_turbulence: mat.map(|m| m.translucency_turbulence).unwrap_or(0.0),
                    is_water: false,
                };
                // #781 / PERF-N4 — `intern_by_hash` skips the
                // `to_gpu_material()` 432-byte construction on the
                // dedup-hit path (~97% of calls on Prospector).
                cmd.material_id =
                    material_table.intern_by_hash(cmd.material_hash(), || cmd.to_gpu_material());
                draw_commands.push(cmd);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::COLOR_FADE_STEPS;
    use super::*;

    #[test]
    fn lod_shadow_reach_includes_intersecting_sphere() {
        let reach = byroredux_renderer::shader_constants::LOD_SHADOW_CASTER_DISTANCE;
        assert!(lod_shadow_caster_in_range(
            Vec3::new(reach + 99.0, 0.0, 0.0),
            100.0,
            Vec3::ZERO,
        ));
        assert!(!lod_shadow_caster_in_range(
            Vec3::new(reach + 101.0, 0.0, 0.0),
            100.0,
            Vec3::ZERO,
        ));
    }

    use byroredux_core::ecs::{GlobalTransform, World};
    use byroredux_core::math::Quat;
    use byroredux_renderer::MaterialTable;

    fn spawn_mesh_entity(world: &mut World) -> EntityId {
        let entity = world.spawn();
        world.insert(
            entity,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert(entity, MeshHandle(1));
        entity
    }

    /// #2221 end-to-end: an animated alpha/color sink must REPLACE the
    /// static `Material` value all the way through to the interned
    /// `GpuMaterial` — not just land in the `DrawCommand` intermediate.
    /// Exercises the full `collect_static_mesh_draws` → `intern_by_hash`
    /// → `to_gpu_material` chain, matching the issue's "apply animated
    /// material values before GpuMaterial interning" ask.
    #[test]
    fn animated_alpha_and_diffuse_override_the_static_material_through_interning() {
        let mut world = World::new();
        let entity = spawn_mesh_entity(&mut world);
        world.insert(
            entity,
            Material {
                alpha: 1.0,
                diffuse_color: [1.0, 1.0, 1.0],
                ..Default::default()
            },
        );
        world.insert(entity, AnimatedAlpha(0.3));
        world.insert(entity, AnimatedDiffuseColor(Vec3::new(0.2, 0.4, 0.6)));

        let frustum = FrustumPlanes::from_view_proj(Mat4::IDENTITY);
        let mut draw_commands = Vec::new();
        let mut material_table = MaterialTable::new();
        collect_static_mesh_draws(
            &world,
            &frustum,
            Mat4::IDENTITY,
            Vec3::ZERO,
            &FxHashMap::default(),
            &mut draw_commands,
            &mut material_table,
        );

        assert_eq!(
            draw_commands.len(),
            1,
            "the mesh entity must produce a draw"
        );
        // #3246 / D7-01 — animated sinks now go through `quantize_fade`
        // before landing on the DrawCommand, so the expected values below
        // route through the same function rather than restating its
        // rounding by hand.
        let cmd = &draw_commands[0];
        assert_eq!(
            cmd.material_alpha,
            quantize_fade(0.3),
            "AnimatedAlpha must override the static Material.alpha (1.0) on the DrawCommand"
        );
        assert_eq!(
            cmd.diffuse_color,
            [0.2, 0.4, 0.6].map(quantize_fade),
            "AnimatedDiffuseColor must override the static Material.diffuse_color on the DrawCommand"
        );

        let gpu_mat = &material_table.materials()[cmd.material_id as usize];
        assert_eq!(
            gpu_mat.material_alpha,
            quantize_fade(0.3),
            "the animated alpha must survive to_gpu_material interning, not just the DrawCommand"
        );
        assert_eq!(
            [gpu_mat.diffuse_r, gpu_mat.diffuse_g, gpu_mat.diffuse_b],
            [0.2, 0.4, 0.6].map(quantize_fade),
            "the animated diffuse color must survive to_gpu_material interning"
        );
    }

    /// A mesh with NO animated sinks must read the static `Material`
    /// values unchanged — the override is additive, not a behavior
    /// change for the common (unanimated) case.
    #[test]
    fn no_animated_sinks_falls_back_to_the_static_material() {
        let mut world = World::new();
        let entity = spawn_mesh_entity(&mut world);
        world.insert(
            entity,
            Material {
                alpha: 0.75,
                diffuse_color: [0.9, 0.8, 0.7],
                ..Default::default()
            },
        );

        let frustum = FrustumPlanes::from_view_proj(Mat4::IDENTITY);
        let mut draw_commands = Vec::new();
        let mut material_table = MaterialTable::new();
        collect_static_mesh_draws(
            &world,
            &frustum,
            Mat4::IDENTITY,
            Vec3::ZERO,
            &FxHashMap::default(),
            &mut draw_commands,
            &mut material_table,
        );

        assert_eq!(draw_commands.len(), 1);
        assert_eq!(draw_commands[0].material_alpha, 0.75);
        assert_eq!(draw_commands[0].diffuse_color, [0.9, 0.8, 0.7]);
    }

    /// #2221 — `AnimatedShaderColor` / `AnimatedShaderFloat` must reach
    /// `DrawCommand` and survive interning too, even though no shader
    /// samples them yet (see `GpuMaterial::shader_color_r`'s doc).
    #[test]
    fn animated_shader_color_and_float_reach_the_draw_command() {
        let mut world = World::new();
        let entity = spawn_mesh_entity(&mut world);
        world.insert(entity, AnimatedShaderColor(Vec3::new(0.1, 0.2, 0.3)));
        world.insert(entity, AnimatedShaderFloat(0.42));

        let frustum = FrustumPlanes::from_view_proj(Mat4::IDENTITY);
        let mut draw_commands = Vec::new();
        let mut material_table = MaterialTable::new();
        collect_static_mesh_draws(
            &world,
            &frustum,
            Mat4::IDENTITY,
            Vec3::ZERO,
            &FxHashMap::default(),
            &mut draw_commands,
            &mut material_table,
        );

        assert_eq!(draw_commands.len(), 1);
        // #3246 / D7-01 — quantized before landing on the DrawCommand.
        assert_eq!(
            draw_commands[0].shader_color,
            [0.1, 0.2, 0.3].map(quantize_fade)
        );
        assert_eq!(draw_commands[0].shader_float, quantize_fade(0.42));

        let gpu_mat = &material_table.materials()[draw_commands[0].material_id as usize];
        assert_eq!(
            [
                gpu_mat.shader_color_r,
                gpu_mat.shader_color_g,
                gpu_mat.shader_color_b
            ],
            [0.1, 0.2, 0.3].map(quantize_fade)
        );
        assert_eq!(gpu_mat.shader_float, quantize_fade(0.42));
    }

    /// #2221 — an active base-color flipbook (`AnimatedTextureFlip` slot
    /// 0) must REPLACE the spawn-time-resolved `TextureHandle`, same as
    /// every other animated-sink override in this loop.
    #[test]
    fn animated_texture_flip_overrides_the_texture_handle() {
        use byroredux_core::ecs::TextureFlipEntry;

        let mut world = World::new();
        let entity = spawn_mesh_entity(&mut world);
        world.insert(entity, TextureHandle(1));
        world.insert(
            entity,
            AnimatedTextureFlip(vec![TextureFlipEntry {
                texture_slot: 0,
                handles: vec![10, 20, 30],
                current_index: 1,
            }]),
        );

        let frustum = FrustumPlanes::from_view_proj(Mat4::IDENTITY);
        let mut draw_commands = Vec::new();
        let mut material_table = MaterialTable::new();
        collect_static_mesh_draws(
            &world,
            &frustum,
            Mat4::IDENTITY,
            Vec3::ZERO,
            &FxHashMap::default(),
            &mut draw_commands,
            &mut material_table,
        );

        assert_eq!(draw_commands.len(), 1);
        assert_eq!(
            draw_commands[0].texture_handle, 20,
            "the flipbook's current_index=1 handle (20) must win over the \
             spawn-time TextureHandle (1)"
        );
    }

    /// No `AnimatedTextureFlip` on the entity: the spawn-time
    /// `TextureHandle` must ride through unchanged.
    #[test]
    fn no_texture_flip_falls_back_to_the_static_texture_handle() {
        let mut world = World::new();
        let entity = spawn_mesh_entity(&mut world);
        world.insert(entity, TextureHandle(7));

        let frustum = FrustumPlanes::from_view_proj(Mat4::IDENTITY);
        let mut draw_commands = Vec::new();
        let mut material_table = MaterialTable::new();
        collect_static_mesh_draws(
            &world,
            &frustum,
            Mat4::IDENTITY,
            Vec3::ZERO,
            &FxHashMap::default(),
            &mut draw_commands,
            &mut material_table,
        );

        assert_eq!(draw_commands.len(), 1);
        assert_eq!(draw_commands[0].texture_handle, 7);
    }

    /// #3246 / D7-01 regression: a population of entities sharing one
    /// animated-alpha clip but attaching on different streaming-spawn
    /// frames (phase-jittered, not synchronized) must collapse onto a
    /// bounded `MaterialTable` slot count — one `MaterialTable` slot per
    /// live phase offset, not one per entity, is exactly the cardinality
    /// blowup this fix closes. Pre-fix, every distinct raw `f32` phase
    /// hashed to its own slot, so this test would have interned as many
    /// materials as entities (200); post-fix it's bounded by
    /// `COLOR_FADE_STEPS + 1` regardless of population size.
    #[test]
    fn phase_jittered_animated_alpha_population_dedups_to_bounded_material_count() {
        let mut world = World::new();
        const N: usize = 200;
        for i in 0..N {
            let entity = spawn_mesh_entity(&mut world);
            // Continuous phase spread across the full 0.0..=1.0 fade
            // domain — the "props spawn as the player approaches, not
            // all at once" shape the issue describes.
            let phase = i as f32 / N as f32;
            world.insert(entity, AnimatedAlpha(phase));
        }

        let frustum = FrustumPlanes::from_view_proj(Mat4::IDENTITY);
        let mut draw_commands = Vec::new();
        let mut material_table = MaterialTable::new();
        collect_static_mesh_draws(
            &world,
            &frustum,
            Mat4::IDENTITY,
            Vec3::ZERO,
            &FxHashMap::default(),
            &mut draw_commands,
            &mut material_table,
        );

        assert_eq!(draw_commands.len(), N);
        // `quantize_fade` bounds distinct alpha values to
        // `COLOR_FADE_STEPS + 1` (0..=COLOR_FADE_STEPS inclusive); +1
        // more for `MaterialTable::new()`'s seeded neutral-default slot
        // 0, which every fresh table carries regardless of population.
        let bound = COLOR_FADE_STEPS as usize + 1 + 1;
        assert!(
            material_table.len() <= bound,
            "expected at most {bound} unique materials for {N} phase-jittered \
             AnimatedAlpha entities (quantize_fade bound + neutral-default \
             slot), got {}",
            material_table.len()
        );
    }
}
