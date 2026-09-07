//! Unit tests for `GpuMaterial` / `MaterialTable` — dedup hashing, the
//! `std430` layout pin, preset construction, and material-table interning.
//! Extracted from `material.rs` (#2257) to keep the production code from
//! carrying its own ~900-line test module inline, mirroring the
//! `texture_registry.rs` / `texture_registry_tests.rs` split already
//! established in this directory.

/// #2515 — `Material::alpha` reaches `GpuMaterial.material_alpha` and is
/// hashed by `hash_gpu_material_fields`, so it is part of the dedup
/// identity `MaterialTable::intern_by_hash` keys on. The Cornell
/// harness's `glass()` doc comment used to call the value "currently
/// unconsumed downstream", which is true only of `taa.comp` /
/// `composite.frag`; read as "inert" it invites a future edit that
/// silently splits or merges material-table slots. Pin the fact.
#[test]
fn material_alpha_participates_in_the_dedup_hash() {
    let mut a = GpuMaterial {
        material_alpha: 1.0,
        ..Default::default()
    };
    let opaque = super::hash_gpu_material_fields(&a);
    a.material_alpha = 0.25; // the Cornell glass() probe value
    let glassy = super::hash_gpu_material_fields(&a);
    assert_ne!(
        opaque, glassy,
        "material_alpha must change the dedup hash — two otherwise \
         identical materials differing only in alpha have to occupy \
         distinct MaterialTable slots"
    );
}

use super::*;

/// Pin the std430 layout. Any growth must be intentional and
/// matched by the shader-side `struct GpuMaterial` declaration in
/// lockstep — same contract as `GpuInstance`.
///
/// Was 272 B until #804 / R1-N4 dropped `avg_albedo_r/g/b` (12 B,
/// no shader read `mat.avgAlbedo*` — caustic_splat.comp + the
/// triangle.frag GI miss path both sample from the per-instance
/// `GpuInstance.avgAlbedo*` copy instead).
///
/// Grew 260 → 280 under #1147 / FO4-D6-003 Phase 2b (+20 B for
/// `translucency_subsurface_r/g/b` + `translucency_transmissive_scale`
/// + `translucency_turbulence`), then 280 → 284 under #1248 (+4 B
///   for `ior`, the per-material refractive index that drives
///   Schlick F0 derivation), then 284 → 296 under #1249 (+12 B for
///   the Disney diffuse lobe — `subsurface` + `sheen` + `sheen_tint`),
///   then 296 → 300 under #1250 (+4 B for `anisotropic`, the GGX
///   ax/ay aspect ratio driver), then 300 → 348 for the twelve common
///   supplemental texture roles, then 348 → 364 under #2221 (+16 B for
///   `shader_color_r/g/b` + `shader_float`, captured for the animated
///   BSShaderProperty color/float sinks but not yet sampled by any
///   shader — same deferred-lane precedent as the three unsampled
///   texture roles), then 364 → 396 for the BGEM v21+ glass optical
///   scalars and two dedicated overlay-map handles, then 396 → 432 for
///   seven Bethesda lighting-response scalars and two translated mask
///   handles. Test name includes
///   the size so a future size
///   shift updates it in lockstep with the assertion.
#[test]
fn gpu_material_size_is_428_bytes() {
    assert_eq!(std::mem::size_of::<GpuMaterial>(), 428);
}

/// `#[repr(C)]` puts no implicit padding between f32/u32 fields,
/// but verify the alignment matches std430 (16 B for vec4).
#[test]
fn gpu_material_alignment_is_4_bytes() {
    // Underlying field alignment is 4 (largest scalar). std430
    // vec4 alignment of 16 comes from the buffer-stride rule, not
    // from the struct declaration itself.
    assert_eq!(std::mem::align_of::<GpuMaterial>(), 4);
}

/// #2712 — pin which supplemental role lanes are actually sampled.
///
/// Three of the sixteen (`lightingMapIndex`, `flowMapIndex`,
/// `wrinkleMapIndex`) are produced, uploaded and hashed but read by no
/// shader — a deliberate deferral that previously lived only in a one-off
/// audit report and had already failed to propagate to a sibling report.
/// This pins it in both directions: a lane silently going dead fails here,
/// and so does implementing one of the three without removing its
/// "captured, not yet shaded" note.
///
/// #3910 (REN-2026-09-05-D7-03) rewrote the guard. Two defects:
///
/// 1. **It covered 9 of the 13 sampled lanes.** The four newest — both
///    glass-optics lanes and the lighting-mask / back-lighting masks — were
///    listed nowhere, so exactly the lanes most likely to move were the
///    least protected. The lane set is now DERIVED from
///    `material::supplemental_texture_slot` and asserted exhaustive against
///    its `COUNT`, so lane 17 cannot reopen the same gap: adding a slot
///    fails this test until it is classified.
/// 2. **It matched bare identifiers.** `water.frag` declares a *local*
///    `uint flowMapIndex` read from its own `WaterParams` SSBO push data,
///    which has nothing to do with `GpuMaterial.flowMapIndex`. A
///    `src.contains("flowMapIndex")` scan across `shaders/` therefore
///    reports flow as sampled when it is not. The needle is now the struct
///    access `mat.<name>`, which is the only way a supplemental lane can
///    actually be read.
///
/// The scan covers every GLSL source that can reach `GpuMaterial`, not just
/// `triangle.frag`: the struct lives in `include/bindings.glsl`, and any
/// `#include`d helper (`include/lighting.glsl` already reads other material
/// fields this way) can sample a lane without `triangle.frag` naming it.
#[cfg(test)]
mod supplemental_lane_guard {
    /// Every GLSL source that could sample a `GpuMaterial` lane. Excludes
    /// `include/bindings.glsl` itself — that is the declaration, not a read.
    const GLSL_SOURCES: &[(&str, &str)] = &[
        ("triangle.frag", include_str!("../../shaders/triangle.frag")),
        ("water.frag", include_str!("../../shaders/water.frag")),
        ("triangle.vert", include_str!("../../shaders/triangle.vert")),
        (
            "groundcover_blade.frag",
            include_str!("../../shaders/groundcover_blade.frag"),
        ),
        (
            "volumetrics_inject.comp",
            include_str!("../../shaders/volumetrics_inject.comp"),
        ),
        (
            "include/lighting.glsl",
            include_str!("../../shaders/include/lighting.glsl"),
        ),
        (
            "include/ray_hit.glsl",
            include_str!("../../shaders/include/ray_hit.glsl"),
        ),
        (
            "include/material_sampling.glsl",
            include_str!("../../shaders/include/material_sampling.glsl"),
        ),
    ];

