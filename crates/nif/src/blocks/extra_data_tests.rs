//! Unit tests for the extra-data block parsers. Extracted from
//! `extra_data.rs` to keep the production code under ~1500 lines;
//! pulled in via `#[cfg(test)] #[path = "..."] mod tests;`.

use super::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;

fn oblivion_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_0_0_5,
        little_endian: true,
        user_version: 0,
        user_version_2: 11, // BSVER=11 for Oblivion
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

fn skyrim_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 83, // BSVER=83 for Skyrim
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
fn bs_furniture_marker_oblivion() {
    // Oblivion wire layout: inline name (len=0 → None), u32 count,
    // then each position: vec3 offset + u16 orientation + u8 ref1 + u8 ref2.
    let header = oblivion_header();
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes()); // inline string: empty
    data.extend_from_slice(&2u32.to_le_bytes()); // 2 positions
                                                 // Position 0
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&2.0f32.to_le_bytes());
    data.extend_from_slice(&3.0f32.to_le_bytes());
    data.extend_from_slice(&0x1234u16.to_le_bytes()); // orientation
    data.push(0x56u8); // ref1
    data.push(0x78u8); // ref2
                       // Position 1
    data.extend_from_slice(&4.0f32.to_le_bytes());
    data.extend_from_slice(&5.0f32.to_le_bytes());
    data.extend_from_slice(&6.0f32.to_le_bytes());
    data.extend_from_slice(&0x9abcu16.to_le_bytes());
    data.push(0xdeu8);
    data.push(0xefu8);

    let mut stream = NifStream::new(&data, &header);
    let marker = BsFurnitureMarker::parse(&mut stream, "BSFurnitureMarker").unwrap();

    assert_eq!(marker.type_name, "BSFurnitureMarker");
    assert_eq!(marker.positions.len(), 2);
    assert_eq!(marker.positions[0].offset, [1.0, 2.0, 3.0]);
    match marker.positions[0].data {
        FurniturePositionData::Legacy {
            orientation,
            position_ref_1,
            position_ref_2,
        } => {
            assert_eq!(orientation, 0x1234);
            assert_eq!(position_ref_1, 0x56);
            assert_eq!(position_ref_2, 0x78);
        }
        _ => panic!("expected Legacy variant for Oblivion (BSVER=11)"),
    }
    assert_eq!(stream.position() as usize, data.len());
}

#[test]
fn bs_furniture_marker_skyrim() {
    // Skyrim wire layout: string-table name (-1 = None), u32 count,
    // then each position: vec3 offset + f32 heading + u16 anim + u16 entry.
    let header = skyrim_header();
    let mut data = Vec::new();
    data.extend_from_slice(&(-1i32).to_le_bytes()); // string table: None
    data.extend_from_slice(&1u32.to_le_bytes()); // 1 position
    data.extend_from_slice(&10.0f32.to_le_bytes());
    data.extend_from_slice(&20.0f32.to_le_bytes());
    data.extend_from_slice(&30.0f32.to_le_bytes());
    data.extend_from_slice(&1.5707964f32.to_le_bytes()); // heading ≈ π/2
    data.extend_from_slice(&1u16.to_le_bytes()); // AnimationType::Sit
    data.extend_from_slice(&0x0003u16.to_le_bytes()); // Entry: Front|Behind

    let mut stream = NifStream::new(&data, &header);
    let marker = BsFurnitureMarker::parse(&mut stream, "BSFurnitureMarkerNode").unwrap();

    assert_eq!(marker.type_name, "BSFurnitureMarkerNode");
    assert_eq!(marker.positions.len(), 1);
    assert_eq!(marker.positions[0].offset, [10.0, 20.0, 30.0]);
    match marker.positions[0].data {
        FurniturePositionData::Modern {
            heading,
            animation_type,
            entry_properties,
        } => {
            assert!((heading - 1.5707964).abs() < 1e-6);
            assert_eq!(animation_type, 1);
            assert_eq!(entry_properties, 0x0003);
        }
        _ => panic!("expected Modern variant for Skyrim (BSVER=83)"),
    }
    assert_eq!(stream.position() as usize, data.len());
}

