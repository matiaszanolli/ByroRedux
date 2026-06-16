//! CELL (interior + exterior cell) parsing tests.
//!
//! XCLW water height, RCLR regional color override, Skyrim extended
//! subrecords (FULL string, XCCM climate override, XCLL directional ambient
//! cube), FNV XCLL color decode + 40-byte tail.

use super::super::walkers::parse_cell_group;
use super::super::*;

#[test]
fn parse_cell_xclw_populates_water_height() {
    // Regression: #397 — CELL XCLW (f32 water plane height) was
    // silently dropped by the walker, so flooded Ayleid ruins /
    // sewer interiors / coastal exteriors rendered without water.
    // Build an interior CELL record with EDID + DATA(interior) +
    // XCLW(10.0) and run it through `parse_cell_group`, which is
    // reachable directly once the CELL record is followed by no
    // further groups.
    let water_bytes = 10.0_f32.to_le_bytes();

    // Sub-record payload (type(4) + size(2) + bytes).
    let mut sub_data = Vec::new();
    let edid = "FloodedRuin\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01); // is_interior bit.

    sub_data.extend_from_slice(b"XCLW");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&water_bytes);

    // CELL record (Tes5Plus layout — 24-byte header).
    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // form_id
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    assert_eq!(cells.len(), 1, "interior CELL must be registered");
    let cell = cells.get("floodedruin").expect("lowercase key");
    assert!(cell.is_interior);
    assert_eq!(
        cell.water_height,
        Some(10.0),
        "XCLW water height must flow through to CellData"
    );
}

/// Regression: #970 / OBL-D3-NEW-06 — Oblivion CELL RCLR
/// (3-byte RGB regional tint) was silently dropped by the walker;
/// editor-authored cell-level colour overrides never surfaced to
/// the downstream renderer. The audit's typical authoring site is
/// Oblivion exterior cells but the parse arm is cross-game (rare
/// in vanilla, modder-portable).
#[test]
fn parse_cell_rclr_populates_regional_color_override() {
    let mut sub_data = Vec::new();
    let edid = "OblivionFog\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01); // is_interior

    // RCLR = 3 bytes RGB.
    sub_data.extend_from_slice(b"RCLR");
    sub_data.extend_from_slice(&3u16.to_le_bytes());
    sub_data.extend_from_slice(&[0x40, 0x80, 0xC0]); // tint = (64, 128, 192)

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // form_id
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    let cell = cells.get("oblivionfog").expect("lowercase key");
    assert_eq!(
        cell.regional_color_override,
        Some([0x40, 0x80, 0xC0]),
        "RCLR must populate regional_color_override on CellData"
    );
}

/// Companion: a CELL without RCLR keeps `regional_color_override =
/// None`. Pins that the parse path doesn't fabricate a default value.
#[test]
fn parse_cell_without_rclr_leaves_regional_color_override_none() {
    let mut sub_data = Vec::new();
    let edid = "NoRclrCell\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0xCAFE_BABEu32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();
    assert!(cells
        .get("norclrcell")
        .unwrap()
        .regional_color_override
        .is_none());
}

#[test]
fn parse_cell_skyrim_extended_subrecords() {
    // Regression: #356 — Skyrim CELL extended sub-records were
    // dropped on the walker's `_` arm. Build an interior CELL with
    // every extended FormID + a 3-entry XCLR region list and assert
    // they all flow through to `CellData`.
    let mut sub_data = Vec::new();
    let edid = "SkyrimRoom\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01); // is_interior

    // Helper to append a 4-byte FormID sub-record.
    fn push_form_sub(out: &mut Vec<u8>, ty: &[u8; 4], form_id: u32) {
        out.extend_from_slice(ty);
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&form_id.to_le_bytes());
    }
    push_form_sub(&mut sub_data, b"XCIM", 0x000A1234); // image space
    push_form_sub(&mut sub_data, b"XCWT", 0x000B5678); // water type
    push_form_sub(&mut sub_data, b"XCAS", 0x000C9ABC); // acoustic space
    push_form_sub(&mut sub_data, b"XCMO", 0x000DEF01); // music type
    push_form_sub(&mut sub_data, b"XLCN", 0x000E2345); // location

    // XCLR: variable-length packed FormID array — three entries.
    let regions = [0x111u32, 0x222u32, 0x333u32];
    sub_data.extend_from_slice(b"XCLR");
    sub_data.extend_from_slice(&(regions.len() as u16 * 4).to_le_bytes());
    for r in regions {
        sub_data.extend_from_slice(&r.to_le_bytes());
    }

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&0xCAFEBABEu32.to_le_bytes()); // form_id
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    let cell = cells.get("skyrimroom").expect("interior CELL present");
    assert_eq!(cell.image_space_form, Some(0x000A1234));
    assert_eq!(cell.water_type_form, Some(0x000B5678));
    assert_eq!(cell.acoustic_space_form, Some(0x000C9ABC));
    assert_eq!(cell.music_type_form, Some(0x000DEF01));
    assert_eq!(cell.location_form, Some(0x000E2345));
    assert_eq!(cell.regions, vec![0x111, 0x222, 0x333]);
    // Sanity: `water_height` stays None because no XCLW present.
    assert_eq!(cell.water_height, None);
}

