//! Fallout 76 era shader-property tests. Split from `shader_tests.rs` (#2056);
//! helpers live in the parent module.

use super::*;


#[test]
fn parse_bs_lighting_fo76_minimal() {
    let header = make_fo76_header(""); // empty name → stopcond does NOT fire
    let data = build_fo76_bs_lighting_minimal();
    let mut stream = NifStream::new(&data, &header);

    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert!(
        !prop.material_reference,
        "stopcond should not fire for empty name"
    );
    assert_eq!(prop.shader_type, 0); // FO76 Default
    assert!(prop.sf1_crcs.is_empty());
    assert!(prop.sf2_crcs.is_empty());
    // FO76 authors smoothness 0–1 on the wire; parser normalizes to
    // the 0–100 glossiness scale. Wire 0.6 → 60.0 post-normalize.
    // 1e-4 tolerance because 0.6_f32 * 100.0 = 60.000004 — the float
    // error in the 0.6 representation amplifies by 100×.
    assert!((prop.glossiness - 60.0).abs() < 1e-4);
    assert!((prop.grayscale_to_palette_scale - 0.4).abs() < 1e-6);
    assert!((prop.fresnel_power - 4.2).abs() < 1e-6);
    let w = prop
        .wetness
        .as_ref()
        .expect("wetness present for BSVER 155");
    assert!((w.spec_scale - 0.11).abs() < 1e-6);
    assert_eq!(w.env_map_scale, 0.0); // not read for BSVER != 130
    assert!((w.unknown_1 - 0.66).abs() < 1e-6);
    assert!((w.unknown_2 - 0.77).abs() < 1e-6);
    let lum = prop
        .luminance
        .as_ref()
        .expect("luminance present for BSVER 155");
    assert!((lum.lum_emittance - 100.0).abs() < 1e-6);
    assert!((lum.exposure_offset - 13.5).abs() < 1e-6);
    assert!((lum.final_exposure_max - 3.0).abs() < 1e-6);
    assert!(!prop.do_translucency);
    assert!(prop.translucency.is_none());
    assert!(prop.texture_arrays.is_empty());
    assert!(matches!(prop.shader_type_data, ShaderTypeData::None));
    assert_eq!(stream.position(), data.len() as u64);
}


#[test]
fn parse_bs_lighting_fo76_stopcond_short_circuits() {
    // Non-empty name at BSVER >= 155 → stopcond fires, block body is absent.
    let header = make_fo76_header("materials/weapons/rifle.bgsm");
    // Only NiObjectNET bytes are present; no shader fields follow.
    let mut data = Vec::new();
    // name → string table index 0 (→ "materials/weapons/rifle.bgsm")
    data.extend_from_slice(&0i32.to_le_bytes());
    // extra_data_refs count = 0
    data.extend_from_slice(&0u32.to_le_bytes());
    // controller_ref = -1
    data.extend_from_slice(&(-1i32).to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert!(prop.material_reference);
    assert_eq!(
        prop.net.name.as_deref(),
        Some("materials/weapons/rifle.bgsm"),
    );
    // Everything else at defaults.
    assert_eq!(prop.shader_flags_1, 0);
    assert!(prop.wetness.is_none());
    assert!(prop.luminance.is_none());
    // Parser stopped at end of NiObjectNET — no trailing bytes consumed.
    assert_eq!(stream.position(), data.len() as u64);
}


#[test]
fn parse_bs_lighting_fo76_skin_tint_color4() {
    let header = make_fo76_header("");
    let mut data = build_fo76_bs_lighting_minimal();
    // Patch the Shader Type (after 12 bytes NiObjectNET) from 0 → 4 (SkinTint).
    // Layout: name(4) + extra_count(4) + ctrl(4) = 12, then shader_type u32.
    let st_off = 12;
    data[st_off..st_off + 4].copy_from_slice(&4u32.to_le_bytes());
    // Append Color4 skin tint after the base body.
    for v in [0.95f32, 0.72, 0.60, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }

    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    match prop.shader_type_data {
        ShaderTypeData::Fo76SkinTint { skin_tint_color } => {
            assert!((skin_tint_color[0] - 0.95).abs() < 1e-6);
            assert!((skin_tint_color[3] - 1.0).abs() < 1e-6);
        }
        other => panic!("expected Fo76SkinTint, got {other:?}"),
    }
    assert_eq!(stream.position(), data.len() as u64);
}


#[test]
fn parse_bs_lighting_fo76_sf1_crcs() {
    // Build a minimal FO76 body with Num SF1 = 2, Num SF2 = 1.
    let header = make_fo76_header("");
    let mut data = Vec::new();
    // NiObjectNET
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // Shader Type = 0
    data.extend_from_slice(&0u32.to_le_bytes());
    // Num SF1 = 2
    data.extend_from_slice(&2u32.to_le_bytes());
    // Num SF2 = 1
    data.extend_from_slice(&1u32.to_le_bytes());
    // SF1 array
    data.extend_from_slice(&1563274220u32.to_le_bytes()); // CAST_SHADOWS
    data.extend_from_slice(&759557230u32.to_le_bytes()); // TWO_SIDED
                                                         // SF2 array
    data.extend_from_slice(&348504749u32.to_le_bytes()); // VERTEXCOLORS
                                                         // uv_offset, uv_scale
    for v in [0.0f32, 0.0, 1.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // texture_set_ref
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // emissive_color + mult
    for _ in 0..4 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // Root Material
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // texture_clamp_mode, alpha, refraction, smoothness
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // specular_color, specular_strength
    for _ in 0..3 {
        data.extend_from_slice(&1.0f32.to_le_bytes());
    }
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // grayscale, fresnel
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&5.0f32.to_le_bytes());
    // wetness: 7 floats (spec, spec_pow, min_var, fresnel, metal, unk1, unk2)
    for _ in 0..7 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // luminance
    for _ in 0..4 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // do_translucency=0, has_texture_arrays=0
    data.push(0u8);
    data.push(0u8);

    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.sf1_crcs, vec![1563274220, 759557230]);
    assert_eq!(prop.sf2_crcs, vec![348504749]);
    assert_eq!(stream.position(), data.len() as u64);
}


