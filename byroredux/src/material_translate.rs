//! NIFAL (NIF Abstraction Layer) — the **material** translation boundary.
//!
//! [`translate_material`] is the **single base-material** site that turns a raw,
//! per-game [`ImportedMaterial`] (with BGSM/BGEM already merged into it by
//! [`crate::asset_provider`]'s `merge_external_material`) into the engine's
//! canonical [`Material`] ECS component. Every consumer downstream of
//! `Material` reads game-agnostic, fully-resolved data — the per-game
//! quirks are resolved here, exactly once. This is the material slice of
//! NIFAL, the engine's cross-game canonical translation tier.
//!
//! Before this module existed, the `Material` struct literal was built
//! verbatim at two sites — [`crate::cell_loader`]'s `spawn` (REFR cell
//! placement) and [`crate::scene`]'s `nif_loader` (loose-NIF load) —
//! ~110 near-identical lines each, kept in sync by hand. That
//! duplication was itself a translation leak: a field added to one site
//! and not the other silently diverged the two load paths. Both sites
//! now call this boundary. Mesh water then passes through the second, explicit
//! canonical producer [`attach_mesh_water`], which translates that resolved
//! [`Material`] plus placement/bounds into [`WaterPlane`] / [`WaterFlow`] /
//! [`WaterVolume`]. ESM WATR water uses `env_translate::resolve_water_material`;
//! the two producers deliberately share [`WaterKind`] classification and foam
//! semantics rather than pretending unlike source formats are one record.
//!
//! Architecture: see `docs/engine/nifal.md`. The canonical tier is the
//! ECS `Material` component itself (it already lives in `byroredux_core`,
//! is game-agnostic, and is what the renderer reads) — this boundary is
//! the `translate()` step, not a new type.

use crate::components::MaterialTextureHandles;
use byroredux_core::ecs::components::material::{EffectFalloff, Material};
use byroredux_core::ecs::components::water::{
    WaterFlow, WaterKind, WaterMaterial, WaterPlane, WaterVolume,
};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_nif::import::{ImportedMaterial, MaterialTextureSet};

/// `GpuMaterial.ior` is a discriminated optical scalar. For ordinary
/// materials it remains the canonical dielectric index of refraction. A
/// fire-refraction proxy instead stores the authored heat-haze distortion
/// strength there; the material kind makes the two meanings unambiguous
/// without adding another field to the hot 348-byte GPU material record.
///
/// **SKY-D7-02 / #2327 — the non-`FIRE_REFRACTION` discard below is
/// deliberate, not an oversight.** `refraction_strength` (`BSLightingShaderProperty`'s
/// SLSF1 `Refraction`-gated scalar, captured for every Skyrim+ material at
/// `dedicated_shader.rs`'s `apply_bs_lighting_shader` — shared code, so
/// this covers FO4/FO76/Starfield identically, not just Skyrim) is NOT a
/// physical index of refraction: nif.xml's own field spec for "Refraction
/// Strength" states verbatim "**Not based on physically accurate
/// refractive index**" (range 0-1, "the amount of distortion"). Reusing
/// its authored value as `ior` for an ordinary dielectric — a real 1.0+
/// physical IOR the RT refraction path traces against — would be a
/// category error, not a fix: a surface authored with e.g.
/// `refraction_strength = 0.3` would get `ior = 0.3`, physically nonsense
/// (below vacuum) and worse than the `DEFAULT_DIELECTRIC_IOR` fallback it
/// would replace.
///
/// What IS a real, undone gap: ordinary Skyrim+ content that authors the
/// standalone SLSF1 `Refraction` bit (normal-map-driven distortion,
/// without `Fire_Refraction`) has zero engine consumer for that authored
/// intent — building one would mean a new screen-space distortion shader
/// path for non-fire refractive materials (own canonical field, own
/// `MAT_FLAG_*` bit, new `triangle.frag` consumer), not a change to this
/// function. Tracked as future work, not silently fixed here. See
/// `docs/engine/nifal.md`'s Shader flags section for the canonical-tier
/// framing.
fn material_optical_scalar(material_kind: u32, refraction_strength: f32) -> f32 {
    if material_kind == byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION {
        if refraction_strength.is_finite() {
            refraction_strength.clamp(0.0, 1.0)
        } else {
            0.0
        }
    } else {
        byroredux_core::ecs::components::material::DEFAULT_DIELECTRIC_IOR
    }
}

/// Spawn-resolved texture-slot paths the caller computes before
/// translation: the REFR XATO/XTNM/XTXR overlay (cell loader) has
/// already been applied and each populated [`byroredux_core::string::
/// FixedString`] handle resolved to an owned `String`.
///
/// The complete common semantic set stays intact at this boundary. The
/// canonical [`Material`] consumes the path-bearing roles it exposes, while
/// [`MaterialTextureHandles`] carries every role to the renderer.
pub(crate) struct ResolvedPaths {
    pub textures: MaterialTextureSet<Option<String>>,
    pub material_path: Option<String>,
}

/// Build the canonical water payload for a mesh-bound water shader. Legacy
/// NIF water properties do not carry a WATR record, so they cannot use the
/// cell-loader's full per-record translation; they still author optical
/// response through the shared material property chain.
pub(crate) fn water_material_from_mesh(
    material: &Material,
    normal_map_index: u32,
    flow_map_index: u32,
) -> WaterMaterial {
    // Texture handle 0 is the registry's diagnostic placeholder; the water
    // shader reserves `u32::MAX` for its procedural normal fallback.
    let mut water = WaterMaterial {
        shader_flags: material.water_shader_flags,
        normal_map_index: if normal_map_index == 0 {
            u32::MAX
        } else {
            normal_map_index
        },
        flow_map_index: if flow_map_index == 0 {
            u32::MAX
        } else {
            flow_map_index
        },
        ..Default::default()
    };
    if material.env_map_scale.is_finite() {
        water.reflectivity = material.env_map_scale.clamp(0.0, 1.0);
    }
    if material.alpha.is_finite() {
        water.opacity = material.alpha.clamp(0.0, 1.0);
    }
    // `BSWaterShaderProperty.uv_scale` is preserved by the NIF material
    // importer as the canonical Material UV scale. Mesh-bound water does not
    // have a WATR record, so apply that authored multiplier to the renderer's
    // world-space tiling defaults instead of silently reverting to generic
    // lake scales. The unit scale is the compatibility sentinel; malformed
    // or non-positive values keep the default tiling.
    if material
        .uv_scale
        .iter()
        .all(|value| value.is_finite() && *value > 0.0)
    {
        water.uv_scale_a = (water.uv_scale_a * material.uv_scale[0]).clamp(1.0 / 4096.0, 1.0 / 8.0);
        water.uv_scale_b = (water.uv_scale_b * material.uv_scale[1]).clamp(1.0 / 4096.0, 1.0 / 8.0);
    }
    if material.uv_offset.iter().all(|value| value.is_finite()) {
        water.uv_offset = material.uv_offset;
    }
    // `BSWaterShaderProperty::WaterFlag` uses nif.xml's dedicated
    // WaterShaderPropertyFlags vocabulary (bits 0..=13), not generic shader
    // flags. The zero word remains the compatibility sentinel for the older
    // field-less `WaterShaderProperty`. Reflection/refraction are the only
    // optical gates this compact material currently exposes; other bits stay
    // preserved in `shader_flags` for future consumers (#3152).
    if water.shader_flags != 0 {
        const REFLECTIONS: u32 = 1 << 6;
        const REFRACTIONS: u32 = 1 << 7;
        if water.shader_flags & REFLECTIONS == 0 {
            water.effect_controls[2] = 0.0;
        }
        if water.shader_flags & REFRACTIONS == 0 {
            // Positive/zero is the authored/default refraction-magnitude
            // domain. A negative value is the compact canonical sentinel
            // consumed by water.frag to skip the refraction ray entirely.
            water.effect_controls[0] = -1.0;
        }
    }
    water
}

/// The **single** name-token → [`WaterKind`] table, shared by both water
/// producers: WATR `EditorID`s on the CELL path
/// ([`crate::env_translate::resolve_water_material`]) and mesh/asset names on
/// the loose-NIF path ([`water_kind_from_mesh_name`]).
///
/// It is one function because the two token sets drifted apart while they were
/// two (#3154, #3198): the mesh side knew `canal` and the CELL side did not,
/// and neither knew `creek`. A token added for one producer must reach both.
///
/// Vocabulary coverage is verified against shipped masters, not guessed:
///
/// - `creek` — 5 records in `FalloutNV.esm`, 2 in `Fallout3.esm`, and Skyrim's
///   `CreekWaterFlow` / `DefaultCreekWater` / `CreekWaterFlowSW|SE`. Without it
///   **all 78 vanilla FNV records classified `Calm`** and the whole WATAL flow
///   arm was unreachable on the reference title (#3198).
/// - `canal` — no vanilla FNV/FO3 hits; retained from the mesh-side set.
///
/// Deliberately **not** tokens, checked and rejected against the same census:
///
/// - `spill` — its only two FNV records are `ToxicSpillPuddle` and
///   `WaterTypeQuantumColaSpill`. Both are static puddles; the token would
///   classify standing water as flowing, which is the error this table's
///   conservative default exists to avoid.
/// - `fountain`, `potomac` — `TenPenWaterFountain` / `VStripULFountain` are
///   basins, and while the Potomac is genuinely a river, `Potomac`
///   (`00030009`) is the `WRLD` `NAM2` default water for **10 worldspaces**.
///   Promoting it would add foam and a current to every un-overridden body in
///   all of them — a blast radius that needs its own evidence, not a token.
fn water_kind_from_name(name: &str) -> WaterKind {
    let lowered = name.to_ascii_lowercase();
    if lowered.contains("rapid") {
        WaterKind::Rapids
    } else if lowered.contains("waterfall") || lowered.contains("falls") {
        WaterKind::Waterfall
    } else if lowered.contains("river")
        || lowered.contains("stream")
        || lowered.contains("canal")
        || lowered.contains("creek")
    {
        WaterKind::River
    } else {
        WaterKind::Calm
    }
}