    /// `(slot constant, GLSL field name, sampled?)` for every supplemental
    /// lane. The slot names are checked against the authoritative
    /// `supplemental_texture_slot` module below, so this table cannot fall
    /// behind it — the failure mode #3910 filed.
    const LANES: &[(&str, &str, bool)] = &[
        ("TINT", "tintMapIndex", true),
        ("INNER_LAYER", "innerLayerMapIndex", true),
        ("SPECULAR", "specularMapIndex", true),
        // Unsampled: lighting-map semantics are undecided for an RT-lit frame.
        ("LIGHTING", "lightingMapIndex", false),
        // Unsampled: needs a settled UV-advection convention.
        ("FLOW", "flowMapIndex", false),
        // Unsampled: needs per-expression weights the animation path does not
        // deliver yet.
        ("WRINKLE", "wrinkleMapIndex", false),
        ("REFLECTANCE", "reflectanceMapIndex", true),
        ("EMITTANCE_GRADIENT", "emittanceGradientMapIndex", true),
        ("DECAL_0", "decalMap0Index", true),
        ("DECAL_1", "decalMap1Index", true),
        ("DECAL_2", "decalMap2Index", true),
        ("DECAL_3", "decalMap3Index", true),
        (
            "GLASS_ROUGHNESS_SCRATCH",
            "glassRoughnessScratchMapIndex",
            true,
        ),
        ("GLASS_DIRT_OVERLAY", "glassDirtOverlayMapIndex", true),
        ("LIGHTING_MASK", "lightingMaskMapIndex", true),
        ("BACK_LIGHTING", "backLightingMapIndex", true),
    ];

    /// The slot constants declared by `material::supplemental_texture_slot`,
    /// in declaration order, excluding `COUNT`.
    fn declared_slots() -> Vec<String> {
        const MATERIAL_RS: &str = include_str!("material.rs");
        let start = MATERIAL_RS
            .find("pub mod supplemental_texture_slot {")
            .expect("the supplemental slot module must still exist");
        let block = &MATERIAL_RS[start..];
        let end = block.find("\n}\n").expect("the slot module must be closed");
        block[..end]
            .lines()
            .filter_map(|line| {
                let decl = line.trim().strip_prefix("pub const ")?;
                let (name, _) = decl.split_once(": usize =")?;
                (name != "COUNT").then(|| name.to_owned())
            })
            .collect()
    }

    /// Whether any GLSL source reads `mat.<field>`.
    fn sampling_sites(field: &str) -> Vec<&'static str> {
        let needle = format!("mat.{field}");
        GLSL_SOURCES
            .iter()
            .filter(|(_, src)| src.contains(&needle))
            .map(|(name, _)| *name)
            .collect()
    }

    /// The gap #3910 filed: the guard's list must be exhaustive against the
    /// supplemental set, not hand-maintained. Adding a slot without
    /// classifying it here fails immediately.
    #[test]
    fn the_lane_table_is_exhaustive_against_the_slot_module() {
        let declared = declared_slots();
        assert_eq!(
            declared.len(),
            crate::vulkan::material::supplemental_texture_slot::COUNT,
            "the slot module's named lanes and COUNT must stay in lockstep",
        );
        let listed: Vec<&str> = LANES.iter().map(|(slot, _, _)| *slot).collect();
        assert_eq!(
            listed, declared,
            "LANES must name every supplemental slot, in slot order. A new lane \
             that is not classified sampled/unsampled here is exactly how #2712's \
             guard silently fell to 9-of-13 coverage (#3910)",
        );
    }

    #[test]
    fn every_supplemental_lane_matches_its_declared_sampling_state() {
        for (slot, field, sampled) in LANES {
            let sites = sampling_sites(field);
            if *sampled {
                assert!(
                    !sites.is_empty(),
                    "slot::{slot} (`mat.{field}`) is a wired supplemental role, but \
                     no shader reads it any more — the lane is now produced, \
                     uploaded and hashed for nothing (#2712 / #3910)"
                );
            } else {
                assert!(
                    sites.is_empty(),
                    "slot::{slot} (`mat.{field}`) is now sampled by {sites:?} — good, \
                     but the deferral notes on GpuMaterial and in \
                     include/bindings.glsl say it is not. Remove them together with \
                     this lane's `false` (#2712 / #3910)"
                );
            }
        }
    }

    /// The needle must be the struct access, never the bare identifier.
    /// `water.frag` declares a local `uint flowMapIndex` from its own
    /// `WaterParams` push data; a bare-identifier scan reports
    /// `GpuMaterial.flowMapIndex` as sampled on the strength of a shader that
    /// never touches the material table. Pin the collision so nobody
    /// "simplifies" the needle back (#3910).
    #[test]
    fn the_bare_identifier_scan_would_have_been_wrong_about_flow() {
        let water = GLSL_SOURCES
            .iter()
            .find(|(name, _)| *name == "water.frag")
            .expect("water.frag must still be scanned")
            .1;
        assert!(
            water.contains("flowMapIndex"),
            "fixture precondition: water.frag still declares its own local \
             flowMapIndex — if it stopped, this test documents a collision that \
             no longer exists and can go (#3910)"
        );
        assert!(
            !water.contains("mat.flowMapIndex"),
            "water.frag must not read the GpuMaterial flow lane — its own \
             flowMapIndex comes from the WaterParams SSBO (#3910)"
        );
    }

    /// #3908 (REN-2026-09-05-D6-01) — the seven canonical `Material` fields
    /// whose docs claimed "captured, not yet shaded" long after the
    /// 2026-08-25 GPU follow-up wired them. Pin both halves: the shader still
    /// reads each one, and the canonical docs no longer say otherwise.
    #[test]
    fn the_2284_shading_scalars_are_shaded_and_their_docs_say_so() {
        for field in [
            "lightingEffect1",
            "lightingEffect2",
            "subsurfaceRolloff",
            "rimlightPower",
            "backlightPower",
            "fresnelPower",
            "grayscaleToPaletteScale",
        ] {
            assert!(
                !sampling_sites(field).is_empty(),
                "`mat.{field}` is documented as shaded but no GLSL source reads \
                 it — either the consumer was removed or the canonical doc in \
                 core's Material needs its deferral note back (#3908)"
            );
        }

        const CORE_MATERIAL_RS: &str = include_str!("../../../core/src/ecs/components/material.rs");
        // Scoped past this file's own quoting of the phrase — the needle is
        // reproduced verbatim in the doc comments that record the fix.
        for stale in [
            "Landed here (captured, not yet shaded)",
            "Captured here, not yet shaded",
        ] {
            assert!(
                !CORE_MATERIAL_RS.contains(stale),
                "core's Material still tells a reader that a shaded field is \
                 unshaded: {stale:?}. That hides real progress and invites \
                 redundant re-plumbing (#3908)"
            );
        }
    }
}

