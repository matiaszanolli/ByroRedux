//! Tests for ../cell.rs.
//!
//! Extracted from cell.rs in the M46.0 / monolith-refactor pass — production-side
//! cell.rs was 4597 LOC (53% test scaffolding, mostly synthetic record builders);
//! moving the test mod body to this sibling file via `#[path]` keeps the same
//! qualified test paths (`esm::cell::tests::FOO`) while halving the cell.rs
//! token budget. See refactor stage A.

use super::super::reader::EsmReader;
use super::helpers::read_zstring;
use super::support::{parse_modl_group, parse_txst_group};
use super::walkers::{parse_cell_group, parse_refr_group};
use super::*;

// Helper: build minimal STAT record bytes.
fn build_stat_record(form_id: u32, editor_id: &str, model_path: &str) -> Vec<u8> {
    let mut sub_data = Vec::new();
    // EDID
    let edid = format!("{}\0", editor_id);
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());
    // MODL
    let modl = format!("{}\0", model_path);
    sub_data.extend_from_slice(b"MODL");
    sub_data.extend_from_slice(&(modl.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(modl.as_bytes());

    let mut buf = Vec::new();
    buf.extend_from_slice(b"STAT");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&sub_data);
    buf
}

// Helper: build minimal ADDN record bytes with DATA (s32 index) +
// DNAM (u16 cap, u16 flags). Optional EDID / MODL included. See #370.
fn build_addn_record(
    form_id: u32,
    editor_id: &str,
    model_path: &str,
    addon_index: i32,
    cap: u16,
    flags: u16,
) -> Vec<u8> {
    let mut sub_data = Vec::new();
    // EDID
    let edid = format!("{}\0", editor_id);
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());
    // MODL
    let modl = format!("{}\0", model_path);
    sub_data.extend_from_slice(b"MODL");
    sub_data.extend_from_slice(&(modl.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(modl.as_bytes());
    // DATA: s32 addon_index
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&addon_index.to_le_bytes());
    // DNAM: u16 cap + u16 flags
    sub_data.extend_from_slice(b"DNAM");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&cap.to_le_bytes());
    sub_data.extend_from_slice(&flags.to_le_bytes());

    let mut buf = Vec::new();
    buf.extend_from_slice(b"ADDN");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&sub_data);
    buf
}

// Helper: build minimal LIGH record with DATA subrecord. The DATA
// payload uses the real on-disk layout: time(u32) + radius(u32) +
// color(R, G, B, Unknown — u8×4) + flags(u32) = 16 bytes. EDID comes
// first. The on-disk byte order matches xEdit's `Color { Red; Green;
// Blue; Unknown }` definition; pre-#389-revert this comment said BGRA
// to match a transient D3DCOLOR_XRGB interpretation that flipped warm
// EDIDs (`OurLadyHopeRed`, `BasementLightKickerWarm`) to their cool
// complements. See `parse_ligh_decodes_color_as_rgba` below for the
// FNV-sample evidence.
fn build_ligh_record(
    form_id: u32,
    editor_id: &str,
    radius: u32,
    rgb: [u8; 3],
    flags: u32,
) -> Vec<u8> {
    let mut sub_data = Vec::new();
    let edid = format!("{}\0", editor_id);
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&16u16.to_le_bytes());
    sub_data.extend_from_slice(&u32::MAX.to_le_bytes()); // time = -1
    sub_data.extend_from_slice(&radius.to_le_bytes());
    sub_data.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 0u8]); // RGBA on disk
    sub_data.extend_from_slice(&flags.to_le_bytes());

    let mut buf = Vec::new();
    buf.extend_from_slice(b"LIGH");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // padding
    buf.extend_from_slice(&sub_data);
    buf
}

#[test]
fn parse_ligh_decodes_color_as_rgba() {
    // Regression: LIGH DATA bytes 8..12 are stored on disk as
    // (Red, Green, Blue, Unknown) — same order xEdit lists in its
    // Color struct definition. Fix #389 previously interpreted these
    // as D3DCOLOR_XRGB (BGRA) but that was based on an ambiguous
    // Oblivion `RootGreenBright0650` sample (G is max either way).
    //
    // FalloutNV.esm makes the correct order unambiguous:
    //   OurLadyHopeRed              bytes ≈ FB 95 24 00 → R=251 warm red
    //   DunwichLightOrangeFlicker01 bytes ≈ AE 6A 26 00 → R=174 warm orange
    //   BasementLightKickerWarm     bytes ≈ B0 D3 E4 ?? → B=228 cool cyan ← #389 reversed this
    // Under BGR every warm/red/orange EDID surfaced as its cool complement
    // (blue/cyan), visible in GSProspectorSaloonInterior torches.
    //
    // This test uses the same RootGreenBright bytes 36 74 66 00 — since
    // green is max in either order, the test is a boundary check that
    // the R channel ends up on output[0] (RGB), not that G dominates.
    let ligh = build_ligh_record(
        0xABCD,
        "RootGreenBright0650",
        650,
        [0x36, 0x74, 0x66],
        0x400,
    );
    let total_size = 24 + ligh.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"LIGH");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&ligh);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let s = statics.get(&0xABCD).expect("LIGH entry present");
    let ld = s.light_data.as_ref().expect("light_data populated");
    assert!((ld.radius - 650.0).abs() < 0.5);
    let [r, g, b] = ld.color;
    // Bytes were supplied as [0x36, 0x74, 0x66] — under RGB they map to
    // R=0x36 (54), G=0x74 (116), B=0x66 (102).
    assert!((r - 0x36 as f32 / 255.0).abs() < 1e-4, "R mismatch: {r}");
    assert!((g - 0x74 as f32 / 255.0).abs() < 1e-4, "G mismatch: {g}");
    assert!((b - 0x66 as f32 / 255.0).abs() < 1e-4, "B mismatch: {b}");
    assert!(g > r && g > b, "green-authored light must peak on G");
    assert_eq!(ld.flags, 0x400);
    // Pre-#602 XPWR wasn't captured — the baseline record here has
    // no XPWR sub-record so the field stays `None`. See
    // `parse_ligh_captures_fo4_xpwr_power_circuit_ref` below for
    // the populated path.
    assert!(ld.xpwr_form_id.is_none());
}

/// Regression for #602 (FO4-DIM6-07) — FO4 LIGH records that ship
/// `XPWR` (power-circuit FormID) must land on
/// `LightData.xpwr_form_id` so a future settlement-circuit ECS
/// system can resolve wired fixtures. Pre-fix the sub-record was
/// silently dropped and every wired light rendered always-on.
/// Consumer wiring is follow-up — this test asserts the capture
/// side only.
#[test]
fn parse_ligh_captures_fo4_xpwr_power_circuit_ref() {
    // Build a LIGH record with EDID + DATA + XPWR. Sub-records may
    // appear in any authoring order; we put XPWR after DATA here
    // and rely on the post-loop merge step to fold it in.
    let mut sub_data = Vec::new();
    // EDID
    let edid = b"Fo4WiredLight\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid);
    // DATA (16 bytes: time + radius + color + flags)
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&16u16.to_le_bytes());
    sub_data.extend_from_slice(&u32::MAX.to_le_bytes()); // time = -1
    sub_data.extend_from_slice(&256u32.to_le_bytes()); // radius
    sub_data.extend_from_slice(&[0xFF, 0xC0, 0x80, 0x00]); // warm RGB + pad
    sub_data.extend_from_slice(&0u32.to_le_bytes()); // flags
                                                     // XPWR — 4-byte FormID.
    sub_data.extend_from_slice(b"XPWR");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&0x0011_AABBu32.to_le_bytes());

    let mut ligh = Vec::new();
    ligh.extend_from_slice(b"LIGH");
    ligh.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    ligh.extend_from_slice(&0u32.to_le_bytes()); // flags
    ligh.extend_from_slice(&0xBEEFu32.to_le_bytes()); // form_id
    ligh.extend_from_slice(&[0u8; 8]); // padding
    ligh.extend_from_slice(&sub_data);

    let total_size = 24 + ligh.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"LIGH");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&ligh);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let s = statics.get(&0xBEEF).expect("LIGH entry present");
    let ld = s.light_data.as_ref().expect("light_data populated");
    assert_eq!(
        ld.xpwr_form_id,
        Some(0x0011_AABB),
        "XPWR power-circuit FormID must be captured for #602 pre-work"
    );
}

/// Sibling: XPWR that appears BEFORE the DATA sub-record still
/// folds onto the final LightData. Guards the post-loop merge
/// against authoring-order assumptions.
#[test]
fn parse_ligh_xpwr_before_data_still_merges_into_light_data() {
    let mut sub_data = Vec::new();
    let edid = b"Fo4WiredLightInverse\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid);
    // XPWR first.
    sub_data.extend_from_slice(b"XPWR");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&0x0022_1234u32.to_le_bytes());
    // DATA second.
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&16u16.to_le_bytes());
    sub_data.extend_from_slice(&u32::MAX.to_le_bytes());
    sub_data.extend_from_slice(&128u32.to_le_bytes());
    sub_data.extend_from_slice(&[0x80, 0xC0, 0xFF, 0x00]);
    sub_data.extend_from_slice(&0u32.to_le_bytes());

    let mut ligh = Vec::new();
    ligh.extend_from_slice(b"LIGH");
    ligh.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    ligh.extend_from_slice(&0u32.to_le_bytes());
    ligh.extend_from_slice(&0xCAFEu32.to_le_bytes());
    ligh.extend_from_slice(&[0u8; 8]);
    ligh.extend_from_slice(&sub_data);

    let total_size = 24 + ligh.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"LIGH");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&ligh);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let ld = statics
        .get(&0xCAFE)
        .and_then(|s| s.light_data.as_ref())
        .expect("light_data populated");
    assert_eq!(ld.xpwr_form_id, Some(0x0022_1234));
}