/// Classify WATR `EditorID`s on the CELL path. Cell-level water planes are
/// **always horizontal** (XCLW gives a Y height; the mesh is a flat quad), so a
/// `Waterfall` name is demoted to `River`: the horizontal plane below a
/// waterfall is a fast turbulent pool, not a falling sheet, and Skyrim applies
/// `fall`-named records to horizontal bodies (`DLC2WaterFallingStream`,
/// `WaterFallingPool`, `WaterRiverFallingSlow`). The `Waterfall` shader mode is
/// for vertical sheet geometry, which the cell loader never spawns.
pub(crate) fn water_kind_from_cell_record_name(editor_id: &str) -> WaterKind {
    match water_kind_from_name(editor_id) {
        WaterKind::Waterfall => WaterKind::River,
        other => other,
    }
}

/// Classify a dedicated water mesh when no CELL/WATR record exists. The NIF
/// water shader properties carry optics but no semantic kind, so only explicit
/// asset-name tokens promote the default `Calm` path; ordinary meshes remain
/// conservative rather than becoming fast-flowing water by accident.
pub(crate) fn water_kind_from_mesh_name(name: Option<&str>) -> (WaterKind, Option<WaterFlow>) {
    let kind = water_kind_from_name(name.unwrap_or_default());
    let flow = match kind {
        WaterKind::Calm | WaterKind::Lava => None,
        WaterKind::Waterfall => Some(WaterFlow::for_kind(kind, [0.0, -1.0, 0.0])),
        // A name can establish the semantic kind but cannot establish the
        // sign or axis of a horizontal current. Emit no physics flow rather
        // than fabricating a world-+X push for rotated/localized assets
        // (#3185). Authored cell/WATR vectors still produce WaterFlow.
        WaterKind::River | WaterKind::Rapids => None,
    };
    (kind, flow)
}

/// Classify mesh water with a geometry fallback for localized/unnamed assets.
///
/// The legacy name heuristic remains authoritative when it identifies a kind.
/// Otherwise, a tall, narrow finite mesh is the characteristic shape of a
/// waterfall sheet (horizontal rivers/lakes have their largest span in X/Z).
/// This keeps non-English water assets from falling into the calm-water path
/// without changing ordinary flat water or malformed geometry.
pub(crate) fn water_kind_from_mesh_geometry(
    name: Option<&str>,
    positions: &[[f32; 3]],
) -> (WaterKind, Option<WaterFlow>) {
    let named = water_kind_from_mesh_name(name);
    if !matches!(named.0, WaterKind::Calm) || positions.len() < 3 {
        return named;
    }

    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for position in positions {
        if !position.iter().all(|value| value.is_finite()) {
            continue;
        }
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    if !min.iter().all(|value| value.is_finite()) {
        return named;
    }
    let spans = [max[0] - min[0], max[1] - min[1], max[2] - min[2]];
    let horizontal = spans[0].max(spans[2]).max(1.0);
    // Conservative, explicitly unmeasured compatibility heuristic. These
    // named thresholds isolate a geometry fallback that shipped-mesh census
    // work can replace without mistaking them for source-format constants.
    const MIN_WATERFALL_VERTICAL_SPAN: f32 = 16.0;
    const MIN_WATERFALL_VERTICAL_ASPECT: f32 = 1.5;
    if spans[1] > MIN_WATERFALL_VERTICAL_SPAN
        && spans[1] > horizontal * MIN_WATERFALL_VERTICAL_ASPECT
    {
        (
            WaterKind::Waterfall,
            Some(WaterFlow::for_kind(WaterKind::Waterfall, [0.0, -1.0, 0.0])),
        )
    } else {
        named
    }
}

/// Derive a conservative physics volume for a mesh-bound water surface. NIF
/// water blocks do not author a volume or current; the rendered surface is the
/// entity's transformed Y plane, while imported bounds provide coverage/depth.
pub(crate) fn water_volume_from_mesh(
    position: Vec3,
    rotation: Quat,
    scale: f32,
    local_center: Vec3,
    local_radius: f32,
) -> WaterVolume {
    let center = position + rotation * (local_center * scale);
    let radius = (local_radius * scale.abs()).max(1.0);
    // Conservative, unmeasured gameplay volume depth used only when a NIF
    // water surface has no CELL/XCLW bounds. Named so a future mesh census or
    // authored volume source can replace it without ABI churn (#3185).
    const MESH_WATER_VOLUME_DEPTH_RADII: f32 = 4.0;
    WaterVolume {
        min: [
            center.x - radius,
            position.y - radius * MESH_WATER_VOLUME_DEPTH_RADII,
            center.z - radius,
        ],
        max: [center.x + radius, position.y, center.z + radius],
    }
}

/// Placement data needed to translate and attach one mesh-bound water
/// surface. Both loose-NIF loading and placed-cell meshes feed this exact
/// boundary so classification, optics, foam, current, and gameplay volume
/// cannot drift between the two spawn paths (#3182).
pub(crate) struct MeshWaterSource<'a> {
    pub name: Option<&'a str>,
    pub positions: &'a [[f32; 3]],
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    pub local_bound_center: Vec3,
    pub local_bound_radius: f32,
}

pub(crate) fn attach_mesh_water(
    world: &mut World,
    entity: EntityId,
    normal_map_index: u32,
    flow_map_index: u32,
    source: MeshWaterSource<'_>,
) {
    let mut water_material = {
        let material = world
            .get::<Material>(entity)
            .expect("mesh water attachment requires canonical Material");
        water_material_from_mesh(&material, normal_map_index, flow_map_index)
    };
    let (kind, flow) = water_kind_from_mesh_geometry(source.name, source.positions);
    water_material.foam_strength = kind.canonical_foam_strength();
    world.insert(
        entity,
        WaterPlane {
            kind,
            material: water_material,
            damage_per_second: 0.0,
        },
    );
    if let Some(flow) = flow {
        world.insert(entity, flow);
    }
    if kind != WaterKind::Waterfall {
        world.insert(
            entity,
            water_volume_from_mesh(
                source.position,
                source.rotation,
                source.scale,
                source.local_bound_center,
                source.local_bound_radius,
            ),
        );
    }
}