/// Regression: #106 — `BSBehaviorGraphExtraData.Controls Base
/// Skeleton` is a 1-byte bool per nif.xml line 8192. Pre-fix the
/// parser read a u32 (4 bytes), desyncing every Skyrim skeleton
/// NIF with a behavior-graph reference by 3 bytes. Block-size
/// recovery realigned the next block, but the tail of every
/// behavior-graph block was silently misread.
#[test]
fn behavior_graph_extra_data_reads_bool_as_one_byte_on_skyrim() {
    let header = skyrim_header();
    let mut data = Vec::new();
    // name: string-table index = -1 (None).
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // behaviour_graph_file: string-table index = -1 (None).
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // controls_base_skeleton: 1 byte (true).
    data.push(0x01u8);
    // Block end. Total: 4 + 4 + 1 = 9 bytes.
    assert_eq!(data.len(), 9);

    let mut stream = NifStream::new(&data, &header);
    let block = BsBehaviorGraphExtraData::parse(&mut stream).unwrap();
    assert!(block.controls_base_skeleton);
    // Critical assertion — the parser must consume EXACTLY 9 bytes,
    // not 12. Pre-fix we'd consume 4 + 4 + 4 = 12 and trip a
    // truncation EOF or read 3 bytes from the next block.
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "BSBehaviorGraphExtraData must consume the bool as 1 byte on Skyrim"
    );
}

/// Sibling — the `false` case must also consume exactly 1 byte.
#[test]
fn behavior_graph_extra_data_reads_false_as_one_byte() {
    let header = skyrim_header();
    let mut data = Vec::new();
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.push(0x00u8); // false

    let mut stream = NifStream::new(&data, &header);
    let block = BsBehaviorGraphExtraData::parse(&mut stream).unwrap();
    assert!(!block.controls_base_skeleton);
    assert_eq!(stream.position() as usize, data.len());
}

/// Regression for #413 / FO4-D5-M1: the audit observed 736
/// over-read warnings on `Fallout4 - Meshes.ba2` ("expected 9
/// bytes, consumed 12"). The root-cause fix landed in #106 —
/// `read_bool` is version-aware and returns 1 byte for any
/// `version >= 4.1.0.1`, covering Skyrim (BSVER=83), FO4
/// (BSVER=130), FO76 (BSVER=155), and Starfield (BSVER=174).
/// The existing Skyrim test already pinned the 9-byte consumption
/// invariant; this test extends the guarantee to BSVER=130 so a
/// future refactor that accidentally gates the bool width on
/// `BSVER < 130` instead of `version < 4.1.0.1` fails here. Live
/// FO4 BA2 sweep confirms zero `BSBehaviorGraphExtraData` size
/// mismatches post-fix.
#[test]
fn behavior_graph_extra_data_reads_nine_bytes_on_fo4() {
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130, // BSVER=130 for FO4
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };

    let mut data = Vec::new();
    data.extend_from_slice(&(-1i32).to_le_bytes()); // name absent
    data.extend_from_slice(&(-1i32).to_le_bytes()); // behaviour_graph_file absent
    data.push(0x01u8); // controls_base_skeleton = true (1 byte!)
    assert_eq!(data.len(), 9, "FO4 wire size must be 9 bytes");

    let mut stream = NifStream::new(&data, &header);
    let block = BsBehaviorGraphExtraData::parse(&mut stream).unwrap();
    assert!(block.controls_base_skeleton);
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "FO4 must consume 9 bytes — previous u32 bool misread 12 and \
         triggered 736 warnings across Fallout4 - Meshes.ba2 pre-#106"
    );
}

// ── BSAnimNote / BSAnimNotes regression tests (#432) ──────────────
//
// Each test asserts the parser consumes exactly the right number of
// bytes and produces the right typed payload. Exact consumption is
// load-bearing for Oblivion's block-sizes-less recovery path — if we
// under-read, the next block's start offset is wrong and the whole
// file cascades into NiUnknown.