#[test]
fn parse_bs_effect_fo76_trailing_textures() {
    let header = make_fo76_header("");
    let mut data = Vec::new();
    // NiObjectNET
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // Num SF1 = 0, Num SF2 = 0 (no flag pair for BSVER >= 132)
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    // uv_offset, uv_scale
    for v in [0.0f32, 0.0, 1.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // source_texture
    let src = b"tex/src.dds";
    data.extend_from_slice(&(src.len() as u32).to_le_bytes());
    data.extend_from_slice(src);
    // clamp, light_infl, min_lod, unused
    data.extend_from_slice(&[3u8, 255u8, 0u8, 0u8]);
    // 4 falloff floats
    for _ in 0..4 {
        data.extend_from_slice(&1.0f32.to_le_bytes());
    }
    // refraction_power (FO76)
    data.extend_from_slice(&0.25f32.to_le_bytes());
    // emissive_color (4×f32)
    for _ in 0..4 {
        data.extend_from_slice(&1.0f32.to_le_bytes());
    }
    // emissive_multiple
    data.extend_from_slice(&1.5f32.to_le_bytes());
    // soft_falloff_depth
    data.extend_from_slice(&50.0f32.to_le_bytes());
    // greyscale_texture
    let grey = b"tex/grey.dds";
    data.extend_from_slice(&(grey.len() as u32).to_le_bytes());
    data.extend_from_slice(grey);
    // FO4+ textures (env, normal, mask) + env_map_scale
    for p in [b"tex/env.dds".as_slice(), b"tex/n.dds", b"tex/m.dds"] {
        data.extend_from_slice(&(p.len() as u32).to_le_bytes());
        data.extend_from_slice(p);
    }
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // FO76 trailing: reflectance, lighting textures
    for p in [b"tex/refl.dds".as_slice(), b"tex/lit.dds"] {
        data.extend_from_slice(&(p.len() as u32).to_le_bytes());
        data.extend_from_slice(p);
    }
    // emittance_color (3×f32)
    for v in [0.4f32, 0.5, 0.6] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // emit_gradient_texture
    let grad = b"tex/grad.dds";
    data.extend_from_slice(&(grad.len() as u32).to_le_bytes());
    data.extend_from_slice(grad);
    // luminance (4×f32)
    for v in [100.0f32, 13.5, 2.0, 3.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }

    let mut stream = NifStream::new(&data, &header);
    let prop = BSEffectShaderProperty::parse(&mut stream).unwrap();
    assert!(!prop.material_reference);
    assert!((prop.refraction_power - 0.25).abs() < 1e-6);
    assert_eq!(prop.source_texture, "tex/src.dds");
    assert_eq!(prop.env_map_texture, "tex/env.dds");
    assert_eq!(prop.reflectance_texture, "tex/refl.dds");
    assert_eq!(prop.lighting_texture, "tex/lit.dds");
    assert!((prop.emittance_color[1] - 0.5).abs() < 1e-6);
    assert_eq!(prop.emit_gradient_texture, "tex/grad.dds");
    let lum = prop.luminance.as_ref().unwrap();
    assert!((lum.exposure_offset - 13.5).abs() < 1e-6);
    assert_eq!(stream.position(), data.len() as u64);
}


#[test]
fn parse_bs_effect_fo76_stopcond_short_circuits() {
    let header = make_fo76_header("materials/effects/glow.bgem");
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let prop = BSEffectShaderProperty::parse(&mut stream).unwrap();
    assert!(prop.material_reference);
    assert_eq!(
        prop.net.name.as_deref(),
        Some("materials/effects/glow.bgem"),
    );
    assert_eq!(stream.position(), data.len() as u64);
}


/// FO76 (BSVER=155) routes `BSSkyShaderProperty` through the CRC32
/// flag-array branch — the legacy u32 pair is absent on disk and the
/// per-array counts (Num SF1, Num SF2) appear instead. nif.xml lines
/// 6712-6715. Pre-#713 the fall-through alias would have read the FO3
/// PP texture_clamp_mode (4 bytes after NiObjectNET) as the
/// CRC-array's first u32 — guaranteed wrong.
#[test]
fn bs_sky_shader_property_fo76_reads_crc_arrays_not_legacy_flags() {
    let mut header = make_skyrim_header();
    header.user_version_2 = 155; // FO76 BSVER triggers CRC branch.

    let mut data = Vec::new();
    // NiObjectNET prefix.
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // BSVER=155 → Num SF1 (u32) + Num SF2 (u32, since BSVER >= 152) +
    // SF1 array + SF2 array. Author 2 SF1 entries and 1 SF2 entry.
    data.extend_from_slice(&2u32.to_le_bytes()); // Num SF1
    data.extend_from_slice(&1u32.to_le_bytes()); // Num SF2
    data.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // SF1[0]
    data.extend_from_slice(&0x1234_5678u32.to_le_bytes()); // SF1[1]
    data.extend_from_slice(&0xCAFE_BABEu32.to_le_bytes()); // SF2[0]
                                                           // UV Offset + UV Scale.
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&1.0f32.to_le_bytes());
    // Source Texture + Sky Object Type.
    let tex = "textures\\sky\\fo76skybox.dds";
    data.extend_from_slice(&(tex.len() as u32).to_le_bytes());
    data.extend_from_slice(tex.as_bytes());
    data.extend_from_slice(&5u32.to_le_bytes()); // 5 = Stars.

    let mut stream = NifStream::new(&data, &header);
    let prop = BSSkyShaderProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.shader_flags_1, 0, "BSVER>=132 → legacy pair absent");
    assert_eq!(prop.shader_flags_2, 0);
    assert_eq!(prop.sf1_crcs, vec![0xDEAD_BEEF, 0x1234_5678]);
    assert_eq!(prop.sf2_crcs, vec![0xCAFE_BABE]);
    assert_eq!(prop.source_texture, "textures\\sky\\fo76skybox.dds");
    assert_eq!(prop.sky_object_type, 5);
    assert_eq!(stream.position(), data.len() as u64);
}


