//! Skyrim SE era shader-property tests (incl. BSSky/BSWater). Split from `shader_tests.rs` (#2056);
//! helpers live in the parent module.

use super::*;


#[test]
fn parse_bs_lighting_default_no_trailing() {
    let header = make_skyrim_header();
    let data = build_bs_lighting_common(0); // shader_type=0 (Default)
    let mut stream = NifStream::new(&data, &header);

    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.shader_type, 0);
    assert!((prop.glossiness - 50.0).abs() < 1e-6);
    assert!(matches!(prop.shader_type_data, ShaderTypeData::None));
    // All common data consumed, no trailing fields.
    assert_eq!(stream.position(), data.len() as u64);
}


#[test]
fn parse_bs_lighting_env_map_trailing() {
    let header = make_skyrim_header();
    let mut data = build_bs_lighting_common(1); // shader_type=1 (EnvironmentMap)
    data.extend_from_slice(&0.75f32.to_le_bytes()); // env_map_scale
    let mut stream = NifStream::new(&data, &header);

    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.shader_type, 1);
    match prop.shader_type_data {
        ShaderTypeData::EnvironmentMap { env_map_scale } => {
            assert!((env_map_scale - 0.75).abs() < 1e-6);
        }
        _ => panic!("expected EnvironmentMap"),
    }
    assert_eq!(stream.position(), data.len() as u64);
}


#[test]
fn parse_bs_lighting_skin_tint_trailing() {
    let header = make_skyrim_header();
    let mut data = build_bs_lighting_common(5); // shader_type=5 (SkinTint)
    data.extend_from_slice(&0.9f32.to_le_bytes());
    data.extend_from_slice(&0.7f32.to_le_bytes());
    data.extend_from_slice(&0.5f32.to_le_bytes());
    let mut stream = NifStream::new(&data, &header);

    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    match prop.shader_type_data {
        ShaderTypeData::SkinTint { skin_tint_color } => {
            assert!((skin_tint_color[0] - 0.9).abs() < 1e-6);
            assert!((skin_tint_color[1] - 0.7).abs() < 1e-6);
            assert!((skin_tint_color[2] - 0.5).abs() < 1e-6);
        }
        _ => panic!("expected SkinTint"),
    }
    assert_eq!(stream.position(), data.len() as u64);
}


#[test]
fn parse_bs_lighting_eye_envmap_trailing() {
    let header = make_skyrim_header();
    let mut data = build_bs_lighting_common(16); // shader_type=16 (EyeEnvmap)
                                                 // eye_cubemap_scale
    data.extend_from_slice(&1.2f32.to_le_bytes());
    // left_eye_reflection_center (3x f32)
    data.extend_from_slice(&(-0.05f32).to_le_bytes());
    data.extend_from_slice(&0.12f32.to_le_bytes());
    data.extend_from_slice(&0.03f32.to_le_bytes());
    // right_eye_reflection_center (3x f32)
    data.extend_from_slice(&0.05f32.to_le_bytes());
    data.extend_from_slice(&0.12f32.to_le_bytes());
    data.extend_from_slice(&0.03f32.to_le_bytes());
    let mut stream = NifStream::new(&data, &header);

    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    match prop.shader_type_data {
        ShaderTypeData::EyeEnvmap {
            eye_cubemap_scale,
            left_eye_reflection_center,
            right_eye_reflection_center,
        } => {
            assert!((eye_cubemap_scale - 1.2).abs() < 1e-6);
            assert!((left_eye_reflection_center[0] - (-0.05)).abs() < 1e-6);
            assert!((right_eye_reflection_center[0] - 0.05).abs() < 1e-6);
        }
        _ => panic!("expected EyeEnvmap"),
    }
    assert_eq!(stream.position(), data.len() as u64);
}


#[test]
fn parse_bs_lighting_multilayer_parallax_trailing() {
    let header = make_skyrim_header();
    let mut data = build_bs_lighting_common(11); // shader_type=11 (MultiLayerParallax)
    data.extend_from_slice(&0.1f32.to_le_bytes()); // inner_layer_thickness
    data.extend_from_slice(&0.5f32.to_le_bytes()); // refraction_scale
    data.extend_from_slice(&2.0f32.to_le_bytes()); // inner_layer_texture_scale u
    data.extend_from_slice(&2.0f32.to_le_bytes()); // inner_layer_texture_scale v
    data.extend_from_slice(&0.8f32.to_le_bytes()); // envmap_strength
    let mut stream = NifStream::new(&data, &header);

    let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
    match prop.shader_type_data {
        ShaderTypeData::MultiLayerParallax {
            inner_layer_thickness,
            envmap_strength,
            ..
        } => {
            assert!((inner_layer_thickness - 0.1).abs() < 1e-6);
            assert!((envmap_strength - 0.8).abs() < 1e-6);
        }
        _ => panic!("expected MultiLayerParallax"),
    }
    assert_eq!(stream.position(), data.len() as u64);
}