/// Translate a source-normalized [`ImportedMaterial`] + caller-resolved
/// paths into the
/// canonical [`Material`] component.
///
/// Resolution performed here (the "single source of truth"):
///   - all material scalars / colors / flags copied across;
///   - `effect_shader_flags` packed as the union of the BSEffectShader
///     SLSF bits ([`crate::cell_loader::pack_effect_shader_flags`]), the
///     BGSM v>2 PBR/translucency/model-space-normals bits
///     ([`crate::cell_loader::pack_imported_material_flags`]), and any
///     `extra_material_flags` the caller supplies (the cell loader's
///     REFR-overlay model-space-normals bit; `0` for loose-NIF loads);
///   - PBR scalars resolved: for NIF-imported content the keyword
///     classifier already ran at import time (`classify_legacy_pbr` in the
///     NIF mesh extractors) and populated `source.metalness_override/
///     roughness_override` as `Some(…)`, so [`Material::resolve_pbr`] here
///     only clamps — its classifier arm is a sentinel-backstop (only fires
///     when the override is `NaN`, i.e. for future non-NIF paths). BGSM/BGEM
///     content also arrives pre-classified as `Some`. Either way every
///     material exits with explicit `(metalness, roughness)` scalars; no
///     render-time fallback. `feedback_format_translation.md` Stage 1.
///     (Structure: classify-at-import + clamp-at-translate. See #1346.)
///   - glass classified once, alpha-aware
///     ([`crate::helpers::classify_glass_into_material`]), after the PBR
///     resolve so the forced glass roughness wins.
pub(crate) fn translate_material(
    source: &ImportedMaterial,
    mesh_name: Option<&str>,
    paths: ResolvedPaths,
    extra_material_flags: u32,
) -> Material {
    let ResolvedPaths {
        textures,
        material_path,
    } = paths;
    let texture_path = textures.base_color.clone();
    let mut material = Material {
        water_shader_flags: source.water_shader_flags,
        is_water_shader: source.is_water_shader,
        emissive_color: source.emissive_color,
        emissive_mult: source.emissive_mult,
        emissive_source: source.emissive_source,
        specular_color: source.specular_color,
        specular_strength: source.specular_strength,
        diffuse_color: source.diffuse_color,
        ambient_color: source.ambient_color,
        glossiness: source.glossiness,
        uv_offset: source.uv_offset,
        uv_scale: source.uv_scale,
        alpha: source.mat_alpha,
        env_map_scale: source.env_map_scale,
        normal_map: textures.normal,
        texture_path: texture_path.clone(),
        material_path,
        glow_map: textures.emissive,
        detail_map: textures.detail,
        gloss_map: textures.smooth_spec,
        dark_map: textures.dark,
        vertex_color_mode: source.vertex_color_mode,
        alpha_test: source.alpha_test,
        alpha_threshold: source.alpha_threshold,
        alpha_test_func: source.alpha_test_func,
        material_kind: source.material_kind,
        wireframe: source.wireframe,
        flat_shading: source.flat_shading,
        z_test: source.z_test,
        z_write: source.z_write,
        z_function: source.z_function,
        shader_type_fields: if source.shader_type_fields.is_empty() {
            None
        } else {
            Some(Box::new(source.shader_type_fields.clone()))
        },
        // #620 / #451 — BSEffectShaderProperty falloff cone (Skyrim+) OR
        // BSShaderNoLightingProperty falloff cone (FO3/FNV sibling).
        // BSShaderNoLighting fills `soft_falloff_depth = 0.0` (no
        // soft-depth field on that block).
        effect_falloff: source
            .effect_shader
            .as_ref()
            .map(|es| EffectFalloff {
                start_angle: es.falloff_start_angle,
                stop_angle: es.falloff_stop_angle,
                start_opacity: es.falloff_start_opacity,
                stop_opacity: es.falloff_stop_opacity,
                soft_falloff_depth: es.soft_falloff_depth,
            })
            .or_else(|| {
                source.no_lighting_falloff.as_ref().map(|nl| EffectFalloff {
                    start_angle: nl.start_angle,
                    stop_angle: nl.stop_angle,
                    start_opacity: nl.start_opacity,
                    stop_opacity: nl.stop_opacity,
                    soft_falloff_depth: 0.0,
                })
            }),
        // #890 Stage 2 / #1077 Phase 2a — union of the BSEffect SLSF
        // bits, the BGSM v>2 bits, and the caller's extra bits (REFR
        // overlay model-space-normals on the cell path). All three
        // contributors target the same `material_flag::*` layout so a
        // single OR yields the word `GpuMaterial.material_flags` consumes.
        effect_shader_flags: crate::cell_loader::pack_effect_shader_flags(
            source.effect_shader.as_ref(),
        ) | crate::cell_loader::pack_imported_material_flags(source)
            | extra_material_flags,
        // #1147 Phase 2b — BGSM v>=8 translucency suite; only meaningful
        // when `pack_imported_material_flags` set MAT_FLAG_BGSM_TRANSLUCENCY.
        translucency_subsurface_color: source.translucency_subsurface_color,
        translucency_transmissive_scale: source.translucency_transmissive_scale,
        translucency_turbulence: source.translucency_turbulence,
        // #2284 (MAT-D1-NEW-04) — Skyrim+/FO4 BSLightingShaderProperty
        // shading scalars. Captured, not yet shaded (no GpuMaterial /
        // triangle.frag consumer) — see the field docs on `Material`.
        lighting_effect_1: source.lighting_effect_1,
        lighting_effect_2: source.lighting_effect_2,
        subsurface_rolloff: source.subsurface_rolloff,
        rimlight_power: source.rimlight_power,
        backlight_power: source.backlight_power,
        fresnel_power: source.fresnel_power,
        soft_lighting: source.soft_lighting,
        rim_lighting: source.rim_lighting,
        back_lighting: source.back_lighting,
        // #890 Stage 2c — BSEffectShaderProperty greyscale LUT path;
        // resolved to a bindless handle at draw-build time.
        greyscale_texture: textures.greyscale_lut,
        // #2443 (MAT-D3-01) — the LUT's strength modulator. Captured by both
        // producers (inline BSVER>=130 shader block, BGSM/BGEM merge) but
        // dropped here until now, because no canonical field existed to copy
        // into — the palette *LUT* crossed the boundary while its *scale* did
        // not. Captured, not yet shaded (see the field doc on `Material`).
        grayscale_to_palette_scale: source.grayscale_to_palette_scale,
        // Canonical PBR — seed authored BGSM/BGEM scalars
        // (`merge_external_material`) or a NaN sentinel for legacy
        // inline-shader content; `resolve_pbr` below fills any sentinel
        // from the keyword classifier and clamps to the renderer ranges.
        metalness: source.metalness_override.unwrap_or(f32::NAN),
        roughness: source.roughness_override.unwrap_or(f32::NAN),
        // Generic dielectric for ordinary materials; fire-refraction uses
        // this discriminated scalar as its authored distortion strength.
        // Glass promotion below replaces the ordinary value with the shared
        // glass IOR while preserving source texture overlays.
        ior: material_optical_scalar(source.material_kind, source.refraction_strength),
        glass_fresnel_color: source.glass_fresnel_color,
        glass_refraction_scale: source.glass_refraction_scale,
        glass_blur_scale: source.glass_blur_scale,
        glass_blur_scale_factor: source.glass_blur_scale_factor,
        // #2514 — Disney-BSDF-only parameters with no source-format
        // equivalent (no BGSM/BGEM/inline-NIF field maps to them); zero
        // matches `Material::default()`'s Burley/isotropic-only fallback.
        // Reachable only via `mat.set` (Cornell harness).
        subsurface: 0.0,
        sheen: 0.0,
        sheen_tint: 0.0,
        anisotropic: 0.0,
        // #2571 (OBL-D5-01) — copied verbatim so spawn sites read the
        // canonical component instead of re-reading `ImportedMaterial`
        // independently. See the field docs on `Material`.
        texture_clamp_mode: source.texture_clamp_mode,
        src_blend_mode: source.src_blend_mode,
        dst_blend_mode: source.dst_blend_mode,
    };
    material.resolve_pbr();
    crate::helpers::classify_glass_into_material(
        &mut material,
        mesh_name,
        texture_path.as_deref(),
        // `has_alpha` tracks blended transparency, while broken panes and
        // mirrors commonly express their transparent coverage exclusively
        // through NiAlphaProperty's alpha-test bit.  Both are valid glass
        // coverage; alpha-test must not make an otherwise explicit glass
        // texture look like an opaque wall.
        source.has_alpha || source.alpha_test,
        source.is_decal,
        source.bgem_glass,
        // #2710 — external-material provenance. Gates the glass-keyword
        // promotion of an effect-shader carrier to FO4+ content, where a
        // `.bgem` exists beside the mesh; Skyrim's inline effect shaders
        // never set it, so their keyword-sharing haze layers stay effects.
        source.from_bgsm,
    );
    material
}

/// Canonical `Material` for a drawn surface that has **no source material
/// record** — only a bound diffuse texture path.
///
/// Three exterior draw populations are in this shape: LAND terrain
/// (`cell_loader/terrain.rs`), distant terrain LOD (`terrain_lod.rs`), and
/// object-LOD imposters (`object_lod.rs`). None of them comes from a NIF
/// shader property or a BGSM/BGEM, so [`translate_material`] — which takes an
/// `ImportedMaterial` — has nothing to translate. What they *do* have is the
/// real texture they sample, which is exactly the input
/// [`Material::resolve_pbr`]'s keyword classifier consumes.
///
/// #2444 (MAT-D3-02) — before this, all three spawned without any `Material`
/// component at all, so their draws fell into `render/static_meshes.rs`'s
/// no-`Material` arm and picked up an 11-tuple of hardcoded literals
/// (`roughness 0.5`, `metalness 0.0`, …). That was a second materialization
/// site living in the render path, outside the documented single source of
/// truth, and it made the NIFAL invariant "every drawn surface's canonical
/// material is produced at one boundary" false for the entire outdoors. It
/// also shaded landscape with a markedly tighter GGX lobe (0.5) than the
/// stone/dirt statics standing on it (the classifier's 0.85), a visible
/// mismatch at every ground-meets-architecture seam, and gave object-LOD
/// imposters a shading pop on top of their geometric one.
///
/// This is deliberately *not* a fourth ad hoc materialization site: it owns
/// no scalar literals of its own. Every canonical value it produces comes
/// from `Material::default()` or from `resolve_pbr`'s classifier, the same
/// one `translate_material` calls, so terrain now classifies by the same
/// rules as the architecture standing on it.
pub(crate) fn translate_texture_only_material(texture_path: Option<String>) -> Material {
    let mut material = Material {
        texture_path,
        // NaN sentinel = "no authored override" — exactly what
        // `translate_material` seeds for inline-shader content, and what
        // makes `resolve_pbr` run its keyword classifier rather than just
        // clamping the `Material::default()` values through unchanged.
        metalness: f32::NAN,
        roughness: f32::NAN,
        // `Material::default()`'s `env_map_scale` is 1.0 — the raw
        // on-disk `BSShaderPPLighting` field value before #2315 gated its
        // forwarding on an authored environment-mapping flag (see
        // `classify_pbr_keyword`'s env-map arm, #2555, for the current
        // reachability). That default doesn't apply here regardless:
        // these populations have no shader property at all, so nothing
        // authored environment mapping for them and 0.0 is the honest
        // input.
        //
        // Load-bearing twice over. It steers the classifier: a non-zero
        // scale takes `classify_pbr_keyword`'s env_map_scale arm (roughness
        // 0.8) instead of the matte default (0.85) that ordinary
        // architecture lands on, which is the number this whole fix is about
        // matching. And `render/static_meshes.rs` forwards `env_map_scale`
        // to `GpuMaterial` as the cubemap reflection strength — reading
        // `0.0` for these draws before they had a `Material` at all — so
        // inheriting the 1.0 default would have switched distant terrain and
        // LOD imposters into full-strength environment reflections as a side
        // effect of a PBR-scalar fix.
        env_map_scale: 0.0,
        ..Material::default()
    };
    material.resolve_pbr();
    material
}

/// High bit OR'd into the gloss texture slot to tell the fragment shader
/// "sample the per-pixel spec/smoothness mask from the NORMAL map's alpha
/// channel" — the Skyrim/Gamebryo normal-alpha-as-spec convention. The
/// gloss slot then points at the normal map's bindless handle. This bit is
/// applied per-draw in `render::static_meshes` because it is a transient
/// texture-binding instruction, not canonical material state; the matching
/// *roughness* scalar is resolved once at spawn by
/// [`resolve_normal_alpha_spec_roughness`] (#1480 / REN-D22-NEW-01).
///
/// #1500 / REN2-15 — re-exported from the renderer's shader-constants single
/// source of truth (which also generates the `triangle.frag` `#define`),
/// rather than re-declaring the literal here. A value flip now changes both
/// sides in lockstep and is impossible to desync.
pub(crate) const NORMAL_ALPHA_SPEC_BIT: u32 =
    byroredux_renderer::shader_constants::NORMAL_ALPHA_SPEC_BIT;

/// The normal-alpha-as-spec population gate (Skyrim/Gamebryo era): a lit
/// surface (`material_kind < 100`) that ships a normal map but no dedicated
/// gloss/specular texture. Environment-mapped materials remain eligible:
/// when no environment mask is authored, normal alpha is also the fallback
/// reflection mask. Excludes glass/effect kinds (>= 100, own optics). The
/// inputs are the exact values both the spawn write-back and the render
/// path read from the `Material` / [`MaterialTextureHandles`] components,
/// so the gate cannot diverge between the two call sites.
pub(crate) fn normal_alpha_spec_applies(
    material_kind: u32,
    _metalness: f32,
    _env_map_scale: f32,
    normal_map_index: u32,
    gloss_map_index: u32,
) -> bool {
    material_kind < 100 && normal_map_index != 0 && gloss_map_index == 0
}