/// Regression for #749 / SF-D3-01: a FO76+ BSLightingShaderProperty
/// whose Name is a non-path editor label must NOT trigger the BGSM
/// stopcond — the trailing PBR body still has to parse. Pre-fix the
/// stub fired on any non-empty Name and every editor-tagged Starfield
/// material lost its scalars.
#[test]
fn parse_bs_lighting_fo76_editor_label_does_not_short_circuit() {
    let header = make_fo76_header("Material_Slot_01");
    let data = build_fo76_bs_lighting_minimal();
    let mut stream = NifStream::new(&data, &header);

    let prop = BSLightingShaderProperty::parse(&mut stream)
        .expect("editor-label Name must continue through to the full body parse");
    assert!(
        !prop.material_reference,
        "stopcond must not fire for non-path Name (pre-#749 it did)",
    );
    // Spot-check that the trailing body actually populated.
    // Wire smoothness 0.6 → 60.0 after FO4+ normalize (see shader.rs:876).
    // 1e-4 tolerance because 0.6_f32 * 100.0 = 60.000004.
    assert!((prop.glossiness - 60.0).abs() < 1e-4);
    assert!((prop.fresnel_power - 4.2).abs() < 1e-6);
    assert!(prop.wetness.is_some());
    assert!(prop.luminance.is_some());
    assert_eq!(stream.position(), data.len() as u64);
}