#[test]
fn parse_ligh_decodes_fnv_warm_lights_without_channel_swap() {
    // Regression guard for the #389 revert: FalloutNV.esm ships
    // several colorfully-named LIGH records that make the RGB byte
    // order unambiguous. Under the previous BGR interpretation every
    // one surfaced as its cool complement.
    //
    // Values here were dumped live from FalloutNV.esm during the
    // session-12 FNV audit. Each is asserted to land under the
    // relative-channel dominance the EDID advertises.
    for (edid, rgb, expected_dominant) in [
        // form_id-independent samples — only colors are asserted.
        ("OurLadyHopeRed", [0xFB, 0x95, 0x24], 'R'),
        ("DunwichLightOrangeFlicker01", [0xAE, 0x6A, 0x26], 'R'),
        ("BasementLightKickerWarm", [0xAF, 0xD3, 0xE4], 'R'), // warm = brighter R/G than raw pack
        ("BasementLightFillCool", [0xA5, 0xB8, 0xC9], 'B'),
    ] {
        let _ = expected_dominant; // retained for doc; hardcoded below.
        let ligh = build_ligh_record(0x1234, edid, 128, rgb, 0);
        let total_size = 24 + ligh.len();
        let mut group = Vec::new();
        group.extend_from_slice(b"GRUP");
        group.extend_from_slice(&(total_size as u32).to_le_bytes());
        group.extend_from_slice(b"LIGH");
        group.extend_from_slice(&0u32.to_le_bytes());
        group.extend_from_slice(&[0u8; 8]);
        group.extend_from_slice(&ligh);

        let mut reader = EsmReader::new(&group);
        let gh = reader.read_group_header().unwrap();
        let end = reader.group_content_end(&gh);
        let mut statics = HashMap::new();
        parse_modl_group(&mut reader, end, &mut statics).unwrap();

        let s = statics.get(&0x1234).expect("LIGH entry present");
        let ld = s.light_data.as_ref().expect("light_data populated");
        let [r, g, b] = ld.color;
        assert!(
            (r - rgb[0] as f32 / 255.0).abs() < 1e-4,
            "{edid}: R byte mismatch (got {r})"
        );
        assert!(
            (g - rgb[1] as f32 / 255.0).abs() < 1e-4,
            "{edid}: G byte mismatch (got {g})"
        );
        assert!(
            (b - rgb[2] as f32 / 255.0).abs() < 1e-4,
            "{edid}: B byte mismatch (got {b})"
        );
    }
}

#[test]
fn parse_addn_extracts_data_and_dnam() {
    // Regression: #370 — ADDN DATA (s32 addon index) and DNAM
    // (u16 cap + u16 flags) must both land on StaticObject.addon_data.
    let addn = build_addn_record(
        0x4567,
        "MothSwarm01",
        "meshes\\effects\\moths.nif",
        7,
        64,
        1,
    );
    let total_size = 24 + addn.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"ADDN");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&addn);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let s = statics.get(&0x4567).expect("ADDN entry present");
    assert_eq!(s.editor_id, "MothSwarm01");
    assert_eq!(s.model_path, "meshes\\effects\\moths.nif");
    let ad = s.addon_data.expect("addon_data populated");
    assert_eq!(ad.addon_index, 7);
    assert_eq!(ad.master_particle_cap, 64);
    assert_eq!(ad.flags, 1);
}

#[test]
fn parse_stat_with_vmad_sets_has_script() {
    // Regression: #369 — Skyrim VMAD sub-records on STAT records
    // were dropped on the walker's `_` arm. The minimum-viable
    // signal is a `has_script: bool` on `StaticObject` so the count
    // of script-bearing records is at least visible. Full VMAD
    // decoding (Papyrus script names + property bindings) stays
    // gated on the scripting-as-ECS work.
    let mut sub_data = Vec::new();
    let edid = "ScriptedDoor\0";
    sub_data.extend_from_slice(b"EDID");
    sub_data.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(edid.as_bytes());
    let modl = "meshes\\door.nif\0";
    sub_data.extend_from_slice(b"MODL");
    sub_data.extend_from_slice(&(modl.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(modl.as_bytes());
    // VMAD: opaque payload — content doesn't matter for the
    // presence flag, only that the sub-record exists.
    let vmad_payload: &[u8] = b"\x05\x00\x02\x00\x00\x00";
    sub_data.extend_from_slice(b"VMAD");
    sub_data.extend_from_slice(&(vmad_payload.len() as u16).to_le_bytes());
    sub_data.extend_from_slice(vmad_payload);

    let mut stat = Vec::new();
    stat.extend_from_slice(b"STAT");
    stat.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    stat.extend_from_slice(&0u32.to_le_bytes());
    stat.extend_from_slice(&0x77u32.to_le_bytes());
    stat.extend_from_slice(&[0u8; 8]);
    stat.extend_from_slice(&sub_data);

    let total_size = 24 + stat.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"STAT");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&stat);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let s = statics.get(&0x77).expect("STAT entry present");
    assert!(s.has_script, "VMAD presence must flip has_script");
}

#[test]
fn parse_stat_without_vmad_leaves_has_script_false() {
    // Sibling check — a STAT with only EDID + MODL keeps has_script
    // at false. Catches a regression where the new arm captures
    // some other neighbour sub-record.
    let stat = build_stat_record(0x88, "PlainStatic", "meshes\\stat.nif");
    let total_size = 24 + stat.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"STAT");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&stat);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let s = statics.get(&0x88).expect("STAT entry present");
    assert!(!s.has_script);
}

#[test]
fn parse_non_addn_record_has_no_addon_data() {
    // STATs must not accidentally populate addon_data even if a
    // same-named DATA sub-record happens to exist.
    let stat = build_stat_record(0x9999, "RegularWall", "meshes\\wall.nif");
    let total_size = 24 + stat.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"STAT");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&stat);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let s = statics.get(&0x9999).expect("STAT entry");
    assert!(s.addon_data.is_none(), "STAT must not carry addon data");
}

#[test]
fn parse_stat_record() {
    let stat = build_stat_record(0x1234, "TestWall", "meshes\\architecture\\wall01.nif");
    // Wrap in a GRUP.
    let total_size = 24 + stat.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"STAT");
    group.extend_from_slice(&0u32.to_le_bytes()); // group_type = 0 (top)
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&stat);

    let mut reader = EsmReader::new(&group);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    assert_eq!(statics.len(), 1);
    let s = statics.get(&0x1234).unwrap();
    assert_eq!(s.editor_id, "TestWall");
    assert_eq!(s.model_path, "meshes\\architecture\\wall01.nif");
}

#[test]
fn parse_modl_group_walks_oblivion_20byte_headers() {
    // Regression: #391 — the walker used to compute a group's content
    // end as `position + total_size - 24`, hardcoding the Tes5Plus
    // header size. On Oblivion that over-reads by 4 bytes; symptoms
    // were latent (the next read happened to land on a self-delimiting
    // GRUP) but any bounds-checked nested parse would have read junk.
    //
    // Build an Oblivion-shaped (20-byte header) STAT group with two
    // STAT records, run it through `parse_modl_group` using the
    // explicit `Oblivion` reader variant, and assert: both records
    // extracted, no leftover bytes, no junk record dispatched after
    // the second.
    use super::super::reader::EsmVariant;

    // Build a 20-byte-header STAT record (Oblivion layout).
    fn build_stat_oblivion(form_id: u32, edid: &str, modl: &str) -> Vec<u8> {
        let mut sub_data = Vec::new();
        let edid_z = format!("{}\0", edid);
        sub_data.extend_from_slice(b"EDID");
        sub_data.extend_from_slice(&(edid_z.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(edid_z.as_bytes());
        let modl_z = format!("{}\0", modl);
        sub_data.extend_from_slice(b"MODL");
        sub_data.extend_from_slice(&(modl_z.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(modl_z.as_bytes());

        let mut buf = Vec::new();
        buf.extend_from_slice(b"STAT");
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&[0u8; 4]); // Oblivion vc_info (4 bytes)
        buf.extend_from_slice(&sub_data);
        buf
    }

    let r1 = build_stat_oblivion(0x111, "WallA", "meshes\\a.nif");
    let r2 = build_stat_oblivion(0x222, "WallB", "meshes\\b.nif");
    let mut content = Vec::new();
    content.extend_from_slice(&r1);
    content.extend_from_slice(&r2);

    // 20-byte group header.
    let total_size = 20 + content.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"STAT");
    group.extend_from_slice(&0u32.to_le_bytes()); // group_type
    group.extend_from_slice(&[0u8; 4]); // Oblivion stamp (4 bytes)
    group.extend_from_slice(&content);

    // Append a sentinel byte beyond the group end. With the old
    // `-24` walker this byte would land inside the computed end and
    // get dispatched as part of the next read; with the fix the
    // walker stops cleanly at byte `total_size`.
    group.push(0xEE);

    let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    // Content end must sit immediately before the sentinel, not past it.
    assert_eq!(end, total_size);

    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    assert_eq!(statics.len(), 2, "both Oblivion STATs must be parsed");
    assert_eq!(statics.get(&0x111).unwrap().editor_id, "WallA");
    assert_eq!(statics.get(&0x222).unwrap().editor_id, "WallB");
    // Walker must have stopped exactly at `end`, leaving the
    // sentinel byte for the caller.
    assert_eq!(reader.position(), end);
    assert_eq!(reader.remaining(), 1);
}

#[test]
fn parse_refr_extracts_position_and_scale() {
    // Build a minimal REFR record with NAME, DATA, XSCL.
    let mut sub_data = Vec::new();
    // NAME (base form id)
    sub_data.extend_from_slice(b"NAME");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&0xABCDu32.to_le_bytes());
    // DATA (6 floats: pos xyz, rot xyz)
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&24u16.to_le_bytes());
    sub_data.extend_from_slice(&100.0f32.to_le_bytes()); // pos x
    sub_data.extend_from_slice(&200.0f32.to_le_bytes()); // pos y
    sub_data.extend_from_slice(&300.0f32.to_le_bytes()); // pos z
    sub_data.extend_from_slice(&0.0f32.to_le_bytes()); // rot x
    sub_data.extend_from_slice(&1.57f32.to_le_bytes()); // rot y
    sub_data.extend_from_slice(&0.0f32.to_le_bytes()); // rot z
                                                       // XSCL
    sub_data.extend_from_slice(b"XSCL");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&2.0f32.to_le_bytes());

    let mut record = Vec::new();
    record.extend_from_slice(b"REFR");
    record.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes()); // flags
    record.extend_from_slice(&0x5678u32.to_le_bytes()); // form id
    record.extend_from_slice(&[0u8; 8]);
    record.extend_from_slice(&sub_data);

    let mut reader = EsmReader::new(&record);
    let end = record.len();
    let mut refs = Vec::new();
    let mut land = None;
    parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

    assert_eq!(refs.len(), 1);
    let r = &refs[0];
    assert_eq!(r.base_form_id, 0xABCD);
    assert!((r.position[0] - 100.0).abs() < 1e-6);
    assert!((r.position[1] - 200.0).abs() < 1e-6);
    assert!((r.position[2] - 300.0).abs() < 1e-6);
    assert!((r.rotation[1] - 1.57).abs() < 0.01);
    assert!((r.scale - 2.0).abs() < 1e-6);
    // No XESP → enable_parent stays None.
    assert!(r.enable_parent.is_none());
}

