//! WRLD (worldspace) parsing tests.
//!
//! Oblivion root worldspace fields, derived worldspace parent link,
//! truncated-payload defaults.

use super::super::super::reader::EsmReader;
use super::super::*;

/// Append one sub-record (4-CC + u16 length + payload) to a buffer.
pub(super) fn put_sub(buf: &mut Vec<u8>, ty: &[u8; 4], payload: &[u8]) {
    buf.extend_from_slice(ty);
    buf.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    buf.extend_from_slice(payload);
}

/// Build a synthetic 24-byte-header WRLD record from a sub-record
/// list. The record header is a stock FNV/Skyrim layout (no
/// timestamp / version-control fields populated).
pub(super) fn build_wrld_record(form_id: u32, subs: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut sub_data = Vec::new();
    for (ty, payload) in subs {
        put_sub(&mut sub_data, ty, payload);
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(b"WRLD");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // timestamp + VC + unknown
    buf.extend_from_slice(&sub_data);
    buf
}

/// Wrap a sequence of WRLD records in a top-level GRUP-WRLD group
/// header (24-byte). Mirrors the on-disk layout the fused walker
/// dispatches through.
pub(super) fn build_wrld_group(records: &[Vec<u8>]) -> Vec<u8> {
    let payload_len: usize = records.iter().map(|r| r.len()).sum();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&((24 + payload_len) as u32).to_le_bytes());
    group.extend_from_slice(b"WRLD"); // label
    group.extend_from_slice(&0u32.to_le_bytes()); // group_type = 0 (top-level)
    group.extend_from_slice(&[0u8; 8]); // timestamp + VC
    for rec in records {
        group.extend_from_slice(rec);
    }
    group
}

/// Drive `parse_wrld_group` over a synthetic GRUP-WRLD buffer.
pub(super) fn parse_synthetic_wrld(
    buf: &[u8],
) -> (
    HashMap<String, super::super::WorldspaceRecord>,
    HashMap<String, u32>,
    HashMap<String, HashMap<(i32, i32), CellData>>,
) {
    let mut reader = EsmReader::new(buf);
    let gh = reader.read_group_header().expect("WRLD group header");
    let end = reader.group_content_end(&gh);
    let mut exterior = HashMap::new();
    let mut worldspaces = HashMap::new();
    let mut climates = HashMap::new();
    super::super::wrld::parse_wrld_group(
        &mut reader,
        end,
        &mut exterior,
        &mut worldspaces,
        &mut climates,
    )
    .expect("parse_wrld_group");
    (worldspaces, climates, exterior)
}


#[test]
fn parse_wrld_captures_oblivion_root_worldspace_fields() {
    // Synthetic Tamriel-shaped WRLD: object bounds (f32 world units),
    // climate, default water, default music, ICON, DATA flag byte.
    // No WNAM (root worldspace), no PNAM (TES4 doesn't author it).
    // World-unit values pin to round cell offsets so the
    // `usable_cell_bounds()` helper has clean asserts.
    let nam0 = {
        let mut v = Vec::with_capacity(8);
        v.extend_from_slice(&(-122_880.0_f32).to_le_bytes()); // -30 cells
        v.extend_from_slice(&(-204_800.0_f32).to_le_bytes()); // -50 cells
        v
    };
    let nam9 = {
        let mut v = Vec::with_capacity(8);
        v.extend_from_slice(&143_360.0_f32.to_le_bytes()); //  35 cells
        v.extend_from_slice(&225_280.0_f32.to_le_bytes()); //  55 cells
        v
    };
    let wrld = build_wrld_record(
        0x0000_003C,
        &[
            (b"EDID", b"Tamriel\0".to_vec()),
            (b"CNAM", 0xDEAD_BEEF_u32.to_le_bytes().to_vec()),
            (b"NAM2", 0x0000_BABE_u32.to_le_bytes().to_vec()),
            (b"ICON", b"textures/menus80/tamriel.dds\0".to_vec()),
            (b"ZNAM", 0x0009_0908_u32.to_le_bytes().to_vec()),
            (b"NAM0", nam0),
            (b"NAM9", nam9),
            (b"DATA", vec![0x40]), // no-grass bit
        ],
    );
    let buf = build_wrld_group(&[wrld]);

    let (worldspaces, climates, _exterior) = parse_synthetic_wrld(&buf);
    let tam = worldspaces.get("tamriel").expect("tamriel decoded");
    assert_eq!(tam.form_id, 0x0000_003C);
    assert_eq!(tam.editor_id, "Tamriel");
    assert_eq!(tam.usable_min, (-122_880.0, -204_800.0));
    assert_eq!(tam.usable_max, (143_360.0, 225_280.0));
    assert_eq!(
        tam.usable_cell_bounds(),
        Some(((-30, -50), (35, 55))),
        "world-unit bounds should snap to cell-grid via /4096"
    );
    assert_eq!(tam.parent_worldspace, None, "root worldspace has no WNAM");
    assert_eq!(tam.parent_flags, 0, "TES4 omits PNAM");
    assert_eq!(tam.water_form, Some(0x0000_BABE));
    assert_eq!(tam.default_music, Some(0x0009_0908));
    assert_eq!(tam.map_texture, "textures/menus80/tamriel.dds");
    assert_eq!(tam.flags, 0x40);
    // CNAM still mirrors into the legacy climate lookup the cell
    // loader reads from.
    assert_eq!(climates.get("tamriel"), Some(&0xDEAD_BEEF));
}