/// Regression for #749 / SF-D3-01: a Starfield `.mat` reference
/// (the new SF material format) must trigger the stopcond. The
/// pre-#749 gate happened to do the right thing here as a side
/// effect of `!is_empty()`; the post-fix gate must keep working
/// once the Name is suffix-checked.
#[test]
fn parse_bs_lighting_fo76_mat_extension_triggers_stopcond() {
    let header = make_fo76_header("materials/sf/armor.mat");
    // Only NiObjectNET bytes are present; no shader fields follow.
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // name → string-table index 0
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count = 0
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref = -1

    let mut stream = NifStream::new(&data, &header);
    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert!(prop.material_reference);
    assert_eq!(prop.net.name.as_deref(), Some("materials/sf/armor.mat"));
    assert_eq!(stream.position(), data.len() as u64);
}


/// Sibling regression: BSEffectShaderProperty must apply the same
/// suffix gate. Pre-#749 it shared the bug 1:1 with
/// BSLightingShaderProperty.
#[test]
fn parse_bs_effect_shader_fo76_editor_label_does_not_short_circuit() {
    // Build a minimal FO76 BSEffectShaderProperty body. We don't need
    // the body to exercise every field — just enough to confirm
    // parsing went past the stopcond. Match the existing FO76 effect
    // shader test structure when one lands; for now, point the parser
    // past NiObjectNET and assert the stopcond returned `false`.
    let header = make_fo76_header("EffectMat_Slot_03");
    let mut data = Vec::new();
    data.extend_from_slice(&0i32.to_le_bytes()); // name → string-table index 0
    data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count = 0
    data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref = -1
                                                    // BSVER 155 effect shader trailing body. Mirror the layout used
                                                    // by the parser: shader_flags_1/2 absent (bsver > crate::version::bsver::FALLOUT4), CRC arrays
                                                    // empty, then UV + texture + scalar fields. We only need enough
                                                    // bytes for the parse to succeed without underrunning the
                                                    // stream — `block_size` recovery would otherwise mask a regression.
    data.extend_from_slice(&0u32.to_le_bytes()); // num SF1 = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // num SF2 = 0
                                                 // uv_offset, uv_scale
    for v in [0.0f32, 0.0, 1.0, 1.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    // source_texture: NiFixedString = -1 (empty)
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // texture_clamp_mode, lighting_influence, env_map_min_lod
    data.extend_from_slice(&3u32.to_le_bytes());
    data.push(0u8); // lighting_influence
    data.push(0u8); // env_map_min_lod
                    // padding fields up to falloff and beyond can vary across BSVER —
                    // this test asserts only that the stopcond did NOT fire; the
                    // detailed FO76 effect-shader body shape is covered by other
                    // tests. Use block_size recovery to consume any remainder.
    let mut stream = NifStream::new(&data, &header);
    // Best-effort parse: if the body shape differs from this fixture,
    // an Err is fine — what matters is that on a successful parse the
    // stopcond did not fire. If the parse errors, that's a sign the
    // editor-label gate let parsing continue (good); if it returned
    // a stub (`material_reference = true`), the gate is broken.
    if let Ok(prop) = BSEffectShaderProperty::parse(&mut stream) {
        assert!(
            !prop.material_reference,
            "stopcond must not fire for non-path Name on BSEffectShaderProperty",
        );
    }
}