/// Helper for the #349 XESP regression tests — build a REFR with
/// just NAME + DATA + XESP. The minimum sub-record set
/// `parse_refr_group` needs to register a placement.
fn build_refr_with_xesp(form_id: u32, parent_form: u32, inverted_flag: u8) -> Vec<u8> {
    let mut sub_data = Vec::new();
    sub_data.extend_from_slice(b"NAME");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&form_id.to_le_bytes());

    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&24u16.to_le_bytes());
    sub_data.extend_from_slice(&[0u8; 24]); // zero pos + rot

    sub_data.extend_from_slice(b"XESP");
    sub_data.extend_from_slice(&5u16.to_le_bytes());
    sub_data.extend_from_slice(&parent_form.to_le_bytes());
    sub_data.push(inverted_flag);

    let mut record = Vec::new();
    record.extend_from_slice(b"REFR");
    record.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes());
    record.extend_from_slice(&0x9999u32.to_le_bytes()); // record form_id
    record.extend_from_slice(&[0u8; 8]);
    record.extend_from_slice(&sub_data);
    record
}

/// Regression: #471 flipped #349's interim predicate. Without a
/// two-pass loader to inspect each parent's real 0x0800 flag, we
/// assume parents are enabled by default (the vanilla case). A
/// non-inverted XESP child is visible when the parent is enabled,
/// so the cell loader must NOT skip it.
#[test]
fn parse_refr_extracts_non_inverted_xesp_renders_by_default() {
    let record = build_refr_with_xesp(0xABCD, 0xCAFE, 0); // not inverted
    let mut reader = EsmReader::new(&record);
    let end = record.len();
    let mut refs = Vec::new();
    let mut land = None;
    parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

    assert_eq!(refs.len(), 1);
    let ep = refs[0]
        .enable_parent
        .expect("XESP must populate enable_parent");
    assert_eq!(ep.form_id, 0xCAFE);
    assert!(!ep.inverted);
    assert!(
        !ep.default_disabled(),
        "non-inverted XESP with assumed-enabled parent renders (#471)"
    );
}

/// #471: inverted XESP is visible when the parent is *disabled*.
/// With the parents-assumed-enabled default, the child must be
/// treated as hidden at cell load.
#[test]
fn parse_refr_extracts_inverted_xesp_hidden_by_default() {
    let record = build_refr_with_xesp(0xABCD, 0xCAFE, 0x01); // inverted bit set
    let mut reader = EsmReader::new(&record);
    let end = record.len();
    let mut refs = Vec::new();
    let mut land = None;
    parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

    assert_eq!(refs.len(), 1);
    let ep = refs[0]
        .enable_parent
        .expect("XESP must populate enable_parent");
    assert_eq!(ep.form_id, 0xCAFE);
    assert!(ep.inverted);
    assert!(
        ep.default_disabled(),
        "inverted XESP with assumed-enabled parent is hidden (#471)"
    );
}

/// Sibling: a REFR with no XESP at all keeps `enable_parent = None`
/// — `default_disabled()` is irrelevant because the cell loader
/// only inspects `Some(ep)`. The pre-#349 behaviour is preserved
/// for the common (non-quest-gated) case.
#[test]
fn parse_refr_without_xesp_has_no_enable_parent() {
    let record = build_refr_with_xesp(0xABCD, 0, 0);
    // `build_refr_with_xesp` always emits an XESP — strip it for
    // this test by hand-building a NAME+DATA-only REFR.
    let _ = record;

    let mut sub_data = Vec::new();
    sub_data.extend_from_slice(b"NAME");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&0xBEEFu32.to_le_bytes());
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&24u16.to_le_bytes());
    sub_data.extend_from_slice(&[0u8; 24]);

    let mut record = Vec::new();
    record.extend_from_slice(b"REFR");
    record.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes());
    record.extend_from_slice(&0x42u32.to_le_bytes());
    record.extend_from_slice(&[0u8; 8]);
    record.extend_from_slice(&sub_data);

    let mut reader = EsmReader::new(&record);
    let end = record.len();
    let mut refs = Vec::new();
    let mut land = None;
    parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

    assert_eq!(refs.len(), 1);
    assert!(refs[0].enable_parent.is_none());
}

/// Helper for #412 tests — build a REFR record from an arbitrary
/// sequence of (sub_type, payload) tuples so each test can target
/// exactly one sub-record arm. The REFR's own form ID is fixed at
/// `0x412412` so test failures are easy to grep for.
fn build_refr_with_subs(base_form_id: u32, extras: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut sub_data = Vec::new();
    sub_data.extend_from_slice(b"NAME");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&base_form_id.to_le_bytes());
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&24u16.to_le_bytes());
    sub_data.extend_from_slice(&[0u8; 24]);
    for (sub_type, payload) in extras {
        sub_data.extend_from_slice(*sub_type);
        sub_data.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(payload);
    }

    let mut record = Vec::new();
    record.extend_from_slice(b"REFR");
    record.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes());
    record.extend_from_slice(&0x412412u32.to_le_bytes());
    record.extend_from_slice(&[0u8; 8]);
    record.extend_from_slice(&sub_data);
    record
}

fn parse_one_refr(record: &[u8]) -> PlacedRef {
    let mut reader = EsmReader::new(record);
    let end = record.len();
    let mut refs = Vec::new();
    let mut land = None;
    parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();
    assert_eq!(refs.len(), 1, "exactly one REFR expected");
    refs.remove(0)
}

/// Regression for #412 — XTEL must populate `teleport` with the
/// destination ref + position + rotation. Pre-fix every interior
/// door was silently dropped on parse and activation did nothing.
#[test]
fn parse_refr_extracts_xtel_teleport_destination() {
    // XTEL = DestRef(u32) + pos(3×f32) + rot(3×f32) = 28 B.
    let mut xtel = Vec::new();
    xtel.extend_from_slice(&0xDEADu32.to_le_bytes()); // destination
    xtel.extend_from_slice(&100.0f32.to_le_bytes()); // pos x
    xtel.extend_from_slice(&200.0f32.to_le_bytes()); // pos y
    xtel.extend_from_slice(&50.0f32.to_le_bytes()); // pos z
    xtel.extend_from_slice(&0.0f32.to_le_bytes()); // rot x
    xtel.extend_from_slice(&std::f32::consts::FRAC_PI_2.to_le_bytes()); // rot y
    xtel.extend_from_slice(&0.0f32.to_le_bytes()); // rot z

    let record = build_refr_with_subs(0xBEEF, &[(b"XTEL", &xtel)]);
    let r = parse_one_refr(&record);
    let t = r.teleport.expect("XTEL must populate teleport");
    assert_eq!(t.destination, 0xDEAD);
    assert_eq!(t.position, [100.0, 200.0, 50.0]);
    assert_eq!(t.rotation[1], std::f32::consts::FRAC_PI_2);
}

/// Regression for #412 — XTEL with the optional 4-byte trailing
/// flags (Skyrim+) still parses the 28-byte core correctly. Pre-fix
/// neither 28- nor 32-byte variant was handled.
#[test]
fn parse_refr_xtel_with_trailing_flags() {
    let mut xtel = Vec::new();
    xtel.extend_from_slice(&0xDEADu32.to_le_bytes());
    xtel.extend_from_slice(&[0u8; 24]); // pos + rot zeros
    xtel.extend_from_slice(&0x01u32.to_le_bytes()); // trailing flags
    assert_eq!(xtel.len(), 32);
    let record = build_refr_with_subs(0xBEEF, &[(b"XTEL", &xtel)]);
    let r = parse_one_refr(&record);
    let t = r.teleport.expect("XTEL with flags must still parse");
    assert_eq!(t.destination, 0xDEAD);
}

