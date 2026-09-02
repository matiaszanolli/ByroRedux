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
fn gpu_material_size_is_432_bytes() {
    assert_eq!(std::mem::size_of::<GpuMaterial>(), 432);
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

/// Regression guard for `GpuMaterial` GLSL field names —
/// REN-D14-NEW-02 (audit 2026-05-09). The offset pin
/// (`gpu_material_field_offsets_match_shader_contract`) and the
/// size pin (`gpu_material_size_is_432_bytes`) catch byte-level
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
/// #2712 — pin which supplemental role lanes are actually sampled.
///
/// Three of the twelve (`lightingMapIndex`, `flowMapIndex`,
/// `wrinkleMapIndex`) are produced, uploaded and hashed but read by
/// no shader — a deliberate deferral that previously lived only in a
/// one-off audit report and had already failed to propagate to a
/// sibling report. This pins it in both directions: a lane silently
/// going dead fails here, and so does implementing one of the three
/// without removing its "captured, not yet shaded" note.
///
/// `triangle.frag` is the only pass that reads the supplemental
/// roles (checked across `shaders/`), and it `#include`s the struct
/// rather than declaring it, so a name appearing in this file means
/// the lane is genuinely sampled.
#[test]
fn supplemental_role_lanes_sampled_by_triangle_frag_are_exactly_the_nine() {
    let src = include_str!("../../shaders/triangle.frag");

    for name in &[
        "tintMapIndex",
        "innerLayerMapIndex",
        "specularMapIndex",
        "reflectanceMapIndex",
        "emittanceGradientMapIndex",
        "decalMap0Index",
        "decalMap1Index",
        "decalMap2Index",
        "decalMap3Index",
    ] {
        assert!(
            src.contains(name),
            "{name} is a wired supplemental role — if this pass stopped \
             sampling it, the lane is now uploaded and hashed for nothing \
             (#2712)"
        );
    }

    for name in &["lightingMapIndex", "flowMapIndex", "wrinkleMapIndex"] {
        assert!(
            !src.contains(name),
            "{name} is now sampled — good, but the deferral notes on \
             GpuMaterial and in include/bindings.glsl say it is not. \
             Remove them together with this assertion (#2712)"
        );
    }
}

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
        "textureIndex,",
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
/// The size pin (`gpu_material_size_is_432_bytes`) catches additions
/// or removals; this catches reorderings within the record that the
/// size pin alone would miss — e.g. swapping
/// `texture_index` and `normal_map_index` within vec4 #4 would
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

    // ── Texture indices group A (vec4 #4, offsets 48-60) ───────
    assert_eq!(offset_of!(GpuMaterial, texture_index), 48);
    assert_eq!(offset_of!(GpuMaterial, normal_map_index), 52);
    assert_eq!(offset_of!(GpuMaterial, dark_map_index), 56);
    assert_eq!(offset_of!(GpuMaterial, glow_map_index), 60);

    // ── Texture indices group B (vec4 #5, offsets 64-76) ───────
    assert_eq!(offset_of!(GpuMaterial, detail_map_index), 64);
    assert_eq!(offset_of!(GpuMaterial, gloss_map_index), 68);
    assert_eq!(offset_of!(GpuMaterial, parallax_map_index), 72);
    assert_eq!(offset_of!(GpuMaterial, env_map_index), 76);

    // ── env_mask + alpha_test_func + material_kind + alpha
    //    (vec4 #6, offsets 80-92) ───────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, env_mask_index), 80);
    assert_eq!(offset_of!(GpuMaterial, alpha_test_func), 84);
    assert_eq!(offset_of!(GpuMaterial, material_kind), 88);
    assert_eq!(offset_of!(GpuMaterial, material_alpha), 92);

    // ── Parallax POM + UV offset (vec4 #7, offsets 96-108) ─────
    assert_eq!(offset_of!(GpuMaterial, parallax_height_scale), 96);
    assert_eq!(offset_of!(GpuMaterial, parallax_max_passes), 100);
    assert_eq!(offset_of!(GpuMaterial, uv_offset_u), 104);
    assert_eq!(offset_of!(GpuMaterial, uv_offset_v), 108);

    // ── UV scale + diffuse RG (vec4 #8, offsets 112-124) ───────
    assert_eq!(offset_of!(GpuMaterial, uv_scale_u), 112);
    assert_eq!(offset_of!(GpuMaterial, uv_scale_v), 116);
    assert_eq!(offset_of!(GpuMaterial, diffuse_r), 120);
    assert_eq!(offset_of!(GpuMaterial, diffuse_g), 124);

    // ── diffuse_b + ambient RGB (vec4 #9, offsets 128-140) ─────
    assert_eq!(offset_of!(GpuMaterial, diffuse_b), 128);
    assert_eq!(offset_of!(GpuMaterial, ambient_r), 132);
    assert_eq!(offset_of!(GpuMaterial, ambient_g), 136);
    assert_eq!(offset_of!(GpuMaterial, ambient_b), 140);

    // (#804 / R1-N4 dropped `avg_albedo_r/g/b` — what would have
    // been vec4 #10 at offsets 144-152 is gone; subsequent fields
    // shift down by 12 bytes from their pre-#804 positions.)

    // ── skin_tint A/R/G/B (offsets 144-156) ────────────────────
    assert_eq!(offset_of!(GpuMaterial, skin_tint_a), 144);
    assert_eq!(offset_of!(GpuMaterial, skin_tint_r), 148);
    assert_eq!(offset_of!(GpuMaterial, skin_tint_g), 152);
    assert_eq!(offset_of!(GpuMaterial, skin_tint_b), 156);

    // ── hair_tint RGB + multi_layer_envmap_strength
    //    (offsets 160-172) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, hair_tint_r), 160);
    assert_eq!(offset_of!(GpuMaterial, hair_tint_g), 164);
    assert_eq!(offset_of!(GpuMaterial, hair_tint_b), 168);
    assert_eq!(offset_of!(GpuMaterial, multi_layer_envmap_strength), 172);

    // ── eye_left RGB + eye_cubemap_scale (offsets 176-188) ─────
    assert_eq!(offset_of!(GpuMaterial, eye_left_center_x), 176);
    assert_eq!(offset_of!(GpuMaterial, eye_left_center_y), 180);
    assert_eq!(offset_of!(GpuMaterial, eye_left_center_z), 184);
    assert_eq!(offset_of!(GpuMaterial, eye_cubemap_scale), 188);

    // ── eye_right RGB + multi_layer_inner_thickness
    //    (offsets 192-204) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, eye_right_center_x), 192);
    assert_eq!(offset_of!(GpuMaterial, eye_right_center_y), 196);
    assert_eq!(offset_of!(GpuMaterial, eye_right_center_z), 200);
    assert_eq!(offset_of!(GpuMaterial, multi_layer_inner_thickness), 204);

    // ── refraction_scale + multi_layer_inner_scale UV + sparkle_r
    //    (offsets 208-220) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, multi_layer_refraction_scale), 208);
    assert_eq!(offset_of!(GpuMaterial, multi_layer_inner_scale_u), 212);
    assert_eq!(offset_of!(GpuMaterial, multi_layer_inner_scale_v), 216);
    assert_eq!(offset_of!(GpuMaterial, sparkle_r), 220);

    // ── sparkle GB + sparkle_intensity + falloff_start
    //    (offsets 224-236) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, sparkle_g), 224);
    assert_eq!(offset_of!(GpuMaterial, sparkle_b), 228);
    assert_eq!(offset_of!(GpuMaterial, sparkle_intensity), 232);
    assert_eq!(offset_of!(GpuMaterial, falloff_start_angle), 236);

    // ── falloff_stop + opacities + soft_falloff_depth
    //    (offsets 240-252) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, falloff_stop_angle), 240);
    assert_eq!(offset_of!(GpuMaterial, falloff_start_opacity), 244);
    assert_eq!(offset_of!(GpuMaterial, falloff_stop_opacity), 248);
    assert_eq!(offset_of!(GpuMaterial, soft_falloff_depth), 252);

    // ── greyscale palette LUT bindless handle, #890 Stage 2c
    //    (offset 256) ─────────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, greyscale_lut_index), 256);

    // ── BGSM translucency parameter suite, #1147 Phase 2b
    //    (offsets 260-280) ─────────────────────────────────────
    assert_eq!(offset_of!(GpuMaterial, translucency_subsurface_r), 260);
    assert_eq!(offset_of!(GpuMaterial, translucency_subsurface_g), 264);
    assert_eq!(offset_of!(GpuMaterial, translucency_subsurface_b), 268);
    assert_eq!(
        offset_of!(GpuMaterial, translucency_transmissive_scale),
        272
    );
    assert_eq!(offset_of!(GpuMaterial, translucency_turbulence), 276);

    // ── PBR IOR (#1248, offset 280) ──────────────────────────
    assert_eq!(offset_of!(GpuMaterial, ior), 280);

    // ── Disney diffuse lobe (#1249, offsets 284-292) ──────────
    assert_eq!(offset_of!(GpuMaterial, subsurface), 284);
    assert_eq!(offset_of!(GpuMaterial, sheen), 288);
    assert_eq!(offset_of!(GpuMaterial, sheen_tint), 292);

    // ── Anisotropic GGX (#1250, offset 296) ───────────────────
    assert_eq!(offset_of!(GpuMaterial, anisotropic), 296);
    assert_eq!(offset_of!(GpuMaterial, tint_map_index), 300);
    assert_eq!(offset_of!(GpuMaterial, inner_layer_map_index), 304);
    assert_eq!(offset_of!(GpuMaterial, specular_map_index), 308);
    assert_eq!(offset_of!(GpuMaterial, lighting_map_index), 312);
    assert_eq!(offset_of!(GpuMaterial, flow_map_index), 316);
    assert_eq!(offset_of!(GpuMaterial, wrinkle_map_index), 320);
    assert_eq!(offset_of!(GpuMaterial, reflectance_map_index), 324);
    assert_eq!(offset_of!(GpuMaterial, emittance_gradient_map_index), 328);
    assert_eq!(offset_of!(GpuMaterial, decal_map_0_index), 332);
    assert_eq!(offset_of!(GpuMaterial, decal_map_1_index), 336);
    assert_eq!(offset_of!(GpuMaterial, decal_map_2_index), 340);
    assert_eq!(offset_of!(GpuMaterial, decal_map_3_index), 344);

    // ── Animated BSShaderProperty color/scalar (#2221, offsets 348-360)
    assert_eq!(offset_of!(GpuMaterial, shader_color_r), 348);
    assert_eq!(offset_of!(GpuMaterial, shader_color_g), 352);
    assert_eq!(offset_of!(GpuMaterial, shader_color_b), 356);
    assert_eq!(offset_of!(GpuMaterial, shader_float), 360);
    assert_eq!(offset_of!(GpuMaterial, glass_fresnel_r), 364);
    assert_eq!(offset_of!(GpuMaterial, glass_fresnel_g), 368);
    assert_eq!(offset_of!(GpuMaterial, glass_fresnel_b), 372);
    assert_eq!(offset_of!(GpuMaterial, glass_refraction_scale), 376);
    assert_eq!(offset_of!(GpuMaterial, glass_blur_scale), 380);
    assert_eq!(offset_of!(GpuMaterial, glass_blur_scale_factor), 384);
    assert_eq!(
        offset_of!(GpuMaterial, glass_roughness_scratch_map_index),
        388
    );
    assert_eq!(offset_of!(GpuMaterial, glass_dirt_overlay_map_index), 392);
    assert_eq!(offset_of!(GpuMaterial, lighting_effect_1), 396);
    assert_eq!(offset_of!(GpuMaterial, lighting_effect_2), 400);
    assert_eq!(offset_of!(GpuMaterial, subsurface_rolloff), 404);
    assert_eq!(offset_of!(GpuMaterial, rimlight_power), 408);
    assert_eq!(offset_of!(GpuMaterial, backlight_power), 412);
    assert_eq!(offset_of!(GpuMaterial, fresnel_power), 416);
    assert_eq!(offset_of!(GpuMaterial, grayscale_to_palette_scale), 420);
    assert_eq!(offset_of!(GpuMaterial, lighting_mask_map_index), 424);
    assert_eq!(offset_of!(GpuMaterial, back_lighting_map_index), 428);
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
/// different diffuse on otherwise-identical material) must NOT
/// dedup — they're genuinely distinct on the GPU. Pin this
/// because a buggy hash that drops bits could collapse them and
/// silently swap textures across draws.
#[test]
fn texture_index_difference_is_distinct() {
    let mut table = MaterialTable::new();
    let mut a = GpuMaterial::default();
    let mut b = GpuMaterial::default();
    a.texture_index = 7;
    b.texture_index = 8;
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
            texture_index: i,
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
    mats[0].texture_index = 100;
    mats[1].texture_index = 200;
    mats[2].texture_index = 300;
    for m in &mats {
        table.intern(*m);
    }
    let slice = table.materials();
    // Slot 0 is the seeded neutral default (#807); user materials
    // start at slot 1 in insertion order.
    assert_eq!(slice.len(), 4);
    assert!(slice[0] == GpuMaterial::default(), "slot 0 = neutral");
    assert_eq!(slice[1].texture_index, 100);
    assert_eq!(slice[2].texture_index, 200);
    assert_eq!(slice[3].texture_index, 300);
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
/// entries (each varying by `texture_index`), then asserts:
///   1. The first `intern` of `texture_index = 0` HITS the seeded
///      neutral slot (id 0), and `intern` of `texture_index = i`
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
    // `texture_index` is part of the byte-Hash dedup so each
    // increment produces a fresh GpuMaterial. Lucky alignment:
    // `texture_index = i` lands at slot `i` because the seeded
    // neutral has `texture_index = 0`, and `intern` of i=0 hits
    // it. Subsequent i=1..MAX_MATERIALS-1 each push a fresh slot.
    for i in 0..MAX_MATERIALS as u32 {
        let m = GpuMaterial {
            texture_index: i,
            ..Default::default()
        };
        let id = table.intern(m);
        assert_eq!(id, i, "in-cap intern must return sequential ids");
    }
    assert_eq!(table.len(), MAX_MATERIALS);

    // Over-cap intern: distinct material, but no slot to land in.
    let overflow = GpuMaterial {
        texture_index: MAX_MATERIALS as u32,
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
        texture_index: MAX_MATERIALS as u32 + 1,
        ..Default::default()
    };
    assert_eq!(table.intern(overflow2), 0);
    assert_eq!(table.len(), MAX_MATERIALS);

    // Already-interned materials still resolve to their original
    // id — the cap path doesn't poison the dedup map.
    let existing = GpuMaterial {
        texture_index: 42, // interned at id 42 in the loop above
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
            texture_index: i,
            ..Default::default()
        };
        table.intern(m);
    }
    let overflow = GpuMaterial {
        texture_index: u32::MAX,
        ..Default::default()
    };
    assert_eq!(table.intern(overflow), 0);

    table.clear();
    // After clear the seeded neutral default re-occupies slot 0
    // (#807). A user intern of a material distinct from neutral
    // pushes at slot 1 — NOT slot 0, since slot 0 is reserved.
    let first = GpuMaterial {
        texture_index: 1,
        ..Default::default()
    };
    assert_eq!(table.intern(first), 1);
    assert_eq!(table.len(), 2);

    // Interning the neutral default itself dedupes to slot 0.
    assert_eq!(table.intern(GpuMaterial::default()), 0);
}