/// Should the per-draw gloss slot be re-pointed at the normal map with the
/// [`NORMAL_ALPHA_SPEC_BIT`]? The render-side half of the pair whose spawn-side
/// half is [`resolve_normal_alpha_spec_roughness`].
///
/// #2445 (MAT-D3-03) — the `material.is_none()` arm is the point of this
/// function existing. The two halves were documented as unable to diverge
/// because they share [`normal_alpha_spec_applies`], which is necessary but
/// not sufficient: the spawn half *early-returns* on an entity with no
/// `Material` (it reads its inputs out of that component), while the render
/// half had no equivalent guard and simply substituted its no-`Material`
/// fallback scalars — which still pass the shared predicate. A `Material`-less
/// draw carrying a normal map therefore bound the gloss slot with nothing
/// having resolved the paired roughness. #2444 removed the only population in
/// that shape (exterior terrain / LOD, which do carry normal maps), so this
/// makes the invariant hold by construction rather than by the accident of no
/// such population existing.
pub(crate) fn normal_alpha_spec_binding_applies(
    material: Option<&Material>,
    normal_has_alpha: bool,
    material_kind: u32,
    metalness: f32,
    normal_map_index: u32,
    gloss_map_index: u32,
) -> bool {
    let Some(material) = material else {
        return false;
    };
    normal_has_alpha
        && normal_alpha_spec_applies(
            material_kind,
            metalness,
            material.env_map_scale,
            normal_map_index,
            gloss_map_index,
        )
}

/// Canonical roughness for the normal-alpha-as-spec population, or `None`
/// when the gate does not apply (caller keeps the `resolve_pbr`-resolved
/// roughness). An alpha-bearing normal is deliberately a no-op here: its
/// alpha is the per-pixel specular-intensity mask consumed in the shader,
/// never a smoothness signal. The legacy alpha-less/high-specular fallback
/// remains restricted to matte, non-env-mapped materials and derives from
/// `specular_strength`, never from current roughness, so reruns cannot drift.
#[allow(clippy::too_many_arguments)]
pub(crate) fn normal_alpha_spec_roughness(
    material_kind: u32,
    metalness: f32,
    env_map_scale: f32,
    _glossiness: f32,
    specular_strength: f32,
    normal_map_index: u32,
    gloss_map_index: u32,
    normal_has_alpha: bool,
    bgsm_pbr_scalars_authored: bool,
) -> Option<f32> {
    // #2606 (FO4-D7-01) — never overwrite roughness a real material file
    // authored. This is a Skyrim/Gamebryo-era fallback for content that
    // supplies no smoothness signal at all; an FO4 BGSM supplies one
    // directly (`roughness = 1 - smoothness`), so for BGSM content the
    // heuristic has nothing to add and everything to lose.
    //
    // Live, not latent: `ImportedMaterial::default().env_map_scale` is 0.0,
    // so the `env_map_scale <= 0.3` gate below is satisfied by DEFAULT rather
    // than only when a low value was authored — and #2608 shows the BGSM merge
    // has no authored value to put there for a v>=10 material. Any BGSM with a
    // normal map, no smooth/spec texture and `specular_mult > 1.2` therefore
    // reached the overwrite. #1352 makes it bite: `MAT_FLAG_PBR_BSDF` is now
    // unconditional for BGSM content, so the Disney lobe consumes the
    // clobbered roughness directly.
    //
    // Gated on `bgsm_pbr_scalars_authored`, NOT on `from_bgsm`/`BGSM_AUTHORED`
    // (which the BGEM arm sets while leaving both overrides `None` — #2609)
    // and NOT on `roughness_override.is_some()` (true for keyword-classified
    // legacy content too, which would disable this fallback for the very
    // population it exists to serve).
    if bgsm_pbr_scalars_authored {
        return None;
    }
    if !normal_alpha_spec_applies(
        material_kind,
        metalness,
        env_map_scale,
        normal_map_index,
        gloss_map_index,
    ) {
        return None;
    }
    if normal_has_alpha {
        None
    } else if metalness < 0.3 && env_map_scale <= 0.3 && specular_strength > 1.2 {
        Some((0.85 - (specular_strength - 1.0) * 0.1).clamp(0.4, 0.85))
    } else {
        None
    }
}

/// Resolve the normal-alpha-as-spec roughness ONCE at spawn and write it
/// into the canonical [`Material::roughness`], instead of recomputing it
/// per draw in the render path. This is the #1480 / REN-D22-NEW-01 contract
/// fix: the renderer reads the resolved scalar directly (NIFAL
/// resolve-once), with no render-time heuristic mutating canonical state.
///
/// Reads the SAME components the render path reads (`Material`,
/// `MaterialTextureHandles`), so the written value is
/// byte-identical to the legacy per-draw result — only its home (the
/// canonical field, now visible to `mat.*` / `material_dump` tooling) and
/// its timing (once at spawn, not every frame) change. Idempotent (see
/// [`normal_alpha_spec_roughness`]). Call after all three components are
/// attached to `entity`.
pub(crate) fn resolve_normal_alpha_spec_roughness(
    world: &mut World,
    entity: EntityId,
    bgsm_pbr_scalars_authored: bool,
) {
    let Some((material_kind, metalness, env_map_scale, glossiness, specular_strength)) =
        world.get::<Material>(entity).map(|m| {
            (
                m.material_kind,
                m.metalness,
                m.env_map_scale,
                m.glossiness,
                m.specular_strength,
            )
        })
    else {
        return;
    };
    let (normal_map_index, gloss_map_index, normal_has_alpha) = world
        .get::<MaterialTextureHandles>(entity)
        .map(|handles| {
            (
                handles.textures.normal,
                handles.textures.smooth_spec,
                handles.normal_has_alpha,
            )
        })
        .unwrap_or((0, 0, false));
    if let Some(r) = normal_alpha_spec_roughness(
        material_kind,
        metalness,
        env_map_scale,
        glossiness,
        specular_strength,
        normal_map_index,
        gloss_map_index,
        normal_has_alpha,
        bgsm_pbr_scalars_authored,
    ) {
        if let Some(m) = world.get_mut::<Material>(entity) {
            m.roughness = r;
        }
    }
}

/// Pure decision function backing [`resolve_msn_z_source`]: whether the
/// bound model-space normal map's blue channel carries authored Z, i.e.
/// whether `MAT_FLAG_MSN_HAS_AUTHORED_Z` should be set. Only meaningful
/// when `model_space_normals` is set — see [`resolve_msn_z_source`] for
/// the DXGI-format rationale behind reusing `normal_has_alpha` as the
/// authored-Z signal.
fn msn_has_authored_z(model_space_normals: bool, normal_has_alpha: bool) -> bool {
    model_space_normals && normal_has_alpha
}