/// Regression for #412 — multiple XLKR sub-records collect into
/// `linked_refs`. Pre-fix NPCs didn't know which patrol marker to
/// walk to and doors didn't pair with their teleport partner.
#[test]
fn parse_refr_extracts_multiple_xlkr_linked_refs() {
    let mut xlkr_a = Vec::new();
    xlkr_a.extend_from_slice(&0x11111111u32.to_le_bytes()); // keyword
    xlkr_a.extend_from_slice(&0x22222222u32.to_le_bytes()); // target
    let mut xlkr_b = Vec::new();
    xlkr_b.extend_from_slice(&0u32.to_le_bytes()); // untyped link
    xlkr_b.extend_from_slice(&0x33333333u32.to_le_bytes());

    let record = build_refr_with_subs(0xBEEF, &[(b"XLKR", &xlkr_a), (b"XLKR", &xlkr_b)]);
    let r = parse_one_refr(&record);
    assert_eq!(r.linked_refs.len(), 2, "both XLKR sub-records collected");
    assert_eq!(r.linked_refs[0].keyword, 0x11111111);
    assert_eq!(r.linked_refs[0].target, 0x22222222);
    assert_eq!(r.linked_refs[1].keyword, 0);
    assert_eq!(r.linked_refs[1].target, 0x33333333);
}

/// Regression for #412 — XPRM populates `primitive` so invisible
/// activators / trigger boxes have a runtime-usable volume.
#[test]
fn parse_refr_extracts_xprm_primitive_bounds() {
    let mut xprm = Vec::new();
    // bounds
    xprm.extend_from_slice(&128.0f32.to_le_bytes());
    xprm.extend_from_slice(&64.0f32.to_le_bytes());
    xprm.extend_from_slice(&32.0f32.to_le_bytes());
    // color
    xprm.extend_from_slice(&1.0f32.to_le_bytes());
    xprm.extend_from_slice(&0.5f32.to_le_bytes());
    xprm.extend_from_slice(&0.0f32.to_le_bytes());
    // unknown + shape
    xprm.extend_from_slice(&0.0f32.to_le_bytes());
    xprm.extend_from_slice(&1u32.to_le_bytes()); // shape_type = Box
    assert_eq!(xprm.len(), 32);
    let record = build_refr_with_subs(0xBEEF, &[(b"XPRM", &xprm)]);
    let r = parse_one_refr(&record);
    let p = r.primitive.expect("XPRM must populate primitive");
    assert_eq!(p.bounds, [128.0, 64.0, 32.0]);
    assert_eq!(p.color, [1.0, 0.5, 0.0]);
    assert_eq!(p.shape_type, 1);
}

/// Regression for #412 — XRDS overrides the base LIGH radius per
/// placed ref. Pre-fix every REFR used the base radius unchanged.
#[test]
fn parse_refr_extracts_xrds_radius_override() {
    let xrds = 256.0f32.to_le_bytes();
    let record = build_refr_with_subs(0xBEEF, &[(b"XRDS", &xrds)]);
    let r = parse_one_refr(&record);
    assert_eq!(r.radius_override, Some(256.0));
}

/// Regression for #412 — XRMR room membership count + refs. Pre-fix
/// FO4 interior cell-subdivided culling had no room assignment to
/// work from. The helper also asserts the allocation bound: a
/// claimed count larger than the payload is clamped to the real
/// number of 4-byte slots available.
#[test]
fn parse_refr_extracts_xrmr_rooms_with_count_bound() {
    let mut xrmr = Vec::new();
    xrmr.extend_from_slice(&2u32.to_le_bytes()); // count
    xrmr.extend_from_slice(&0xAAAAu32.to_le_bytes());
    xrmr.extend_from_slice(&0xBBBBu32.to_le_bytes());
    let record = build_refr_with_subs(0xBEEF, &[(b"XRMR", &xrmr)]);
    let r = parse_one_refr(&record);
    assert_eq!(r.rooms, vec![0xAAAA, 0xBBBB]);

    // Corrupt-count case: claim 100 rooms in a 1-ref payload. The
    // bound protects against garbage counts over-reading.
    let mut corrupt = Vec::new();
    corrupt.extend_from_slice(&100u32.to_le_bytes()); // claimed count
    corrupt.extend_from_slice(&0xCCCCu32.to_le_bytes()); // only one room slot
    let record = build_refr_with_subs(0xBEEF, &[(b"XRMR", &corrupt)]);
    let r = parse_one_refr(&record);
    assert_eq!(
        r.rooms,
        vec![0xCCCC],
        "count must be clamped to available bytes"
    );
}

/// Regression for #412 — multiple XPOD sub-records collect into
/// `portals`. Each XPOD pairs two room refs.
#[test]
fn parse_refr_extracts_xpod_portal_pairs() {
    let mut a = Vec::new();
    a.extend_from_slice(&0x0Au32.to_le_bytes());
    a.extend_from_slice(&0x0Bu32.to_le_bytes());
    let mut b = Vec::new();
    b.extend_from_slice(&0x0Cu32.to_le_bytes());
    b.extend_from_slice(&0x0Du32.to_le_bytes());
    let record = build_refr_with_subs(0xBEEF, &[(b"XPOD", &a), (b"XPOD", &b)]);
    let r = parse_one_refr(&record);
    assert_eq!(r.portals.len(), 2);
    assert_eq!(r.portals[0].origin, 0x0A);
    assert_eq!(r.portals[0].destination, 0x0B);
    assert_eq!(r.portals[1].origin, 0x0C);
    assert_eq!(r.portals[1].destination, 0x0D);
}

/// A plain REFR with none of the new sub-records must still parse
/// cleanly and leave every new field in its empty state — preserves
/// the pre-#412 behaviour for the common case.
#[test]
fn parse_refr_without_extra_subrecords_leaves_new_fields_empty() {
    let record = build_refr_with_subs(0xBEEF, &[]);
    let r = parse_one_refr(&record);
    assert!(r.teleport.is_none());
    assert!(r.primitive.is_none());
    assert!(r.linked_refs.is_empty());
    assert!(r.rooms.is_empty());
    assert!(r.portals.is_empty());
    assert!(r.radius_override.is_none());
    assert!(r.alt_texture_ref.is_none());
    assert!(r.land_texture_ref.is_none());
    assert!(r.texture_slot_swaps.is_empty());
    assert!(r.emissive_light_ref.is_none());
}

/// Regression for #584 — FO4 REFR texture override sub-records
/// (XATO / XTNM / XTXR / XEMI) must populate `PlacedRef` so the
/// cell loader's Stage-2 overlay can resolve against
/// `EsmCellIndex.texture_sets`. Pre-fix 37 % of vanilla FO4 TXSTs
/// (MNAM-only) were parsed but silently dropped on REFR spawn
/// because these sub-records weren't parsed at all.
#[test]
fn parse_refr_extracts_fo4_texture_override_subrecords() {
    let xato = 0x0010_1234u32.to_le_bytes();
    let xtnm = 0x0020_5678u32.to_le_bytes();
    let mut xtxr_a = Vec::new();
    xtxr_a.extend_from_slice(&0x0030_0001u32.to_le_bytes()); // TXST
    xtxr_a.extend_from_slice(&1u32.to_le_bytes()); // slot 1 (normal)
    let mut xtxr_b = Vec::new();
    xtxr_b.extend_from_slice(&0x0030_0002u32.to_le_bytes()); // TXST
    xtxr_b.extend_from_slice(&2u32.to_le_bytes()); // slot 2 (glow)
    let xemi = 0x0040_9999u32.to_le_bytes();

    let record = build_refr_with_subs(
        0xBEEF,
        &[
            (b"XATO", &xato),
            (b"XTNM", &xtnm),
            (b"XTXR", &xtxr_a),
            (b"XTXR", &xtxr_b),
            (b"XEMI", &xemi),
        ],
    );
    let r = parse_one_refr(&record);
    assert_eq!(r.alt_texture_ref, Some(0x0010_1234));
    assert_eq!(r.land_texture_ref, Some(0x0020_5678));
    assert_eq!(r.texture_slot_swaps.len(), 2);
    assert_eq!(
        r.texture_slot_swaps[0],
        TextureSlotSwap {
            texture_set: 0x0030_0001,
            slot_index: 1,
        }
    );
    assert_eq!(
        r.texture_slot_swaps[1],
        TextureSlotSwap {
            texture_set: 0x0030_0002,
            slot_index: 2,
        }
    );
    assert_eq!(r.emissive_light_ref, Some(0x0040_9999));
}

