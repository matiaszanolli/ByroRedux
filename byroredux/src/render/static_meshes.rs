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

use std::collections::HashMap;

use byroredux_core::ecs::{
    AnimatedUvTransform, AnimatedVisibility, EntityId, GlobalTransform, Material, MeshHandle,
    RenderLayer, TextureHandle, World, WorldBound,
};
use byroredux_core::math::Mat4;
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::MaterialTable;

use crate::components::{
    AlphaBlend, DarkMapHandle, ExtraTextureMaps, GreyscaleLutHandle, IsFxMesh, NormalMapHandle,
    TerrainTileSlot, TwoSided,
};

use super::camera::FrustumPlanes;
use super::f32_sortable_u32;

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
    skin_offsets: &HashMap<EntityId, u32>,
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
    // #renderlayer — per-entity content-class for the depth-bias
    // ladder (Architecture / Clutter / Actor / Decal). Attached at
    // cell-load time from the REFR's base-record `RecordType` (see
    // `RecordType::render_layer`). Absent component falls back to
    // `Architecture` (zero bias) — identical to pre-fix behaviour.
    // The Decal escalation (`mesh.is_decal || alpha_test_func != 0`)
    // is applied at spawn time, not here, so this query reads the
    // final per-entity layer directly.
    let render_layer_q = world.query::<RenderLayer>();
    let nmap_q = world.query::<NormalMapHandle>();
    let dmap_q = world.query::<DarkMapHandle>();
    let extra_q = world.query::<ExtraTextureMaps>();
    // #890 Stage 2c — bindless handle for `BSEffectShaderProperty`
    // greyscale palette LUT. Sparse: ~50–200 entities per Skyrim cell
    // (fire effects, magic VFX, alchemy steam) vs ~5K total meshes.
    let lut_q = world.query::<GreyscaleLutHandle>();
    let terrain_tile_q = world.query::<TerrainTileSlot>();
    let wb_q = world.query::<WorldBound>();
    // PERF-D3-NEW-02 / #1136 — query once instead of 6 substring scans
    // per draw per frame. Entities tagged at spawn by `cell_loader::spawn`
    // + `scene::nif_loader` when the texture path matches an FX needle.
    let fx_q = world.query::<IsFxMesh>();
    if let (Some(tq), Some(mq)) = (tq, mq) {
        for (entity, mesh) in mq.iter() {
            // Skip entities hidden by animation.
            let visible = vis_q
                .as_ref()
                .and_then(|q| q.get(entity))
                .map(|v| v.0)
                .unwrap_or(true);
            if !visible {
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
            let in_raster = match wb_q.as_ref().and_then(|q| q.get(entity)) {
                Some(wb) if wb.radius > 0.0 => frustum.contains_sphere(wb.center, wb.radius),
                _ => true,
            };

            if let Some(transform) = tq.get(entity) {
                let tex_handle = tex_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|t| t.0)
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
                let normal_map_index = nmap_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|n| n.0)
                    .unwrap_or(0);
                let dark_map_index = dmap_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|d| d.0)
                    .unwrap_or(0);
                // #890 Stage 2c — only non-zero on entities the
                // importer flagged with a `BSEffectShaderProperty
                // .greyscale_texture` AND the path resolved to a real
                // bindless handle. The fragment shader gates the LUT
                // sample on `handle != 0u`, so the default-zero
                // fall-through is the "no LUT, sample source texture
                // raw" path.
                let greyscale_lut_index = lut_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|h| h.0)
                    .unwrap_or(0);
                // #399 — three NiTexturingProperty extra slots packed in
                // one component to keep the per-frame query count fixed.
                // Default to 0 (= no map; fragment shader falls through
                // to the inline material constants) for entities that
                // never had `ExtraTextureMaps` attached at cell load.
                let (
                    glow_map_index,
                    detail_map_index,
                    gloss_map_index,
                    parallax_map_index,
                    env_map_index,
                    env_mask_index,
                    parallax_height_scale,
                    parallax_max_passes,
                ) = extra_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|e| {
                        (
                            e.glow,
                            e.detail,
                            e.gloss,
                            e.parallax,
                            e.env,
                            e.env_mask,
                            e.parallax_height_scale,
                            e.parallax_max_passes,
                        )
                    })
                    .unwrap_or((0, 0, 0, 0, 0, 0, 0.04, 4.0));

                // Terrain splat tile index (#470). Only LAND terrain
                // entities carry the component; statics pass `None`.
                let terrain_tile_index = terrain_tile_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|s| s.0);

                // Material data + PBR classification.
                let mat = mat_q.as_ref().and_then(|q| q.get(entity));

                // Skip Gamebryo effect meshes (crossed glow quads, god rays).
                // These are sprite-billboard fakes for bloom halos — in a RT
                // renderer the actual point light already provides illumination
                // and these quads just render as blown-out white surfaces.
                // FX-decoration skip — PERF-D3-NEW-02 / #1136. The
                // classification (texture-path substring scan over 6
                // needles) is precomputed at spawn time and stored as
                // an `IsFxMesh` marker so this hot path is one
                // component-lookup instead of 6 byte-windowed substring
                // scans per draw per frame.
                if fx_q.as_ref().is_some_and(|q| q.get(entity).is_some()) {
                    continue;
                }

                let (
                    roughness,
                    metalness,
                    emissive_mult,
                    emissive_color,
                    specular_strength,
                    specular_color,
                    diffuse_color,
                    ambient_color,
                    alpha_threshold,
                    alpha_test_func,
                ) = if let Some(m) = mat {
                    let pbr = m.classify_pbr(m.texture_path.as_deref());
                    let thresh = if m.alpha_test { m.alpha_threshold } else { 0.0 };
                    let func = if m.alpha_test {
                        m.alpha_test_func as u32
                    } else {
                        0
                    };
                    (
                        pbr.roughness,
                        pbr.metalness,
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
                        0.5, 0.0, 0.0, [0.0; 3], 1.0, [1.0; 3], [1.0; 3], [1.0; 3], 0.0, 0u32,
                    )
                };

                // #398 — depth state from NiZBufferProperty (Material).
                // Defaults match the Gamebryo runtime defaults the
                // pre-#398 hardcoded pipeline state used: depth test+
                // write on, LESSEQUAL.
                let (z_test, z_write, z_function) = mat
                    .map(|m| (m.z_test, m.z_write, m.z_function))
                    .unwrap_or((true, true, 3));

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

                // Classify the effective material kind for shader
                // dispatch. BSLightingShaderProperty.shader_type is
                // forwarded verbatim for Skyrim+ variants (0..=19 —
                // SkinTint / HairTint / EyeEnvmap / MultiLayerParallax /
                // etc., see #344). Engine-synthesized kinds live in
                // the high range (100+):
                //   - `MATERIAL_KIND_GLASS` when the material is
                //     alpha-blend + low-metal + not a decal + low-
                //     roughness. The roughness gate was added after
                //     Tier C Phase 2 shipped with only the first three
                //     criteria: FNV wood tables and picture frames
                //     carry `NiAlphaProperty.flags=0x12ED` (blend=1) for
                //     edge smoothing, not because the surface is glass.
                //     Under the three-criterion rule they were tagged
                //     as glass and rendered through the RT refract /
                //     Fresnel path as near-white surfaces. Roughness
                //     classified from the texture path (glass=0.1,
                //     wood=0.7, fabric=0.95) cleanly separates actual
                //     transparent-refractive materials from
                //     alpha-blend-for-edges. See follow-up to #515.
                let base_material_kind = mat.map(|m| m.material_kind).unwrap_or(0);
                // Engine-synthesized kinds (>= 100) are pre-classified
                // upstream and must win over the heuristic Glass branch.
                // Today: BSEffectShaderProperty meshes arrive with
                // material_kind=101 (MATERIAL_KIND_EFFECT_SHADER) set at
                // import; the glass heuristic would otherwise misclassify
                // a fire plane as glass. See #706.
                //
                // Heuristic glass classification requires an EXPLICIT
                // texture-path glass-keyword signal alongside the
                // alpha/metal/roughness gates. Pre-fix any alpha-blend
                // material whose glossiness-derived roughness happened
                // to land below 0.4 was classified as glass — that
                // included Skyrim cloth banners (Markarth heraldic
                // hangings have `BSLightingShaderProperty.glossiness ≈ 80`
                // → roughness 0.2 via `1 - 80/100` — the cloth-keyword
                // arm of `classify_pbr` didn't fire because the texture
                // path was `architecture/markarth/markarthbanner01.dds`,
                // not `cloth/banner01.dds`). The misclassification
                // routed the cloth through the IOR refraction +
                // chromatic-dispersion shader path, producing visible
                // rainbow banners. Requiring the path-keyword signal
                // (glass / crystal / ice / gem / window / bottle / jar
                // / vial) is conservative: meshes without one of those
                // tokens never reach the glass renderer regardless of
                // their PBR fallback, eliminating the cloth-as-glass
                // false-positive without losing actual glass cups /
                // bottles. See Markarth probe 2026-05-13.
                let path_indicates_glass = mat
                    .and_then(|m| m.texture_path.as_deref())
                    .map(|p| {
                        byroredux_core::ecs::components::Material::path_indicates_glass(Some(p))
                    })
                    .unwrap_or(false);
                let material_kind = if base_material_kind >= 100 {
                    base_material_kind
                } else if alpha_blend
                    && !is_decal
                    && metalness < 0.3
                    && roughness < 0.4
                    && path_indicates_glass
                {
                    byroredux_renderer::MATERIAL_KIND_GLASS
                } else {
                    base_material_kind
                };

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
                // static, ~99% of a typical cell). `GpuInstance::default`
                // already zeroes the slots so non-active kinds emit
                // neutral output identical to pre-fix.
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
                let skin_tint_rgba = if base_material_kind == 5 {
                    stf.and_then(|f| {
                        f.skin_tint_color
                            .map(|c| [c[0], c[1], c[2], f.skin_tint_alpha.unwrap_or(1.0)])
                    })
                    .unwrap_or([0.0; 4])
                } else {
                    [0.0; 4]
                };
                let hair_tint_rgb = if base_material_kind == 6 {
                    stf.and_then(|f| f.hair_tint_color).unwrap_or([0.0; 3])
                } else {
                    [0.0; 3]
                };
                let sparkle_rgba = if base_material_kind == 14 {
                    stf.and_then(|f| f.sparkle_parameters).unwrap_or([0.0; 4])
                } else {
                    [0.0; 4]
                };
                let (
                    multi_layer_envmap_strength,
                    multi_layer_inner_thickness,
                    multi_layer_refraction_scale,
                    multi_layer_inner_scale,
                ) = if base_material_kind == 11 {
                    (
                        stf.and_then(|f| f.multi_layer_envmap_strength)
                            .unwrap_or(0.0),
                        stf.and_then(|f| f.multi_layer_inner_thickness)
                            .unwrap_or(0.0),
                        stf.and_then(|f| f.multi_layer_refraction_scale)
                            .unwrap_or(0.0),
                        stf.and_then(|f| f.multi_layer_inner_layer_scale)
                            .unwrap_or([1.0, 1.0]),
                    )
                } else {
                    (0.0, 0.0, 0.0, [1.0, 1.0])
                };
                let (eye_left_center, eye_cubemap_scale, eye_right_center) =
                    if base_material_kind == 16 {
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
                    // #1248 — Material struct has no IOR field today
                    // (NIF/BGSM v<9 doesn't author one), so every
                    // static mesh uses the GpuMaterial default 1.5.
                    // Mirrors the pre-#1248 hardcoded vec3(0.04)
                    // dielectric F0 byte-for-byte. Plumbing for
                    // BGSM v9+ / Starfield .mat IOR routes through
                    // Material → here when the importer surfaces it.
                    ior: 1.5,
                    // #1249 — Disney diffuse defaults zero so the
                    // shader-side Lambert/Disney branch picks Lambert
                    // (every NIF without MAT_FLAG_BGSM_PBR). BGSM v9+
                    // sheen / subsurface fields can be plumbed through
                    // Material when the importer surfaces them.
                    subsurface: 0.0,
                    sheen: 0.0,
                    sheen_tint: 0.0,
                    // #1250 — isotropic GGX. Hair / brushed-metal
                    // anisotropy is BGSM-only metadata not yet
                    // authored on the legacy NIF path.
                    anisotropic: 0.0,
                    emissive_mult,
                    emissive_color,
                    specular_strength,
                    specular_color,
                    diffuse_color,
                    ambient_color,
                    vertex_offset: v_off,
                    index_offset: i_off,
                    vertex_count: v_count,
                    sort_depth,
                    in_tlas: true,
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
                    material_alpha: mat.map(|m| m.alpha).unwrap_or(1.0),
                    // Average albedo for fast GI bounce approximation.
                    // Falls back to mid-gray (0.5) when no texture color
                    // data is available. A proper implementation would
                    // downsample the texture to 1×1 during asset load;
                    // for now we derive a heuristic from the material.
                    avg_albedo: [0.5, 0.5, 0.5],
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
                    // #1147 Phase 2b — BGSM v>=8 translucency suite.
                    // Gated at the shader by `MAT_FLAG_BGSM_TRANSLUCENCY`
                    // (packed via `pack_bgsm_material_flags`).
                    translucency_subsurface_color: mat
                        .map(|m| m.translucency_subsurface_color)
                        .unwrap_or([0.0; 3]),
                    translucency_transmissive_scale: mat
                        .map(|m| m.translucency_transmissive_scale)
                        .unwrap_or(0.0),
                    translucency_turbulence: mat
                        .map(|m| m.translucency_turbulence)
                        .unwrap_or(0.0),
                    is_water: false,
                };
                // #781 / PERF-N4 — `intern_by_hash` skips the
                // `to_gpu_material()` 260-byte construction on the
                // dedup-hit path (~97% of calls on Prospector).
                cmd.material_id =
                    material_table.intern_by_hash(cmd.material_hash(), || cmd.to_gpu_material());
                draw_commands.push(cmd);
            }
        }
    }
}
