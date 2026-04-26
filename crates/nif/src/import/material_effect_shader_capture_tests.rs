//! Tests for `effect_shader_capture_tests` extracted from ../material.rs (refactor stage A).
//!
//! Same qualified path preserved (`effect_shader_capture_tests::FOO`).

    use super::*;
    use crate::blocks::base::NiObjectNETData;
    use crate::blocks::shader::BSEffectShaderProperty;
    use crate::types::BlockRef;

    /// Build a fully-populated FO4-style `BSEffectShaderProperty` with
    /// every field set to a distinct, recognisable value.
    fn fully_populated_fo4_shader() -> BSEffectShaderProperty {
        BSEffectShaderProperty {
            net: NiObjectNETData {
                name: None,
                extra_data_refs: Vec::new(),
                controller_ref: BlockRef::NULL,
            },
            material_reference: false,
            shader_flags_1: 0,
            shader_flags_2: 0,
            sf1_crcs: Vec::new(),
            sf2_crcs: Vec::new(),
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            source_texture: "fx/glow.dds".to_string(),
            texture_clamp_mode: 3,
            lighting_influence: 200,
            env_map_min_lod: 4,
            falloff_start_angle: 0.95,
            falloff_stop_angle: 0.30,
            falloff_start_opacity: 1.0,
            falloff_stop_opacity: 0.0,
            refraction_power: 0.0, // pre-FO76 default
            base_color: [0.0; 4],
            base_color_scale: 1.0,
            soft_falloff_depth: 8.0,
            greyscale_texture: "fx/grad.dds".to_string(),
            env_map_texture: "fx/env.dds".to_string(),
            normal_texture: "fx/n.dds".to_string(),
            env_mask_texture: "fx/mask.dds".to_string(),
            env_map_scale: 1.5,
            reflectance_texture: String::new(),
            lighting_texture: String::new(),
            emittance_color: [0.0; 3],
            emit_gradient_texture: String::new(),
            luminance: None,
        }
    }

    #[test]
    fn capture_lifts_every_rich_field() {
        let shader = fully_populated_fo4_shader();
        let captured = capture_effect_shader_data(&shader);
        assert_eq!(captured.falloff_start_angle, 0.95);
        assert_eq!(captured.falloff_stop_angle, 0.30);
        assert_eq!(captured.falloff_start_opacity, 1.0);
        assert_eq!(captured.falloff_stop_opacity, 0.0);
        assert_eq!(captured.soft_falloff_depth, 8.0);
        assert_eq!(captured.lighting_influence, 200);
        assert_eq!(captured.env_map_min_lod, 4);
        assert_eq!(captured.texture_clamp_mode, 3);
        assert_eq!(captured.env_map_scale, 1.5);
        assert_eq!(captured.greyscale_texture.as_deref(), Some("fx/grad.dds"));
        assert_eq!(captured.env_map_texture.as_deref(), Some("fx/env.dds"));
        assert_eq!(captured.normal_texture.as_deref(), Some("fx/n.dds"));
        assert_eq!(captured.env_mask_texture.as_deref(), Some("fx/mask.dds"));
        // Pre-FO76: refraction_power = 0.0 surfaces as None.
        assert_eq!(captured.refraction_power, None);
    }

    #[test]
    fn capture_collapses_empty_texture_strings_to_none() {
        let mut shader = fully_populated_fo4_shader();
        shader.greyscale_texture.clear();
        shader.env_map_texture.clear();
        shader.normal_texture.clear();
        shader.env_mask_texture.clear();
        let captured = capture_effect_shader_data(&shader);
        assert_eq!(captured.greyscale_texture, None);
        assert_eq!(captured.env_map_texture, None);
        assert_eq!(captured.normal_texture, None);
        assert_eq!(captured.env_mask_texture, None);
    }

    #[test]
    fn capture_surfaces_fo76_refraction_power() {
        let mut shader = fully_populated_fo4_shader();
        shader.refraction_power = 0.5;
        let captured = capture_effect_shader_data(&shader);
        assert_eq!(captured.refraction_power, Some(0.5));
    }

    #[test]
    fn material_info_default_has_no_effect_shader() {
        // Sibling check — the new field defaults to `None` so non-effect
        // materials don't get spurious capture data.
        let info = MaterialInfo::default();
        assert!(info.effect_shader.is_none());
    }