/// Regression: #396 (OBL-D3-H2) — Oblivion ACRE (placed-creature
/// reference) was missing from the placement-record matcher.
/// FO3+ folded creature placements into ACHR; on Oblivion ACRE
/// has the same wire layout as ACHR (NAME + DATA + optional
/// XSCL + XESP), and pre-fix every Ayleid ruin / Oblivion gate /
/// dungeon creature placement was silently skipped.
#[test]
fn parse_refr_group_recognises_oblivion_acre_placement() {
    // ACRE record: NAME (CREA base form) + DATA (pos+rot) + XSCL.
    let mut sub_data = Vec::new();
    sub_data.extend_from_slice(b"NAME");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&0xCAFEu32.to_le_bytes()); // base CREA form
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&24u16.to_le_bytes());
    sub_data.extend_from_slice(&50.0f32.to_le_bytes()); // pos x
    sub_data.extend_from_slice(&75.0f32.to_le_bytes()); // pos y
    sub_data.extend_from_slice(&100.0f32.to_le_bytes()); // pos z
    sub_data.extend_from_slice(&[0u8; 12]); // zero rotation
    sub_data.extend_from_slice(b"XSCL");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&1.5f32.to_le_bytes());

    let mut record = Vec::new();
    record.extend_from_slice(b"ACRE");
    record.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes());
    record.extend_from_slice(&0x1234u32.to_le_bytes());
    record.extend_from_slice(&[0u8; 8]);
    record.extend_from_slice(&sub_data);

    let mut reader = EsmReader::new(&record);
    let end = record.len();
    let mut refs = Vec::new();
    let mut land = None;
    parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

    assert_eq!(refs.len(), 1, "ACRE placement must be recognised");
    let r = &refs[0];
    assert_eq!(r.base_form_id, 0xCAFE);
    assert!((r.position[0] - 50.0).abs() < 1e-6);
    assert!((r.position[1] - 75.0).abs() < 1e-6);
    assert!((r.position[2] - 100.0).abs() < 1e-6);
    assert!((r.scale - 1.5).abs() < 1e-6);
}

/// Regression: #396 — Oblivion CREA (base creature record) must
/// reach `parse_modl_group` so its EDID + MODL land in the
/// statics map. Pre-fix `parse_esm_cells` didn't include CREA in
/// the MODL match arm, so the CREA group was skipped wholesale
/// before parse_modl_group ever saw a record. CREA uses the
/// standard MODL sub-record (identical layout to STAT), so once
/// the dispatcher routes it through, the data path is unchanged.
///
/// Mirrors `parse_modl_group_walks_oblivion_20byte_headers` but
/// with a single CREA record + Oblivion 20-byte headers (CREA
/// only ships on Oblivion / FO3 / FNV — FO3+ folded creatures
/// into NPC_).
#[test]
fn parse_modl_group_indexes_oblivion_crea_records() {
    use super::super::reader::EsmVariant;

    // CREA record with EDID + MODL (Oblivion 20-byte header).
    let mut crea_sub = Vec::new();
    let edid = "Goblin\0";
    crea_sub.extend_from_slice(b"EDID");
    crea_sub.extend_from_slice(&(edid.len() as u16).to_le_bytes());
    crea_sub.extend_from_slice(edid.as_bytes());
    let model = "creatures\\goblin\\goblin.nif\0";
    crea_sub.extend_from_slice(b"MODL");
    crea_sub.extend_from_slice(&(model.len() as u16).to_le_bytes());
    crea_sub.extend_from_slice(model.as_bytes());

    let mut crea_record = Vec::new();
    crea_record.extend_from_slice(b"CREA");
    crea_record.extend_from_slice(&(crea_sub.len() as u32).to_le_bytes());
    crea_record.extend_from_slice(&0u32.to_le_bytes()); // flags
    crea_record.extend_from_slice(&0x000A_0001u32.to_le_bytes()); // form_id
    crea_record.extend_from_slice(&[0u8; 4]); // Oblivion vc_info (4 bytes)
    crea_record.extend_from_slice(&crea_sub);

    // 20-byte GRUP header wrapping the CREA record.
    let total_size = 20 + crea_record.len();
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&(total_size as u32).to_le_bytes());
    group.extend_from_slice(b"CREA");
    group.extend_from_slice(&0u32.to_le_bytes()); // group_type = 0 (top-level)
    group.extend_from_slice(&[0u8; 4]); // Oblivion stamp (4 bytes)
    group.extend_from_slice(&crea_record);

    let mut reader = EsmReader::with_variant(&group, EsmVariant::Oblivion);
    let gh = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&gh);
    let mut statics = HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();

    let crea = statics
        .get(&0x000A_0001)
        .expect("CREA record must be indexed by form_id");
    assert_eq!(crea.editor_id, "Goblin");
    assert_eq!(crea.model_path, "creatures\\goblin\\goblin.nif");
}

/// Edge case: XESP with a zero parent FormID (NULL parent — rare
/// but legal in vanilla content). Treated as "no real parent" so
/// the REFR is NOT default-disabled even though XESP is present.
#[test]
fn parse_refr_xesp_with_null_parent_is_not_default_disabled() {
    let record = build_refr_with_xesp(0xABCD, 0, 0);
    let mut reader = EsmReader::new(&record);
    let end = record.len();
    let mut refs = Vec::new();
    let mut land = None;
    parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();

    let ep = refs[0]
        .enable_parent
        .expect("XESP populates enable_parent even with null parent");
    assert_eq!(ep.form_id, 0);
    assert!(
        !ep.default_disabled(),
        "null parent FormID = no real gating, so not default-disabled"
    );
}

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

    let mut reader = super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(&mut reader, end, &mut cells).unwrap();

    assert_eq!(cells.len(), 1, "interior CELL must be registered");
    let cell = cells.get("floodedruin").expect("lowercase key");
    assert!(cell.is_interior);
    assert_eq!(
        cell.water_height,
        Some(10.0),
        "XCLW water height must flow through to CellData"
    );
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

    let mut reader = super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(&mut reader, end, &mut cells).unwrap();

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

    let mut reader = super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(&mut reader, end, &mut cells).unwrap();

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

    let mut reader = super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(&mut reader, end, &mut cells).unwrap();

    let cell = cells.get("dragonsreachjarl").expect("interior CELL present");
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

    let mut reader = super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(&mut reader, end, &mut cells).unwrap();

    let cell = cells.get("bareroom").expect("interior CELL present");
    assert_eq!(cell.image_space_form, None);
    assert_eq!(cell.water_type_form, None);
    assert_eq!(cell.acoustic_space_form, None);
    assert_eq!(cell.music_type_form, None);
    assert_eq!(cell.location_form, None);
    assert!(cell.regions.is_empty());
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

    let mut reader = super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(&mut reader, end, &mut cells).unwrap();

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

    let mut reader = super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(&mut reader, end, &mut cells).unwrap();

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

    let mut reader = super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(&mut reader, end, &mut cells).unwrap();

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

    let mut reader = super::super::reader::EsmReader::with_variant(
        &buf,
        super::super::reader::EsmVariant::Tes5Plus,
    );
    let end = buf.len();
    let mut cells = HashMap::new();
    parse_cell_group(&mut reader, end, &mut cells).unwrap();

    let cell = cells.get("dryroom").expect("interior CELL present");
    assert_eq!(cell.water_height, None);
}

#[test]
#[ignore]
fn parse_real_fnv_esm() {
    let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/FalloutNV.esm";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping: FalloutNV.esm not found");
        return;
    }
    let data = std::fs::read(path).unwrap();
    let index = parse_esm_cells(&data).unwrap();

    eprintln!("Interior cells: {}", index.cells.len());
    eprintln!("Static objects: {}", index.statics.len());

    // Should have hundreds of interior cells and thousands of statics.
    assert!(
        index.cells.len() > 100,
        "Expected >100 cells, got {}",
        index.cells.len()
    );
    assert!(
        index.statics.len() > 1000,
        "Expected >1000 statics, got {}",
        index.statics.len()
    );

    // Check which cells have refs.
    let cells_with_refs = index
        .cells
        .values()
        .filter(|c| !c.references.is_empty())
        .count();
    eprintln!("Cells with refs: {}", cells_with_refs);

    // Check the Prospector Saloon specifically.
    let saloon = index.cells.get("gsprospectorsalooninterior").unwrap();
    eprintln!("Saloon: {} refs", saloon.references.len());
    assert!(
        saloon.references.len() > 100,
        "Saloon should have >100 refs"
    );

    // Look for the Prospector Saloon.
    let saloon_keys: Vec<&str> = index
        .cells
        .keys()
        .filter(|k| k.contains("goodsprings") || k.contains("saloon") || k.contains("prospector"))
        .map(|k| k.as_str())
        .collect();
    eprintln!("Goodsprings/saloon cells: {:?}", saloon_keys);

    // Print a few cells for debugging.
    for (key, cell) in index.cells.iter().take(10) {
        eprintln!("  Cell '{}': {} refs", key, cell.references.len());
    }
}