/// Regression for #624 / SK-D6-NEW-02. Skyrim cells DO ship FULL —
/// `WhiterunBanneredMare` carries `"The Bannered Mare"` per UESP. The
/// pre-fix walker dropped FULL on the catch-all `_` arm, so the
/// display name was lost. This test builds a non-localized CELL
/// record with an inline FULL and asserts the new
/// `CellData.display_name` field surfaces it.
#[test]
fn parse_cell_full_inline_zstring_populates_display_name() {
    // Non-localized plugin path — explicit clear via guard so any
    // earlier test that set the lstring flag can't leak through.
    use crate::esm::records::common::LocalizedPluginGuard;
    let _guard = LocalizedPluginGuard::new(false);

    let mut sub_data = Vec::new();
    let edid = "WhiterunBanneredMare\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());

    let full = "The Bannered Mare\0";
    sub_data.extend_from_slice(b"FULL");
    sub_data.extend_from_slice(&(full.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(full.as_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01); // is_interior

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // form_id
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    let cell = cells
        .get("whiterunbanneredmare")
        .expect("interior CELL present");
    assert_eq!(
        cell.display_name.as_deref(),
        Some("The Bannered Mare"),
        "FULL must surface on CellData.display_name (#624)"
    );
}

/// Companion to the FULL test — a localized plugin's 4-byte FULL
/// payload is a STRINGS-table index. Until the real loader lands
/// (#348 Phase 2), `read_lstring_or_zstring` renders it as a
/// `<lstring 0xNNNNNNNN>` placeholder. Pin that behaviour so a
/// future loader integration can flip the assertion to the resolved
/// string without a separate audit pass.
#[test]
fn parse_cell_full_lstring_index_renders_as_placeholder() {
    use crate::esm::records::common::LocalizedPluginGuard;
    let _guard = LocalizedPluginGuard::new(true);

    let mut sub_data = Vec::new();
    let edid = "DragonsreachJarl\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());

    // Localized FULL: 4-byte LE u32 = STRINGS-table index 0x00012345.
    sub_data.extend_from_slice(b"FULL");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&0x00012345u32.to_le_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0xCAFEFEEDu32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    let cell = cells
        .get("dragonsreachjarl")
        .expect("interior CELL present");
    assert_eq!(
        cell.display_name.as_deref(),
        Some("<lstring 0x00012345>"),
        "localized FULL must render as the lstring placeholder until #348 Phase 2"
    );
}

#[test]
fn parse_cell_without_skyrim_extras_leaves_them_default() {
    // Sibling check for the new arms — a CELL with only EDID + DATA
    // must keep every extended FormID at None and `regions` empty.
    // Catches a regression where one of the new arms accidentally
    // captures another sub-record's payload.
    let mut sub_data = Vec::new();
    let edid = "BareRoom\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0x42u32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    let cell = cells.get("bareroom").expect("interior CELL present");
    assert_eq!(cell.image_space_form, None);
    assert_eq!(cell.water_type_form, None);
    assert_eq!(cell.acoustic_space_form, None);
    assert_eq!(cell.music_type_form, None);
    assert_eq!(cell.music_type_enum, None);
    assert_eq!(cell.climate_override, None);
    assert_eq!(cell.location_form, None);
    assert!(cell.regions.is_empty());
}

/// #693 / O3-N-05 — pre-Skyrim interior cells (Oblivion / FO3 /
/// FNV) carry XCMT (1-byte enum: 0=Default, 1=Public, 2=Dungeon,
/// 3=None) instead of the FormID-based XCMO. The walker dropped
/// XCMT on the catch-all `_` arm, so every interior music type
/// across the entire pre-Skyrim ESM library was lost.
#[test]
fn parse_cell_tes4_xcmt_populates_music_type_enum() {
    // 1-byte XCMT payload; value = 2 (Dungeon).
    let mut sub_data = Vec::new();
    let edid = "AyleidRuin\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01); // is_interior
    sub_data.extend_from_slice(b"XCMT");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x02); // Dungeon

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0xCAFE_BABEu32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    // Tes5Plus covers FO3 / FNV which both author XCMT despite
    // their 24-byte header (the audit groups them with Oblivion as
    // the pre-Skyrim cohort that uses XCMT). Oblivion's 20-byte
    // header is structurally tested elsewhere; the XCMT sub-record
    // parses identically across variants.
    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    let cell = cells.get("ayleidruin").expect("interior CELL present");
    assert_eq!(cell.music_type_enum, Some(0x02));
    // Sanity: the FormID-based slot stays None when only XCMT is present.
    assert_eq!(cell.music_type_form, None);
}

/// #693 / O3-N-05 — Skyrim+ exterior cells can override the
/// worldspace climate via XCCM (4-byte CLMT FormID). Pre-fix the
/// walker dropped it on the catch-all arm and the renderer fell
/// back to the worldspace default everywhere, missing scripted
/// weather pockets and boss-arena climate overrides.
///
/// The test exercises the interior walker (cheaper to set up than a
/// full WRLD/CELL group), which now also accepts XCCM since it's
/// well-formed on the rare interior mod that authors it. The wrld.rs
/// path uses identical match-arm code (verified by sibling check).
#[test]
fn parse_cell_skyrim_xccm_populates_climate_override() {
    let mut sub_data = Vec::new();
    let edid = "BossArena\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01);
    sub_data.extend_from_slice(b"XCCM");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&0x0001_A2B3u32.to_le_bytes());

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    let cell = cells.get("bossarena").expect("interior CELL present");
    assert_eq!(cell.climate_override, Some(0x0001_A2B3));
}

