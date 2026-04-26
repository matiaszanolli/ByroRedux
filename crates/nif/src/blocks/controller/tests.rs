//! Tests for `tests` extracted from ../controller.rs (refactor stage A).
//!
//! Same qualified path preserved (`tests::FOO`).

    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::NifVersion;

    pub(super) fn make_header_fnv() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 11,
            user_version_2: 34,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("TestName")],
            max_string_length: 8,
            num_groups: 0,
        }
    }

    pub(super) fn write_time_controller_base(data: &mut Vec<u8>) {
        // next_controller_ref: -1
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // flags: 0x000C
        data.extend_from_slice(&0x000Cu16.to_le_bytes());
        // frequency: 1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // phase: 0.0
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // start_time: 0.0
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // stop_time: 1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // target_ref: 0
        data.extend_from_slice(&0i32.to_le_bytes());
    }

    #[test]
    fn parse_ni_time_controller_base_26_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        assert_eq!(data.len(), 26);
        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiTimeController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 26);
        assert!(ctrl.base.next_controller_ref.is_null());
    }

    #[test]
    fn parse_single_interp_controller_30_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // interpolator_ref: 5
        data.extend_from_slice(&5i32.to_le_bytes());
        assert_eq!(data.len(), 30);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiSingleInterpController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 30);
        assert_eq!(ctrl.interpolator_ref.index(), Some(5));
    }

    /// Regression for #551 — `bhkBlendController` must parse as
    /// `NiTimeController` base (26 B) + `Keys: uint` (4 B) = 30 B
    /// total per nif.xml line 3927. Pre-fix this block had no dispatch
    /// arm and 1,427 FNV+FO3 vanilla blocks fell into NiUnknown.
    ///
    /// Contrary to the audit's suggestion, this is NOT a
    /// NiSingleInterpController — it inherits NiTimeController directly.
    #[test]
    fn parse_bhk_blend_controller_30_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // keys: uint — "Seems to be always zero" per nif.xml, but write
        // a non-zero value so the test would catch a u32 vs i32 mix-up.
        data.extend_from_slice(&0x12345678u32.to_le_bytes());
        assert_eq!(data.len(), 30);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = BhkBlendController::parse(&mut stream).unwrap();
        assert_eq!(
            stream.position(),
            30,
            "bhkBlendController must consume exactly NiTimeController(26) + u32(4) = 30 B"
        );
        assert_eq!(ctrl.keys, 0x12345678);
        assert!(ctrl.base.next_controller_ref.is_null());
    }

    /// Regression for #551 — dispatch must route `bhkBlendController`
    /// through `BhkBlendController::parse`, not the `NiTimeController`
    /// fallback. Verifies the block_type_name() round-trip.
    #[test]
    fn bhk_blend_controller_dispatches_via_parse_block() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0u32.to_le_bytes()); // keys = 0

        let mut stream = NifStream::new(&data, &header);
        let block = crate::blocks::parse_block(
            "bhkBlendController",
            &mut stream,
            Some(data.len() as u32),
        )
        .expect("dispatch must route bhkBlendController — pre-fix it was NiUnknown");
        assert_eq!(block.block_type_name(), "bhkBlendController");
        assert_eq!(
            stream.position() as usize,
            data.len(),
            "dispatcher must consume the full 30-byte body"
        );
        let ctrl = block
            .as_any()
            .downcast_ref::<BhkBlendController>()
            .expect("dispatch type must be BhkBlendController, not NiTimeController");
        assert_eq!(ctrl.keys, 0);
    }

    /// Regression for #552 — `BSNiAlphaPropertyTestRefController` must
    /// dispatch and parse as `NiSingleInterpController` (nif.xml line
    /// 6279: inherits NiFloatInterpController, no extra fields).
    /// Pre-fix 751 Skyrim SE vanilla blocks fell into NiUnknown.
    /// The newtype wrapper preserves the RTTI name so telemetry and
    /// the future alpha-animation importer can match on it.
    #[test]
    fn bs_ni_alpha_property_test_ref_controller_dispatches() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&7i32.to_le_bytes()); // interpolator_ref = 7
        assert_eq!(data.len(), 30);

        let mut stream = NifStream::new(&data, &header);
        let block = crate::blocks::parse_block(
            "BSNiAlphaPropertyTestRefController",
            &mut stream,
            Some(data.len() as u32),
        )
        .expect("dispatch must route BSNiAlphaPropertyTestRefController");
        assert_eq!(
            stream.position() as usize,
            data.len(),
            "must consume 26 B TimeController base + 4 B interpolator_ref"
        );
        assert_eq!(
            block.block_type_name(),
            "BSNiAlphaPropertyTestRefController",
            "newtype wrapper preserves RTTI for downstream dispatch"
        );
        let ctrl = block
            .as_any()
            .downcast_ref::<BsNiAlphaPropertyTestRefController>()
            .expect("dispatch type must be BsNiAlphaPropertyTestRefController");
        assert_eq!(ctrl.base.interpolator_ref.index(), Some(7));
    }

    /// Regression for #553 — `NiFloatExtraDataController` must parse
    /// as `NiTimeController` base (26 B) + `interpolator_ref` (4 B,
    /// since 10.1.0.104) + `extra_data_name` string index (4 B, since
    /// 10.2.0.0) = 34 B on FO3+/FNV/SE. Pre-fix no dispatch arm
    /// existed.
    #[test]
    fn parse_ni_float_extra_data_controller_34_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&11i32.to_le_bytes()); // interpolator_ref = 11
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_name: string idx 0
        assert_eq!(data.len(), 34);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiFloatExtraDataController::parse(&mut stream)
            .expect("NiFloatExtraDataController must parse at FNV bsver");
        assert_eq!(stream.position(), 34);
        assert_eq!(ctrl.interpolator_ref.index(), Some(11));
        assert_eq!(ctrl.extra_data_name.as_deref(), Some("TestName"));
    }

    /// Regression for #553 — dispatcher must route
    /// `NiFloatExtraDataController` through its own parser, not the
    /// `NiTimeController` fallback stub (which would leave interpolator_ref
    /// and extra_data_name unread and drift subsequent blocks).
    #[test]
    fn ni_float_extra_data_controller_dispatches_via_parse_block() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&5i32.to_le_bytes()); // interpolator_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // extra_data_name idx

        let mut stream = NifStream::new(&data, &header);
        let block = crate::blocks::parse_block(
            "NiFloatExtraDataController",
            &mut stream,
            Some(data.len() as u32),
        )
        .expect("dispatch must route NiFloatExtraDataController");
        assert_eq!(block.block_type_name(), "NiFloatExtraDataController");
        assert_eq!(stream.position() as usize, data.len());
        let ctrl = block
            .as_any()
            .downcast_ref::<NiFloatExtraDataController>()
            .expect("dispatch type must be NiFloatExtraDataController");
        assert_eq!(ctrl.interpolator_ref.index(), Some(5));
    }

    /// Regression for #433 — `NiLightColorController` must parse as
    /// `NiTimeController` base (26 B) + `interpolator_ref` (4 B, since
    /// 10.1.0.104) + `target_color: u16` (since 10.1.0.0) = 32 B.
    /// Pre-fix the block had no dispatch arm — every animated light
    /// color (lantern ambient shift, magic-spell glow color cycling)
    /// landed as NiUnknown and silently stopped animating.
    #[test]
    fn parse_ni_light_color_controller_32_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&9i32.to_le_bytes()); // interpolator_ref = 9
        // target_color: 1 = Ambient (LightColor enum nif.xml line 1241).
        data.extend_from_slice(&1u16.to_le_bytes());
        assert_eq!(data.len(), 32);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiLightColorController::parse(&mut stream)
            .expect("NiLightColorController must parse at FNV bsver");
        assert_eq!(stream.position(), 32);
        assert_eq!(ctrl.interpolator_ref.index(), Some(9));
        assert_eq!(
            ctrl.target_color, 1,
            "target_color = 1 (Ambient) — pre-fix this field was never \
             read and block-size recovery silently elided it"
        );
    }

    /// Regression for #433 — the three plain `NiFloatInterpController`
    /// subclasses (NiLightDimmerController, NiLightIntensityController,
    /// NiLightRadiusController) share the 30-byte `NiSingleInterpController`
    /// layout with no additional fields (nif.xml lines 3750 / 5025 / 8444).
    /// Dispatcher routes them through `NiLightFloatController::parse` so
    /// `block_type_name()` reports the original subclass.
    #[test]
    fn ni_light_float_controller_dispatches_preserving_rtti() {
        for type_name in [
            "NiLightDimmerController",
            "NiLightIntensityController",
            "NiLightRadiusController",
        ] {
            let header = make_header_fnv();
            let mut data = Vec::new();
            write_time_controller_base(&mut data);
            data.extend_from_slice(&7i32.to_le_bytes()); // interpolator_ref

            let mut stream = NifStream::new(&data, &header);
            let block = crate::blocks::parse_block(
                type_name,
                &mut stream,
                Some(data.len() as u32),
            )
            .unwrap_or_else(|e| panic!("{type_name} dispatch failed: {e}"));
            assert_eq!(
                stream.position() as usize,
                data.len(),
                "{type_name} must consume the 30-byte NiSingleInterpController body"
            );
            assert_eq!(
                block.block_type_name(),
                type_name,
                "NiLightFloatController must preserve RTTI via its type_name field"
            );
            let ctrl = block
                .as_any()
                .downcast_ref::<NiLightFloatController>()
                .expect("dispatch type must be NiLightFloatController");
            assert_eq!(ctrl.base.interpolator_ref.index(), Some(7));
        }
    }

    #[test]
    fn parse_bs_refraction_fire_period_controller_30_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // interpolator_ref: 3
        data.extend_from_slice(&3i32.to_le_bytes());
        assert_eq!(data.len(), 30);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = BsRefractionFirePeriodController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 30);
        assert_eq!(ctrl.interpolator_ref.index(), Some(3));
        assert!(ctrl.base.next_controller_ref.is_null());
    }

    #[test]
    fn parse_material_color_controller_32_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&3i32.to_le_bytes()); // interpolator_ref
        data.extend_from_slice(&1u16.to_le_bytes()); // target_color
        assert_eq!(data.len(), 32);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiMaterialColorController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 32);
        assert_eq!(ctrl.target_color, 1);
    }

    #[test]
    fn parse_multi_target_transform_controller() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // num_extra_targets: 4
        data.extend_from_slice(&4u16.to_le_bytes());
        // 4 target refs
        for i in 0..4 {
            data.extend_from_slice(&(i as i32).to_le_bytes());
        }
        assert_eq!(data.len(), 44);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiMultiTargetTransformController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 44);
        assert_eq!(ctrl.extra_targets.len(), 4);
    }

    #[test]
    fn parse_controller_manager_1_sequence() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.push(1); // cumulative = true (byte bool)
        data.extend_from_slice(&1u32.to_le_bytes()); // num_sequences
        data.extend_from_slice(&7i32.to_le_bytes()); // sequence_refs[0]
        data.extend_from_slice(&8i32.to_le_bytes()); // object_palette_ref
        assert_eq!(data.len(), 39);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiControllerManager::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 39);
        assert!(ctrl.cumulative);
        assert_eq!(ctrl.sequence_refs.len(), 1);
        assert_eq!(ctrl.sequence_refs[0].index(), Some(7));
        assert_eq!(ctrl.object_palette_ref.index(), Some(8));
    }

    /// Regression: #350 / S5-02. Every BSShaderProperty*Controller
    /// block carries a trailing u32 enum identifying the driven slot.
    /// Pre-fix the dispatch discarded the value (`_controlled_variable`)
    /// and emitted `Box<NiSingleInterpController>`, so the animation
    /// importer had no way to learn which shader uniform to drive. The
    /// typed `BsShaderController` now preserves the enum in
    /// `ShaderControllerKind` and reports its original RTTI name.
    #[test]
    fn parse_bs_shader_controller_preserves_controlled_variable() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data); // 26 bytes
                                                 // NiSingleInterpController: interpolator_ref (since 10.1.0.104,
                                                 // FNV v=20.2.0.7 is above that).
        data.extend_from_slice(&5i32.to_le_bytes()); // interpolator_ref
                                                      // BSShaderController trailing enum.
        data.extend_from_slice(&3u32.to_le_bytes()); // controlled_variable = 3
        assert_eq!(data.len(), 34);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = BsShaderController::parse(&mut stream, "BSEffectShaderPropertyFloatController")
            .expect("shader controller with 4-byte enum tail must parse");
        assert_eq!(stream.position() as usize, data.len());
        assert_eq!(ctrl.type_name, "BSEffectShaderPropertyFloatController");
        assert_eq!(ctrl.base.interpolator_ref.index(), Some(5));
        assert_eq!(ctrl.kind, ShaderControllerKind::EffectFloat(3));
    }

    /// Each of the five controller type names must map to its own
    /// `ShaderControllerKind` variant so downstream dispatch can match
    /// on the kind rather than re-parsing the type string. Verifies the
    /// u32 payload rides through identically on all five.
    #[test]
    fn parse_bs_shader_controller_dispatches_all_five_kinds() {
        let header = make_header_fnv();
        for (type_name, expected) in [
            (
                "BSEffectShaderPropertyFloatController",
                ShaderControllerKind::EffectFloat(7),
            ),
            (
                "BSEffectShaderPropertyColorController",
                ShaderControllerKind::EffectColor(7),
            ),
            (
                "BSLightingShaderPropertyFloatController",
                ShaderControllerKind::LightingFloat(7),
            ),
            (
                "BSLightingShaderPropertyColorController",
                ShaderControllerKind::LightingColor(7),
            ),
            (
                "BSLightingShaderPropertyUShortController",
                ShaderControllerKind::LightingUShort(7),
            ),
        ] {
            let mut data = Vec::new();
            write_time_controller_base(&mut data);
            data.extend_from_slice(&0i32.to_le_bytes()); // interpolator_ref
            data.extend_from_slice(&7u32.to_le_bytes()); // controlled_variable

            let mut stream = NifStream::new(&data, &header);
            let ctrl = BsShaderController::parse(&mut stream, type_name).unwrap_or_else(|e| {
                panic!("{type_name} should parse: {e}");
            });
            assert_eq!(
                stream.position() as usize,
                data.len(),
                "{type_name} must consume all 34 bytes"
            );
            assert_eq!(ctrl.kind, expected, "{type_name} dispatched to wrong kind");
            assert_eq!(ctrl.type_name, type_name);
        }
    }

    #[test]
    fn parse_controller_sequence_no_blocks() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        // NiSequence: name (string table index 0)
        data.extend_from_slice(&0i32.to_le_bytes());
        // num_controlled_blocks: 0
        data.extend_from_slice(&0u32.to_le_bytes());
        // array_grow_by: 0
        data.extend_from_slice(&0u32.to_le_bytes());
        // NiControllerSequence fields:
        data.extend_from_slice(&1.0f32.to_le_bytes()); // weight
        data.extend_from_slice(&(-1i32).to_le_bytes()); // text_keys_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // cycle_type
        data.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
        data.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
        data.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
        data.extend_from_slice(&(-1i32).to_le_bytes()); // manager_ref
        data.extend_from_slice(&(-1i32).to_le_bytes()); // accum_root_name
                                                        // anim note arrays (BSVER > 28 = yes for FNV)
        data.extend_from_slice(&0u16.to_le_bytes()); // num_anim_note_arrays
        let expected_len = data.len();

        let mut stream = NifStream::new(&data, &header);
        let seq = NiControllerSequence::parse(&mut stream).unwrap();
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(seq.name.as_deref(), Some("TestName"));
        assert_eq!(seq.controlled_blocks.len(), 0);
        assert!(seq.text_keys_ref.is_null());
    }

    /// Build an Oblivion-era header (v20.0.0.5, user_version=11, uv2=11).
    /// String table is empty — Oblivion doesn't use it, and per-block
    /// strings go through the NiStringPalette format instead.
    pub(super) fn make_header_oblivion() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_0_0_5,
            little_endian: true,
            user_version: 11,
            user_version_2: 11,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    /// Regression test for issue #107: Oblivion .kf files encode the
    /// ControlledBlock string fields via a NiStringPalette block ref +
    /// five byte offsets (since 10.2.0.0, until 20.1.0.0). The old
    /// parser called `read_string` unconditionally and mis-parsed the
    /// first u32 offset as a string length, shifting the stream and
    /// cascading into corrupted downstream blocks. The fix switches to
    /// a version branch; this test pins the Oblivion path.
    #[test]
    fn parse_controller_sequence_oblivion_string_palette_format() {
        let header = make_header_oblivion();
        let mut data = Vec::new();

        // NiSequence pre-10.1 string encoding: `read_string` returns
        // Ok(None) on len=0, so a 4-byte zero-length acts as an empty
        // "name" header field.
        data.extend_from_slice(&0u32.to_le_bytes()); // name: empty inline string
        data.extend_from_slice(&1u32.to_le_bytes()); // num_controlled_blocks
        data.extend_from_slice(&0u32.to_le_bytes()); // array_grow_by

        // One ControlledBlock in Oblivion palette format:
        //   interpolator_ref (i32)
        //   controller_ref   (i32)
        //   priority         (u8)          — bsver=11 > 0, so present
        //   string_palette_ref (i32)
        //   node_name_offset        (u32)
        //   property_type_offset    (u32)
        //   controller_type_offset  (u32)
        //   controller_id_offset    (u32)
        //   interpolator_id_offset  (u32)
        data.extend_from_slice(&12i32.to_le_bytes()); // interpolator_ref
        data.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
        data.push(42); // priority
        data.extend_from_slice(&9i32.to_le_bytes()); // string_palette_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // node_name_offset
        data.extend_from_slice(&6u32.to_le_bytes()); // property_type_offset
        data.extend_from_slice(&11u32.to_le_bytes()); // controller_type_offset
        data.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // controller_id_offset (unset sentinel)
        data.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // interpolator_id_offset

        // NiControllerSequence trailer (same on all post-10.1 paths).
        data.extend_from_slice(&1.0f32.to_le_bytes()); // weight
        data.extend_from_slice(&(-1i32).to_le_bytes()); // text_keys_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // cycle_type
        data.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
        data.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
        data.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
        data.extend_from_slice(&(-1i32).to_le_bytes()); // manager_ref
        data.extend_from_slice(&0u32.to_le_bytes()); // accum_root_name: empty inline
        // #402 — Oblivion (v ∈ [10.1.0.113, 20.1.0.1)) trails a
        // Ref<NiStringPalette>. Gamebryo 2.3's LoadBinary reads this so
        // the legacy IDTag palette offsets can be converted to
        // NiFixedStrings during link; on-disk it sits between
        // accum_root_name and the anim-note block.
        data.extend_from_slice(&9i32.to_le_bytes()); // deprecated string palette ref

        // Oblivion bsver=11, 11 <= 28 → no anim note list, so don't
        // append anything here.

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let seq = NiControllerSequence::parse(&mut stream)
            .expect("Oblivion NiControllerSequence must parse the palette format");
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "Oblivion parse consumed {} bytes, expected {}",
            stream.position(),
            expected_len,
        );

        assert_eq!(seq.controlled_blocks.len(), 1);
        let cb = &seq.controlled_blocks[0];
        assert_eq!(cb.interpolator_ref.index(), Some(12));
        assert!(cb.controller_ref.is_null());
        assert_eq!(cb.priority, 42);
        // Palette fields must be populated, name fields left None.
        assert_eq!(cb.string_palette_ref.index(), Some(9));
        assert_eq!(cb.node_name_offset, 0);
        assert_eq!(cb.property_type_offset, 6);
        assert_eq!(cb.controller_type_offset, 11);
        assert_eq!(cb.controller_id_offset, 0xFFFF_FFFF);
        assert_eq!(cb.interpolator_id_offset, 0xFFFF_FFFF);
        assert!(cb.node_name.is_none());
        assert!(cb.property_type.is_none());
    }