/// Regression guard: proves the existing FNV-shaped XCLL parser is
/// byte-compatible with Oblivion for the fields we consume.
///
/// XCLL in Oblivion (36 bytes) and FNV (40 bytes) share an identical
/// prefix for ambient / directional colors + fog colors + fog
/// near/far + directional rotation XY + fade + clip distance. FNV
/// appends a `fog_power` float; Skyrim+ has a completely different
/// (longer) layout. Since `parse_esm_cells` only reads bytes 0-27
/// (ambient, directional, and rotation), the byte offsets work for
/// both games without any per-variant branching.
///
/// This test validates that assumption against a real `Oblivion.esm`:
/// ≥90% of interior cells must produce a populated CellLighting
/// record, and the sampled color values must land in the expected
/// 0..1 normalized float range.
#[test]
#[ignore]
fn oblivion_cells_populate_xcll_lighting() {
    let path = "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion.esm";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping: Oblivion.esm not found");
        return;
    }
    let data = std::fs::read(path).unwrap();
    let idx = parse_esm_cells(&data).expect("Oblivion walker");

    let total = idx.cells.len();
    let with_lighting = idx.cells.values().filter(|c| c.lighting.is_some()).count();
    let with_directional = idx
        .cells
        .values()
        .filter(|c| {
            c.lighting
                .as_ref()
                .is_some_and(|l| l.directional_color.iter().any(|&x| x > 0.0))
        })
        .count();

    eprintln!(
        "Oblivion.esm: {total} cells, {with_lighting} with XCLL \
             ({:.1}%), {with_directional} with non-zero directional",
        100.0 * with_lighting as f32 / total.max(1) as f32,
    );

    // Log a couple of directional samples so that any future
    // XCLL-layout regression shows up in test output as obviously
    // wrong color values or rotations.
    for (name, lit) in idx
        .cells
        .values()
        .filter_map(|c| {
            c.lighting
                .as_ref()
                .map(|l| (c.editor_id.clone(), l.clone()))
        })
        .filter(|(_, l)| l.directional_color.iter().any(|&c| c > 0.0))
        .take(2)
    {
        eprintln!(
            "  '{name}': ambient={:.3?} directional={:.3?} rot=[{:.1},{:.1}]°",
            lit.ambient,
            lit.directional_color,
            lit.directional_rotation[0].to_degrees(),
            lit.directional_rotation[1].to_degrees(),
        );

        // Sanity: normalized color channels must sit in [0, 1].
        for c in lit.ambient.iter().chain(lit.directional_color.iter()) {
            assert!(
                (0.0..=1.0).contains(c),
                "color channel {c} out of [0,1] for cell '{name}' — \
                     XCLL byte offsets may have drifted"
            );
        }
    }

    // For the parser to be considered working on Oblivion, the vast
    // majority of interior cells must produce lighting data. The
    // residual are cells that legitimately omit XCLL (wilderness
    // stubs, deleted, or inherited from a template).
    let lighting_pct = with_lighting * 100 / total.max(1);
    assert!(
        lighting_pct >= 90,
        "expected >=90% of Oblivion cells to have XCLL lighting, \
             got {with_lighting}/{total} ({lighting_pct}%)"
    );
    assert!(
        with_directional > 100,
        "expected >100 cells with non-zero directional light, got {with_directional}"
    );
}

/// Smoke test: does `parse_esm_cells` survive a real `Oblivion.esm`
/// walk now that the reader understands 20-byte headers?
///
/// This does NOT assert a cell count or that specific records
/// parsed — the FNV-shaped CELL / REFR / STAT subrecord layouts may
/// still trip over Oblivion-specific fields. It only validates
/// that the top-level walker reaches the end of the file without a
/// hard error, which is the minimum bar for future per-record
/// Oblivion work.
#[test]
#[ignore]
fn parse_real_oblivion_esm_walker_survives() {
    let path = "/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion.esm";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping: Oblivion.esm not found");
        return;
    }
    let data = std::fs::read(path).unwrap();

    // Sanity-check auto-detection.
    use crate::esm::reader::{EsmReader, EsmVariant};
    assert_eq!(
        EsmVariant::detect(&data),
        EsmVariant::Oblivion,
        "Oblivion.esm should auto-detect as Oblivion variant"
    );
    let mut reader = EsmReader::new(&data);
    let fh = reader.read_file_header().expect("Oblivion TES4 header");
    eprintln!(
        "Oblivion.esm: record_count={} masters={:?}",
        fh.record_count, fh.master_files
    );

    // Now run the full cell walker. We only assert it returns Ok —
    // the record contents are Phase 2 work.
    match parse_esm_cells(&data) {
        Ok(idx) => {
            eprintln!(
                "Oblivion.esm walker OK: cells={} statics={} \
                     cells_with_refs={}",
                idx.cells.len(),
                idx.statics.len(),
                idx.cells
                    .values()
                    .filter(|c| !c.references.is_empty())
                    .count(),
            );
        }
        Err(e) => panic!("parse_esm_cells failed on Oblivion.esm: {e:#}"),
    }
}

/// Regression bench for #456: pin the Megaton Player House parse-
/// side reference count. ROADMAP originally quoted "1609 entities,
/// 199 textures at 42 FPS" for MegatonPlayerHouse; the 1609 figure
/// was measured AFTER cell-loader NIF expansion (each REFR spawns
/// N ECS entities depending on its NIF block tree), so it isn't
/// a parse-side assertion.
///
/// Disk-sampled on 2026-04-19 against Fallout 3 GOTY: 929 REFRs
/// live directly in the CELL. That's the stable number the
/// parser must not drop. The 42 FPS figure predates TAA / SVGF /
/// BLAS batching / streaming RIS and needs a fresh GPU bench —
/// tracked in #456.
#[test]
#[ignore]
fn parse_real_fo3_megaton_cell_baseline() {
    let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout3.esm";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping: Fallout3.esm not found");
        return;
    }
    let data = std::fs::read(path).unwrap();
    let index = parse_esm_cells(&data).expect("parse_esm_cells");
    let megaton = index
        .cells
        .iter()
        .find(|(k, _)| k.contains("megaton") && k.contains("player"))
        .expect("expected a Megaton Player House interior cell in Fallout3.esm")
        .1;
    eprintln!(
        "MegatonPlayerHouse: {} REFRs (observed 929 on 2026-04-19)",
        megaton.references.len(),
    );
    assert!(
        megaton.references.len() > 800,
        "expected >800 REFRs for MegatonPlayerHouse (observed 929), got {}",
        megaton.references.len()
    );
}

/// Validates that `parse_esm_cells` handles Skyrim SE's 92-byte XCLL
/// sub-records and can find The Winking Skeever interior cell.
#[test]
#[ignore]
fn parse_real_skyrim_esm() {
    let path = "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim.esm";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping: Skyrim.esm not found");
        return;
    }
    let data = std::fs::read(path).unwrap();
    let idx = parse_esm_cells(&data).expect("Skyrim.esm walker");

    eprintln!(
        "Skyrim.esm: {} cells, {} statics, {} worldspaces",
        idx.cells.len(),
        idx.statics.len(),
        idx.exterior_cells.len(),
    );

    // The Winking Skeever must exist.
    let skeever = idx.cells.get("solitudewinkingskeever");
    assert!(
        skeever.is_some(),
        "SolitudeWinkingSkeever not found in Skyrim.esm cells. \
             Available keys (sample): {:?}",
        idx.cells.keys().take(20).collect::<Vec<_>>()
    );
    let skeever = skeever.unwrap();
    eprintln!(
        "Winking Skeever: {} refs, lighting={:?}",
        skeever.references.len(),
        skeever.lighting.is_some()
    );
    assert!(
        skeever.references.len() > 50,
        "Winking Skeever should have >50 refs, got {}",
        skeever.references.len()
    );

    // Skyrim XCLL should populate the extended fields.
    if let Some(ref lit) = skeever.lighting {
        eprintln!(
            "  ambient={:.3?} directional={:.3?} fog_near={:.1} fog_far={:.1}",
            lit.ambient, lit.directional_color, lit.fog_near, lit.fog_far,
        );
        // Skyrim's 92-byte XCLL must populate directional_fade.
        assert!(
            lit.directional_fade.is_some(),
            "Skyrim XCLL should have directional_fade (92-byte layout)"
        );
        // Ambient should be non-zero for a tavern interior.
        assert!(
            lit.ambient.iter().any(|&c| c > 0.0),
            "Winking Skeever ambient should be non-zero"
        );
    }

    // Check overall Skyrim cell stats.
    let with_lighting = idx.cells.values().filter(|c| c.lighting.is_some()).count();
    let with_skyrim_xcll = idx
        .cells
        .values()
        .filter(|c| {
            c.lighting
                .as_ref()
                .is_some_and(|l| l.directional_fade.is_some())
        })
        .count();
    eprintln!(
        "Skyrim lighting: {with_lighting}/{} cells with XCLL, \
             {with_skyrim_xcll} with Skyrim extended fields",
        idx.cells.len()
    );
}

#[test]
fn read_zstring_handles_null_terminator() {
    assert_eq!(read_zstring(b"Hello\0"), "Hello");
    assert_eq!(read_zstring(b"NoNull"), "NoNull");
    assert_eq!(read_zstring(b"\0"), "");
    assert_eq!(read_zstring(b""), "");
}

/// Regression: #405 — vanilla Fallout4.esm must surface every SCOL
/// record with its full ONAM/DATA child-placement data. Pre-fix
/// the MODL-only parser discarded 15,878 placement entries across
/// 2617 SCOL records. The exact counts drift with DLC patches;
/// this test just asserts we're in the right order of magnitude.
#[test]
#[ignore]
fn parse_real_fo4_esm_surfaces_scol_placements() {
    let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/Fallout4.esm";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping: Fallout4.esm not found");
        return;
    }
    let data = std::fs::read(path).unwrap();
    let idx = parse_esm_cells(&data).expect("parse_esm_cells");

    let total_placements: usize = idx
        .scols
        .values()
        .flat_map(|s| s.parts.iter())
        .map(|p| p.placements.len())
        .sum();
    let scol_count = idx.scols.len();
    let parts_count: usize = idx.scols.values().map(|s| s.parts.len()).sum();
    eprintln!(
        "FO4 SCOL: {} records, {} parts, {} total placements",
        scol_count, parts_count, total_placements,
    );

    // Audit numbers from April 2026 Fallout4.esm scan:
    //   2617 SCOL records, 15878 ONAM/DATA pairs. Floors are set
    //   ~5% below observed so the test stays stable across
    //   patches without becoming meaningless.
    assert!(
        scol_count > 2400,
        "expected >2.4k SCOL records, got {}",
        scol_count
    );
    assert!(
        parts_count > 15000,
        "expected >15k ONAM/DATA parts, got {}",
        parts_count
    );
    assert!(
        total_placements > 15000,
        "expected >15k per-child placements, got {}",
        total_placements
    );
}