#[test]
fn parse_cell_skyrim_xcll_extracts_directional_ambient_cube() {
    // Regression: #367 (S6-05) — the 92-byte Skyrim XCLL's
    // bytes 40-71 (6×RGBA ambient cube + specular RGBA + fresnel
    // f32) were parsed-past and dropped. Build a synthetic 92-byte
    // XCLL with distinctive per-face colours and assert all six
    // slots round-trip along with the specular / fresnel fields.
    let mut xcll = Vec::with_capacity(92);

    // Bytes 0-7: Ambient RGBA + Directional RGBA (just need valid bytes).
    xcll.extend_from_slice(&[80, 82, 85, 0]); // ambient
    xcll.extend_from_slice(&[200, 195, 180, 0]); // directional
                                                 // Bytes 8-11: Fog color RGBA (fog_near color).
    xcll.extend_from_slice(&[50, 55, 60, 0]);
    // Byte 11 == 0 is the alpha; already appended above.
    // Bytes 12-15: fog near (f32).
    xcll.extend_from_slice(&100.0f32.to_le_bytes());
    // Bytes 16-19: fog far (f32).
    xcll.extend_from_slice(&5000.0f32.to_le_bytes());
    // Bytes 20-23: directional rot X (i32, degrees).
    xcll.extend_from_slice(&(45i32).to_le_bytes());
    // Bytes 24-27: directional rot Y.
    xcll.extend_from_slice(&(30i32).to_le_bytes());
    // Bytes 28-31: directional fade (f32).
    xcll.extend_from_slice(&1.25f32.to_le_bytes());
    // Bytes 32-35: fog clip.
    xcll.extend_from_slice(&7500.0f32.to_le_bytes());
    // Bytes 36-39: fog power.
    xcll.extend_from_slice(&1.5f32.to_le_bytes());

    // Bytes 40-63: 6 × RGBA ambient cube. CK order: +X, -X, +Y, -Y, +Z, -Z.
    //   Face colors chosen so every byte is distinct — catches a
    //   wrong-stride / wrong-offset bug that shuffles the cube.
    //   (r=10, g=20, b=30) for +X, +10 per channel per face.
    for face in 0u8..6 {
        let base = (face * 10) + 10;
        xcll.push(base); // R
        xcll.push(base + 1); // G
        xcll.push(base + 2); // B
        xcll.push(0); // A (vanilla-zero)
    }

    // Bytes 64-67: specular RGBA (255, 200, 150, 128).
    xcll.extend_from_slice(&[255, 200, 150, 128]);
    // Bytes 68-71: fresnel power (f32).
    xcll.extend_from_slice(&2.5f32.to_le_bytes());
    // Bytes 72-75: fog far color RGBA.
    xcll.extend_from_slice(&[120, 130, 140, 0]);
    // Bytes 76-79: fog max (f32).
    xcll.extend_from_slice(&0.85f32.to_le_bytes());
    // Bytes 80-83: light fade begin.
    xcll.extend_from_slice(&500.0f32.to_le_bytes());
    // Bytes 84-87: light fade end.
    xcll.extend_from_slice(&800.0f32.to_le_bytes());
    // Bytes 88-91: inherits flags (u32, unused by the parser).
    xcll.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(xcll.len(), 92, "Skyrim XCLL must be 92 bytes");

    let mut sub_data = Vec::new();
    let edid = "SkyrimCave\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01); // interior

    sub_data.extend_from_slice(b"XCLL");
    sub_data.extend_from_slice(&(xcll.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(&xcll);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0xCAFEu32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    let cell = cells.get("skyrimcave").expect("interior CELL present");
    let lit = cell.lighting.as_ref().expect("XCLL must populate lighting");

    // Directional ambient cube — all 6 faces extracted with the
    // expected distinctive RGB per face. Per-face bytes written as
    // (base, base+1, base+2, 0) come back as rgb = (base, base+1, base+2).
    let cube = lit
        .directional_ambient
        .expect("Skyrim XCLL must populate directional_ambient");
    for (face, rgb) in cube.iter().enumerate() {
        let base = (face as u8 * 10) + 10;
        assert!(
            (rgb[0] - base as f32 / 255.0).abs() < 1e-6,
            "face {face} R mismatch: got {}, expected {}",
            rgb[0],
            base as f32 / 255.0,
        );
        assert!(
            (rgb[1] - (base + 1) as f32 / 255.0).abs() < 1e-6,
            "face {face} G mismatch"
        );
        assert!(
            (rgb[2] - (base + 2) as f32 / 255.0).abs() < 1e-6,
            "face {face} B mismatch"
        );
    }

    // Specular + fresnel. Raw bytes [255, 200, 150, 128] → RGB.
    assert_eq!(
        lit.specular_color,
        Some([255.0 / 255.0, 200.0 / 255.0, 150.0 / 255.0])
    );
    assert_eq!(lit.specular_alpha, Some(128.0 / 255.0));
    assert_eq!(lit.fresnel_power, Some(2.5));

    // Pre-existing extended fields still ride along unchanged.
    assert_eq!(lit.directional_fade, Some(1.25));
    assert_eq!(lit.fog_clip, Some(7500.0));
    assert_eq!(lit.fog_power, Some(1.5));
    assert_eq!(lit.fog_max, Some(0.85));
    assert_eq!(lit.light_fade_begin, Some(500.0));
    assert_eq!(lit.light_fade_end, Some(800.0));
}

#[test]
fn parse_cell_starfield_xcll_decodes_volumetric_height_fog_tail() {
    // #1293 — Starfield's 108-byte XCLL diverges from the Skyrim
    // layout at offset 40: bytes 40-107 are a volumetric height-fog
    // model (xEdit SF1 `wbStruct(XCLL,'Lighting')`), NOT the Skyrim
    // ambient-cube / specular / fresnel. Build a synthetic 108-byte
    // XCLL and assert the SF fields land in `CellLighting.starfield`
    // (with the 40-55 fog-far/max/light-fade on the base fields), and
    // that the Skyrim-only ambient cube / specular / fresnel stay None.
    let mut xcll = Vec::with_capacity(108);
    xcll.extend_from_slice(&[80, 82, 85, 0]); // 0  ambient
    xcll.extend_from_slice(&[200, 195, 180, 0]); // 4  directional
    xcll.extend_from_slice(&[50, 55, 60, 0]); // 8  fog color near
    xcll.extend_from_slice(&100.0f32.to_le_bytes()); // 12 fog near
    xcll.extend_from_slice(&5000.0f32.to_le_bytes()); // 16 fog far
    xcll.extend_from_slice(&(45i32).to_le_bytes()); // 20 rot XY
    xcll.extend_from_slice(&(30i32).to_le_bytes()); // 24 rot Z
    xcll.extend_from_slice(&0.7f32.to_le_bytes()); // 28 gravity scale
    xcll.extend_from_slice(&7500.0f32.to_le_bytes()); // 32 fog clip
    xcll.extend_from_slice(&1.5f32.to_le_bytes()); // 36 fog power
    xcll.extend_from_slice(&[120, 130, 140, 0]); // 40 fog color far
    xcll.extend_from_slice(&0.85f32.to_le_bytes()); // 44 fog max
    xcll.extend_from_slice(&500.0f32.to_le_bytes()); // 48 light fade begin
    xcll.extend_from_slice(&800.0f32.to_le_bytes()); // 52 light fade end
    xcll.extend_from_slice(&[10, 11, 12, 0]); // 56 unknown color
    xcll.extend_from_slice(&(-20.0f32).to_le_bytes()); // 60 near height mid
    xcll.extend_from_slice(&24.0f32.to_le_bytes()); // 64 near height range
    xcll.extend_from_slice(&[30, 31, 32, 0]); // 68 fog color high near
    xcll.extend_from_slice(&[40, 41, 42, 0]); // 72 fog color high far
    xcll.extend_from_slice(&0.5f32.to_le_bytes()); // 76 high density scale
    xcll.extend_from_slice(&1.1f32.to_le_bytes()); // 80 fog near scale
    xcll.extend_from_slice(&1.2f32.to_le_bytes()); // 84 fog far scale
    xcll.extend_from_slice(&1.3f32.to_le_bytes()); // 88 fog high near scale
    xcll.extend_from_slice(&0.8f32.to_le_bytes()); // 92 fog high far scale
    xcll.extend_from_slice(&20.0f32.to_le_bytes()); // 96 far height mid
    xcll.extend_from_slice(&10.0f32.to_le_bytes()); // 100 far height range
    xcll.push(2); // 104 interior type = Space Cell
    xcll.extend_from_slice(&[0xAB, 0xAB, 0xAB]); // 105 unused (CK garbage)
    assert_eq!(xcll.len(), 108, "Starfield XCLL must be 108 bytes");

    let mut sub_data = Vec::new();
    let edid = "CydoniaInt\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01); // interior
    sub_data.extend_from_slice(b"XCLL");
    sub_data.extend_from_slice(&(xcll.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(&xcll);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0xCAFEu32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Starfield,
    )
    .unwrap();

    let cell = cells.get("cydoniaint").expect("interior CELL present");
    let lit = cell.lighting.as_ref().expect("XCLL must populate lighting");

    // Shared head decodes as on every game.
    assert_eq!(lit.fog_near, 100.0);
    assert_eq!(lit.fog_far, 5000.0);
    // 32/36 fog clip/power are shared offsets.
    assert_eq!(lit.fog_clip, Some(7500.0));
    assert_eq!(lit.fog_power, Some(1.5));
    // 40-55 fog-far-colour / max / light-fade map onto the base fields.
    assert_eq!(
        lit.fog_far_color,
        Some([120.0 / 255.0, 130.0 / 255.0, 140.0 / 255.0])
    );
    assert_eq!(lit.fog_max, Some(0.85));
    assert_eq!(lit.light_fade_begin, Some(500.0));
    assert_eq!(lit.light_fade_end, Some(800.0));

    // Skyrim-only fields MUST stay None — Starfield has no ambient
    // cube / specular / fresnel, and byte 28 is gravity_scale not fade.
    assert!(
        lit.directional_ambient.is_none(),
        "SF XCLL has no Skyrim ambient cube"
    );
    assert!(lit.specular_color.is_none());
    assert!(lit.specular_alpha.is_none());
    assert!(lit.fresnel_power.is_none());
    assert!(
        lit.directional_fade.is_none(),
        "SF reuses byte 28 as gravity_scale"
    );

    // SF-specific volumetric height-fog tail.
    let sf = lit
        .starfield
        .as_ref()
        .expect("Starfield XCLL must populate .starfield");
    assert_eq!(sf.gravity_scale, 0.7);
    assert_eq!(sf.unknown_color, [10.0 / 255.0, 11.0 / 255.0, 12.0 / 255.0]);
    assert_eq!(sf.near_height_mid, -20.0);
    assert_eq!(sf.near_height_range, 24.0);
    assert_eq!(
        sf.fog_color_high_near,
        [30.0 / 255.0, 31.0 / 255.0, 32.0 / 255.0]
    );
    assert_eq!(
        sf.fog_color_high_far,
        [40.0 / 255.0, 41.0 / 255.0, 42.0 / 255.0]
    );
    assert_eq!(sf.high_density_scale, 0.5);
    assert_eq!(sf.fog_near_scale, 1.1);
    assert_eq!(sf.fog_far_scale, 1.2);
    assert_eq!(sf.fog_high_near_scale, 1.3);
    assert_eq!(sf.fog_high_far_scale, 0.8);
    assert_eq!(sf.far_height_mid, 20.0);
    assert_eq!(sf.far_height_range, 10.0);
    assert_eq!(
        sf.interior_type, 2,
        "Space Cell — and the 3 garbage pad bytes are ignored"
    );
}

#[test]
fn parse_cell_fo4_136byte_xcll_decodes_shared_body() {
    // Render-bug regression: FO4 ships a 92-byte Skyrim body + height-fog
    // tail (here 136 bytes), NOT the FNV 40-byte tail it used to be bucketed
    // with. Field values lifted from the real `HalluciGen01` (form 0x000342CB)
    // XCLL in Fallout4.esm, byte-verified. The length-based dispatch must
    // decode the shared body (fog + ambient cube) at the Skyrim offsets — the
    // 44-byte FO4 height-fog tail (@92-135) is currently ignored.
    let mut xcll = vec![0u8; 136];
    xcll[0..4].copy_from_slice(&[24, 30, 23, 0]); // ambient
    xcll[8..12].copy_from_slice(&[26, 28, 27, 0]); // fog near colour
    xcll[12..16].copy_from_slice(&0.0f32.to_le_bytes()); // fog near
    xcll[16..20].copy_from_slice(&3000.0f32.to_le_bytes()); // fog far
    for face in 0..6 {
        xcll[40 + face * 4..43 + face * 4].copy_from_slice(&[1, 1, 1]); // ambient cube
    }
    xcll[72..76].copy_from_slice(&[44, 52, 50, 0]); // fog far colour
    xcll[76..80].copy_from_slice(&0.7f32.to_le_bytes()); // fog max

    let mut sub_data = Vec::new();
    let edid = "HalluciGen01\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01); // interior
    sub_data.extend_from_slice(b"XCLL");
    sub_data.extend_from_slice(&(xcll.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(&xcll);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0x342CBu32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout4,
    )
    .unwrap();

    let cell = cells.get("hallucigen01").expect("interior CELL present");
    let lit = cell.lighting.as_ref().expect("XCLL must populate lighting");
    // The rendered fog fields decode correctly from the Skyrim-aligned body.
    assert_eq!(lit.ambient, [24.0 / 255.0, 30.0 / 255.0, 23.0 / 255.0]);
    assert_eq!(lit.fog_color, [26.0 / 255.0, 28.0 / 255.0, 27.0 / 255.0]);
    assert_eq!(lit.fog_near, 0.0);
    assert_eq!(lit.fog_far, 3000.0);
    // FO4 reuses the Skyrim 92-byte body, so the ambient cube + fog-far
    // colour decode (unlike a 40-byte FNV cell, where they'd be None).
    assert!(
        lit.directional_ambient.is_some(),
        "FO4 136-byte XCLL has the Skyrim ambient cube"
    );
    assert_eq!(
        lit.fog_far_color,
        Some([44.0 / 255.0, 52.0 / 255.0, 50.0 / 255.0])
    );
}

#[test]
fn parse_cell_fnv_xcll_decodes_colors_as_rgba() {
    // Regression guard: XCLL color fields are RGBA byte order — bytes
    // 0=R, 1=G, 2=B, 3=unused — matching the LIGH DATA revert and
    // xEdit's record definition. The raw bytes here are lifted
    // verbatim from FalloutNV.esm's `GSProspectorSaloonInterior`:
    //
    //   bytes 0..4   1E 29 4D 00   → (R=30, G=41, B=77)
    //   bytes 4..8   1A 20 31 00   → (R=26, G=32, B=49)
    //   bytes 8..12  37 37 5E 00   → (R=55, G=55, B=94)
    //
    // The saloon's ambient is dim cool-blue by design — the warm
    // amber of oil lanterns is delivered by placed LIGH refs, not
    // the cell fill. Under the earlier BGR misread this ambient
    // was flipped to warm (appearing as daytime) which looked
    // "right" on inspection but was factually wrong per the file.
    let mut xcll = vec![0u8; 40];
    xcll[0..4].copy_from_slice(&[0x1E, 0x29, 0x4D, 0x00]);
    xcll[4..8].copy_from_slice(&[0x1A, 0x20, 0x31, 0x00]);
    xcll[8..12].copy_from_slice(&[0x37, 0x37, 0x5E, 0x00]);
    xcll[12..16].copy_from_slice(&64.0f32.to_le_bytes());
    xcll[16..20].copy_from_slice(&3750.0f32.to_le_bytes());
    xcll[24..28].copy_from_slice(&250i32.to_le_bytes());

    let mut sub_data = Vec::new();
    let edid = "GSProspectorSaloonInterior\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01);

    sub_data.extend_from_slice(b"XCLL");
    sub_data.extend_from_slice(&(xcll.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(&xcll);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0x0005B33Eu32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    let cell = cells
        .get("gsprospectorsalooninterior")
        .expect("FNV-shaped interior CELL present");
    let lit = cell.lighting.as_ref().expect("XCLL populated");

    // Ambient: bytes (0x1E, 0x29, 0x4D) → RGB → (R=30, G=41, B=77).
    assert!((lit.ambient[0] - 30.0 / 255.0).abs() < 1e-6, "ambient R");
    assert!((lit.ambient[1] - 41.0 / 255.0).abs() < 1e-6, "ambient G");
    assert!((lit.ambient[2] - 77.0 / 255.0).abs() < 1e-6, "ambient B");

    // Directional: bytes (0x1A, 0x20, 0x31) → (R=26, G=32, B=49).
    assert!((lit.directional_color[0] - 26.0 / 255.0).abs() < 1e-6);
    assert!((lit.directional_color[1] - 32.0 / 255.0).abs() < 1e-6);
    assert!((lit.directional_color[2] - 49.0 / 255.0).abs() < 1e-6);

    // Fog: bytes (0x37, 0x37, 0x5E) → (R=55, G=55, B=94).
    assert!((lit.fog_color[0] - 55.0 / 255.0).abs() < 1e-6);
    assert!((lit.fog_color[1] - 55.0 / 255.0).abs() < 1e-6);
    assert!((lit.fog_color[2] - 94.0 / 255.0).abs() < 1e-6);
    assert_eq!(lit.fog_near, 64.0);
    assert_eq!(lit.fog_far, 3750.0);
}

#[test]
fn parse_cell_fnv_xcll_extracts_40byte_tail_and_skips_skyrim_fields() {
    // The 40-byte FNV XCLL carries `directional_fade`, `fog_clip`,
    // and `fog_power` in the 28..40 tail per nif.xml + UESP. Pre-#379
    // those fields were only surfaced when the whole block was
    // Skyrim-extended (`d.len() >= 92`), so FNV cells silently
    // reported all three as `None` and fell back to hardcoded
    // renderer defaults.
    //
    // Post-#379 the 28..40 tail has its own `>= 40` gate, separate
    // from the Skyrim-only `>= 92` block that carries the ambient
    // cube / specular / fresnel / fog-far-color. This test pins
    // both halves.
    let mut xcll = vec![0u8; 40];
    xcll[0..4].copy_from_slice(&[80, 82, 85, 0]); // ambient
    xcll[4..8].copy_from_slice(&[200, 195, 180, 0]); // directional
    xcll[12..16].copy_from_slice(&100.0f32.to_le_bytes());
    xcll[16..20].copy_from_slice(&5000.0f32.to_le_bytes());
    // FNV extended tail (bytes 28-39).
    xcll[28..32].copy_from_slice(&0.75f32.to_le_bytes()); // directional_fade
    xcll[32..36].copy_from_slice(&6500.0f32.to_le_bytes()); // fog_clip
    xcll[36..40].copy_from_slice(&1.25f32.to_le_bytes()); // fog_power

    let mut sub_data = Vec::new();
    let edid = "FnvRoom\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01);

    sub_data.extend_from_slice(b"XCLL");
    sub_data.extend_from_slice(&(xcll.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(&xcll);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0xF00Du32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    let cell = cells.get("fnvroom").expect("FNV-shaped interior CELL");
    let lit = cell.lighting.as_ref().unwrap();

    // FNV-extended tail — now populated for 40-byte XCLL.
    assert_eq!(lit.directional_fade, Some(0.75));
    assert_eq!(lit.fog_clip, Some(6500.0));
    assert_eq!(lit.fog_power, Some(1.25));

    // Skyrim-only fields are still None at 40 bytes.
    assert!(
        lit.directional_ambient.is_none(),
        "FNV XCLL has no ambient cube"
    );
    assert!(lit.specular_color.is_none());
    assert!(lit.specular_alpha.is_none());
    assert!(lit.fresnel_power.is_none());
    assert!(lit.fog_far_color.is_none());
    assert!(lit.fog_max.is_none());
    assert!(lit.light_fade_begin.is_none());
    assert!(lit.light_fade_end.is_none());
}

/// Helper: build a CELL buffer wrapping a single interior `XCLL` of the
/// given bytes, then parse it with `game` and return its `CellLighting`.
fn parse_oblivion_xcll(
    edid: &str,
    xcll: &[u8],
    game: crate::esm::reader::GameKind,
) -> CellLighting {
    let mut sub_data = Vec::new();
    let edid_z = format!("{edid}\0");
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid_z.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid_z.as_bytes());
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01); // interior
    sub_data.extend_from_slice(b"XCLL");
    sub_data.extend_from_slice(&(xcll.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(xcll);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0xAB1Eu32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(&mut reader, end, &mut cells, game).unwrap();
    cells
        .get(&edid.to_ascii_lowercase())
        .expect("interior CELL present")
        .lighting
        .clone()
        .expect("XCLL must populate lighting")
}

#[test]
fn parse_cell_oblivion_36byte_xcll_extracts_dir_fade_and_fog_clip() {
    // #1312 — the full 36-byte TES4 XCLL is shared(28) +
    // `Directional Fade`(@28) + `Fog Clip Dist`(@32), with NO Fog Power
    // (that's the FO3/FNV 40-byte addition). The old `len >= 40` gate
    // silently dropped both extended fields on every Oblivion cell.
    // Verified layout vs xEdit TES4 `wbStruct(XCLL)` + OpenMW
    // `loadcell.cpp` `case 36`.
    let mut xcll = vec![0u8; 36];
    xcll[0..4].copy_from_slice(&[80, 82, 85, 0]); // ambient
    xcll[12..16].copy_from_slice(&100.0f32.to_le_bytes()); // fog_near
    xcll[16..20].copy_from_slice(&5000.0f32.to_le_bytes()); // fog_far
    xcll[28..32].copy_from_slice(&0.6f32.to_le_bytes()); // Directional Fade
    xcll[32..36].copy_from_slice(&7000.0f32.to_le_bytes()); // Fog Clip Dist

    let lit = parse_oblivion_xcll("OblRoom36", &xcll, crate::esm::reader::GameKind::Oblivion);
    assert_eq!(
        lit.directional_fade,
        Some(0.6),
        "dir_fade(@28) must be read at 36 bytes"
    );
    assert_eq!(
        lit.fog_clip,
        Some(7000.0),
        "fog_clip(@32) must be read at 36 bytes"
    );
    assert_eq!(
        lit.fog_power, None,
        "TES4 has no Fog Power — absent below 40 bytes"
    );
    assert!(lit.directional_ambient.is_none());
    assert!(lit.starfield.is_none());
}

#[test]
fn parse_cell_oblivion_32byte_xcll_extracts_dir_fade_only() {
    // #1312 per-field gating: a 32-byte XCLL (a valid TES4 size) carries
    // `Directional Fade`(@28) but not `Fog Clip Dist`(@32). The fix gates
    // each field on its own offset, so dir_fade survives where fog_clip
    // is correctly absent.
    let mut xcll = vec![0u8; 32];
    xcll[12..16].copy_from_slice(&80.0f32.to_le_bytes()); // fog_near
    xcll[28..32].copy_from_slice(&0.45f32.to_le_bytes()); // Directional Fade
    let lit = parse_oblivion_xcll("OblRoom32", &xcll, crate::esm::reader::GameKind::Oblivion);
    assert_eq!(
        lit.directional_fade,
        Some(0.45),
        "dir_fade(@28) must be read at 32 bytes"
    );
    assert_eq!(lit.fog_clip, None, "fog_clip(@32) absent below 36 bytes");
    assert_eq!(lit.fog_power, None);
}

#[test]
fn parse_cell_without_xclw_leaves_water_height_none() {
    // Sibling check for the XCLW match arm: a CELL with no XCLW
    // sub-record keeps `water_height = None`. Catches a regression
    // where the arm accidentally consumed some other sub-record.
    let mut sub_data = Vec::new();
    let edid = "DryRoom\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&1u16.to_le_bytes());
    sub_data.push(0x01);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"CELL");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&0x01u32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]);
    buf.extend_from_slice(&sub_data);

    let mut reader = super::super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(
        &mut reader,
        end,
        &mut cells,
        crate::esm::reader::GameKind::Fallout3NV,
    )
    .unwrap();

    let cell = cells.get("dryroom").expect("interior CELL present");
    assert_eq!(cell.water_height, None);
}
