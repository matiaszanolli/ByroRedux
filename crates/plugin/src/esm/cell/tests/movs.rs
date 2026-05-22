//! MOVS (Movable Static) parsing tests — #1179 / FO4-D4-002.
//!
//! Vanilla `Fallout4.esm` ships ZERO MOVS records (the `parse_real_esm`
//! assertion pins this to 0), so the `parse_movs_group` walker has no
//! real-data coverage. The unit tests in `records/movs.rs` exercise
//! the per-record `parse_movs` function but not the GRUP walker that
//! drives it. These synthetic-bytes tests close that gap end-to-end.
//!
//! Sub-record coverage: MODL / LNAM / ZNAM / DEST / VMAD. Each test
//! builds a MOVS record + wraps it in a GRUP-MOVS header, then drives
//! `parse_movs_group` and asserts the resulting `MovableStaticRecord`
//! plus the parallel `StaticObject` registration in the `statics` map
//! (the MODL-backed cross-registration that keeps REFR resolution
//! working against MOVS form IDs).

use super::super::super::reader::EsmReader;
use super::super::*;
use crate::esm::records::MovableStaticRecord;
use crate::record::RecordType;

/// Append one sub-record (4-CC + u16 length + payload) to a buffer.
fn put_sub(buf: &mut Vec<u8>, ty: &[u8; 4], payload: &[u8]) {
    buf.extend_from_slice(ty);
    buf.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    buf.extend_from_slice(payload);
}

/// Build a synthetic 24-byte-header MOVS record from a sub-record list.
fn build_movs_record(form_id: u32, subs: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut sub_data = Vec::new();
    for (ty, payload) in subs {
        put_sub(&mut sub_data, ty, payload);
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(b"MOVS");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // timestamp + VC + unknown
    buf.extend_from_slice(&sub_data);
    buf
}

/// Wrap MOVS records in a top-level GRUP-MOVS group (24-byte header).
fn build_movs_group(records: &[Vec<u8>]) -> Vec<u8> {
    let payload_len: usize = records.iter().map(|r| r.len()).sum();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&((24 + payload_len) as u32).to_le_bytes());
    group.extend_from_slice(b"MOVS"); // label
    group.extend_from_slice(&0u32.to_le_bytes()); // group_type = 0 (top-level)
    group.extend_from_slice(&[0u8; 8]); // timestamp + VC
    for rec in records {
        group.extend_from_slice(rec);
    }
    group
}

/// Drive `parse_movs_group` over a synthetic GRUP-MOVS buffer.
fn parse_synthetic_movs(
    buf: &[u8],
) -> (HashMap<u32, StaticObject>, HashMap<u32, MovableStaticRecord>) {
    let mut reader = EsmReader::new(buf);
    let gh = reader.read_group_header().expect("MOVS group header");
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    let mut movables = HashMap::new();
    super::super::support::parse_movs_group(&mut reader, end, &mut statics, &mut movables)
        .expect("parse_movs_group");
    (statics, movables)
}

#[test]
fn parse_movs_captures_full_subrecord_set() {
    // Authored MOVS with every sub-record the parser models —
    // EDID + MODL + LNAM (loop sound) + ZNAM (activate sound) +
    // DEST (destruction template) + VMAD (Papyrus script).
    let movs = build_movs_record(
        0x0010_0001,
        &[
            (b"EDID", b"TestBreakable\0".to_vec()),
            (
                b"MODL",
                b"Furniture\\Test\\BreakableTable01.nif\0".to_vec(),
            ),
            (b"LNAM", 0x0010_AAAA_u32.to_le_bytes().to_vec()),
            (b"ZNAM", 0x0010_BBBB_u32.to_le_bytes().to_vec()),
            // DEST + VMAD payloads are marker-only at this layer —
            // the parser just flips boolean `has_destruction` /
            // `has_script` flags. Use minimal byte payloads so the
            // sub-record sizing is well-formed.
            (b"DEST", vec![0u8; 8]),
            (b"VMAD", vec![0u8; 16]),
        ],
    );
    let buf = build_movs_group(&[movs]);

    let (statics, movables) = parse_synthetic_movs(&buf);

    // ── MovableStaticRecord side ──
    let rec = movables
        .get(&0x0010_0001)
        .expect("MOVS form ID must land in movables");
    assert_eq!(rec.form_id, 0x0010_0001);
    assert_eq!(rec.editor_id, "TestBreakable");
    assert_eq!(rec.model_path, "Furniture\\Test\\BreakableTable01.nif");
    assert_eq!(rec.loop_sound_form_id, Some(0x0010_AAAA));
    assert_eq!(rec.activate_sound_form_id, Some(0x0010_BBBB));
    assert!(rec.has_destruction);
    assert!(rec.has_script);

    // ── StaticObject cross-registration (REFR resolution path) ──
    let stat = statics
        .get(&0x0010_0001)
        .expect("MOVS must also register a StaticObject for REFR resolution");
    assert_eq!(stat.form_id, 0x0010_0001);
    assert_eq!(stat.editor_id, "TestBreakable");
    assert_eq!(stat.model_path, "Furniture\\Test\\BreakableTable01.nif");
    assert_eq!(stat.record_type, RecordType::MOVS);
    assert!(stat.has_script);
    assert!(stat.light_data.is_none());
    assert!(stat.addon_data.is_none());
}