/// Regression guard for `GpuMaterial` GLSL field names —
/// REN-D14-NEW-02 (audit 2026-05-09). The offset pin
/// (`gpu_material_field_offsets_match_shader_contract`) and the
/// size pin (`gpu_material_size_is_428_bytes`) catch byte-level
/// drift, but neither catches a GLSL-side field rename: the
/// shader still reads from the same offset, the value still
/// arrives in the right register, but the field's MEANING in
/// the source no longer matches the Rust struct. A future
/// reader chasing a "what does `mat.foo` mean?" question hits a
/// dead end.
///
/// This test asserts that every documented GLSL field name on
/// the shader-side `struct GpuMaterial` declaration in
/// `include/bindings.glsl` is present in the file. Renaming the
/// Rust field is fine; renaming the GLSL field fails this test
/// and forces an audit of every reader downstream. (The struct
/// was lifted out of `triangle.frag` into the shared
/// `include/bindings.glsl` under #1583/#1590 — `triangle.frag`
/// now `#include`s it.)
#[test]
fn gpu_material_glsl_field_names_pinned() {
    let src = include_str!("../../shaders/include/bindings.glsl");
    // Authoritative list — every named field declared inside
    // `struct GpuMaterial { ... };` in `include/bindings.glsl`.
    // Update both sites together when renaming a field on the
    // GLSL side; the Rust-side rename + this list keep the
    // contract bidirectional. The trailing `;` in the needle
    // disambiguates field declarations from incidental uses of
    // the same identifier in comments / other structs.
    for name in &[
        "roughness;",
        "metalness;",
        "emissiveMult;",
        "materialFlags;",
        "emissiveR,",
        "emissiveG,",
        "emissiveB,",
        "specularStrength;",
        "specularR,",
        "specularG,",
        "specularB,",
        "alphaThreshold;",
        // #3909 — `textureIndex,` used to lead this group; removed as an
        // unsampled lane, so the needle goes with it.
        "normalMapIndex,",
        "darkMapIndex,",
        "glowMapIndex;",
        "detailMapIndex,",
        "glossMapIndex,",
        "parallaxMapIndex,",
        "envMapIndex;",
        "envMaskIndex,",
        "alphaTestFunc,",
        "materialKind;",
        "materialAlpha;",
        "parallaxHeightScale,",
        "parallaxMaxPasses,",
        "uvOffsetU,",
        "uvOffsetV;",
        "uvScaleU,",
        "uvScaleV,",
        "diffuseR,",
        "diffuseG;",
        "diffuseB,",
        "ambientR,",
        "ambientG,",
        "ambientB;",
        "skinTintA,",
        "skinTintR,",
        "skinTintG,",
        "skinTintB;",
        "hairTintR,",
        "hairTintG,",
        "hairTintB,",
        "multiLayerEnvmapStrength;",
        "eyeLeftCenterX,",
        "eyeLeftCenterY,",
        "eyeLeftCenterZ,",
        "eyeCubemapScale;",
        "eyeRightCenterX,",
        "eyeRightCenterY,",
        "eyeRightCenterZ,",
        "multiLayerInnerThickness;",
        "multiLayerRefractionScale,",
        "multiLayerInnerScaleU,",
        "multiLayerInnerScaleV,",
        "sparkleR;",
        "sparkleG,",
        "sparkleB,",
        "sparkleIntensity,",
        "falloffStartAngle;",
        "falloffStopAngle,",
        "falloffStartOpacity,",
        "falloffStopOpacity,",
        "softFalloffDepth;",
        "greyscaleLutIndex;",
        // #1147 Phase 2b — BGSM translucency suite
        "translucencySubsurfaceR,",
        "translucencySubsurfaceG,",
        "translucencySubsurfaceB;",
        "translucencyTransmissiveScale;",
        "translucencyTurbulence;",
        // #1248 — per-material refractive index for Schlick F0
        "ior;",
        // #1249 — Disney diffuse lobe (subsurface + sheen + sheenTint)
        "subsurface;",
        "sheen;",
        "sheenTint;",
        // #1250 — anisotropic GGX ax/ay driver
        "anisotropic;",
        // Common supplemental semantic texture roles
        "tintMapIndex;",
        "innerLayerMapIndex;",
        "specularMapIndex;",
        "lightingMapIndex;",
        "flowMapIndex;",
        "wrinkleMapIndex;",
        "reflectanceMapIndex;",
        "emittanceGradientMapIndex;",
        "decalMap0Index;",
        "decalMap1Index;",
        "decalMap2Index;",
        "decalMap3Index;",
        // #2221 — animated BSShaderProperty color/float, unsampled
        "shaderColorR,",
        "shaderColorG,",
        "shaderColorB;",
        "shaderFloat;",
        "glassFresnelR,",
        "glassFresnelG,",
        "glassFresnelB;",
        "glassRefractionScale;",
        "glassBlurScale;",
        "glassBlurScaleFactor;",
        "glassRoughnessScratchMapIndex;",
        "glassDirtOverlayMapIndex;",
        "lightingEffect1;",
        "lightingEffect2;",
        "subsurfaceRolloff;",
        "rimlightPower;",
        "backlightPower;",
        "fresnelPower;",
        "grayscaleToPaletteScale;",
        "lightingMaskMapIndex;",
        "backLightingMapIndex;",
    ] {
        assert!(
            src.contains(name),
            "include/bindings.glsl: expected GpuMaterial GLSL field needle `{}` not found. \
             If you renamed a field, update both the GLSL source and this list.",
            name
        );
    }
}