/// #2826 (REN-D19-02) — resolves `MAT_FLAG_MSN_HAS_AUTHORED_Z` the same way
/// [`resolve_normal_alpha_spec_roughness`] resolves normal-alpha-as-spec:
/// once at spawn time, after `MaterialTextureHandles` has been attached,
/// into the canonical `Material.effect_shader_flags` — never re-derived
/// per-fragment in the shader.
///
/// Only meaningful when `MAT_FLAG_MODEL_SPACE_NORMALS` is set. Reuses the
/// texture registry's alpha classification (`MaterialTextureHandles::
/// normal_has_alpha`, itself `dds::format_has_alpha` on the bound normal
/// map's DDS format) as the authored-Z signal: in the surveyed FO4
/// archives, three-channel `_msn` maps with real signed Z are BC3 (has
/// alpha), and the genuinely two-channel `_msn` set (FaceCustomization,
/// constant zero blue) is BC1 (no alpha) — see the audit's DXGI survey
/// (docs/audits/AUDIT_RENDERER_2026-08-12b.md REN-D19-02). This is an
/// empirical correlation over the corpus, not a hard format guarantee; a
/// source that authors a three-channel `_msn` in an alpha-less format (or
/// vice versa) would need its own signal.
pub(crate) fn resolve_msn_z_source(world: &mut World, entity: EntityId) {
    let model_space_normals = world
        .get::<Material>(entity)
        .map(|m| {
            m.effect_shader_flags
                & byroredux_renderer::vulkan::material::material_flag::MODEL_SPACE_NORMALS
                != 0
        })
        .unwrap_or(false);
    let normal_has_alpha = world
        .get::<MaterialTextureHandles>(entity)
        .map(|handles| handles.normal_has_alpha)
        .unwrap_or(false);
    if msn_has_authored_z(model_space_normals, normal_has_alpha) {
        if let Some(m) = world.get_mut::<Material>(entity) {
            m.effect_shader_flags |=
                byroredux_renderer::vulkan::material::material_flag::MSN_HAS_AUTHORED_Z;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesh_water_zero_normal_handle_uses_procedural_sentinel() {
        let material = Material::default();
        let water = water_material_from_mesh(&material, 0, 0);
        assert_eq!(water.normal_map_index, u32::MAX);
    }

    #[test]
    fn mesh_water_preserves_real_normal_handle_and_optical_scalars() {
        let mut material = Material::default();
        material.env_map_scale = 0.42;
        material.alpha = 0.73;
        let water = water_material_from_mesh(&material, 17, 0);
        assert_eq!(water.normal_map_index, 17);
        assert!((water.reflectivity - 0.42).abs() < f32::EPSILON);
        assert!((water.opacity - 0.73).abs() < f32::EPSILON);
    }

    #[test]
    fn mesh_water_preserves_authored_flow_map_handle() {
        let water = water_material_from_mesh(&Material::default(), 0, 23);
        assert_eq!(water.flow_map_index, 23);
        assert_eq!(
            water_material_from_mesh(&Material::default(), 0, 0).flow_map_index,
            u32::MAX
        );
    }

    #[test]
    fn mesh_water_applies_authored_uv_scale_to_world_tiling() {
        let mut material = Material::default();
        material.uv_scale = [2.0, 0.5];
        material.uv_offset = [0.125, -0.25];
        let water = water_material_from_mesh(&material, 0, 0);
        assert!((water.uv_scale_a - 2.0 / 256.0).abs() < f32::EPSILON);
        assert!((water.uv_scale_b - 0.5 / 700.0).abs() < f32::EPSILON);
        assert_eq!(water.uv_offset, [0.125, -0.25]);
    }

    #[test]
    fn mesh_water_honors_authored_optical_flag_gates() {
        let mut material = Material::default();
        // Real nif.xml default: DEPTH | REFLECTIONS | REFRACTIONS (0xC4).
        material.water_shader_flags = 0xC4;
        let water = water_material_from_mesh(&material, 9, 0);
        assert_eq!(water.shader_flags, 0xC4);
        assert!(water.effect_controls[0] >= 0.0);
        assert!(water.effect_controls[2] > 0.0);
        assert!(water.blend_normals);

        // A zero word is the compatibility sentinel for pre-Skyrim water.
        material.water_shader_flags = 0;
        let legacy = water_material_from_mesh(&material, 9, 0);
        assert_eq!(
            legacy.effect_controls,
            WaterMaterial::default().effect_controls
        );
    }

    #[test]
    fn mesh_water_honors_authored_reflection_and_refraction_flags() {
        let mut material = Material::default();
        // Reflection only: keep reflection and explicitly suppress the
        // refraction ray through the compact negative sentinel.
        material.water_shader_flags = 1 << 6;
        let water = water_material_from_mesh(&material, 9, 0);
        assert!(water.effect_controls[2] > 0.0);
        assert!(water.effect_controls[0] < 0.0);
        assert!(water.blend_normals);

        // Both documented optical bits keep both responses enabled.
        material.water_shader_flags = (1 << 6) | (1 << 7);
        let authored = water_material_from_mesh(&material, 9, 0);
        assert!(authored.effect_controls[2] > 0.0);
        assert!(authored.effect_controls[0] >= 0.0);
        assert!(authored.blend_normals);
    }

    #[test]
    fn named_horizontal_mesh_water_never_fabricates_world_axis_flow() {
        for name in ["River01", "WhiteRapids"] {
            let (kind, flow) = water_kind_from_mesh_name(Some(name));
            assert!(matches!(kind, WaterKind::River | WaterKind::Rapids));
            assert!(flow.is_none(), "{name} has no authored current axis");
        }
    }

    #[test]
    fn mesh_and_cell_water_kinds_share_the_canonical_foam_profile() {
        for (kind, expected) in [
            (WaterKind::Calm, 0.65),
            (WaterKind::River, 0.20),
            (WaterKind::Rapids, 0.85),
            (WaterKind::Waterfall, 0.85),
            (WaterKind::Lava, 0.0),
        ] {
            assert_eq!(kind.canonical_foam_strength(), expected);
        }
    }

    #[test]
    fn shared_mesh_attachment_applies_kind_foam_without_inventing_current() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Material::default());
        let positions = [[-8.0, 0.0, -2.0], [8.0, 0.0, -2.0], [0.0, 0.0, 2.0]];
        attach_mesh_water(
            &mut world,
            entity,
            0,
            0,
            MeshWaterSource {
                name: Some("RiverSegment01"),
                positions: &positions,
                position: Vec3::new(10.0, 20.0, 30.0),
                rotation: Quat::IDENTITY,
                scale: 1.0,
                local_bound_center: Vec3::ZERO,
                local_bound_radius: 8.0,
            },
        );
        let plane = world.get::<WaterPlane>(entity).expect("WaterPlane");
        assert_eq!(plane.kind, WaterKind::River);
        assert_eq!(plane.material.foam_strength, 0.20);
        assert!(world.get::<WaterFlow>(entity).is_none());
        assert!(world.get::<WaterVolume>(entity).is_some());
    }

    #[test]
    fn mesh_water_volume_top_matches_rendered_surface() {
        let volume = water_volume_from_mesh(
            Vec3::new(10.0, 25.0, -4.0),
            Quat::IDENTITY,
            2.0,
            Vec3::new(1.0, 7.0, -2.0),
            3.0,
        );
        assert_eq!(volume.max[1], 25.0);
        assert_eq!(volume.min[1], 1.0);
        assert_eq!(volume.min[0], 6.0);
        assert_eq!(volume.max[2], -2.0);
    }

    #[test]
    fn mesh_water_name_classifier_only_promotes_explicit_flow_assets() {
        assert!(matches!(
            water_kind_from_mesh_name(Some("WaterfallSheet01")),
            (WaterKind::Waterfall, Some(_))
        ));
        assert!(matches!(
            water_kind_from_mesh_name(Some("RiverSegment01")),
            (WaterKind::River, None)
        ));
        assert!(matches!(
            water_kind_from_mesh_name(Some("LakeWater01")),
            (WaterKind::Calm, None)
        ));
    }

    #[test]
    fn mesh_water_geometry_classifier_recognizes_localized_waterfall_sheet() {
        let vertical_sheet = [
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [-1.0, 96.0, 0.0],
            [1.0, 96.0, 0.0],
        ];
        assert!(matches!(
            water_kind_from_mesh_geometry(Some("Agua_01"), &vertical_sheet),
            (WaterKind::Waterfall, Some(_))
        ));
    }

    #[test]
    fn mesh_water_geometry_classifier_keeps_flat_unnamed_water_calm() {
        let flat_surface = [
            [-96.0, 0.0, -48.0],
            [96.0, 0.0, -48.0],
            [-96.0, 0.5, 48.0],
            [96.0, 0.5, 48.0],
        ];
        assert!(matches!(
            water_kind_from_mesh_geometry(None, &flat_surface),
            (WaterKind::Calm, None)
        ));
    }

    // Inputs that pass the gate (lit Skyrim-era matte surface w/ normal map,
    // no gloss map): material_kind 0, metalness 0, env_map_scale 0,
    // normal_map_index 7, gloss_map_index 0.
    const PASS: (u32, f32, f32, u32, u32) = (0, 0.0, 0.0, 7, 0);

    /// Regression for #2296 / MAT-D1-NEW-01. `crates/nif` depends on
    /// `byroredux-core` only — never `byroredux-renderer` — so the NIF
    /// importer's `material_kind` assignments
    /// (`crates/nif/src/import/material/dedicated_shader.rs` and
    /// `legacy_properties.rs`) are raw `101`/`102`/`103` literals with no
    /// compile-time link to `byroredux_renderer::MATERIAL_KIND_*`. This
    /// binary is the one crate that depends on both, so it's the only
    /// place a cross-crate assert can live. Pins the literals to the
    /// named constants they must always agree with — a future renumber
    /// of the renderer-side constants breaks this loudly instead of
    /// silently misrouting shading for every effect/no-lighting/fire-haze
    /// surface.
    #[test]
    fn nif_importer_material_kind_literals_match_renderer_constants() {
        // dedicated_shader.rs:536 (material_reference early-return guard)
        // and :663 (primary effect-shader detection; pinned in-crate by
        // `effect_shader_sets_material_kind_to_101`).
        assert_eq!(101u32, byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER);
        // legacy_properties.rs:545 (BSShaderNoLightingProperty branch).
        assert_eq!(102u32, byroredux_renderer::MATERIAL_KIND_NO_LIGHTING);
        // dedicated_shader.rs:481 / legacy_properties.rs:451 (FireRefraction
        // shader-flag detection, both dedicated + legacy property paths).
        assert_eq!(103u32, byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION);
    }

    #[test]
    fn fire_refraction_uses_sanitized_authored_strength_as_optical_payload() {
        let kind = byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION;
        assert_eq!(material_optical_scalar(kind, 0.1), 0.1);
        assert_eq!(material_optical_scalar(kind, -2.0), 0.0);
        assert_eq!(material_optical_scalar(kind, 4.0), 1.0);
        assert_eq!(material_optical_scalar(kind, f32::NAN), 0.0);
        assert_eq!(
            material_optical_scalar(0, 0.1),
            byroredux_core::ecs::components::material::DEFAULT_DIELECTRIC_IOR
        );
    }

    /// SKY-D7-02 / #2327 — pins the discard as a deliberate invariant, not
    /// a bug to "fix" by making it fall through. `refraction_strength` is
    /// nif.xml's 0-1 "amount of distortion" scalar, explicitly NOT a
    /// physical index of refraction; letting it leak into `ior` for any
    /// non-fire-refraction `material_kind` would hand the RT dielectric
    /// refraction path a physically-nonsense value (e.g. `ior = 0.1`, below
    /// vacuum) instead of the correct dielectric default. Every ordinary
    /// `material_kind` — including glass (100) and effect-shader (101),
    /// which have their own separate optical handling downstream — must
    /// see `DEFAULT_DIELECTRIC_IOR` here regardless of how strongly
    /// `refraction_strength` is authored.
    #[test]
    fn non_fire_refraction_kinds_never_leak_refraction_strength_into_ior() {
        let default_ior = byroredux_core::ecs::components::material::DEFAULT_DIELECTRIC_IOR;
        for kind in [
            0,                                               // ordinary lit material
            byroredux_renderer::MATERIAL_KIND_GLASS,         // 100
            byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER, // 101
        ] {
            for strength in [0.0, 0.3, 1.0] {
                assert_eq!(
                    material_optical_scalar(kind, strength),
                    default_ior,
                    "material_kind {kind} with refraction_strength {strength} must fall \
                     back to DEFAULT_DIELECTRIC_IOR, not leak the distortion scalar into ior"
                );
            }
        }
    }

    #[test]
    fn alpha_normal_preserves_translate_resolved_roughness() {
        // Normal alpha is specular intensity, never gloss/roughness.
        let r = normal_alpha_spec_roughness(
            PASS.0, PASS.1, PASS.2, 80.0, 1.0, PASS.3, PASS.4, true, false,
        );
        assert_eq!(r, None);
    }

    #[test]
    fn alphaless_normal_uses_specular_strength_when_above_neutral() {
        // specular_strength 2.0 → 0.85 - (1.0)*0.1 = 0.75.
        let r = normal_alpha_spec_roughness(
            PASS.0, PASS.1, PASS.2, 80.0, 2.0, PASS.3, PASS.4, false, false,
        );
        assert!((r.unwrap() - 0.75).abs() < 1e-5, "{r:?}");
    }

    #[test]
    fn alphaless_normal_with_neutral_specular_keeps_resolved_roughness() {
        // specular_strength 1.0 (<= 1.2) and no normal alpha → None (caller
        // keeps the translate-resolved roughness, no override).
        let r = normal_alpha_spec_roughness(
            PASS.0, PASS.1, PASS.2, 80.0, 1.0, PASS.3, PASS.4, false, false,
        );
        assert_eq!(r, None);
    }

    /// Regression for #2606 (FO4-D7-01) — a BGSM that authored its own PBR
    /// scalars must keep the roughness `merge_external_material` derived from
    /// its `smoothness`, never have it replaced by this Skyrim-era fallback.
    ///
    /// The inputs are the real trigger shape, not a contrived one:
    /// `env_map_scale = 0.0` is `ImportedMaterial`'s DEFAULT (so the
    /// `<= 0.3` gate was satisfied without anything authoring a low value —
    /// which is what made this live rather than latent), plus a normal map,
    /// no smooth/spec texture, and `specular_mult > 1.2`. Both arms are
    /// asserted in one body: the `false` arm proves the overwrite still
    /// happens for the legacy population this fallback exists to serve, so
    /// the `true` arm cannot pass vacuously.
    #[test]
    fn authored_bgsm_scalars_survive_the_normal_alpha_spec_fallback() {
        const DEFAULT_ENV_MAP_SCALE: f32 = 0.0;
        let call = |authored: bool| {
            normal_alpha_spec_roughness(
                0,   // lit material kind
                0.0, // metalness < 0.3
                DEFAULT_ENV_MAP_SCALE,
                80.0,
                2.0, // specular_mult > 1.2
                7,   // normal map bound
                0,   // no dedicated gloss/spec map
                false,
                authored,
            )
        };

        assert_eq!(
            call(true),
            None,
            "BGSM-authored roughness must not be overwritten by the heuristic"
        );
        assert!(
            call(false).is_some(),
            "the same inputs without authored scalars must still take the \
             legacy fallback — otherwise the assertion above proves nothing"
        );
    }

    /// #2609 — the gate is specifically NOT `from_bgsm`/`BGSM_AUTHORED`.
    /// A BGEM sets those while leaving both overrides `None`, so an effect
    /// material has no authored roughness to protect and must keep taking the
    /// fallback like any other non-BGSM content.
    #[test]
    fn bgem_provenance_alone_does_not_suppress_the_fallback() {
        let material = ImportedMaterial {
            from_bgsm: true,
            bgsm_pbr_scalars_authored: false,
            ..Default::default()
        };
        assert!(
            normal_alpha_spec_roughness(
                0,
                0.0,
                0.0,
                80.0,
                2.0,
                7,
                0,
                false,
                material.bgsm_pbr_scalars_authored,
            )
            .is_some(),
            "`from_bgsm` is provenance, not a scalar-authored signal"
        );
    }

    #[test]
    fn gate_excludes_glass_glossmapped_and_normalless_but_keeps_envmapped() {
        // material_kind >= 100 (glass/effect — own roughness).
        assert!(!normal_alpha_spec_applies(100, 0.0, 0.0, 7, 0));
        // Metals and environment-mapped materials still need the authored
        // black=zero / white=full reflection mask.
        assert!(normal_alpha_spec_applies(0, 0.3, 0.0, 7, 0));
        assert!(normal_alpha_spec_applies(0, 0.0, 0.31, 7, 0));
        // no normal map.
        assert!(!normal_alpha_spec_applies(0, 0.0, 0.0, 0, 0));
        // dedicated gloss map present.
        assert!(!normal_alpha_spec_applies(0, 0.0, 0.0, 7, 5));
        // baseline passes.
        assert!(normal_alpha_spec_applies(0, 0.0, 0.0, 7, 0));
    }

    #[test]
    fn roughness_clamps_to_renderer_ranges() {
        // huge specular_strength → 0.85 - big, clamped to 0.4 floor.
        assert_eq!(
            normal_alpha_spec_roughness(
                PASS.0, PASS.1, PASS.2, 80.0, 99.0, PASS.3, PASS.4, false, false
            ),
            Some(0.4)
        );
    }

    #[test]
    fn alpha_normal_ignores_even_non_finite_glossiness() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let r = normal_alpha_spec_roughness(
                PASS.0, PASS.1, PASS.2, bad, 1.0, PASS.3, PASS.4, true, false,
            );
            assert_eq!(
                r, None,
                "normal-alpha spec intensity must not derive roughness from {bad}"
            );
        }
    }

    #[test]
    fn derivation_is_idempotent_over_roughness() {
        // The fallback derives from specular_strength, so re-deriving after a
        // prior write is a no-op —
        // the property that makes the resolve-at-spawn relocation safe to run
        // more than once (#1480).
        let first = normal_alpha_spec_roughness(
            PASS.0, PASS.1, PASS.2, 65.0, 2.0, PASS.3, PASS.4, false, false,
        );
        let second = normal_alpha_spec_roughness(
            PASS.0, PASS.1, PASS.2, 65.0, 2.0, PASS.3, PASS.4, false, false,
        );
        assert_eq!(first, second);
        assert!((first.unwrap() - 0.75).abs() < 1e-5, "{first:?}");
    }

    /// Regression: #2284 (MAT-D1-NEW-04) — the 6 `BSLightingShaderProperty`
    /// shading scalars (`lighting_effect_1/2`, `subsurface_rolloff`,
    /// `rimlight_power`, `backlight_power`, `fresnel_power`) must survive
    /// the NIFAL parser→`Material` boundary. Pre-fix they were captured on
    /// `ImportedMaterial` at import time but `translate_material` had no
    /// `Material` field to copy them into, so they silently dead-ended
    /// here regardless of what the source NIF authored.
    #[test]
    fn translate_material_copies_bslsp_shading_scalars() {
        let source = ImportedMaterial {
            lighting_effect_1: 0.25,
            lighting_effect_2: 0.40,
            subsurface_rolloff: 0.35,
            rimlight_power: 2.50,
            backlight_power: 1.75,
            fresnel_power: 3.5,
            ..ImportedMaterial::default()
        };
        let paths = ResolvedPaths {
            textures: MaterialTextureSet::default(),
            material_path: None,
        };
        let material = translate_material(&source, None, paths, 0);

        assert_eq!(material.lighting_effect_1, 0.25);
        assert_eq!(material.lighting_effect_2, 0.40);
        assert_eq!(material.subsurface_rolloff, 0.35);
        assert_eq!(material.rimlight_power, 2.50);
        assert_eq!(material.backlight_power, 1.75);
        assert_eq!(material.fresnel_power, 3.5);
    }

    /// Regression: #2443 (MAT-D3-01) — the palette-remap strength must
    /// survive the boundary. Both producers (the inline BSVER>=130 shader
    /// block and the BGSM/BGEM merge) captured it on `ImportedMaterial`, but
    /// `translate_material` dropped it because `Material` had no such field —
    /// the same shape as #2284 above, one tier earlier. Because
    /// `EFFECT_PALETTE_COLOR`/`ALPHA` is a replace rather than a blend, an
    /// authored 0.5 that should soften a shared greyscale ramp rendered as
    /// the full palette colour.
    #[test]
    fn translate_material_copies_grayscale_to_palette_scale() {
        let source = ImportedMaterial {
            grayscale_to_palette_scale: 0.5,
            ..ImportedMaterial::default()
        };
        let paths = ResolvedPaths {
            textures: MaterialTextureSet::default(),
            material_path: None,
        };
        let material = translate_material(&source, None, paths, 0);
        assert_eq!(material.grayscale_to_palette_scale, 0.5);

        // The format default must also arrive verbatim rather than being
        // re-derived — 1.0 is "full-strength remap", the pre-fix behaviour.
        let default_material = translate_material(
            &ImportedMaterial::default(),
            None,
            ResolvedPaths {
                textures: MaterialTextureSet::default(),
                material_path: None,
            },
            0,
        );
        assert_eq!(default_material.grayscale_to_palette_scale, 1.0);
    }

    /// #2444 (MAT-D3-02) — the texture-path-only boundary helper must
    /// produce classifier-resolved PBR scalars, not the render path's
    /// hardcoded `roughness 0.5` fallback. The three exterior populations
    /// that use it (LAND terrain, terrain LOD, object LOD) have no source
    /// material record, only the texture they sample.
    #[test]
    fn texture_only_material_classifies_from_its_texture_path() {
        // Landscape dirt: no classifier keyword → the matte default that
        // ordinary architecture also lands on. This is the value the
        // hardcoded 0.5 was visibly mismatching at ground/architecture
        // seams.
        let land =
            translate_texture_only_material(Some("textures\\landscape\\dirt02.dds".to_string()));
        assert_eq!(land.roughness, 0.85);
        assert_eq!(land.metalness, 0.0);

        // Same classifier as every other surface — a rock landscape texture
        // resolves through the stone/rock arm, not a terrain-special case.
        let rock =
            translate_texture_only_material(Some("textures\\landscape\\rock01.dds".to_string()));
        assert_eq!(rock.roughness, 0.85);

        // No path at all (unresolved LTEX / untextured LOD atlas) still
        // exits with explicit, in-range canonical scalars — never NaN, which
        // is what the NaN seed would leave if `resolve_pbr` were skipped.
        let untextured = translate_texture_only_material(None);
        assert!(untextured.roughness.is_finite() && untextured.metalness.is_finite());
        assert!((0.04..=1.0).contains(&untextured.roughness));

        // `env_map_scale` stays 0.0 rather than inheriting
        // `Material::default()`'s neutral 1.0. `render/static_meshes.rs`
        // forwards it to `GpuMaterial` as the cubemap reflection strength
        // and read 0.0 for these draws while they had no `Material` at all,
        // so a 1.0 here would turn distant terrain and LOD imposters
        // reflective as a side effect of a PBR-scalar fix.
        for m in [&land, &rock, &untextured] {
            assert_eq!(m.env_map_scale, 0.0);
        }
    }

    /// #2444 (MAT-D3-02) — every exterior draw population must spawn with a
    /// canonical `Material`, produced at this boundary. The spawners need a
    /// live `VulkanContext` (mesh upload + texture resolve), so this pins the
    /// wiring at the source level: what regressed before was not a wrong
    /// value but a missing `world.insert`, which silently rerouted the draw
    /// into `render/static_meshes.rs`'s hardcoded-literal arm.
    #[test]
    fn every_exterior_spawner_inserts_a_boundary_material() {
        // The three the audit named have no source material record, so they
        // use the texture-path-only helper. `placement_lod` (Oblivion
        // `_far.nif`) is the sibling the audit missed — same missing-`Material`
        // shape, but its sub-meshes DO carry an `ImportedMaterial`, so it goes
        // through the full boundary instead.
        for (name, src, boundary_fn) in [
            (
                "cell_loader/terrain.rs",
                include_str!("cell_loader/terrain.rs"),
                "translate_texture_only_material(",
            ),
            (
                "cell_loader/terrain_lod.rs",
                include_str!("cell_loader/terrain_lod.rs"),
                "translate_texture_only_material(",
            ),
            (
                "cell_loader/object_lod.rs",
                include_str!("cell_loader/object_lod.rs"),
                "translate_texture_only_material(",
            ),
            (
                "cell_loader/placement_lod.rs",
                include_str!("cell_loader/placement_lod.rs"),
                "translate_material(",
            ),
        ] {
            assert!(
                src.contains(boundary_fn),
                "{name}: exterior draws must get their canonical `Material` from the \
                 translation boundary (`{boundary_fn}`). Without one they fall into the \
                 render path's no-`Material` arm and shade against hardcoded literals — \
                 a second materialization site outside the single source of truth (#2444)."
            );
        }
    }

    /// #2445 (MAT-D3-03) — a `Material`-less entity must not pass the
    /// render-side gloss-slot gate. The spawn-side write-back early-returns
    /// for such an entity, so if the render side binds anyway the two halves
    /// of a pair documented as "cannot diverge" have done exactly that.
    #[test]
    fn normal_alpha_spec_binding_requires_a_material() {
        // Inputs that satisfy the shared predicate outright: lit kind, a
        // normal map bound, no dedicated gloss map, alpha-bearing normal.
        const KIND: u32 = 0;
        const NORMAL: u32 = 7;
        const GLOSS: u32 = 0;

        let material = Material {
            env_map_scale: 0.0,
            ..Material::default()
        };
        assert!(
            normal_alpha_spec_binding_applies(Some(&material), true, KIND, 0.0, NORMAL, GLOSS),
            "the population this binding exists for must still bind"
        );
        assert!(
            !normal_alpha_spec_binding_applies(None, true, KIND, 0.0, NORMAL, GLOSS),
            "a Material-less draw must not bind the gloss slot — its spawn-side \
             roughness counterpart never ran (#2445)"
        );

        // The pre-existing gates are untouched by the new guard.
        assert!(
            !normal_alpha_spec_binding_applies(Some(&material), false, KIND, 0.0, NORMAL, GLOSS),
            "an alpha-less normal carries no per-pixel spec mask"
        );
        assert!(
            !normal_alpha_spec_binding_applies(Some(&material), true, 100, 0.0, NORMAL, GLOSS),
            "glass/effect kinds own their optics"
        );
        assert!(
            !normal_alpha_spec_binding_applies(Some(&material), true, KIND, 0.0, NORMAL, 9),
            "a dedicated gloss map wins over the normal-alpha fallback"
        );
    }

    /// #2826 (REN-D19-02) — `MAT_FLAG_MSN_HAS_AUTHORED_Z` must only be
    /// asserted when the material is model-space-normal-mapped AND the
    /// bound normal map's format carries alpha (the BC3 signal for a
    /// three-channel `_msn`). Neither condition alone is sufficient: a
    /// tangent-space alpha-carrying normal map (BC3 `_n`, not `_msn`)
    /// must not set it, and a model-space map without alpha (BC1
    /// `_msn`, the genuinely two-channel case) must keep reconstructing.
    #[test]
    fn msn_authored_z_requires_both_model_space_and_alpha() {
        assert!(msn_has_authored_z(true, true));
        assert!(!msn_has_authored_z(true, false));
        assert!(!msn_has_authored_z(false, true));
        assert!(!msn_has_authored_z(false, false));
    }

    /// #3198 — before the shared table existed, the CELL-side token set was
    /// Skyrim vocabulary only (`rapid` / `waterfall` / `falls` / `river` /
    /// `stream`), and **all 78 vanilla `FalloutNV.esm` WATR records classified
    /// `Calm`**: FNV names its moving water `Creek*`. With `NAM0` and `NAM5`
    /// absent from the FO3/FNV record set entirely, no signal could fire, so
    /// `WaterFlow`, `foam_strength` and the flow-biased scroll — the whole
    /// WATAL current half — were unreachable on the reference title.
    #[test]
    fn fnv_creek_records_classify_as_river_on_both_producers() {
        for edid in [
            "CreekWater01",
            "CreekWater02nv",
            "CreekWater02AVGnv",
            "CreekWater02nvbetter",
            "RockCreekEstatesWater",
        ] {
            assert_eq!(
                water_kind_from_cell_record_name(edid),
                WaterKind::River,
                "CELL producer: {edid}"
            );
            assert_eq!(
                water_kind_from_mesh_name(Some(edid)).0,
                WaterKind::River,
                "mesh producer: {edid}"
            );
        }
    }

    /// The arm must not be able to go dark again: at least one record from the
    /// vanilla FNV roster has to classify non-`Calm`.
    #[test]
    fn some_vanilla_fnv_water_record_classifies_as_flowing() {
        // Verbatim EditorIDs from a byte-level GRUP walk of FalloutNV.esm.
        let roster = [
            "NVCleanWater",
            "WaterTypeUtility",
            "PPurityWater01Murky",
            "CreekWater02AVGnv",
            "Potomac",
            "ToxicSpillPuddle",
            "TenPenWaterFountain",
            "WaterTypeIrradiated",
        ];
        assert!(
            roster
                .iter()
                .any(|edid| water_kind_from_cell_record_name(edid) != WaterKind::Calm),
            "no vanilla FNV WATR EditorID classifies as flowing — the WATAL \
             current half is unreachable on the reference title again (#3198)"
        );
    }

    /// Tokens considered and rejected against the same census. These must stay
    /// `Calm`: promoting standing water is the failure mode the conservative
    /// default exists to prevent.
    #[test]
    fn rejected_tokens_stay_calm() {
        for edid in [
            "ToxicSpillPuddle",          // a puddle, not a spillway
            "WaterTypeQuantumColaSpill", // ditto
            "TenPenWaterFountain",       // a basin
            "VStripULFountain",
            "Potomac", // WRLD NAM2 default for 10 worldspaces
            "PotomacNRShallow",
            "WaterTypeUtility",
            "PPurityWater01Murky",
        ] {
            assert_eq!(
                water_kind_from_cell_record_name(edid),
                WaterKind::Calm,
                "{edid}"
            );
        }
    }

    /// The two producers disagreed on `canal` while they were two tables
    /// (#3154). Any token must now resolve identically on both, except for the
    /// deliberate `Waterfall` → `River` demotion the CELL path applies because
    /// its planes are always horizontal.
    #[test]
    fn both_producers_share_one_token_table() {
        for name in [
            "canal",
            "creek",
            "river",
            "stream",
            "rapids",
            "MyCanalWater",
            "DefaultCreekWater",
        ] {
            assert_eq!(
                water_kind_from_cell_record_name(name),
                water_kind_from_mesh_name(Some(name)).0,
                "{name}"
            );
        }
        // The one intended divergence.
        assert_eq!(
            water_kind_from_mesh_name(Some("WaterfallSheet01")).0,
            WaterKind::Waterfall
        );
        assert_eq!(
            water_kind_from_cell_record_name("DLC2WaterFallingStream"),
            WaterKind::River
        );
    }
}