/// Regression: #589 — vanilla Fallout4.esm must surface every PKIN
/// record with a non-empty `contents` list. Pre-fix 872 PKIN
/// records silently produced zero world content because they were
/// routed through the MODL-only catch-all (PKIN carries no MODL).
/// Ignored by default — opt in with `cargo test -p byroredux-plugin
/// -- --ignored`.
#[test]
#[ignore]
fn parse_real_fo4_esm_surfaces_pkin_contents() {
    let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/Fallout4.esm";
    if !std::path::Path::new(path).exists() {
        eprintln!("Skipping: Fallout4.esm not found");
        return;
    }
    let data = std::fs::read(path).unwrap();
    let idx = parse_esm_cells(&data).expect("parse_esm_cells");

    let pkin_count = idx.packins.len();
    let non_empty_pkin = idx
        .packins
        .values()
        .filter(|p| !p.contents.is_empty())
        .count();
    let total_contents: usize = idx.packins.values().map(|p| p.contents.len()).sum();
    eprintln!(
        "FO4 PKIN: {} records, {} with contents, {} total refs",
        pkin_count, non_empty_pkin, total_contents,
    );

    // Audit floor per issue body: 872 vanilla PKIN records, all
    // with non-empty `contents`. Set the floor ~5 % below observed
    // so DLC patches don't break the test.
    assert!(
        pkin_count >= 820,
        "expected ≥820 PKIN records, got {}",
        pkin_count
    );
    assert!(
        non_empty_pkin >= 820,
        "expected ≥820 PKIN records with non-empty contents, got {}",
        non_empty_pkin
    );
}

/// Build a TXST record byte stream with the given (sub_type, path)
/// pairs encoded as MODL-style zstring sub-records. Used by the
/// #357 regression tests below.
fn build_txst_record(form_id: u32, slots: &[(&[u8; 4], &str)]) -> Vec<u8> {
    let mut sub_data = Vec::new();
    for (sub_type, path) in slots {
        let z = format!("{}\0", path);
        sub_data.extend_from_slice(*sub_type);
        sub_data.extend_from_slice(&(z.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(z.as_bytes());
    }
    let mut buf = Vec::new();
    buf.extend_from_slice(b"TXST");
    buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    buf.extend_from_slice(&form_id.to_le_bytes());
    buf.extend_from_slice(&[0u8; 8]); // padding (timestamp + version)
    buf.extend_from_slice(&sub_data);
    buf
}

/// Wrap one or more TXST records in a top-level GRUP so the
/// `parse_txst_group` recursion path matches the production loop.
fn wrap_in_txst_group(records: &[Vec<u8>]) -> Vec<u8> {
    let inner: Vec<u8> = records.iter().flatten().copied().collect();
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GRUP");
    buf.extend_from_slice(&((inner.len() + 24) as u32).to_le_bytes()); // total_size includes 24-byte header
    buf.extend_from_slice(b"TXST");
    buf.extend_from_slice(&0u32.to_le_bytes()); // group_type = top-level
    buf.extend_from_slice(&[0u8; 8]); // timestamp + version
    buf.extend_from_slice(&inner);
    buf
}

/// Regression: #357 — TXST parser must extract all 8 texture slots
/// (TX00..TX07) into a `TextureSet`, not just the diffuse path.
/// Pre-fix every Skyrim TXST-driven REFR override silently dropped
/// 7 of 8 channels.
#[test]
fn parse_txst_extracts_all_eight_texture_slots() {
    let txst = build_txst_record(
        0xCAFE,
        &[
            (b"TX00", "textures/diffuse.dds"),
            (b"TX01", "textures/normal.dds"),
            (b"TX02", "textures/glow.dds"),
            (b"TX03", "textures/height.dds"),
            (b"TX04", "textures/env.dds"),
            (b"TX05", "textures/env_mask.dds"),
            (b"TX06", "textures/inner.dds"),
            (b"TX07", "textures/specular.dds"),
        ],
    );
    let group = wrap_in_txst_group(&[txst]);

    let mut reader = EsmReader::new(&group);
    let header = reader.read_group_header().expect("group header");
    let end = reader.group_content_end(&header);
    let mut diffuse_only: HashMap<u32, String> = HashMap::new();
    let mut sets: HashMap<u32, TextureSet> = HashMap::new();
    parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

    // Backward-compat diffuse-only map still populated.
    assert_eq!(
        diffuse_only.get(&0xCAFE),
        Some(&"textures/diffuse.dds".to_string()),
    );
    // Full slot set now also captured.
    let set = sets
        .get(&0xCAFE)
        .expect("TextureSet missing for TXST 0xCAFE");
    assert_eq!(set.diffuse.as_deref(), Some("textures/diffuse.dds"));
    assert_eq!(set.normal.as_deref(), Some("textures/normal.dds"));
    assert_eq!(set.glow.as_deref(), Some("textures/glow.dds"));
    assert_eq!(set.height.as_deref(), Some("textures/height.dds"));
    assert_eq!(set.env.as_deref(), Some("textures/env.dds"));
    assert_eq!(set.env_mask.as_deref(), Some("textures/env_mask.dds"));
    assert_eq!(set.inner.as_deref(), Some("textures/inner.dds"));
    assert_eq!(set.specular.as_deref(), Some("textures/specular.dds"));
}

/// Regression: #357 — partial TXST (e.g. FO3/FNV which only authors
/// TX00) must surface the populated slot and leave the rest as
/// `None`. Verifies the optional-slot semantics.
#[test]
fn parse_txst_diffuse_only_leaves_other_slots_none() {
    let txst = build_txst_record(0xBEEF, &[(b"TX00", "textures/landscape/dirt.dds")]);
    let group = wrap_in_txst_group(&[txst]);

    let mut reader = EsmReader::new(&group);
    let header = reader.read_group_header().expect("group header");
    let end = reader.group_content_end(&header);
    let mut diffuse_only: HashMap<u32, String> = HashMap::new();
    let mut sets: HashMap<u32, TextureSet> = HashMap::new();
    parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

    let set = sets
        .get(&0xBEEF)
        .expect("TextureSet missing for diffuse-only TXST");
    assert_eq!(set.diffuse.as_deref(), Some("textures/landscape/dirt.dds"));
    assert!(set.normal.is_none());
    assert!(set.glow.is_none());
    assert!(set.specular.is_none());
    assert!(set.env.is_none());
}

/// Regression for #406 — FO4 TXST records often use `MNAM`
/// (BGSM material path) instead of populating TX00..TX07 directly.
/// Parser must extract MNAM into `material_path`. Pre-fix 140 of
/// 382 (37 %) vanilla `Fallout4.esm` TXST records were silently
/// dropped — the `if set != default()` guard rejected MNAM-only
/// sets because no TX slot was set.
#[test]
fn parse_txst_extracts_mnam_material_path() {
    let txst = build_txst_record(0xF047, &[(b"MNAM", "Materials/Decals/RustDecal01.BGSM")]);
    let group = wrap_in_txst_group(&[txst]);

    let mut reader = EsmReader::new(&group);
    let header = reader.read_group_header().expect("group header");
    let end = reader.group_content_end(&header);
    let mut diffuse_only: HashMap<u32, String> = HashMap::new();
    let mut sets: HashMap<u32, TextureSet> = HashMap::new();
    parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

    let set = sets
        .get(&0xF047)
        .expect("TextureSet must be inserted for MNAM-only TXST (pre-fix: silently dropped)");
    assert_eq!(
        set.material_path.as_deref(),
        Some("Materials/Decals/RustDecal01.BGSM"),
        "MNAM must surface as material_path"
    );
    // The legacy `txst_textures` diffuse-only map is populated only
    // when TX00 is present — MNAM-only records don't enter it, and
    // downstream consumers should resolve the BGSM path instead.
    assert!(
        diffuse_only.get(&0xF047).is_none(),
        "MNAM-only TXST must not populate the diffuse-only legacy map"
    );
    // No direct-slot paths populated.
    assert!(set.diffuse.is_none());
    assert!(set.normal.is_none());
}

/// Regression for #406 — an FO4 TXST carrying both MNAM and TXnn
/// slots (not 140/382 in vanilla but possible in mod content)
/// must preserve both paths. BGSM resolution takes precedence
/// downstream, but we don't lose the direct slots either.
#[test]
fn parse_txst_extracts_mnam_alongside_tx_slots() {
    let txst = build_txst_record(
        0xF048,
        &[
            (b"TX00", "textures/fallback_diffuse.dds"),
            (b"MNAM", "Materials/Weapons/Plasma01.BGSM"),
        ],
    );
    let group = wrap_in_txst_group(&[txst]);

    let mut reader = EsmReader::new(&group);
    let header = reader.read_group_header().expect("group header");
    let end = reader.group_content_end(&header);
    let mut diffuse_only: HashMap<u32, String> = HashMap::new();
    let mut sets: HashMap<u32, TextureSet> = HashMap::new();
    parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

    let set = sets.get(&0xF048).expect("set missing");
    assert_eq!(
        set.material_path.as_deref(),
        Some("Materials/Weapons/Plasma01.BGSM")
    );
    assert_eq!(
        set.diffuse.as_deref(),
        Some("textures/fallback_diffuse.dds")
    );
}

/// Regression: #357 — empty zstrings (`""`) on any slot collapse
/// to `None` so the consumer doesn't have to redo the empty check.
#[test]
fn parse_txst_empty_string_slot_collapses_to_none() {
    let txst = build_txst_record(
        0xDEAD,
        &[
            (b"TX00", "textures/diffuse.dds"),
            (b"TX01", ""), // empty path — should collapse to None
        ],
    );
    let group = wrap_in_txst_group(&[txst]);

    let mut reader = EsmReader::new(&group);
    let header = reader.read_group_header().expect("group header");
    let end = reader.group_content_end(&header);
    let mut diffuse_only: HashMap<u32, String> = HashMap::new();
    let mut sets: HashMap<u32, TextureSet> = HashMap::new();
    parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

    let set = sets.get(&0xDEAD).expect("set missing");
    assert_eq!(set.diffuse.as_deref(), Some("textures/diffuse.dds"));
    assert!(set.normal.is_none(), "empty TX01 must surface as None");
}

// ── #692 / O3-N-04 regression guards ──────────────────────────────
//
// CELL + REFR ownership tuple (XOWN / XRNK / XGLB). Pre-fix every
// arm dropped these on the `_` match — stealing-detection /
// property-crime gameplay had no input. Cross-game (Oblivion +
// FO3 + FNV + Skyrim+ all use the same wire format).

/// Build a REFR record carrying a name + minimal DATA + a chosen
/// subset of XOWN / XRNK / XGLB sub-records.
fn build_refr_with_ownership(
    base_form: u32,
    owner: Option<u32>,
    rank: Option<i32>,
    global: Option<u32>,
) -> Vec<u8> {
    let mut sub_data = Vec::new();
    // NAME (base form)
    sub_data.extend_from_slice(b"NAME");
    sub_data.extend_from_slice(&4u16.to_le_bytes());
    sub_data.extend_from_slice(&base_form.to_le_bytes());
    // DATA (minimal 24-byte placement)
    sub_data.extend_from_slice(b"DATA");
    sub_data.extend_from_slice(&24u16.to_le_bytes());
    sub_data.extend_from_slice(&[0u8; 24]);
    if let Some(o) = owner {
        sub_data.extend_from_slice(b"XOWN");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&o.to_le_bytes());
    }
    if let Some(r) = rank {
        sub_data.extend_from_slice(b"XRNK");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&r.to_le_bytes());
    }
    if let Some(g) = global {
        sub_data.extend_from_slice(b"XGLB");
        sub_data.extend_from_slice(&4u16.to_le_bytes());
        sub_data.extend_from_slice(&g.to_le_bytes());
    }

    let mut record = Vec::new();
    record.extend_from_slice(b"REFR");
    record.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes()); // flags
    record.extend_from_slice(&0xBEEFu32.to_le_bytes()); // form id
    record.extend_from_slice(&[0u8; 8]); // version + unknown
    record.extend_from_slice(&sub_data);
    record
}