#[test]
fn parse_wrld_captures_derived_worldspace_parent_link() {
    // Synthetic Shivering Isles → Tamriel. WNAM points at the parent
    // FormID; PNAM carries the inheritance-flag bitfield (FO3+ only —
    // included here to exercise the u16 read path).
    let nam0 = {
        let mut v = Vec::with_capacity(8);
        v.extend_from_slice(&(-40_960.0_f32).to_le_bytes()); // -10 cells
        v.extend_from_slice(&(-40_960.0_f32).to_le_bytes());
        v
    };
    let nam9 = {
        let mut v = Vec::with_capacity(8);
        v.extend_from_slice(&40_960.0_f32.to_le_bytes()); //  10 cells
        v.extend_from_slice(&40_960.0_f32.to_le_bytes());
        v
    };
    let wrld = build_wrld_record(
        0x0001_55C0,
        &[
            (b"EDID", b"SEWorld\0".to_vec()),
            (b"WNAM", 0x0000_003C_u32.to_le_bytes().to_vec()),
            (b"PNAM", 0x0011_u16.to_le_bytes().to_vec()), // Land + Climate
            (b"NAM0", nam0),
            (b"NAM9", nam9),
        ],
    );
    let buf = build_wrld_group(&[wrld]);
    let (worldspaces, _climates, _exterior) = parse_synthetic_wrld(&buf);
    let se = worldspaces
        .get("seworld")
        .expect("derived worldspace decoded");
    assert_eq!(se.parent_worldspace, Some(0x0000_003C));
    assert_eq!(se.parent_flags, 0x0011);
    assert_eq!(se.usable_cell_bounds(), Some(((-10, -10), (10, 10))));
}

#[test]
fn parse_wrld_truncated_payloads_default_safely() {
    // Defensive: a 1-byte PNAM (pre-Skyrim variant), an empty DATA,
    // and a NAM0 short by 1 byte must not panic. The walker should
    // fold whatever it can read and leave the rest at default.
    let wrld = build_wrld_record(
        0x0000_0BAD,
        &[
            (b"EDID", b"TruncatedWorld\0".to_vec()),
            (b"PNAM", vec![0x07]),       // single-byte form
            (b"NAM0", vec![0u8; 7]),     // too short → ignored
            (b"NAM9", vec![]),           // empty → ignored
            (b"DATA", vec![]),           // empty → ignored
            (b"ICON", vec![]),           // empty zstring
        ],
    );
    let buf = build_wrld_group(&[wrld]);
    let (worldspaces, _climates, _exterior) = parse_synthetic_wrld(&buf);
    let w = worldspaces.get("truncatedworld").expect("decoded");
    assert_eq!(w.parent_flags, 0x07, "1-byte PNAM folds into the low byte");
    assert_eq!(w.usable_min, (0.0, 0.0), "short NAM0 stays at default");
    assert_eq!(w.usable_max, (0.0, 0.0), "missing NAM9 stays at default");
    assert_eq!(
        w.usable_cell_bounds(),
        None,
        "default-zero bounds yield None so the consumer falls back to explicit cells"
    );
    assert_eq!(w.flags, 0, "empty DATA stays at default");
    assert!(w.map_texture.is_empty());
}