/// #2214 (NIFAL-D9-02) — canonical-tier completeness harness.
///
/// `crates/nif/tests/translation_completeness.rs` walks real per-game
/// corpora but only ever constructs `ImportedMesh`/`ImportedMaterial` (the
/// RAW, pre-boundary tier) — `translate_material`, the actual NIFAL
/// parser→canonical boundary this whole abstraction layer exists to
/// enforce, lives in `byroredux` and is never called from `crates/nif` at
/// all. That's a crate-graph constraint, not an oversight: `crates/nif`
/// sits below `byroredux` in the dependency graph, so a raw-tier harness
/// there physically cannot reach up to call this crate's function. A
/// translation regression here — a `Material` field silently stopping
/// receiving its `ImportedMaterial` source value — would sail through the
/// raw-tier harness untouched (it never inspects `Material` at all) and
/// through every other test in this file (each pins one specific field or
/// finding, not the boundary as a whole).
///
/// This harness is that missing canonical-tier check, kept in the same
/// crate specifically because that is the only place `translate_material`
/// (`pub(crate)`) is callable at all — `byroredux` has no `[lib]` target,
/// so an external `byroredux/tests/*.rs` integration test cannot see it
/// either (confirmed: every existing file under `byroredux/tests/` only
/// imports OTHER workspace crates, e.g. `byroredux_nif`/`byroredux_core`,
/// never `byroredux` itself).
///
/// Scope: material only, matching the boundary that actually exists today
/// (per `docs/engine/nifal.md`'s rollout order — collision/animation have
/// no `translate_*` boundary yet to test). Extend this module alongside
/// each new canonical boundary as NIFAL's rollout reaches it.
///
/// Every assertion reads `Material` (canonical), never `ImportedMaterial`
/// fill rates — the #2214 complaint about the raw-tier harness.
#[cfg(test)]
mod canonical_completeness_harness {
    use super::*;
    use byroredux_core::ecs::components::material::EmissiveSource;
    use byroredux_nif::import::{BsEffectShaderData, MaterialTextureSet, NoLightingFalloff};

