//! Tests for `path_lookat_tests` extracted from ../controller.rs (refactor stage A).
//!
//! Same qualified path preserved (`path_lookat_tests::FOO`).

    use super::tests::*;
    use super::*;

    #[test]
    fn parse_look_at_controller_32_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // look_at_flags = LOOK_Y_AXIS (bit 1)
        data.extend_from_slice(&0x0002u16.to_le_bytes());
        // look_at_ref = 7
        data.extend_from_slice(&7i32.to_le_bytes());
        assert_eq!(data.len(), 32);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiLookAtController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 32);
        assert_eq!(ctrl.look_at_flags, 0x0002);
        assert_eq!(ctrl.look_at_ref.index(), Some(7));
        assert!(ctrl.base.next_controller_ref.is_null());
    }

    #[test]
    fn parse_path_controller_48_bytes() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        // path_flags
        data.extend_from_slice(&0x0000u16.to_le_bytes());
        // bank_dir = 1 (positive)
        data.extend_from_slice(&1i32.to_le_bytes());
        // max_bank_angle = 0.5 rad
        data.extend_from_slice(&0.5f32.to_le_bytes());
        // smoothing = 0.25
        data.extend_from_slice(&0.25f32.to_le_bytes());
        // follow_axis = 1 (Y)
        data.extend_from_slice(&1i16.to_le_bytes());
        // path_data_ref = 11
        data.extend_from_slice(&11i32.to_le_bytes());
        // percent_data_ref = 12
        data.extend_from_slice(&12i32.to_le_bytes());
        // 26 (base) + 2 + 4 + 4 + 4 + 2 + 4 + 4 = 50
        assert_eq!(data.len(), 50);

        let mut stream = NifStream::new(&data, &header);
        let ctrl = NiPathController::parse(&mut stream).unwrap();
        assert_eq!(stream.position(), 50);
        assert_eq!(ctrl.path_flags, 0);
        assert_eq!(ctrl.bank_dir, 1);
        assert_eq!(ctrl.max_bank_angle, 0.5);
        assert_eq!(ctrl.smoothing, 0.25);
        assert_eq!(ctrl.follow_axis, 1);
        assert_eq!(ctrl.path_data_ref.index(), Some(11));
        assert_eq!(ctrl.percent_data_ref.index(), Some(12));
    }

    #[test]
    fn dispatch_routes_path_and_look_at_controllers() {
        use crate::blocks::parse_block;
        let header = make_header_fnv();

        // ── NiLookAtController ───────────
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0x0004u16.to_le_bytes()); // LOOK_Z_AXIS
        data.extend_from_slice(&3i32.to_le_bytes());
        let size = data.len() as u32;
        let mut stream = NifStream::new(&data, &header);
        let block = parse_block("NiLookAtController", &mut stream, Some(size))
            .expect("NiLookAtController dispatch");
        let c = block.as_any().downcast_ref::<NiLookAtController>().unwrap();
        assert_eq!(c.look_at_flags, 0x0004);
        assert_eq!(c.look_at_ref.index(), Some(3));

        // ── NiPathController ─────────────
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0x0000u16.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes()); // bank_dir = Negative
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.1f32.to_le_bytes());
        data.extend_from_slice(&2i16.to_le_bytes()); // Z
        data.extend_from_slice(&5i32.to_le_bytes());
        data.extend_from_slice(&6i32.to_le_bytes());
        let size = data.len() as u32;
        let mut stream = NifStream::new(&data, &header);
        let block = parse_block("NiPathController", &mut stream, Some(size))
            .expect("NiPathController dispatch");
        let c = block.as_any().downcast_ref::<NiPathController>().unwrap();
        assert_eq!(c.bank_dir, -1);
        assert_eq!(c.follow_axis, 2);
        assert_eq!(c.path_data_ref.index(), Some(5));
        assert_eq!(c.percent_data_ref.index(), Some(6));
    }

    // ── #687 regression guards ────────────────────────────────────────
    //
    // Both perpetrators identified by tracing audit-O5-2 example files
    // — `obgatemini01.nif` (NiGeomMorpherController missing trailing
    // bsver-gated u32 array) and `artrapchannelspikes01.nif`
    // (NiControllerSequence missing the v∈[10.1.0.106,10.4.0.1]
    // `Phase` field). The fix recovered 83 of the 384 truncated
    // Oblivion files (95.21% → 96.24% clean).

    use crate::header::NifHeader;

    fn make_header_pre_oblivion_v10_2() -> NifHeader {
        // Pre-Gamebryo content shipped in Oblivion's BSA — v=10.2.0.0
        // bsver=9 hits the `Phase` window in NiControllerSequence.
        NifHeader {
            version: NifVersion(0x0A020000),
            little_endian: true,
            user_version: 10,
            user_version_2: 9,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        }
    }

    #[test]
    fn nigeommorpher_oblivion_consumes_trailing_unknown_ints() {
        // Layout for v=20.0.0.5 / bsver=11:
        //   NiTimeControllerBase (26 B)
        //   morpher_flags u16 (2 B) + data_ref i32 (4 B) +
        //   always_update u8 (1 B) + num_interpolators u32 (4 B) = 11 B
        //   no interpolator weights for this test (num=0)
        //   trailing num_unknown_ints u32 (4 B) — array empty
        let header = make_header_oblivion();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0u16.to_le_bytes()); // morpher_flags
        data.extend_from_slice(&(-1i32).to_le_bytes()); // data_ref null
        data.push(1); // always_update
        data.extend_from_slice(&0u32.to_le_bytes()); // num_interpolators
        data.extend_from_slice(&0u32.to_le_bytes()); // num_unknown_ints (TRAILING)
        assert_eq!(data.len(), 26 + 11 + 4);

        let mut stream = NifStream::new(&data, &header);
        let _block = NiGeomMorpherController::parse(&mut stream)
            .expect("Oblivion NiGeomMorpherController parses with trailing field");
        assert_eq!(
            stream.position(),
            data.len() as u64,
            "must consume the full Oblivion-trailing layout, not stop at the \
             interpolator-weights end (pre-fix #687 stopped 4 bytes early, \
             cascading drift into NiMorphData)"
        );
    }

    #[test]
    fn nigeommorpher_fnv_skips_trailing_unknown_ints() {
        // FNV bsver=34 — the (BSVER <= 11) gate excludes the trailing
        // u32. Confirms the fix is Oblivion-only and doesn't regress
        // FNV/FO3 (clean rate must remain 100%).
        let header = make_header_fnv();
        let mut data = Vec::new();
        write_time_controller_base(&mut data);
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.push(1);
        data.extend_from_slice(&0u32.to_le_bytes());
        // No trailing field — FNV layout ends here.
        let original_len = data.len();
        // Pad with 4 sentinel bytes that MUST NOT be consumed.
        data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        NiGeomMorpherController::parse(&mut stream).expect("FNV path parses");
        assert_eq!(
            stream.position(),
            original_len as u64,
            "FNV (bsver=34) must NOT read the bsver<=11-gated trailing field \
             — over-consuming would shift downstream blocks"
        );
    }

    #[test]
    fn nicontrollersequence_v10_2_reads_phase() {
        // Pre-Oblivion v=10.2.0.0 content. Layout for the trailing
        // fields: weight + text_keys + cycle_type + frequency +
        // **phase** (here) + start_time + stop_time + manager +
        // accum_root_name + deprecated_string_palette_ref.
        //
        // Pre-fix #687 the parser jumped from `frequency` straight
        // to `start_time`, reading the on-disk `phase` slot as
        // `start_time` and shifting every later field by 4 bytes.
        // accum_root_name's u32 length was then read from
        // stop_time, decoding the first 3 chars of the real
        // accum_root_name and bleeding the rest into the next block.
        let header = make_header_pre_oblivion_v10_2();
        let mut data = Vec::new();
        // name (empty inline)
        data.extend_from_slice(&0u32.to_le_bytes());
        // num_controlled_blocks = 0
        data.extend_from_slice(&0u32.to_le_bytes());
        // array_grow_by (since 10.1.0.106) = 1
        data.extend_from_slice(&1u32.to_le_bytes());
        // weight=1.0, text_keys=null, cycle_type=2 (LOOP), frequency=1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // phase=0.5 — distinctive sentinel
        data.extend_from_slice(&0.5f32.to_le_bytes());
        // start_time=0.0, stop_time=7.36
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&7.36f32.to_le_bytes());
        // manager_ref=3
        data.extend_from_slice(&3u32.to_le_bytes());
        // accum_root_name = "Root" (4 chars)
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(b"Root");
        // deprecated_string_palette_ref (since 10.1.0.113) = -1
        data.extend_from_slice(&(-1i32).to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let seq = NiControllerSequence::parse(&mut stream)
            .expect("v=10.2.0.0 NiControllerSequence parses with phase");
        assert_eq!(
            stream.position(),
            data.len() as u64,
            "must consume the full v=10.2.0.0 layout including the Phase field"
        );
        assert!(
            (seq.phase - 0.5).abs() < 1e-6,
            "phase routes to its own struct field, not start_time"
        );
        assert_eq!(seq.start_time, 0.0, "start_time stays at 0 (not the phase value)");
        assert!(
            (seq.stop_time - 7.36).abs() < 1e-6,
            "stop_time follows phase, not the manager_ref slot"
        );
        assert_eq!(
            seq.accum_root_name.as_deref(),
            Some("Root"),
            "accum_root_name reads its own string, not part of stop_time"
        );
    }

    #[test]
    fn nicontrollersequence_oblivion_skips_phase() {
        // Oblivion v=20.0.0.5 is past the Phase window's `until="10.4.0.1"`.
        // Layout has no phase field — confirming the fix doesn't
        // over-consume on Oblivion's NiControllerSequence (which is the
        // primary KF-file consumer and was previously working).
        let header = make_header_oblivion();
        let mut data = Vec::new();
        // name empty + num_controlled=0 + array_grow_by=1
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        // weight + text_keys + cycle_type + frequency
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&(-1i32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // (no phase on Oblivion)
        data.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
        data.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
        data.extend_from_slice(&(-1i32).to_le_bytes()); // manager
        data.extend_from_slice(&0u32.to_le_bytes()); // accum_root_name empty
        // deprecated_string_palette_ref (within the [10.1.0.113, 20.1.0.1) window)
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // anim notes: bsver=11 — `(24..=28).contains(&bsver)` false,
        // bsver > 28 false → empty Vec (no bytes read).

        let original_len = data.len();
        // Sentinel that MUST NOT be consumed — over-consuming would
        // mean the Oblivion path is reading a phase field it shouldn't.
        data.extend_from_slice(&0xDEADBEEFu32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let seq = NiControllerSequence::parse(&mut stream)
            .expect("Oblivion NiControllerSequence parses without phase");
        assert_eq!(
            stream.position(),
            original_len as u64,
            "Oblivion (v=20.0.0.5) must NOT read Phase — that field is \
             gated to v ≤ 10.4.0.1"
        );
        assert_eq!(seq.phase, 0.0, "phase defaults to 0 outside the gated window");
    }