#[test]
fn bs_anim_note_invalid_consumes_type_plus_time() {
    let header = skyrim_header();
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes()); // type = INVALID
    data.extend_from_slice(&1.25f32.to_le_bytes()); // time
    let mut stream = NifStream::new(&data, &header);
    let note = BsAnimNote::parse(&mut stream).unwrap();
    assert_eq!(note.kind, AnimNoteType::Invalid);
    assert_eq!(note.time, 1.25);
    assert!(note.arm.is_none() && note.gain.is_none() && note.state.is_none());
    assert_eq!(stream.position() as usize, 8, "INVALID reads only 8 bytes");
}

#[test]
fn bs_anim_note_grabik_reads_arm_only() {
    let header = skyrim_header();
    let mut data = Vec::new();
    data.extend_from_slice(&1u32.to_le_bytes()); // type = GRABIK
    data.extend_from_slice(&0.5f32.to_le_bytes()); // time
    data.extend_from_slice(&1u32.to_le_bytes()); // arm (right hand)
    let mut stream = NifStream::new(&data, &header);
    let note = BsAnimNote::parse(&mut stream).unwrap();
    assert_eq!(note.kind, AnimNoteType::GrabIk);
    assert_eq!(note.time, 0.5);
    assert_eq!(note.arm, Some(1));
    assert!(note.gain.is_none());
    assert!(note.state.is_none());
    assert_eq!(stream.position() as usize, 12, "GRABIK reads 4+4+4");
}

#[test]
fn bs_anim_note_lookik_reads_gain_and_state() {
    let header = skyrim_header();
    let mut data = Vec::new();
    data.extend_from_slice(&2u32.to_le_bytes()); // type = LOOKIK
    data.extend_from_slice(&2.0f32.to_le_bytes()); // time
    data.extend_from_slice(&0.75f32.to_le_bytes()); // gain
    data.extend_from_slice(&3u32.to_le_bytes()); // state
    let mut stream = NifStream::new(&data, &header);
    let note = BsAnimNote::parse(&mut stream).unwrap();
    assert_eq!(note.kind, AnimNoteType::LookIk);
    assert_eq!(note.time, 2.0);
    assert_eq!(note.gain, Some(0.75));
    assert_eq!(note.state, Some(3));
    assert!(note.arm.is_none());
    assert_eq!(stream.position() as usize, 16, "LOOKIK reads 4+4+4+4");
}

#[test]
fn bs_anim_note_unknown_type_is_preserved_and_stops_at_time() {
    // Bethesda occasionally ships out-of-range AnimNoteType values
    // on older content. The parser preserves the raw value and
    // stops reading — the conditional tail is only present for the
    // known enum values, not for the unknown ones.
    let header = skyrim_header();
    let mut data = Vec::new();
    data.extend_from_slice(&42u32.to_le_bytes());
    data.extend_from_slice(&0.0f32.to_le_bytes());
    let mut stream = NifStream::new(&data, &header);
    let note = BsAnimNote::parse(&mut stream).unwrap();
    assert_eq!(note.kind, AnimNoteType::Unknown(42));
    assert_eq!(stream.position() as usize, 8);
}

#[test]
fn bs_anim_notes_parses_array_of_refs() {
    let header = skyrim_header();
    let mut data = Vec::new();
    data.extend_from_slice(&3u16.to_le_bytes()); // count = 3
    data.extend_from_slice(&10i32.to_le_bytes());
    data.extend_from_slice(&11i32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes()); // NULL ref
    let mut stream = NifStream::new(&data, &header);
    let notes = BsAnimNotes::parse(&mut stream).unwrap();
    assert_eq!(notes.notes.len(), 3);
    assert_eq!(notes.notes[0].index(), Some(10));
    assert_eq!(notes.notes[1].index(), Some(11));
    assert_eq!(notes.notes[2].index(), None);
    assert_eq!(
        stream.position() as usize,
        14,
        "2 bytes for count + 3 × 4 bytes for refs = 14"
    );
}

#[test]
fn bs_anim_notes_zero_count_reads_only_header() {
    let header = skyrim_header();
    let data = 0u16.to_le_bytes();
    let mut stream = NifStream::new(&data, &header);
    let notes = BsAnimNotes::parse(&mut stream).unwrap();
    assert!(notes.notes.is_empty());
    assert_eq!(stream.position() as usize, 2);
}

