//! Tests for `tests` extracted from ../shader.rs (refactor stage A).
//!
//! Same qualified path preserved (`tests::FOO`).

    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::NifVersion;
    use std::sync::Arc;

    fn make_header(user_version: u32, user_version_2: u32) -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version,
            user_version_2,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("ShaderProp")],
            max_string_length: 10,
            num_groups: 0,
        }
    }

    /// Build bytes for BSShaderPPLightingProperty, optionally including emissive color.
    fn build_bsshader_bytes(user_version_2: u32) -> Vec<u8> {
        let mut data = Vec::new();
        // NiObjectNET: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // NiShadeProperty: shade_flags (u16)
        data.extend_from_slice(&0u16.to_le_bytes());
        // shader_type (u32)
        data.extend_from_slice(&1u32.to_le_bytes());
        // shader_flags_1 (u32)
        data.extend_from_slice(&0x80000000u32.to_le_bytes());
        // shader_flags_2 (u32)
        data.extend_from_slice(&0x00000001u32.to_le_bytes());
        // env_map_scale (f32)
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // texture_clamp_mode (u32)
        data.extend_from_slice(&3u32.to_le_bytes());
        // texture_set_ref (i32)
        data.extend_from_slice(&5i32.to_le_bytes());
        // Refraction/parallax fields — bsver >= 15 reads refraction, bsver >= 24 adds parallax.
        // FNV: bsver=34, so both are present. Oblivion: bsver=0, so neither.
        if user_version_2 >= 15 {
            data.extend_from_slice(&0.5f32.to_le_bytes()); // refraction_strength
            data.extend_from_slice(&10i32.to_le_bytes()); // refraction_fire_period
        }
        if user_version_2 >= 24 {
            data.extend_from_slice(&4.0f32.to_le_bytes()); // parallax_max_passes
            data.extend_from_slice(&1.5f32.to_le_bytes()); // parallax_scale
        }
        data
    }

    /// Regression: #459 — `BSShaderTextureSet::parse` previously read
    /// `Num Textures` as `i32` and clamped `.max(0) as u32`, silently
    /// dropping any negative-interpreted length to an empty set. When
    /// upstream drift flipped the high bit, the block quietly
    /// succeeded at the wrong offset instead of failing loud. Verify
    /// the u32 read still produces an empty set for zero, the expected
    /// set for a normal count, and a loud error for a length that
    /// obviously exceeds the remaining stream (the `allocate_vec`
    /// budget guard from #388 catches it).
    #[test]
    fn parse_bsshader_texture_set_num_textures_as_u32() {
        let header = make_header(11, 34);

        // Case 1: zero textures → empty set, stream fully consumed.
        let zero = 0u32.to_le_bytes();
        let mut stream = NifStream::new(&zero, &header);
        let ts = BSShaderTextureSet::parse(&mut stream).unwrap();
        assert!(ts.textures.is_empty());
        assert_eq!(stream.position(), 4);

        // Case 2: 2 textures — normal path still works.
        let mut data = 2u32.to_le_bytes().to_vec();
        for name in ["diffuse.dds", "normal.dds"] {
            data.extend_from_slice(&(name.len() as u32).to_le_bytes());
            data.extend_from_slice(name.as_bytes());
        }
        let mut stream = NifStream::new(&data, &header);
        let ts = BSShaderTextureSet::parse(&mut stream).unwrap();
        assert_eq!(ts.textures, vec!["diffuse.dds".to_string(), "normal.dds".into()]);

        // Case 3: length of 0xFFFFFFFF (previously silently clamped to 0).
        // Under u32, this exceeds the remaining bytes in the stream and
        // the allocate_vec budget guard short-circuits with InvalidData —
        // loud enough for the outer block_sizes recovery to take over.
        let drift = 0xFFFF_FFFFu32.to_le_bytes();
        let mut stream = NifStream::new(&drift, &header);
        let err = BSShaderTextureSet::parse(&mut stream).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn parse_bsshader_fnv_reads_refraction_parallax() {
        // FNV (bsver=34): reads refraction (bsver>=15) + parallax (bsver>=24) = 16 bytes.
        let header = make_header(11, 34);
        let data = build_bsshader_bytes(34);
        let mut stream = NifStream::new(&data, &header);

        let prop = BSShaderPPLightingProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.texture_set_ref.index(), Some(5));
        assert!((prop.refraction_strength - 0.5).abs() < 1e-6);
        assert_eq!(prop.refraction_fire_period, 10);
        assert!((prop.parallax_max_passes - 4.0).abs() < 1e-6);
        assert!((prop.parallax_scale - 1.5).abs() < 1e-6);
        // All data consumed: 38 base + 16 refraction/parallax = 54 bytes
        assert_eq!(stream.position(), 54);
    }

    /// Regression: #455 — `TileShaderProperty` parses the FO3
    /// `BSShaderLightingProperty` base (NET + shader data + texture
    /// clamp) + a trailing SizedString filename. Pre-fix the dispatch
    /// aliased this type to `BSShaderPPLightingProperty::parse`, which
    /// over-read 20-28 bytes of PP-specific fields and never populated
    /// the filename. HUD overlays (stealth meter / airtimer / quest
    /// markers) lost their texture path as a result.
    #[test]
    fn parse_tile_shader_property_fo3() {
        let header = make_header(11, 34); // FO3/FNV
        let mut data = Vec::new();
        // NiObjectNET: inline name (v <= 20.1.0.0 path)
        data.extend_from_slice(&0i32.to_le_bytes()); // name (string table index)
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        // BSShaderPropertyData: 18 bytes
        data.extend_from_slice(&0u16.to_le_bytes()); // shade_flags
        data.extend_from_slice(&1u32.to_le_bytes()); // shader_type
        data.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_1
        data.extend_from_slice(&0u32.to_le_bytes()); // shader_flags_2
        data.extend_from_slice(&0.0f32.to_le_bytes()); // env_map_scale
        data.extend_from_slice(&3u32.to_le_bytes()); // texture_clamp_mode
        // file_name SizedString (u32 length + bytes; NO trailing null)
        let name = b"textures\\interface\\airtimer.dds";
        data.extend_from_slice(&(name.len() as u32).to_le_bytes());
        data.extend_from_slice(name);
        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = TileShaderProperty::parse(&mut stream)
            .expect("TileShaderProperty should parse with BSShaderLightingProperty base + filename");
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "TileShaderProperty must consume exactly {expected_len} bytes",
        );
        assert_eq!(prop.texture_clamp_mode, 3);
        assert_eq!(prop.file_name, "textures\\interface\\airtimer.dds");
        assert_eq!(prop.shader.shader_type, 1);
    }

    #[test]
    fn parse_bsshader_oblivion_no_extra_fields() {
        // Oblivion (bsver=0): no refraction or parallax fields.
        let header = make_header(0, 0);
        let data = build_bsshader_bytes(0);
        let mut stream = NifStream::new(&data, &header);

        let prop = BSShaderPPLightingProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.texture_set_ref.index(), Some(5));
        assert_eq!(prop.refraction_strength, 0.0);
        assert_eq!(prop.refraction_fire_period, 0);
        assert!((prop.parallax_max_passes - 4.0).abs() < 1e-6); // defaults
        assert!((prop.parallax_scale - 1.0).abs() < 1e-6);
        // Only 38 bytes consumed (no extras)
        assert_eq!(stream.position(), 38);
    }

    fn make_skyrim_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: 83,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("TestShader")],
            max_string_length: 10,
            num_groups: 0,
        }
    }

    /// Build the common bytes for BSLightingShaderProperty (Skyrim LE, BSVER=83).
    fn build_bs_lighting_common(shader_type: u32) -> Vec<u8> {
        let mut data = Vec::new();
        // shader_type (read before NiObjectNET for BSVER 83-130)
        data.extend_from_slice(&shader_type.to_le_bytes());
        // NiObjectNET: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // shader_flags_1, shader_flags_2
        data.extend_from_slice(&0x80000000u32.to_le_bytes());
        data.extend_from_slice(&0x00000010u32.to_le_bytes()); // two-sided flag
                                                              // uv_offset (2x f32)
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // uv_scale (2x f32)
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // texture_set_ref
        data.extend_from_slice(&3i32.to_le_bytes());
        // emissive_color (3x f32)
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // emissive_multiple
        data.extend_from_slice(&2.0f32.to_le_bytes());
        // texture_clamp_mode
        data.extend_from_slice(&3u32.to_le_bytes());
        // alpha
        data.extend_from_slice(&0.8f32.to_le_bytes());
        // refraction_strength
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // glossiness
        data.extend_from_slice(&50.0f32.to_le_bytes());
        // specular_color (3x f32)
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.9f32.to_le_bytes());
        data.extend_from_slice(&0.8f32.to_le_bytes());
        // specular_strength
        data.extend_from_slice(&1.5f32.to_le_bytes());
        // lighting_effect_1, lighting_effect_2
        data.extend_from_slice(&0.3f32.to_le_bytes());
        data.extend_from_slice(&0.7f32.to_le_bytes());
        data
    }

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

    fn make_fo4_header() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: 130,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("FO4Shader")],
            max_string_length: 9,
            num_groups: 0,
        }
    }

    /// Build FO4 BSLightingShaderProperty bytes (BSVER=130, shader_type=1 env map).
    fn build_bs_lighting_fo4_env_map() -> Vec<u8> {
        let mut data = Vec::new();
        // shader_type (read before NiObjectNET for BSVER 83-130)
        data.extend_from_slice(&1u32.to_le_bytes()); // EnvironmentMap
                                                     // NiObjectNET: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // shader_flags_1, shader_flags_2 (FO4 reads u32 pair)
        data.extend_from_slice(&0x80000000u32.to_le_bytes());
        data.extend_from_slice(&0x00000010u32.to_le_bytes());
        // uv_offset, uv_scale
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // texture_set_ref
        data.extend_from_slice(&3i32.to_le_bytes());
        // emissive_color (3x f32)
        for v in [0.0f32, 0.5, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // emissive_multiple
        data.extend_from_slice(&2.0f32.to_le_bytes());
        // Root Material (FO4+: NiFixedString = string table index)
        data.extend_from_slice(&(-1i32).to_le_bytes()); // no root material
                                                        // texture_clamp_mode
        data.extend_from_slice(&3u32.to_le_bytes());
        // alpha
        data.extend_from_slice(&0.8f32.to_le_bytes());
        // refraction_strength
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // glossiness (called "smoothness" in FO4, same f32)
        data.extend_from_slice(&0.5f32.to_le_bytes());
        // specular_color (3x f32)
        for v in [1.0f32, 0.9, 0.8] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // specular_strength
        data.extend_from_slice(&1.5f32.to_le_bytes());
        // lighting_effect_1, lighting_effect_2 — NOT present for BSVER >= 130
        // (the parser skips these with (0.0, 0.0))
        // FO4 common fields:
        data.extend_from_slice(&0.3f32.to_le_bytes()); // subsurface_rolloff
        data.extend_from_slice(&2.5f32.to_le_bytes()); // rimlight_power (< FLT_MAX → has backlight)
        data.extend_from_slice(&1.0f32.to_le_bytes()); // backlight_power
        data.extend_from_slice(&0.7f32.to_le_bytes()); // grayscale_to_palette_scale
        data.extend_from_slice(&5.0f32.to_le_bytes()); // fresnel_power
                                                       // WetnessParams (BSVER=130: 7 floats — #403 widened
                                                       // unknown_1 gate to the full 130..155 FO4/FO76 range).
                                                       // Order: spec_scale, spec_power, min_var, env_map_scale,
                                                       // fresnel, metalness, unknown_1.
        for v in [0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.95] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // Shader type 1 trailing: env_map_scale + 2 bools (FO4 BSVER 130)
        data.extend_from_slice(&0.75f32.to_le_bytes()); // env_map_scale
        data.push(1u8); // use_ssr (bool)
        data.push(0u8); // wetness_use_ssr (bool)
        data
    }

    // ── #409 BSVER-131 / 132 boundary regression tests ───────────────

    /// Build a header with a custom BSVER — share the version number
    /// path with the existing FO4 / FO76 helpers so only the boundary
    /// tests need a fresh fixture. The body is the standard
    /// `BSLightingShaderProperty` minus the flag-pair / CRC slots the
    /// per-BSVER gate controls; callers assemble the rest.
    fn make_fo4_header_with_bsver(bsver: u32) -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: bsver,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("BoundaryShader")],
            max_string_length: 14,
            num_groups: 0,
        }
    }

    /// Regression for #409: at `BSVER == 131` the parser must read
    /// neither the u32 flag pair (gated on `bsver <= 130`) nor the
    /// CRC-array counts (gated on `bsver >= 132`). This is NOT a bug —
    /// nif.xml's `#BS_FO4#` is strict `BSVER == 130` and `#BS_GTE_132#`
    /// starts at 132, leaving 131 as an intentional dev-stream gap
    /// where the flag fields are absent altogether.
    ///
    /// The test constructs a body 8 bytes shorter than BSVER 130 (no
    /// flag pair) and assumes the pre-flag-pair part plus the
    /// post-CRC part line up with `bsver == 131`'s expected layout.
    /// Consumes exactly the authored bytes.
    #[test]
    fn bs_lighting_bsver_131_skips_flag_pair_and_crc_counts() {
        let header = make_fo4_header_with_bsver(131);
        let mut data = Vec::new();

        // shader_type (read before NiObjectNET for BSVER 83-130... but
        // also 131 since the gate at `shader.rs::parse` is `bsver <
        // 155`). See the pre-flag-pair block in the source.
        data.extend_from_slice(&1u32.to_le_bytes()); // EnvironmentMap
        // NiObjectNET: name (string-table idx 0), empty extra_data_refs,
        // no controller.
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // NO flag pair at bsver == 131 (gate is `bsver <= 130`).
        // NO Num SF1/SF2 at bsver == 131 (gate is `bsver >= 132`).
        // Next field: uv_offset + uv_scale.
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // texture_set_ref
        data.extend_from_slice(&3i32.to_le_bytes());
        // emissive_color + emissive_multiple
        for v in [0.0f32, 0.5, 1.0, 2.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // Root Material (FO4+ NiFixedString)
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // texture_clamp_mode, alpha, refraction_strength, glossiness
        data.extend_from_slice(&3u32.to_le_bytes());
        for v in [0.8f32, 0.0, 0.5] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // specular_color + specular_strength
        for v in [1.0f32, 0.9, 0.8, 1.5] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // FO4 common fields (BSVER 130..139): subsurface/rimlight/backlight/
        // grayscale/fresnel.
        data.extend_from_slice(&0.3f32.to_le_bytes());
        data.extend_from_slice(&2.5f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.7f32.to_le_bytes());
        data.extend_from_slice(&5.0f32.to_le_bytes());
        // Wetness (7 floats — same as BSVER 130; the wetness gate is
        // `>= 130` not per-BSVER-specific). `env_map_scale` slot
        // (offset 4 within the wetness block) only present at
        // `bsver == 130` strictly — at 131 the parser reads 6 floats.
        for v in [0.1f32, 0.2, 0.3, 0.5, 0.6, 0.95] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // shader_type=1 (EnvironmentMap) trailing: env_map_scale + 2 bools.
        data.extend_from_slice(&0.75f32.to_le_bytes());
        data.push(1u8);
        data.push(0u8);

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.shader_type, 1);
        // Flag pair stays at the pre-fill-in default `0` because the
        // 131 gate skips the u32 read — this is what the test pins.
        assert_eq!(prop.shader_flags_1, 0, "bsver=131 skips flag pair");
        assert_eq!(prop.shader_flags_2, 0, "bsver=131 skips flag pair");
        // CRC arrays stay empty because Num SF1/SF2 are gated `>= 132`.
        assert!(prop.sf1_crcs.is_empty());
        assert!(prop.sf2_crcs.is_empty());
        // Every authored byte consumed — no under-read into next block.
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "bsver=131 body must consume exactly what was authored"
        );
    }

    /// Regression for #409: at `BSVER == 132` the parser must skip the
    /// flag pair AND read `Num SF1` + the SF1 CRC array (but NOT
    /// `Num SF2` which is gated on `>= 152`). Exercises the other side
    /// of the BSVER 131 gap.
    #[test]
    fn bs_lighting_bsver_132_reads_crc_counts_but_not_flag_pair() {
        let header = make_fo4_header_with_bsver(132);
        let mut data = Vec::new();

        // shader_type + NiObjectNET as usual.
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0i32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // NO flag pair at bsver == 132 (gate is `bsver <= 130`).
        // Num SF1 = 2, Num SF2 NOT read (gated on `bsver >= 152`).
        data.extend_from_slice(&2u32.to_le_bytes());
        // Two SF1 CRC32 entries.
        data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());
        data.extend_from_slice(&0xCAFEBABEu32.to_le_bytes());
        // Standard post-flag payload.
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&3i32.to_le_bytes());
        for v in [0.0f32, 0.5, 1.0, 2.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&3u32.to_le_bytes());
        for v in [0.8f32, 0.0, 0.5] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        for v in [1.0f32, 0.9, 0.8, 1.5] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&0.3f32.to_le_bytes());
        data.extend_from_slice(&2.5f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.7f32.to_le_bytes());
        data.extend_from_slice(&5.0f32.to_le_bytes());
        // Wetness: same 6 floats as bsver 131 (no env_map_scale slot
        // since that's strict `bsver == 130`).
        for v in [0.1f32, 0.2, 0.3, 0.5, 0.6, 0.95] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        data.extend_from_slice(&0.75f32.to_le_bytes());
        data.push(1u8);
        data.push(0u8);

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.shader_type, 1);
        assert_eq!(prop.shader_flags_1, 0);
        assert_eq!(prop.shader_flags_2, 0);
        assert_eq!(prop.sf1_crcs, vec![0xDEADBEEF, 0xCAFEBABE]);
        assert!(prop.sf2_crcs.is_empty(), "Num SF2 requires bsver >= 152");
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "bsver=132 must read CRC array but skip flag pair"
        );
    }

    // ── N23.9: FO76/Starfield tests ──────────────────────────────────

    fn make_fo76_header(name: &str) -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 12,
            user_version_2: 155,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from(name)],
            max_string_length: name.len() as u32,
            num_groups: 0,
        }
    }

    /// Build a BSLightingShaderProperty body with FO76 layout (BSVER=155), empty
    /// name (so stopcond does NOT fire), shader_type=0 (Default → no trailing),
    /// empty SF1/SF2 arrays, wetness + luminance, no translucency, no texture arrays.
    fn build_fo76_bs_lighting_minimal() -> Vec<u8> {
        let mut data = Vec::new();
        // NiObjectNET: name = string table index 0 ("")
        data.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs list: count=0
        data.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref = -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // BSVER == 155: Shader Type (BSShaderType155) = 0 (Default)
        data.extend_from_slice(&0u32.to_le_bytes());
        // Num SF1 = 0 (BSVER >= 132)
        data.extend_from_slice(&0u32.to_le_bytes());
        // Num SF2 = 0 (BSVER >= 152)
        data.extend_from_slice(&0u32.to_le_bytes());
        // (no SF1/SF2 arrays because lengths are zero)
        // uv_offset, uv_scale
        for v in [0.0f32, 0.0, 1.0, 1.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // texture_set_ref
        data.extend_from_slice(&5i32.to_le_bytes());
        // emissive_color (3×f32)
        for v in [0.1f32, 0.2, 0.3] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // emissive_multiple
        data.extend_from_slice(&1.5f32.to_le_bytes());
        // Root Material (NiFixedString, BSVER >= 130): -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // texture_clamp_mode
        data.extend_from_slice(&3u32.to_le_bytes());
        // alpha
        data.extend_from_slice(&0.9f32.to_le_bytes());
        // refraction_strength
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // smoothness (glossiness in struct)
        data.extend_from_slice(&0.6f32.to_le_bytes());
        // specular_color
        for v in [0.7f32, 0.8, 0.9] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // specular_strength
        data.extend_from_slice(&1.25f32.to_le_bytes());
        // (no lighting_effect_1/2 — BSVER >= 130 skips)
        // (no subsurface/rimlight/backlight — not in BS_FO4_2 range)
        // grayscale_to_palette_scale
        data.extend_from_slice(&0.4f32.to_le_bytes());
        // fresnel_power
        data.extend_from_slice(&4.2f32.to_le_bytes());
        // WetnessParams: spec_scale, spec_power, min_var,
        // (env_map_scale only for BSVER==130, skipped here)
        // fresnel, metalness, unknown_1 (>130), unknown_2 (==155)
        for v in [0.11f32, 0.22, 0.33, 0.44, 0.55, 0.66, 0.77] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // FO76 luminance (4×f32)
        for v in [100.0f32, 13.5, 2.0, 3.0] {
            data.extend_from_slice(&v.to_le_bytes());
        }
        // Do Translucency = false (1 byte)
        data.push(0u8);
        // Has Texture Arrays = false (1 byte)
        data.push(0u8);
        // No shader-type trailing for type 0 Default
        data
    }

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
        assert!((prop.glossiness - 0.6).abs() < 1e-6);
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

    #[test]
    fn parse_bs_lighting_fo4_env_map_with_wetness() {
        let header = make_fo4_header();
        let data = build_bs_lighting_fo4_env_map();
        let mut stream = NifStream::new(&data, &header);

        let prop = BSLightingShaderProperty::parse(&mut stream).unwrap();
        assert_eq!(prop.shader_type, 1);
        assert_eq!(prop.shader_flags_1, 0x80000000); // FO4 flags read correctly
        assert!((prop.glossiness - 0.5).abs() < 1e-6); // "smoothness" in FO4
        assert!((prop.subsurface_rolloff - 0.3).abs() < 1e-6);
        assert!((prop.rimlight_power - 2.5).abs() < 1e-6);
        assert!((prop.backlight_power - 1.0).abs() < 1e-6);
        assert!((prop.grayscale_to_palette_scale - 0.7).abs() < 1e-6);
        assert!((prop.fresnel_power - 5.0).abs() < 1e-6);
        // Wetness params — BSVER=130 reads 7 floats (see #403).
        let w = prop.wetness.as_ref().unwrap();
        assert!((w.spec_scale - 0.1).abs() < 1e-6);
        assert!((w.env_map_scale - 0.4).abs() < 1e-6); // BSVER=130 has this
        assert!((w.metalness - 0.6).abs() < 1e-6);
        // #403 regression: unknown_1 is now read for the whole 130..155
        // range (was gated on `> 130` and silently dropped 4 bytes per
        // FO4 lit mesh — observed as 1.87M "4-byte short" warnings on
        // the real FO4 archive sweep).
        assert!(
            (w.unknown_1 - 0.95).abs() < 1e-6,
            "wetness.unknown_1 should round-trip at BSVER=130 (#403)"
        );
        // Shader type data: EnvironmentMap
        match prop.shader_type_data {
            ShaderTypeData::EnvironmentMap { env_map_scale } => {
                assert!((env_map_scale - 0.75).abs() < 1e-6);
            }
            _ => panic!("expected EnvironmentMap"),
        }
        assert_eq!(stream.position(), data.len() as u64);
    }