/// Regression guard for the GpuMaterial Shader Struct Sync (#806).
/// The size pin (`gpu_material_size_is_428_bytes`) catches additions
/// or removals; this catches reorderings within the record that the
/// size pin alone would miss — e.g. swapping
/// `normal_map_index` and `dark_map_index` within vec4 #4 would
/// preserve total size but produce wrong shader reads.
///
/// Mirrors the `gpu_instance_field_offsets_match_shader_contract`
/// pattern (`scene_buffer/gpu_instance_layout_tests.rs`). The shader-side
/// `struct GpuMaterial` declaration in
/// `crates/renderer/shaders/include/bindings.glsl` (lifted out of
/// `triangle.frag` under #1583/#1590 — it now `#include`s it) is the
/// source of truth for these offsets — every named field on the
/// Rust side gets an explicit `offset_of!` assertion against the
/// vec4 group its shader-side counterpart sits in. The GLSL field
/// ORDER is cross-checked against this struct by
/// `gpu_material_glsl_field_order_matches_rust_struct` (#1657).
#[test]
fn gpu_material_field_offsets_match_shader_contract() {
    use std::mem::offset_of;

    // ── PBR scalars (vec4 #1, offsets 0-12) ────────────────────
    assert_eq!(offset_of!(GpuMaterial, roughness), 0);
    assert_eq!(offset_of!(GpuMaterial, metalness), 4);
    assert_eq!(offset_of!(GpuMaterial, emissive_mult), 8);
    assert_eq!(offset_of!(GpuMaterial, material_flags), 12);

    // ── Emissive RGB + specular_strength (vec4 #2, offsets 16-28)
    assert_eq!(offset_of!(GpuMaterial, emissive_r), 16);
    assert_eq!(offset_of!(GpuMaterial, emissive_g), 20);
    assert_eq!(offset_of!(GpuMaterial, emissive_b), 24);
    assert_eq!(offset_of!(GpuMaterial, specular_strength), 28);

    // ── Specular RGB + alpha_threshold (vec4 #3, offsets 32-44) ─
    assert_eq!(offset_of!(GpuMaterial, specular_r), 32);
    assert_eq!(offset_of!(GpuMaterial, specular_g), 36);
    assert_eq!(offset_of!(GpuMaterial, specular_b), 40);
    assert_eq!(offset_of!(GpuMaterial, alpha_threshold), 44);

    // ── Texture indices group A (offsets 48-56) ────────────────
    // #3909 — `texture_index` was removed from the head of this group; it
    // was sampled by no shader and split the dedup key. Everything from
    // here down shifted 4 B toward zero.
    assert_eq!(offset_of!(GpuMaterial, normal_map_index), 48);
    assert_eq!(offset_of!(GpuMaterial, dark_map_index), 52);
    assert_eq!(offset_of!(GpuMaterial, glow_map_index), 56);

    // ── Texture indices group B (vec4 #5, offsets 60-72) ───────
    assert_eq!(offset_of!(GpuMaterial, detail_map_index), 60);
    assert_eq!(offset_of!(GpuMaterial, gloss_map_index), 64);
    assert_eq!(offset_of!(GpuMaterial, parallax_map_index), 68);
    assert_eq!(offset_of!(GpuMaterial, env_map_index), 72);

    // ── env_mask + alpha_test_func + material_kind + alpha
    //    (vec4 #6, offsets 76-88) ───────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, env_mask_index), 76);
    assert_eq!(offset_of!(GpuMaterial, alpha_test_func), 80);
    assert_eq!(offset_of!(GpuMaterial, material_kind), 84);
    assert_eq!(offset_of!(GpuMaterial, material_alpha), 88);

    // ── Parallax POM + UV offset (vec4 #7, offsets 92-104) ─────
    assert_eq!(offset_of!(GpuMaterial, parallax_height_scale), 92);
    assert_eq!(offset_of!(GpuMaterial, parallax_max_passes), 96);
    assert_eq!(offset_of!(GpuMaterial, uv_offset_u), 100);
    assert_eq!(offset_of!(GpuMaterial, uv_offset_v), 104);

    // ── UV scale + diffuse RG (vec4 #8, offsets 108-120) ───────
    assert_eq!(offset_of!(GpuMaterial, uv_scale_u), 108);
    assert_eq!(offset_of!(GpuMaterial, uv_scale_v), 112);
    assert_eq!(offset_of!(GpuMaterial, diffuse_r), 116);
    assert_eq!(offset_of!(GpuMaterial, diffuse_g), 120);

    // ── diffuse_b + ambient RGB (vec4 #9, offsets 124-136) ─────
    assert_eq!(offset_of!(GpuMaterial, diffuse_b), 124);
    assert_eq!(offset_of!(GpuMaterial, ambient_r), 128);
    assert_eq!(offset_of!(GpuMaterial, ambient_g), 132);
    assert_eq!(offset_of!(GpuMaterial, ambient_b), 136);

    // (#804 / R1-N4 dropped `avg_albedo_r/g/b` — what would have
    // been vec4 #10 at offsets 144-152 is gone; subsequent fields
    // shift down by 12 bytes from their pre-#804 positions.)

    // ── skin_tint A/R/G/B (offsets 140-152) ────────────────────
    assert_eq!(offset_of!(GpuMaterial, skin_tint_a), 140);
    assert_eq!(offset_of!(GpuMaterial, skin_tint_r), 144);
    assert_eq!(offset_of!(GpuMaterial, skin_tint_g), 148);
    assert_eq!(offset_of!(GpuMaterial, skin_tint_b), 152);

    // ── hair_tint RGB + multi_layer_envmap_strength
    //    (offsets 156-168) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, hair_tint_r), 156);
    assert_eq!(offset_of!(GpuMaterial, hair_tint_g), 160);
    assert_eq!(offset_of!(GpuMaterial, hair_tint_b), 164);
    assert_eq!(offset_of!(GpuMaterial, multi_layer_envmap_strength), 168);

    // ── eye_left RGB + eye_cubemap_scale (offsets 172-184) ─────
    assert_eq!(offset_of!(GpuMaterial, eye_left_center_x), 172);
    assert_eq!(offset_of!(GpuMaterial, eye_left_center_y), 176);
    assert_eq!(offset_of!(GpuMaterial, eye_left_center_z), 180);
    assert_eq!(offset_of!(GpuMaterial, eye_cubemap_scale), 184);

    // ── eye_right RGB + multi_layer_inner_thickness
    //    (offsets 188-200) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, eye_right_center_x), 188);
    assert_eq!(offset_of!(GpuMaterial, eye_right_center_y), 192);
    assert_eq!(offset_of!(GpuMaterial, eye_right_center_z), 196);
    assert_eq!(offset_of!(GpuMaterial, multi_layer_inner_thickness), 200);

    // ── refraction_scale + multi_layer_inner_scale UV + sparkle_r
    //    (offsets 204-216) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, multi_layer_refraction_scale), 204);
    assert_eq!(offset_of!(GpuMaterial, multi_layer_inner_scale_u), 208);
    assert_eq!(offset_of!(GpuMaterial, multi_layer_inner_scale_v), 212);
    assert_eq!(offset_of!(GpuMaterial, sparkle_r), 216);

    // ── sparkle GB + sparkle_intensity + falloff_start
    //    (offsets 220-232) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, sparkle_g), 220);
    assert_eq!(offset_of!(GpuMaterial, sparkle_b), 224);
    assert_eq!(offset_of!(GpuMaterial, sparkle_intensity), 228);
    assert_eq!(offset_of!(GpuMaterial, falloff_start_angle), 232);

    // ── falloff_stop + opacities + soft_falloff_depth
    //    (offsets 236-248) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, falloff_stop_angle), 236);
    assert_eq!(offset_of!(GpuMaterial, falloff_start_opacity), 240);
    assert_eq!(offset_of!(GpuMaterial, falloff_stop_opacity), 244);
    assert_eq!(offset_of!(GpuMaterial, soft_falloff_depth), 248);

    // ── greyscale palette LUT bindless handle, #890 Stage 2c
    //    (offset 252) ─────────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, greyscale_lut_index), 252);

    // ── BGSM translucency parameter suite, #1147 Phase 2b
    //    (offsets 256-276) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, translucency_subsurface_r), 256);
    assert_eq!(offset_of!(GpuMaterial, translucency_subsurface_g), 260);
    assert_eq!(offset_of!(GpuMaterial, translucency_subsurface_b), 264);
    assert_eq!(
        offset_of!(GpuMaterial, translucency_transmissive_scale),
        268
    );
    assert_eq!(offset_of!(GpuMaterial, translucency_turbulence), 272);

    // ── PBR IOR (#1248, offset 276) ──────────────────────────
    assert_eq!(offset_of!(GpuMaterial, ior), 276);

    // ── Disney diffuse lobe (#1249, offsets 280-288) ──────────
    assert_eq!(offset_of!(GpuMaterial, subsurface), 280);
    assert_eq!(offset_of!(GpuMaterial, sheen), 284);
    assert_eq!(offset_of!(GpuMaterial, sheen_tint), 288);

    // ── Anisotropic GGX (#1250, offset 292) ───────────────────
    assert_eq!(offset_of!(GpuMaterial, anisotropic), 292);
    assert_eq!(offset_of!(GpuMaterial, tint_map_index), 296);
    assert_eq!(offset_of!(GpuMaterial, inner_layer_map_index), 300);
    assert_eq!(offset_of!(GpuMaterial, specular_map_index), 304);
    assert_eq!(offset_of!(GpuMaterial, lighting_map_index), 308);
    assert_eq!(offset_of!(GpuMaterial, flow_map_index), 312);
    assert_eq!(offset_of!(GpuMaterial, wrinkle_map_index), 316);
    assert_eq!(offset_of!(GpuMaterial, reflectance_map_index), 320);
    assert_eq!(offset_of!(GpuMaterial, emittance_gradient_map_index), 324);
    assert_eq!(offset_of!(GpuMaterial, decal_map_0_index), 328);
    assert_eq!(offset_of!(GpuMaterial, decal_map_1_index), 332);
    assert_eq!(offset_of!(GpuMaterial, decal_map_2_index), 336);
    assert_eq!(offset_of!(GpuMaterial, decal_map_3_index), 340);

    // ── Animated BSShaderProperty color/scalar (#2221, offsets 344-356)
    assert_eq!(offset_of!(GpuMaterial, shader_color_r), 344);
    assert_eq!(offset_of!(GpuMaterial, shader_color_g), 348);
    assert_eq!(offset_of!(GpuMaterial, shader_color_b), 352);
    assert_eq!(offset_of!(GpuMaterial, shader_float), 356);
    assert_eq!(offset_of!(GpuMaterial, glass_fresnel_r), 360);
    assert_eq!(offset_of!(GpuMaterial, glass_fresnel_g), 364);
    assert_eq!(offset_of!(GpuMaterial, glass_fresnel_b), 368);
    assert_eq!(offset_of!(GpuMaterial, glass_refraction_scale), 372);
    assert_eq!(offset_of!(GpuMaterial, glass_blur_scale), 376);
    assert_eq!(offset_of!(GpuMaterial, glass_blur_scale_factor), 380);
    assert_eq!(
        offset_of!(GpuMaterial, glass_roughness_scratch_map_index),
        384
    );
    assert_eq!(offset_of!(GpuMaterial, glass_dirt_overlay_map_index), 388);
    assert_eq!(offset_of!(GpuMaterial, lighting_effect_1), 392);
    assert_eq!(offset_of!(GpuMaterial, lighting_effect_2), 396);
    assert_eq!(offset_of!(GpuMaterial, subsurface_rolloff), 400);
    assert_eq!(offset_of!(GpuMaterial, rimlight_power), 404);
    assert_eq!(offset_of!(GpuMaterial, backlight_power), 408);
    assert_eq!(offset_of!(GpuMaterial, fresnel_power), 412);
    assert_eq!(offset_of!(GpuMaterial, grayscale_to_palette_scale), 416);
    assert_eq!(offset_of!(GpuMaterial, lighting_mask_map_index), 420);
    assert_eq!(offset_of!(GpuMaterial, back_lighting_map_index), 424);
}