#[test]
fn parse_movs_with_modl_only_still_registers_static() {
    // Minimum-viable MOVS: just EDID + MODL. No sounds, no destruction,
    // no script. The visual placement still has to resolve, so the
    // StaticObject side must populate.
    let movs = build_movs_record(
        0x0010_0002,
        &[
            (b"EDID", b"PlainMovs\0".to_vec()),
            (b"MODL", b"Movable\\Plain.nif\0".to_vec()),
        ],
    );
    let buf = build_movs_group(&[movs]);

    let (statics, movables) = parse_synthetic_movs(&buf);
    let rec = movables.get(&0x0010_0002).expect("must register");
    assert_eq!(rec.loop_sound_form_id, None);
    assert_eq!(rec.activate_sound_form_id, None);
    assert!(!rec.has_destruction);
    assert!(!rec.has_script);

    assert!(statics.contains_key(&0x0010_0002));
}

#[test]
fn parse_movs_header_only_record_skipped_from_statics() {
    // Pre-#588: header-only MOVS (no EDID, no MODL) shouldn't pollute
    // the StaticObject map — REFR resolution against such a base
    // would point at a stub with no mesh. The `movables` map still
    // registers the typed record (so downstream sound/script wiring
    // doesn't lose the form ID).
    let movs = build_movs_record(0x0010_0003, &[]);
    let buf = build_movs_group(&[movs]);

    let (statics, movables) = parse_synthetic_movs(&buf);
    assert!(
        movables.contains_key(&0x0010_0003),
        "movables registers every MOVS"
    );
    assert!(
        !statics.contains_key(&0x0010_0003),
        "statics skips header-only MOVS with no EDID/MODL"
    );
}

#[test]
fn parse_movs_group_handles_multiple_records() {
    // Multi-record GRUP-MOVS: every record fans out independently.
    let movs_a = build_movs_record(
        0x0010_0010,
        &[
            (b"EDID", b"MovsA\0".to_vec()),
            (b"MODL", b"a.nif\0".to_vec()),
        ],
    );
    let movs_b = build_movs_record(
        0x0010_0011,
        &[
            (b"EDID", b"MovsB\0".to_vec()),
            (b"MODL", b"b.nif\0".to_vec()),
            (b"VMAD", vec![0u8; 4]),
        ],
    );
    let movs_c = build_movs_record(
        0x0010_0012,
        &[
            (b"EDID", b"MovsC\0".to_vec()),
            (b"MODL", b"c.nif\0".to_vec()),
            (b"DEST", vec![0u8; 8]),
        ],
    );
    let buf = build_movs_group(&[movs_a, movs_b, movs_c]);

    let (statics, movables) = parse_synthetic_movs(&buf);
    assert_eq!(movables.len(), 3);
    assert_eq!(statics.len(), 3);
    assert!(movables[&0x0010_0011].has_script);
    assert!(!movables[&0x0010_0010].has_script);
    assert!(movables[&0x0010_0012].has_destruction);
    assert!(!movables[&0x0010_0011].has_destruction);
}