#[test]
fn parse_bs_effect_shader_soft_falloff_and_greyscale() {
    let header = make_skyrim_header();
    let mut data = Vec::new();
    // NiObjectNET: name (string table index 0)
    data.extend_from_slice(&0i32.to_le_bytes());
    // extra_data_refs: count=0
    data.extend_from_slice(&0u32.to_le_bytes());
    // controller_ref: -1
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // shader_flags_1, shader_flags_2
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    // uv_offset, uv_scale
    for _ in 0..4 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // source_texture: sized string "tex/glow.dds"
    let tex = b"tex/glow.dds";
    data.extend_from_slice(&(tex.len() as u32).to_le_bytes());
    data.extend_from_slice(tex);
    // texture_clamp_mode(u8), lighting_influence(u8), env_map_min_lod(u8), unused(u8)
    data.extend_from_slice(&[3u8, 128u8, 5u8, 0u8]);
    // falloff: start_angle, stop_angle, start_opacity, stop_opacity
    for _ in 0..4 {
        data.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // emissive_color (4x f32)
    for _ in 0..4 {
        data.extend_from_slice(&1.0f32.to_le_bytes());
    }
    // emissive_multiple
    data.extend_from_slice(&2.0f32.to_le_bytes());
    // soft_falloff_depth
    data.extend_from_slice(&5.0f32.to_le_bytes());
    // greyscale_texture: sized string "tex/grey.dds"
    let grey = b"tex/grey.dds";
    data.extend_from_slice(&(grey.len() as u32).to_le_bytes());
    data.extend_from_slice(grey);

    let mut stream = NifStream::new(&data, &header);
    let prop = BSEffectShaderProperty::parse(&mut stream).unwrap();

    assert_eq!(prop.source_texture, "tex/glow.dds");
    assert_eq!(prop.lighting_influence, 128);
    assert_eq!(prop.env_map_min_lod, 5);
    assert!((prop.soft_falloff_depth - 5.0).abs() < 1e-6);
    assert_eq!(prop.greyscale_texture, "tex/grey.dds");
    assert!(prop.env_map_texture.is_empty()); // Not FO4+
    assert_eq!(stream.position(), data.len() as u64);
}


/// Pre-#713 `BSSkyShaderProperty` was aliased to
/// `BSShaderPPLightingProperty::parse`, which read the FO3 PP trailer
/// (`texture_clamp_mode + texture_set_ref + refraction + parallax`)
/// and over-consumed 12-28 bytes — the sky filename + sky type never
/// reached the importer. The dedicated parser now consumes exactly
/// the fields nif.xml line 6708 specifies.
#[test]
fn bs_sky_shader_property_parses_skyrim_layout_exactly() {
    let header = make_skyrim_header();
    let data = build_bs_sky_shader_property("textures\\sky\\skyrimclouds01.dds", 3); // 3 = Clouds.
    let mut stream = NifStream::new(&data, &header);

    let prop = BSSkyShaderProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.shader_flags_1, 0x80000000);
    assert_eq!(prop.shader_flags_2, 0x21);
    assert!(prop.sf1_crcs.is_empty(), "BSVER=83 → no CRC arrays");
    assert!(prop.sf2_crcs.is_empty());
    assert_eq!(prop.uv_offset, [0.0, 0.0]);
    assert_eq!(prop.uv_scale, [1.0, 1.0]);
    assert_eq!(prop.source_texture, "textures\\sky\\skyrimclouds01.dds");
    assert_eq!(prop.sky_object_type, 3);
    assert_eq!(
        stream.position(),
        data.len() as u64,
        "parser must consume the block exactly"
    );
}