#[test]
fn default_is_neutral_lit_material() {
    let m = GpuMaterial::default();
    assert_eq!(m.roughness, 0.5);
    assert_eq!(m.metalness, 0.0);
    assert_eq!(m.material_kind, 0);
    assert_eq!(m.material_alpha, 1.0);
    assert_eq!(m.diffuse_r, 1.0);
    assert_eq!(m.uv_scale_u, 1.0);
    assert_eq!(m.parallax_max_passes, 4.0);
    // Identity falloff pass-through.
    assert_eq!(m.falloff_start_angle, 1.0);
    assert_eq!(m.falloff_start_opacity, 1.0);
}

/// #807 — `MaterialTable::new()` reserves slot 0 for the neutral
/// `GpuMaterial::default()` so `material_id == 0` is always a
/// safe-to-read fallback rather than aliasing whichever user
/// material happened to intern first.
#[test]
fn new_seeds_neutral_default_at_slot_zero() {
    let table = MaterialTable::new();
    assert_eq!(table.len(), 1, "slot 0 must be pre-seeded");
    // GpuMaterial has byte-PartialEq but no Debug, so use assert!.
    assert!(
        table.materials()[0] == GpuMaterial::default(),
        "slot 0 must hold the neutral-lit default"
    );
    // No user-driven intern calls yet — telemetry stays honest.
    assert_eq!(table.interned_count(), 0);
}

