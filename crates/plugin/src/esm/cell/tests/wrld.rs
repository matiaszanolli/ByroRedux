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

/// Build a synthetic CELL record (24-byte FNV/Skyrim header) from a
/// sub-record list. Mirrors `build_wrld_record`'s shape; broken out
/// so #1220's exterior precombined test can drop a CELL inside a
/// WRLD-children group.
fn build_cell_record(form_id: u32, subs: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut sub_data = Vec::new();
    for (ty, payload) in subs {
        put_sub(&mut sub_data, ty, payload);
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // timestamp + VC + unknown
    buf.extend_from_slice(&sub_data);
    buf
}

/// Wrap a buffer in a GRUP-world-children header (group_type=1). The
/// `wrld_form_id` becomes the group label per ESM convention — the
/// walker uses this to bind the children back to their parent WRLD.
fn build_world_children_group(wrld_form_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&((24 + payload.len()) as u32).to_le_bytes());
    group.extend_from_slice(&wrld_form_id.to_le_bytes()); // label = parent WRLD form id
    group.extend_from_slice(&1u32.to_le_bytes()); // group_type = 1 (world children)
    group.extend_from_slice(&[0u8; 8]); // timestamp + VC
    group.extend_from_slice(payload);
    group
}

/// #1220 / D3-NEW-01 regression — exterior CELL XCRI + XPRI sub-records
/// must populate `precombined_mesh_hashes` + `absorbed_refs`. Pre-fix
/// the exterior walker hardcoded both to empty on the wrong premise
/// that FO4 precombines are interior-only. The fix lifts the same
/// match arms used by the interior walker; this test pins the
/// behaviour against drift.
#[test]
fn parse_wrld_exterior_cell_captures_precombined_xcri_xpri() {
    let wrld_fid: u32 = 0x0000_003C;
    let cell_fid: u32 = 0x0001_2345;

    // XCLC: grid (x, y) = (0, 0) for a Sanctuary-shaped tile.
    let xclc = {
        let mut v = Vec::with_capacity(8);
        v.extend_from_slice(&0i32.to_le_bytes());
        v.extend_from_slice(&0i32.to_le_bytes());
        v
    };
    // XCRI: 2 mesh hashes + 3 visibility-group refs. Layout matches
    // `walkers.rs:158-190` exactly: u32 mesh_count + u32 ref_count +
    // mesh_count × u32 hashes + ref_count × u32 visibility refs (tail
    // intentionally NOT consumed into absorbed_refs).
    let xcri = {
        let mut v = Vec::with_capacity(8 + 2 * 4 + 3 * 4);
        v.extend_from_slice(&2u32.to_le_bytes()); // mesh_count
        v.extend_from_slice(&3u32.to_le_bytes()); // ref_count (visibility group)
        v.extend_from_slice(&0xDEAD_BEEF_u32.to_le_bytes()); // hash 0
        v.extend_from_slice(&0xCAFE_BABE_u32.to_le_bytes()); // hash 1
        v.extend_from_slice(&0x0100_0001_u32.to_le_bytes()); // vis-group ref (NOT absorbed)
        v.extend_from_slice(&0x0100_0002_u32.to_le_bytes());
        v.extend_from_slice(&0x0100_0003_u32.to_le_bytes());
        v
    };
    // XPRI: 2 REFR formids that MUST land in absorbed_refs.
    let xpri = {
        let mut v = Vec::with_capacity(2 * 4);
        v.extend_from_slice(&0x0200_0001_u32.to_le_bytes());
        v.extend_from_slice(&0x0200_0002_u32.to_le_bytes());
        v
    };

    let cell = build_cell_record(
        cell_fid,
        &[
            (b"EDID", b"SanctuaryTile00\0".to_vec()),
            (b"XCLC", xclc),
            (b"XCRI", xcri),
            (b"XPRI", xpri),
        ],
    );
    let children = build_world_children_group(wrld_fid, &cell);

    let wrld = build_wrld_record(
        wrld_fid,
        &[(b"EDID", b"Commonwealth\0".to_vec())],
    );

    // Layout the WRLD-GRUP payload as [WRLD record] + [children GRUP].
    let mut wrld_grup_payload = Vec::new();
    wrld_grup_payload.extend_from_slice(&wrld);
    wrld_grup_payload.extend_from_slice(&children);
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GRUP");
    buf.extend_from_slice(&((24 + wrld_grup_payload.len()) as u32).to_le_bytes());
    buf.extend_from_slice(b"WRLD"); // label
    buf.extend_from_slice(&0u32.to_le_bytes()); // group_type = 0 (top-level)
    buf.extend_from_slice(&[0u8; 8]); // timestamp + VC
    buf.extend_from_slice(&wrld_grup_payload);

    let (_worldspaces, _climates, exterior) = parse_synthetic_wrld(&buf);
    let commonwealth = exterior
        .get("commonwealth")
        .expect("Commonwealth WRLD's children must be indexed");
    let cell = commonwealth
        .get(&(0, 0))
        .expect("Sanctuary (0,0) exterior CELL must be decoded");

    assert_eq!(
        cell.precombined_mesh_hashes,
        vec![0xDEAD_BEEF, 0xCAFE_BABE],
        "exterior XCRI must populate precombined_mesh_hashes (pre-#1220 was always empty)",
    );
    assert_eq!(
        cell.absorbed_refs.len(),
        2,
        "exterior XPRI must populate absorbed_refs (pre-#1220 was always empty)",
    );
    assert!(cell.absorbed_refs.contains(&0x0200_0001));
    assert!(cell.absorbed_refs.contains(&0x0200_0002));
    // Visibility-group refs (XCRI tail) must NOT leak into absorbed_refs —
    // that was the regression from #1188 first-iteration where Dugout
    // Inn's bar / couch / lamps went invisible.
    assert!(!cell.absorbed_refs.contains(&0x0100_0001));
    assert!(!cell.absorbed_refs.contains(&0x0100_0002));
    assert!(!cell.absorbed_refs.contains(&0x0100_0003));
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

/// #1305 follow-up — FO3/FNV/Skyrim+ WRLD carry a DNAM "Land Data"
/// (`[default_land_height: f32, default_water_height: f32]`, 8 bytes).
/// The parser must capture the SECOND f32 as `default_water_height` so
/// no-XCLW exterior cells inherit the worldspace default water plane.
/// Layout verified against FalloutNV.esm (WastelandNV -2300) and
/// Skyrim.esm (Tamriel -14000).
#[test]
fn wrld_dnam_captures_default_water_height() {
    let mut dnam = Vec::new();
    dnam.extend_from_slice(&(-2500.0f32).to_le_bytes()); // default land height
    dnam.extend_from_slice(&(-2300.0f32).to_le_bytes()); // default water height
    let wrld = build_wrld_record(
        0x0000_0099,
        &[
            (b"EDID", b"WastelandNV\0".to_vec()),
            (b"NAM2", 0x0000_BABE_u32.to_le_bytes().to_vec()),
            (b"DNAM", dnam),
        ],
    );
    let buf = build_wrld_group(&[wrld]);
    let (worldspaces, _climates, _exterior) = parse_synthetic_wrld(&buf);
    let w = worldspaces.get("wastelandnv").expect("WastelandNV decoded");
    assert_eq!(
        w.default_water_height,
        Some(-2300.0),
        "DNAM second f32 is the default water height (not the land height -2500)"
    );
    assert_eq!(w.water_form, Some(0x0000_BABE));
}

/// A short / absent DNAM leaves `default_water_height` at `None` (the
/// loader then falls back appropriately — Oblivion to Z=0, others to dry).
#[test]
fn wrld_short_dnam_leaves_default_water_none() {
    let wrld = build_wrld_record(
        0x0000_009A,
        &[
            (b"EDID", b"NoDnamWorld\0".to_vec()),
            (b"DNAM", vec![0u8; 4]), // only 4 bytes — too short for the water f32
        ],
    );
    let buf = build_wrld_group(&[wrld]);
    let (worldspaces, _c, _e) = parse_synthetic_wrld(&buf);
    let w = worldspaces.get("nodnamworld").expect("decoded");
    assert_eq!(w.default_water_height, None);
}