    /// One `ImportedMaterial` with every field the translation boundary
    /// copies set to a distinctive, non-default value, so a dropped
    /// field is a wrong-value assertion failure, not a false-pass against
    /// an already-zero default. Deliberately NOT glass/decal/effect-carrier
    /// (`material_kind = 0`, `metalness_override = Some(0.42)` which is
    /// `>= 0.3`) so `classify_glass_into_material` is a no-op and every
    /// field below survives `translate_material` untouched — this harness
    /// tests the copy boundary, not the glass/PBR classifiers (those have
    /// their own dedicated tests elsewhere in this file and in
    /// `crates/core`).
    fn kitchen_sink_source() -> ImportedMaterial {
        ImportedMaterial {
            emissive_color: [0.11, 0.22, 0.33],
            emissive_mult: 1.5,
            emissive_source: EmissiveSource::Lighting,
            specular_color: [0.44, 0.55, 0.66],
            specular_strength: 2.5,
            diffuse_color: [0.77, 0.88, 0.99],
            ambient_color: [0.12, 0.34, 0.56],
            glossiness: 62.0,
            uv_offset: [0.1, 0.2],
            uv_scale: [1.3, 1.4],
            mat_alpha: 0.65,
            env_map_scale: 0.2, // < 0.3 so it can't itself gate anything below
            vertex_color_mode: 1,
            alpha_test: true,
            alpha_threshold: 0.72,
            alpha_test_func: 4,
            material_kind: 0,
            wireframe: true,
            flat_shading: true,
            z_test: false,
            z_write: false,
            z_function: 2,
            metalness_override: Some(0.42), // >= 0.3: keeps classify_glass_into_material a no-op
            roughness_override: Some(0.58), // in-range: resolve_pbr's clamp is a no-op
            glass_fresnel_color: [0.17, 0.27, 0.37],
            glass_refraction_scale: 0.07,
            glass_blur_scale: 0.33,
            glass_blur_scale_factor: 1.25,
            translucency_subsurface_color: [0.21, 0.22, 0.23],
            translucency_transmissive_scale: 0.31,
            translucency_turbulence: 0.41,
            lighting_effect_1: 0.51,
            lighting_effect_2: 0.61,
            subsurface_rolloff: 0.71,
            rimlight_power: 0.81,
            backlight_power: 0.91,
            fresnel_power: 4.5,
            soft_lighting: true,
            rim_lighting: true,
            back_lighting: true,
            // #2571 — non-default values so the round-trip assertion below
            // actually exercises the copy (the struct default is 0/6/7).
            texture_clamp_mode: 1, // CLAMP_S_WRAP_T
            src_blend_mode: 2,     // SRC_COLOR
            dst_blend_mode: 3,     // INV_SRC_COLOR
            no_lighting_falloff: Some(NoLightingFalloff {
                start_angle: 0.1,
                stop_angle: 0.9,
                start_opacity: 0.2,
                stop_opacity: 0.8,
            }),
            shader_type_fields: byroredux_core::ecs::components::material::ShaderTypeFields {
                eye_cubemap_scale: Some(1.23),
                ..Default::default()
            },
            ..ImportedMaterial::default()
        }
    }