/// `BSWaterShaderProperty` regression — same root cause as the sky
/// variant. Per-block tail is the single `Water Shader Flags` u32 per
/// nif.xml line 6705 (`WaterShaderPropertyFlags`, default 0xC4 =
/// Reflections + Refractions + Cubemap).
#[test]
fn bs_water_shader_property_parses_skyrim_layout_exactly() {
    let header = make_skyrim_header();
    let data = build_bs_water_shader_property(0xC4);
    let mut stream = NifStream::new(&data, &header);

    let prop = BSWaterShaderProperty::parse(&mut stream).unwrap();
    assert_eq!(prop.shader_flags_1, 0x80000008);
    assert_eq!(prop.shader_flags_2, 0x21);
    assert!(prop.sf1_crcs.is_empty());
    assert!(prop.sf2_crcs.is_empty());
    assert_eq!(prop.uv_offset, [0.0, 0.0]);
    assert_eq!(prop.uv_scale, [1.0, 1.0]);
    assert_eq!(prop.water_shader_flags, 0xC4);
    assert_eq!(stream.position(), data.len() as u64);
}


/// Dispatch routes both names through the dedicated parsers. Pre-#713
/// the dispatch arm in `blocks/mod.rs:305-312` listed both alongside
/// `BSShaderPPLightingProperty` — verify the new arms produce the
/// right downcast types.
#[test]
fn dispatch_routes_bs_sky_and_water_to_dedicated_parsers() {
    let header = make_skyrim_header();

    // BSSkyShaderProperty — must downcast to BSSkyShaderProperty,
    // never to BSShaderPPLightingProperty.
    {
        let data = build_bs_sky_shader_property("textures\\sky\\moon.dds", 7);
        let mut stream = NifStream::new(&data, &header);
        let block =
            crate::blocks::parse_block("BSSkyShaderProperty", &mut stream, Some(data.len() as u32))
                .expect("BSSkyShaderProperty must dispatch");
        let sky = block
            .as_any()
            .downcast_ref::<BSSkyShaderProperty>()
            .expect("downcast to BSSkyShaderProperty");
        assert_eq!(sky.sky_object_type, 7);
        assert_eq!(sky.source_texture, "textures\\sky\\moon.dds");
        assert!(
            block
                .as_any()
                .downcast_ref::<BSShaderPPLightingProperty>()
                .is_none(),
            "BSSkyShaderProperty MUST NOT route through PPLighting parser"
        );
    }

    // BSWaterShaderProperty — same regression for the water sibling.
    {
        let data = build_bs_water_shader_property(0xC4);
        let mut stream = NifStream::new(&data, &header);
        let block = crate::blocks::parse_block(
            "BSWaterShaderProperty",
            &mut stream,
            Some(data.len() as u32),
        )
        .expect("BSWaterShaderProperty must dispatch");
        let water = block
            .as_any()
            .downcast_ref::<BSWaterShaderProperty>()
            .expect("downcast to BSWaterShaderProperty");
        assert_eq!(water.water_shader_flags, 0xC4);
        assert!(
            block
                .as_any()
                .downcast_ref::<BSShaderPPLightingProperty>()
                .is_none(),
            "BSWaterShaderProperty MUST NOT route through PPLighting parser"
        );
    }
}


/// Regression for #716 — BSShaderPPLightingProperty.Emissive Color (Color4)
/// is gated by `#BS_GT_FO3#` (bsver > crate::version::bsver::FO3_FNV).  Pre-fix the field was never read,
/// leaving 16 bytes in the stream; block_size recovery silently masked this on
/// Skyrim-era PPLighting content.
#[test]
fn bsshader_pplighting_skyrim_era_reads_emissive_color() {
    // bsver=83 → Skyrim SE (user_version=12, user_version_2=83)
    let header = make_header(12, 83);
    let emissive = [0.8f32, 0.2, 0.0, 1.0];
    let data = build_bsshader_bytes_with_emissive(83, Some(emissive));
    let expected_len = data.len();
    let mut stream = NifStream::new(&data, &header);

    let prop = BSShaderPPLightingProperty::parse(&mut stream)
        .expect("Skyrim-era PPLighting should parse including emissive color");

    assert_eq!(
        stream.position() as usize,
        expected_len,
        "emissive Color4 (16 bytes) must be consumed on bsver > crate::version::bsver::FO3_FNV"
    );
    assert!((prop.emissive_color[0] - 0.8).abs() < 1e-6, "emissive R");
    assert!((prop.emissive_color[1] - 0.2).abs() < 1e-6, "emissive G");
    assert!((prop.emissive_color[2] - 0.0).abs() < 1e-6, "emissive B");
    assert!((prop.emissive_color[3] - 1.0).abs() < 1e-6, "emissive A");
}