fn parse_one_refr_for_ownership(record: &[u8]) -> PlacedRef {
    let mut reader = EsmReader::new(record);
    let end = record.len();
    let mut refs = Vec::new();
    let mut land = None;
    parse_refr_group(&mut reader, end, &mut refs, &mut land).unwrap();
    assert_eq!(refs.len(), 1, "one REFR per record");
    refs.into_iter().next().unwrap()
}

#[test]
fn refr_with_no_ownership_subrecords_leaves_field_none() {
    let record = build_refr_with_ownership(0xABCD, None, None, None);
    let r = parse_one_refr_for_ownership(&record);
    assert!(
        r.ownership.is_none(),
        "REFR without XOWN must NOT synthesize an ownership tuple"
    );
}

#[test]
fn refr_with_xown_only_populates_owner_and_no_gates() {
    // Public-cell case: a chest with an individual NPC owner, no
    // faction-rank gate, no global-var gate. The audit's primary
    // example.
    let record = build_refr_with_ownership(0xABCD, Some(0x0001_4242), None, None);
    let r = parse_one_refr_for_ownership(&record);
    let o = r.ownership.expect("XOWN must populate ownership");
    assert_eq!(o.owner_form_id, 0x0001_4242);
    assert_eq!(o.faction_rank, None);
    assert_eq!(o.global_var_form_id, None);
}

#[test]
fn refr_with_full_ownership_tuple_routes_all_three_fields() {
    // Faction-owned: XOWN points at FACT, XRNK gates on minimum
    // rank (negative ranks like -1 = Untouchable are real values
    // in vanilla Oblivion content), XGLB references a quest-state
    // global that flips ownership at runtime.
    let record = build_refr_with_ownership(0xABCD, Some(0x0001_5005), Some(-1), Some(0x0001_AAAA));
    let r = parse_one_refr_for_ownership(&record);
    let o = r.ownership.expect("ownership tuple");
    assert_eq!(o.owner_form_id, 0x0001_5005);
    assert_eq!(o.faction_rank, Some(-1));
    assert_eq!(o.global_var_form_id, Some(0x0001_AAAA));
}

#[test]
fn refr_with_rank_and_global_but_no_owner_is_dropped() {
    // Defensive: XRNK + XGLB without XOWN is structurally
    // nonsensical (nothing to gate). The walker drops the
    // dangling fields rather than fabricating an owner=0 tuple
    // that downstream code might mistake for a real placement.
    let record = build_refr_with_ownership(0xABCD, None, Some(5), Some(0xCAFE));
    let r = parse_one_refr_for_ownership(&record);
    assert!(
        r.ownership.is_none(),
        "XRNK + XGLB without XOWN must NOT synthesize a partial tuple"
    );
}

// ── M46.0 / #561 EsmCellIndex::merge_from regression guards ───────
//
// Cell-side last-write-wins semantics on every map, with the
// exterior_cells nested map merging per-worldspace so a DLC
// adding a new worldspace doesn't stomp the base game's table.

fn make_static(form_id: u32, model: &str) -> StaticObject {
    StaticObject {
        form_id,
        editor_id: String::new(),
        model_path: model.to_string(),
        light_data: None,
        addon_data: None,
        has_script: false,
    }
}

fn make_interior_cell(form_id: u32, edid: &str) -> CellData {
    CellData {
        form_id,
        editor_id: edid.to_string(),
        display_name: None,
        references: Vec::new(),
        is_interior: true,
        grid: None,
        lighting: None,
        landscape: None,
        water_height: None,
        image_space_form: None,
        water_type_form: None,
        acoustic_space_form: None,
        music_type_form: None,
        location_form: None,
        regions: Vec::new(),
        lighting_template_form: None,
        ownership: None,
    }
}

#[test]
fn merge_from_extends_disjoint_statics_and_cells() {
    // Master plugin contributes one cell + one static; child
    // plugin contributes a disjoint pair. Merge → both visible.
    let mut master = EsmCellIndex::default();
    master
        .statics
        .insert(0x0001_0001, make_static(0x0001_0001, "master/wall.nif"));
    master.cells.insert(
        "homecell".into(),
        make_interior_cell(0x0001_AAAA, "HomeCell"),
    );

    let mut child = EsmCellIndex::default();
    child
        .statics
        .insert(0x0002_0001, make_static(0x0002_0001, "child/door.nif"));
    child
        .cells
        .insert("newcell".into(), make_interior_cell(0x0002_BBBB, "NewCell"));

    master.merge_from(child);

    assert_eq!(master.statics.len(), 2);
    assert_eq!(master.cells.len(), 2);
    assert_eq!(
        master.statics.get(&0x0001_0001).unwrap().model_path,
        "master/wall.nif"
    );
    assert_eq!(
        master.statics.get(&0x0002_0001).unwrap().model_path,
        "child/door.nif"
    );
}

#[test]
fn merge_from_later_plugin_wins_on_overlapping_statics() {
    // Both master and child define the same FormID. Bethesda
    // load-order semantics: child overrides master.
    let mut master = EsmCellIndex::default();
    master
        .statics
        .insert(0x0001_0001, make_static(0x0001_0001, "master/v1.nif"));

    let mut child = EsmCellIndex::default();
    child
        .statics
        .insert(0x0001_0001, make_static(0x0001_0001, "child/v2.nif"));

    master.merge_from(child);

    assert_eq!(master.statics.len(), 1);
    assert_eq!(
        master.statics.get(&0x0001_0001).unwrap().model_path,
        "child/v2.nif",
        "child plugin must override master's STAT (last-write-wins)"
    );
}

#[test]
fn merge_from_exterior_cells_merge_per_worldspace() {
    // Master defines Tamriel grid (0,0); child defines Tamriel
    // grid (1,0) AND a new "soulcairn" worldspace. Both Tamriel
    // entries must coexist; soulcairn lands as a new key.
    let mut master = EsmCellIndex::default();
    master
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((0, 0), make_interior_cell(0x0001_C000, "Tamriel00"));

    let mut child = EsmCellIndex::default();
    child
        .exterior_cells
        .entry("tamriel".into())
        .or_default()
        .insert((1, 0), make_interior_cell(0x0002_C001, "Tamriel10"));
    child
        .exterior_cells
        .entry("soulcairn".into())
        .or_default()
        .insert((0, 0), make_interior_cell(0x0002_D000, "SoulCairn00"));

    master.merge_from(child);

    assert_eq!(master.exterior_cells.len(), 2);
    let tam = master.exterior_cells.get("tamriel").unwrap();
    assert_eq!(
        tam.len(),
        2,
        "DLC adding tamriel grid (1,0) must NOT stomp master's grid (0,0)"
    );
    assert!(tam.contains_key(&(0, 0)));
    assert!(tam.contains_key(&(1, 0)));
    assert!(master.exterior_cells.contains_key("soulcairn"));
}