/// #1032 / REN-D14-NEW-01 — `unique_user_count` excludes the
/// seeded slot 0 so `ctx.scratch` reports actual user-distinct
/// material counts. Pin the contract on the four shapes that
/// matter:
///   * fresh table (no user interns) → 0
///   * one user material → 1 (not 2)
///   * default-only interns (dedup to slot 0) → 0
///   * post-clear → 0
#[test]
fn unique_user_count_excludes_seeded_slot() {
    let mut table = MaterialTable::new();
    assert_eq!(
        table.unique_user_count(),
        0,
        "fresh table has only the seeded neutral; zero user materials"
    );
    assert_eq!(table.len(), 1, "sanity: len() still counts the seeded slot");

    let user = GpuMaterial {
        roughness: 0.7,
        ..Default::default()
    };
    table.intern(user);
    assert_eq!(
        table.unique_user_count(),
        1,
        "one user material — pre-fix `ctx.scratch` reported 2 here"
    );
    assert_eq!(table.len(), 2, "sanity: len() = seeded + 1 user");

    // Interning the default GpuMaterial dedups to slot 0 — it
    // bumps `interned_count` but NOT the user count.
    let mut bare_default_table = MaterialTable::new();
    let _ = bare_default_table.intern(GpuMaterial::default());
    let _ = bare_default_table.intern(GpuMaterial::default());
    assert_eq!(
        bare_default_table.unique_user_count(),
        0,
        "default-only interns dedup to slot 0 — zero distinct user materials"
    );

    table.clear();
    assert_eq!(
        table.unique_user_count(),
        0,
        "clear re-seeds slot 0 only — user count drops to zero"
    );
}

/// #807 — `clear()` re-seeds slot 0 so the per-frame contract
/// (id 0 == neutral default) holds at frame start, not just at
/// engine boot.
#[test]
fn clear_re_seeds_neutral_default() {
    let mut table = MaterialTable::new();
    let user = GpuMaterial {
        roughness: 0.7,
        ..Default::default()
    };
    table.intern(user); // slot 1
    assert_eq!(table.len(), 2);

    table.clear();
    assert_eq!(table.len(), 1, "clear must leave slot 0 seeded");
    assert!(
        table.materials()[0] == GpuMaterial::default(),
        "clear must re-seed the neutral-lit default at slot 0"
    );
    assert_eq!(table.interned_count(), 0);
}

#[test]
fn identical_materials_dedup_to_same_id() {
    let mut table = MaterialTable::new();
    let mat = GpuMaterial::default();
    let id_a = table.intern(mat);
    let id_b = table.intern(mat);
    assert_eq!(id_a, id_b);
    // Slot 0 (neutral default) absorbs both interns — the table
    // already had 1 entry seeded, so len stays at 1. #807.
    assert_eq!(id_a, 0, "default GpuMaterial must dedup to slot 0");
    assert_eq!(table.len(), 1);
}

#[test]
fn distinct_materials_get_distinct_ids() {
    let mut table = MaterialTable::new();
    let a = GpuMaterial::default();
    let b = GpuMaterial {
        roughness: 0.7,
        ..Default::default()
    };

    let id_a = table.intern(a);
    let id_b = table.intern(b);
    assert_ne!(id_a, id_b);
    // `a` dedupes to the seeded slot 0; `b` is distinct → slot 1.
    // Total len = 2 (seeded neutral + one user material). #807.
    assert_eq!(id_a, 0);
    assert_eq!(id_b, 1);
    assert_eq!(table.len(), 2);

    // Repeats still dedup to the original id.
    let a2 = GpuMaterial {
        roughness: 0.5, // same as default
        ..Default::default()
    };
    assert_eq!(table.intern(a2), id_a);
    assert_eq!(table.intern(b), id_b);
    assert_eq!(table.len(), 2);
}

/// Two materials differing in a single texture index (e.g.
/// different normal map on otherwise-identical material) must NOT
/// dedup — they're genuinely distinct on the GPU. Pin this
/// because a buggy hash that drops bits could collapse them and
/// silently swap textures across draws.
///
/// #3909 — this used to vary `texture_index`, which was the one texture
/// lane on this struct that NO shader sampled (the diffuse handle lives on
/// `GpuInstance` by design). It has been removed, so the fixture varies
/// `normal_map_index` instead: a lane that is both hashed and genuinely
/// read, which is what makes the "must not dedup" claim meaningful.
#[test]
fn normal_map_index_difference_is_distinct() {
    let mut table = MaterialTable::new();
    let mut a = GpuMaterial::default();
    let mut b = GpuMaterial::default();
    a.normal_map_index = 7;
    b.normal_map_index = 8;
    assert_ne!(table.intern(a), table.intern(b));
    // Slot 0 = seeded neutral, slot 1 = `a`, slot 2 = `b`. #807.
    assert_eq!(table.len(), 3);
}

/// #890 Stage 2c — two `BSEffectShaderProperty` materials that
/// differ ONLY in their `greyscale_lut_index` MUST dedup to
/// distinct slots. Pre-Stage-2c the field at offset 256 was
/// `_pad_falloff`, intentionally excluded from
/// `hash_gpu_material_fields` (and therefore from
/// `MaterialTable::intern`'s reverse index) because it was always
/// 0.0. Now that the slot carries a real bindless handle, the
/// hash MUST include it — otherwise two fire-effect meshes
/// referencing different palette LUTs (e.g.
/// `GradFireExplosion.dds` vs `GradPlasmaCold.dds`) would collapse
/// to the same `material_id` and the second mesh would sample
/// the wrong LUT.
#[test]
fn greyscale_lut_index_difference_is_distinct() {
    let mut table = MaterialTable::new();
    let mut a = GpuMaterial::default();
    let mut b = GpuMaterial::default();
    a.material_kind = 101; // MATERIAL_KIND_EFFECT_SHADER
    a.material_flags = material_flag::EFFECT_PALETTE_COLOR;
    a.greyscale_lut_index = 42;
    b.material_kind = 101;
    b.material_flags = material_flag::EFFECT_PALETTE_COLOR;
    b.greyscale_lut_index = 43;
    let id_a = table.intern(a);
    let id_b = table.intern(b);
    assert_ne!(
        id_a, id_b,
        "different greyscale_lut_index must NOT dedup — pre-Stage-2c the offset-256 \
         slot was excluded from hash_gpu_material_fields"
    );
    // Sanity: the hash function itself must produce different
    // outputs so the reverse-index lookup splits them.
    assert_ne!(
        hash_gpu_material_fields(&a),
        hash_gpu_material_fields(&b),
        "hash_gpu_material_fields must include greyscale_lut_index"
    );
}