/// Regression: #329. Pre-10.0.1.0 streams have no `Name` field on
/// `NiExtraData` per nif.xml (`since="10.0.1.0"`). BsBound is only
/// ever parsed via the subclass dispatcher on Bethesda content
/// (which is >= 10.0.1.0), but a fuzzed or non-Bethesda file can
/// hit these subclass parsers directly — `read_extra_data_name`
/// must NOT consume any bytes on those streams.
#[test]
fn read_extra_data_name_returns_none_pre_10_0_1_0() {
    let header = NifHeader {
        version: NifVersion(0x0A000006), // 10.0.0.6 — just below the gate
        little_endian: true,
        user_version: 0,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };
    // Body is 24 bytes of BsBound (center + dimensions); no name
    // prefix on this version per the gate.
    let mut data = Vec::new();
    for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    let mut stream = NifStream::new(&data, &header);
    let bound = BsBound::parse(&mut stream).expect("pre-10.0.1.0 BsBound should parse");
    assert!(bound.name.is_none(), "pre-10.0.1.0 has no Name field");
    assert_eq!(bound.center, [1.0, 2.0, 3.0]);
    assert_eq!(bound.dimensions, [4.0, 5.0, 6.0]);
    assert_eq!(stream.position() as usize, data.len());
}

/// Regression: #330. Files in the NetImmerse→Gamebryo gap window
/// (v ∈ (4.2.2.0, 10.0.1.0)) have neither `Next Extra Data` /
/// `Num Bytes` (until 4.2.2.0) nor `Name` (since 10.0.1.0). Before
/// the fix, `NiExtraData::parse` treated this entire range as
/// `parse_legacy` and consumed phantom 8 bytes (ref + u32 length),
/// misaligning every subsequent block.
#[test]
fn ni_extra_data_gap_window_reads_only_subclass_body() {
    let header = NifHeader {
        version: NifVersion(0x0A000006), // 10.0.0.6 — in the gap
        little_endian: true,
        user_version: 0,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    };
    // NiStringExtraData body only — no name, no next_ref, no
    // bytes_remaining. Just a sized-string payload.
    let mut data = Vec::new();
    data.extend_from_slice(&5u32.to_le_bytes()); // payload length
    data.extend_from_slice(b"hello");

    let mut stream = NifStream::new(&data, &header);
    let extra = NiExtraData::parse(&mut stream, "NiStringExtraData")
        .expect("gap-window NiExtraData should parse");
    assert!(extra.name.is_none());
    assert_eq!(extra.string_value.as_deref(), Some("hello"));
    assert_eq!(stream.position() as usize, data.len());
}

#[test]
fn bs_anim_notes_malicious_count_errors_without_panic() {
    // Regression test for #408: a corrupt/malicious count must not OOM
    // via Vec::with_capacity. allocate_vec bounds count against
    // remaining bytes and returns an io::Error instead of panicking.
    let header = skyrim_header();
    let data = u16::MAX.to_le_bytes(); // count = 65535, zero body bytes
    let mut stream = NifStream::new(&data, &header);
    let err = BsAnimNotes::parse(&mut stream).expect_err("expected bounds error");
    let msg = err.to_string();
    assert!(
        msg.contains("bytes remain") || msg.contains("only"),
        "expected allocate_vec bounds error, got: {msg}"
    );
}

#[test]
fn parse_bs_w_array_three_items() {
    let header = skyrim_header();
    let mut data = Vec::new();
    // name: string index 0 (u32 in v20.2.0.7 string-table format)
    data.extend_from_slice(&0u32.to_le_bytes());
    // count: 3
    data.extend_from_slice(&3u32.to_le_bytes());
    // items: -1, 0, 42
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&42i32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let arr = BsWArray::parse(&mut stream).unwrap();
    assert_eq!(arr.items.len(), 3);
    assert_eq!(arr.items[0], -1);
    assert_eq!(arr.items[1], 0);
    assert_eq!(arr.items[2], 42);
    assert_eq!(stream.position() as usize, data.len());
}