    fn kitchen_sink_paths() -> ResolvedPaths {
        ResolvedPaths {
            textures: MaterialTextureSet {
                base_color: Some("Textures/Test/diffuse.dds".to_string()),
                normal: Some("Textures/Test/normal.dds".to_string()),
                emissive: Some("Textures/Test/glow.dds".to_string()),
                detail: Some("Textures/Test/detail.dds".to_string()),
                smooth_spec: Some("Textures/Test/gloss.dds".to_string()),
                dark: Some("Textures/Test/dark.dds".to_string()),
                greyscale_lut: Some("Textures/Test/lut.dds".to_string()),
                ..MaterialTextureSet::default()
            },
            material_path: Some("Materials/Test/test.bgsm".to_string()),
        }
    }

    /// The core regression: every canonical-tier field the boundary is
    /// documented to copy must carry its source value through unchanged.
    /// Deliberately reverting any single `source.X` → `material.X` line in
    /// `translate_material` fails exactly the corresponding assertion
    /// below — this is the "fails on a deliberately reintroduced boundary
    /// drop" contract #2214 asked for.
    #[test]
    fn translate_material_copies_every_canonical_field() {
        let source = kitchen_sink_source();
        let material = translate_material(&source, Some("TestMesh"), kitchen_sink_paths(), 0);

        assert_eq!(material.emissive_color, [0.11, 0.22, 0.33]);
        assert_eq!(material.emissive_mult, 1.5);
        assert_eq!(material.emissive_source, EmissiveSource::Lighting);
        assert_eq!(material.specular_color, [0.44, 0.55, 0.66]);
        assert_eq!(material.specular_strength, 2.5);
        assert_eq!(material.diffuse_color, [0.77, 0.88, 0.99]);
        assert_eq!(material.ambient_color, [0.12, 0.34, 0.56]);
        assert_eq!(material.glossiness, 62.0);
        assert_eq!(material.uv_offset, [0.1, 0.2]);
        assert_eq!(material.uv_scale, [1.3, 1.4]);
        assert_eq!(material.alpha, 0.65, "Material::alpha ← source.mat_alpha");
        assert_eq!(material.env_map_scale, 0.2);
        assert_eq!(material.vertex_color_mode, 1);
        assert!(material.alpha_test);
        assert_eq!(material.alpha_threshold, 0.72);
        assert_eq!(material.alpha_test_func, 4);
        assert_eq!(material.material_kind, 0);
        assert!(material.wireframe);
        assert!(material.flat_shading);
        assert!(!material.z_test);
        assert!(!material.z_write);
        assert_eq!(material.z_function, 2);
        assert_eq!(material.metalness, 0.42);
        assert_eq!(material.roughness, 0.58);
        assert_eq!(material.glass_fresnel_color, [0.17, 0.27, 0.37]);
        assert_eq!(material.glass_refraction_scale, 0.07);
        assert_eq!(material.glass_blur_scale, 0.33);
        assert_eq!(material.glass_blur_scale_factor, 1.25);
        assert_eq!(material.translucency_subsurface_color, [0.21, 0.22, 0.23]);
        assert_eq!(material.translucency_transmissive_scale, 0.31);
        assert_eq!(material.translucency_turbulence, 0.41);
        assert_eq!(material.lighting_effect_1, 0.51);
        assert_eq!(material.lighting_effect_2, 0.61);
        assert_eq!(material.subsurface_rolloff, 0.71);
        assert_eq!(material.rimlight_power, 0.81);
        assert_eq!(material.backlight_power, 0.91);
        assert_eq!(material.fresnel_power, 4.5);
        assert!(material.soft_lighting);
        assert!(material.rim_lighting);
        assert!(material.back_lighting);
        // #2571 (OBL-D5-01)
        assert_eq!(material.texture_clamp_mode, 1);
        assert_eq!(material.src_blend_mode, 2);
        assert_eq!(material.dst_blend_mode, 3);

        // Texture handles.
        assert_eq!(
            material.texture_path.as_deref(),
            Some("Textures/Test/diffuse.dds")
        );
        assert_eq!(
            material.normal_map.as_deref(),
            Some("Textures/Test/normal.dds")
        );
        assert_eq!(material.glow_map.as_deref(), Some("Textures/Test/glow.dds"));
        assert_eq!(
            material.detail_map.as_deref(),
            Some("Textures/Test/detail.dds")
        );
        assert_eq!(
            material.gloss_map.as_deref(),
            Some("Textures/Test/gloss.dds")
        );
        assert_eq!(material.dark_map.as_deref(), Some("Textures/Test/dark.dds"));
        assert_eq!(
            material.greyscale_texture.as_deref(),
            Some("Textures/Test/lut.dds")
        );
        assert_eq!(
            material.material_path.as_deref(),
            Some("Materials/Test/test.bgsm")
        );

        // Falloff cone — `no_lighting_falloff` fallback arm (no
        // `effect_shader` on this fixture, so `.or_else` must be reached).
        let falloff = material
            .effect_falloff
            .expect("no_lighting_falloff must translate to Some(EffectFalloff)");
        assert_eq!(falloff.start_angle, 0.1);
        assert_eq!(falloff.stop_angle, 0.9);
        assert_eq!(falloff.start_opacity, 0.2);
        assert_eq!(falloff.stop_opacity, 0.8);

        // Non-empty ShaderTypeFields must survive as `Some(Box<..>)`, not
        // silently dropped to `None`.
        let stf = material
            .shader_type_fields
            .expect("non-empty ShaderTypeFields must translate to Some");
        assert_eq!(stf.eye_cubemap_scale, Some(1.23));
    }

    /// A `BsEffectShaderData` falloff must win over `no_lighting_falloff`
    /// when both are present — pins the `.or_else` precedence order in
    /// `translate_material` (`effect_shader` first) rather than just its
    /// presence.
    #[test]
    fn effect_shader_falloff_takes_precedence_over_no_lighting_falloff() {
        let source = ImportedMaterial {
            effect_shader: Some(BsEffectShaderData {
                falloff_start_angle: 0.15,
                falloff_stop_angle: 0.85,
                falloff_start_opacity: 0.25,
                falloff_stop_opacity: 0.75,
                soft_falloff_depth: 0.05,
                ..Default::default()
            }),
            no_lighting_falloff: Some(NoLightingFalloff {
                start_angle: 0.0,
                stop_angle: 1.0,
                start_opacity: 0.0,
                stop_opacity: 1.0,
            }),
            ..ImportedMaterial::default()
        };
        let paths = ResolvedPaths {
            textures: MaterialTextureSet::default(),
            material_path: None,
        };
        let material = translate_material(&source, None, paths, 0);

        let falloff = material.effect_falloff.expect("must translate to Some");
        assert_eq!(falloff.start_angle, 0.15, "effect_shader falloff must win");
        assert_eq!(falloff.soft_falloff_depth, 0.05);
    }

    /// An empty `ShaderTypeFields` (the common case — no shader variant
    /// authored an additional payload) must translate to `None`, not
    /// `Some(Box::new(Default))`, so downstream `Option` checks stay a
    /// cheap `is_some()` rather than needing to inspect contents.
    #[test]
    fn empty_shader_type_fields_translates_to_none() {
        let source = ImportedMaterial::default();
        let paths = ResolvedPaths {
            textures: MaterialTextureSet::default(),
            material_path: None,
        };
        let material = translate_material(&source, None, paths, 0);
        assert!(material.shader_type_fields.is_none());
    }
}