/// Float-bit equality check — two materials whose only difference
/// is a fractional roughness must distinguish, even at very small
/// epsilons. Byte-level eq + hash via `to_bits` semantics.
#[test]
fn small_float_difference_is_distinct() {
    let mut table = MaterialTable::new();
    let mut a = GpuMaterial::default();
    let mut b = GpuMaterial::default();
    a.roughness = 0.500_001;
    b.roughness = 0.500_002;
    assert_ne!(table.intern(a), table.intern(b));
}

#[test]
fn clear_resets_table_but_keeps_capacity() {
    let mut table = MaterialTable::new();
    // Loop interns 10 materials. i=0 hits the seeded neutral slot;
    // i=1..9 each push a fresh slot. Total len = 1 (neutral) + 9
    // (user) = 10. #807.
    for i in 0..10 {
        let m = GpuMaterial {
            normal_map_index: i,
            ..Default::default()
        };
        table.intern(m);
    }
    assert_eq!(table.len(), 10);
    let cap_before = table.materials.capacity();
    table.clear();
    // Post-clear the seeded neutral default is re-pushed (#807),
    // so `len()` is 1 — not 0. The underlying allocation
    // capacity stays at the pre-clear size.
    assert_eq!(table.len(), 1);
    assert!(
        table.materials()[0] == GpuMaterial::default(),
        "post-clear slot 0 must hold the seeded neutral default"
    );
    assert!(table.materials.capacity() >= cap_before);
}

/// #780 / PERF-N1 — `interned_count` ticks on every `intern` call
/// (hits AND misses) so the dedup ratio `len / interned_count` is
/// computable from telemetry. `clear` resets it in lockstep with
/// the materials Vec so the per-frame snapshot is honest.
///
/// Post-#807: `intern(GpuMaterial::default())` is now a HIT on the
/// seeded slot 0 (not a miss as it was pre-fix). `interned_count`
/// still ticks because the producer-side `intern` call rate is
/// unchanged — only the dedup hit/miss accounting shifts.
#[test]
fn interned_count_increments_on_hit_and_miss() {
    let mut table = MaterialTable::new();
    assert_eq!(table.interned_count(), 0);
    // Seed counts as a slot but NOT a producer intern (#807).
    assert_eq!(table.len(), 1);

    let a = GpuMaterial::default();
    let b = GpuMaterial {
        roughness: 0.7,
        ..Default::default()
    };

    table.intern(a); // hit on seeded slot 0
    assert_eq!(table.interned_count(), 1);
    assert_eq!(table.len(), 1);

    table.intern(a); // hit again — count still ticks
    assert_eq!(table.interned_count(), 2);
    assert_eq!(table.len(), 1);

    table.intern(b); // miss → push slot 1
    assert_eq!(table.interned_count(), 3);
    assert_eq!(table.len(), 2);

    // 5 more hits on b — only `interned_count` moves.
    for _ in 0..5 {
        table.intern(b);
    }
    assert_eq!(table.interned_count(), 8);
    assert_eq!(table.len(), 2);

    // Tweaking a fresh local must not retroactively count against
    // the original — byte-equal to default still hits slot 0.
    let a2 = GpuMaterial {
        roughness: 0.5, // same as default
        ..Default::default()
    };
    table.intern(a2);
    assert_eq!(table.interned_count(), 9);
    assert_eq!(table.len(), 2);

    table.clear();
    assert_eq!(table.interned_count(), 0);
    // Post-clear the seeded neutral persists (#807).
    assert_eq!(table.len(), 1);
}

#[test]
fn materials_slice_matches_insertion_order() {
    let mut table = MaterialTable::new();
    let mut mats = [GpuMaterial::default(); 3];
    mats[0].normal_map_index = 100;
    mats[1].normal_map_index = 200;
    mats[2].normal_map_index = 300;
    for m in &mats {
        table.intern(*m);
    }
    let slice = table.materials();
    // Slot 0 is the seeded neutral default (#807); user materials
    // start at slot 1 in insertion order.
    assert_eq!(slice.len(), 4);
    assert!(slice[0] == GpuMaterial::default(), "slot 0 = neutral");
    assert_eq!(slice[1].normal_map_index, 100);
    assert_eq!(slice[2].normal_map_index, 200);
    assert_eq!(slice[3].normal_map_index, 300);
}

/// #797 / SAFE-22 + #807 — over-cap interns return id `0` and
/// share the neutral-default material's record (slot 0 is reserved
/// for the neutral default per #807, which makes the over-cap
/// fallback semantically clean: "use the neutral material" rather
/// than "alias whichever user material happened to intern first").
/// Without this cap a DrawCommand carrying the over-cap id would
/// index past the MaterialBuffer SSBO end on the GPU
/// (implementation-defined OOB read).
///
/// Builds a fresh table, fills it to `MAX_MATERIALS` distinct
/// entries (each varying by `normal_map_index`), then asserts:
///   1. The first `intern` of `normal_map_index = 0` HITS the seeded
///      neutral slot (id 0), and `intern` of `normal_map_index = i`
///      for `i >= 1` pushes a distinct slot at id `i` — total
///      table grows to exactly `MAX_MATERIALS` slots.
///   2. The next over-cap intern returns id `0` (the neutral).
///   3. The reverse-lookup map's count also stays bounded.
///   4. A subsequent intern of an already-interned material
///      still returns its original id — the cap doesn't poison
///      the dedup map.
#[test]
fn intern_overflow_returns_material_zero() {
    let mut table = MaterialTable::new();
    // Fill the table to exactly `MAX_MATERIALS` distinct entries.
    // `normal_map_index` is part of the byte-Hash dedup so each
    // increment produces a fresh GpuMaterial. Lucky alignment:
    // `normal_map_index = i` lands at slot `i` because the seeded
    // neutral has `normal_map_index = 0`, and `intern` of i=0 hits
    // it. Subsequent i=1..MAX_MATERIALS-1 each push a fresh slot.
    for i in 0..MAX_MATERIALS as u32 {
        let m = GpuMaterial {
            normal_map_index: i,
            ..Default::default()
        };
        let id = table.intern(m);
        assert_eq!(id, i, "in-cap intern must return sequential ids");
    }
    assert_eq!(table.len(), MAX_MATERIALS);

    // Over-cap intern: distinct material, but no slot to land in.
    let overflow = GpuMaterial {
        normal_map_index: MAX_MATERIALS as u32,
        ..Default::default()
    };
    let overflow_id = table.intern(overflow);
    assert_eq!(
        overflow_id, 0,
        "over-cap intern must return id 0 (sentinel) so the GPU \
         read at materials[id] stays within bounds"
    );

    // Table count must not grow past the cap.
    assert_eq!(
        table.len(),
        MAX_MATERIALS,
        "over-cap intern must NOT push to materials Vec"
    );

    // Subsequent over-cap interns also fold to id 0 — the warn
    // is `Once`-gated so the second call is silent.
    let overflow2 = GpuMaterial {
        normal_map_index: MAX_MATERIALS as u32 + 1,
        ..Default::default()
    };
    assert_eq!(table.intern(overflow2), 0);
    assert_eq!(table.len(), MAX_MATERIALS);

    // Already-interned materials still resolve to their original
    // id — the cap path doesn't poison the dedup map.
    let existing = GpuMaterial {
        normal_map_index: 42, // interned at id 42 in the loop above
        ..Default::default()
    };
    assert_eq!(
        table.intern(existing),
        42,
        "in-cap dedup hit must still return the original id even \
         after the cap has been reached"
    );
}