/// Regression for #553 — `NiFloatExtraData` must parse the name
/// string index (FO3+) + the f32 float payload. Pre-fix there was
/// no match arm and 1,492 Skyrim SE vanilla blocks fell into
/// NiUnknown. The float_value on the parsed struct is how the
/// future importer will surface FOV multipliers / wetness knobs.
#[test]
fn ni_float_extra_data_skyrim() {
    let header = skyrim_header();
    let mut data = Vec::new();
    // name: string index 0 (u32, string-table format at 20.2+).
    data.extend_from_slice(&0u32.to_le_bytes());
    // float_data: 2.5
    data.extend_from_slice(&2.5f32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let extra = NiExtraData::parse(&mut stream, "NiFloatExtraData")
        .expect("NiFloatExtraData must parse cleanly");
    assert_eq!(stream.position() as usize, data.len());
    assert_eq!(extra.type_name, "NiFloatExtraData");
    assert_eq!(extra.float_value, Some(2.5));
    // Guard against accidental cross-population of sibling fields.
    assert!(extra.integer_value.is_none());
    assert!(extra.floats_array.is_none());
}

/// Regression for #615 / SK-D5-04. NiStringsExtraData on Skyrim+
/// (v20.1.0.1+) hits the same string-table dispatch as `NiObjectNET.name`
/// — but the strings *array body* per nif.xml is always inline
/// `SizedString`, not a list of string-table indices. Pre-#615 the
/// loop used `read_string()`, so on Skyrim it consumed a 4-byte
/// table index per "string" and silently dropped the array contents.
/// Block-size recovery hid the drift behind a clean parse rate.
/// This test pins the post-fix `read_sized_string()` path.
#[test]
fn ni_strings_extra_data_skyrim_round_trips_inline_strings() {
    let header = skyrim_header();
    let mut data = Vec::new();
    // Name: string-table index = -1 (no name).
    data.extend_from_slice(&(-1i32).to_le_bytes());
    // Count = 3 — covers SpeedTree LOD bone names + an empty entry
    // (vanilla content sometimes ships placeholder slots).
    data.extend_from_slice(&3u32.to_le_bytes());
    for s in ["TrunkBone01", "", "BranchRoot_Lod1"] {
        data.extend_from_slice(&(s.len() as u32).to_le_bytes());
        data.extend_from_slice(s.as_bytes());
    }

    let mut stream = NifStream::new(&data, &header);
    let extra = NiExtraData::parse(&mut stream, "NiStringsExtraData")
        .expect("NiStringsExtraData must parse cleanly on Skyrim+");
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "consumed bytes must match payload exactly — no string-table\
         drift, no missed inline body"
    );
    assert_eq!(extra.type_name, "NiStringsExtraData");
    let arr = extra
        .strings_array
        .expect("strings_array populates from #615 fix");
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_deref(), Some("TrunkBone01"));
    assert!(
        arr[1].is_none(),
        "empty inline string yields None, not a zero-length Arc"
    );
    assert_eq!(arr[2].as_deref(), Some("BranchRoot_Lod1"));
    // Sibling-field guard.
    assert!(extra.string_value.is_none());
    assert!(extra.integer_value.is_none());
}

/// Regression for #553 — `NiFloatsExtraData` (the array variant)
/// must consume the u32 count + N f32 payloads. Bundled with the
/// Float case because authoring tools emit both variants in the
/// same DLC content streams.
#[test]
fn ni_floats_extra_data_skyrim() {
    let header = skyrim_header();
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes()); // name: string idx 0
    data.extend_from_slice(&3u32.to_le_bytes()); // num floats
    data.extend_from_slice(&1.0f32.to_le_bytes());
    data.extend_from_slice(&2.0f32.to_le_bytes());
    data.extend_from_slice(&3.0f32.to_le_bytes());

    let mut stream = NifStream::new(&data, &header);
    let extra = NiExtraData::parse(&mut stream, "NiFloatsExtraData")
        .expect("NiFloatsExtraData must parse cleanly");
    assert_eq!(stream.position() as usize, data.len());
    let arr = extra.floats_array.expect("floats_array must populate");
    assert_eq!(arr, vec![1.0, 2.0, 3.0]);
}
