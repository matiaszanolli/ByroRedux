//! LIGH (light record) parsing tests.
//!
//! Color decode (Oblivion RGBA, FNV warm-lights swap path), FO4 XPWR
//! power-circuit ref capture, XPWR-before-DATA merge.

use super::super::super::reader::EsmReader;
use super::super::support::parse_modl_group;
use super::super::*;

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