/// `clear()` releases the `Once`-guard implicitly by replacing
/// the table; verify the next overflow on a freshly-cleared
/// table still routes to id 0 (the *behaviour*, not the warn,
/// is what matters per-frame).
#[test]
fn intern_overflow_persists_across_clear() {
    let mut table = MaterialTable::new();
    for i in 0..MAX_MATERIALS as u32 {
        let m = GpuMaterial {
            normal_map_index: i,
            ..Default::default()
        };
        table.intern(m);
    }
    let overflow = GpuMaterial {
        normal_map_index: u32::MAX,
        ..Default::default()
    };
    assert_eq!(table.intern(overflow), 0);

    table.clear();
    // After clear the seeded neutral default re-occupies slot 0
    // (#807). A user intern of a material distinct from neutral
    // pushes at slot 1 — NOT slot 0, since slot 0 is reserved.
    let first = GpuMaterial {
        normal_map_index: 1,
        ..Default::default()
    };
    assert_eq!(table.intern(first), 1);
    assert_eq!(table.len(), 2);

    // Interning the neutral default itself dedupes to slot 0.
    assert_eq!(table.intern(GpuMaterial::default()), 0);
}

/// #3911 (REN-2026-09-05-D7-04), SIBLING half — the *second* place a
/// supplemental role can be transposed.
///
/// `static_meshes.rs` fills `DrawCommand::supplemental_texture_indices` from
/// the role set, and `every_supplemental_texture_slot_is_written_exactly_once`
/// now pins that correspondence. But the array is then projected onto named
/// `GpuMaterial` fields by `DrawCommand::to_gpu_material`, and *that* mapping
/// had no correspondence pin either — `tint_map_index:
/// self.supplemental_texture_indices[slot::INNER_LAYER]` would compile, upload,
/// hash and render, just with the wrong map in the wrong lane.
///
/// Same derived shape as the CPU-side pin: the expected field name comes from
/// the slot constant (`GLASS_DIRT_OVERLAY` → `glass_dirt_overlay_map_index`),
/// so a new lane is covered the moment it is declared rather than when someone
/// remembers to extend a list.
#[cfg(test)]
mod supplemental_projection_pin {
    const CONTEXT_MOD_RS: &str = include_str!("context/mod.rs");

    /// The `GpuMaterial` field a supplemental slot must be projected onto.
    /// `DECAL_2` → `decal_map_2_index` is the one irregular shape; everything
    /// else is `<slot lowercased>_map_index`.
    fn expected_material_field(slot: &str) -> String {
        match slot.strip_prefix("DECAL_") {
            Some(n) => format!("decal_map_{n}_index"),
            None => format!("{}_map_index", slot.to_lowercase()),
        }
    }

    /// `(GpuMaterial field, slot constant)` for every supplemental projection
    /// inside `to_gpu_material`, tolerating rustfmt's line wrapping.
    fn projections() -> Vec<(String, String)> {
        let start = CONTEXT_MOD_RS
            .find("pub fn to_gpu_material(&self) -> GpuMaterial {")
            .expect("to_gpu_material must still exist");
        let body = &CONTEXT_MOD_RS[start..];
        let end = body
            .find("\n    /// Hash of the material-relevant DrawCommand fields")
            .expect("to_gpu_material's following sibling must still exist");
        // Collapse wrapping so `field: self.supplemental_texture_indices\n
        // [slot::NAME]` reads the same as the single-line form.
        let flat = body[..end].split_whitespace().collect::<Vec<_>>().join(" ");
        let flat = flat.replace(
            "supplemental_texture_indices [",
            "supplemental_texture_indices[",
        );

        const NEEDLE: &str = "self.supplemental_texture_indices[slot::";
        let mut out = Vec::new();
        let mut rest = flat.as_str();
        let mut consumed = 0usize;
        while let Some(at) = rest[consumed..].find(NEEDLE) {
            let abs = consumed + at;
            let slot = rest[abs + NEEDLE.len()..]
                .split(']')
                .next()
                .expect("a projection must close its slot index")
                .to_owned();
            // Walk back over `: ` to the field name.
            let head = rest[..abs].trim_end();
            let head = head
                .strip_suffix(':')
                .expect("a supplemental projection must be a `field: self...` initializer");
            let field = head
                .rsplit([' ', ','])
                .next()
                .expect("a field name must precede the colon")
                .to_owned();
            out.push((field, slot));
            consumed = abs + NEEDLE.len();
            rest = &flat;
        }
        out
    }

    #[test]
    fn every_supplemental_slot_projects_onto_its_own_gpu_material_field() {
        let projections = projections();
        assert_eq!(
            projections.len(),
            crate::vulkan::material::supplemental_texture_slot::COUNT,
            "to_gpu_material must project every supplemental slot exactly once — \
             a missing projection leaves that lane at the GpuMaterial default and \
             is invisible to the CPU-side write pin (#3911)",
        );
        for (field, slot) in &projections {
            let expected = expected_material_field(slot);
            assert_eq!(
                field, &expected,
                "to_gpu_material projects slot::{slot} onto `{field}`, but its role \
                 field is `{expected}`. Two lanes swapping here keeps every arity \
                 and CPU-side pin green while sampling the wrong map for the wrong \
                 purpose (#3911)",
            );
        }
    }
}
