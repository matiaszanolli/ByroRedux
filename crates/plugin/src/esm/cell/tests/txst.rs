//! TXST (texture set) parsing tests.
//!
//! All 8 texture slots, partial slot fills, MNAM material path, DODT decal
//! data, DNAM flags (FO4 + Skyrim single-byte).

use super::super::super::reader::EsmReader;
use super::super::support::parse_txst_group;
use super::super::*;

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

/// Build a TXST record carrying raw-byte sub-records (used to encode
/// the fixed-layout DODT and DNAM payloads that don't fit the
/// `build_txst_record` zstring helper).
fn build_txst_record_raw(form_id: u32, subs: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let mut sub_data = Vec::new();
    for (sub_type, payload) in subs {
        sub_data.extend_from_slice(*sub_type);
        sub_data.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        sub_data.extend_from_slice(payload);
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

/// Build a 36-byte DODT payload from named field values.
fn dodt_payload(
    min_w: f32,
    max_w: f32,
    min_h: f32,
    max_h: f32,
    depth: f32,
    shininess: f32,
    parallax_scale: f32,
    parallax_passes: u8,
    flags: u8,
    color: [u8; 4],
) -> Vec<u8> {
    let mut p = Vec::with_capacity(36);
    p.extend_from_slice(&min_w.to_le_bytes());
    p.extend_from_slice(&max_w.to_le_bytes());
    p.extend_from_slice(&min_h.to_le_bytes());
    p.extend_from_slice(&max_h.to_le_bytes());
    p.extend_from_slice(&depth.to_le_bytes());
    p.extend_from_slice(&shininess.to_le_bytes());
    p.extend_from_slice(&parallax_scale.to_le_bytes());
    p.push(parallax_passes);
    p.push(flags);
    p.extend_from_slice(&[0u8; 2]); // unused
    p.extend_from_slice(&color);
    p
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

// ── #813 / FO4-D4-NEW-01 (DODT) + #814 / FO4-D4-NEW-02 (DNAM) ─────
//
// 207 / 382 vanilla `Fallout4.esm` TXST records ship a DODT decal-
// data sub-record; 382 / 382 ship a DNAM flags sub-record. Pre-fix
// both fell to the catch-all `_ => {}` arm in `parse_txst_group`,
// silently dropping decal authoring (depth / parallax / colour) and
// the `HasModelSpaceNormals` / `NoSpecular` / `FaceGenTinting` flags.

/// Regression for #813: DODT must populate `decal_data` with the
/// xEdit-defined field layout. Pre-fix every decal-bearing TXST lost
/// its width / depth / parallax / colour.
#[test]
fn parse_txst_extracts_dodt_decal_data() {
    let dodt = dodt_payload(
        10.0,              // min_width
        50.0,              // max_width
        20.0,              // min_height
        80.0,              // max_height
        0.5,               // depth
        16.0,              // shininess
        0.04,              // parallax_scale
        4,                 // parallax_passes
        0b0000_0011,       // flags: Parallax + Alpha-Blending
        [255, 64, 0, 200], // color RGBA
    );
    let txst = build_txst_record_raw(
        0xD0D7,
        &[(b"TX00", b"textures/decal_diffuse.dds\0"), (b"DODT", &dodt)],
    );
    let group = wrap_in_txst_group(&[txst]);

    let mut reader = EsmReader::new(&group);
    let header = reader.read_group_header().expect("group header");
    let end = reader.group_content_end(&header);
    let mut diffuse_only: HashMap<u32, String> = HashMap::new();
    let mut sets: HashMap<u32, TextureSet> = HashMap::new();
    parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

    let set = sets.get(&0xD0D7).expect("set missing for DODT TXST");
    let dd = set.decal_data.expect("DODT must populate decal_data");
    assert_eq!(dd.min_width, 10.0);
    assert_eq!(dd.max_width, 50.0);
    assert_eq!(dd.min_height, 20.0);
    assert_eq!(dd.max_height, 80.0);
    assert_eq!(dd.depth, 0.5);
    assert_eq!(dd.shininess, 16.0);
    assert_eq!(dd.parallax_scale, 0.04);
    assert_eq!(dd.parallax_passes, 4);
    assert_eq!(dd.flags, 0b0000_0011);
    assert_eq!(dd.color, [255, 64, 0, 200]);
}

/// Regression for #814: DNAM (FO4 u16) must populate `flags` with
/// the full flag byte/word, including bit 2 (HasModelSpaceNormals)
/// which switches the renderer's normal-map decode path.
#[test]
fn parse_txst_extracts_dnam_flags_fo4() {
    // FO4 DNAM is 2 bytes. Set NoSpecular | HasModelSpaceNormals.
    let dnam = (0x05u16).to_le_bytes();
    let txst = build_txst_record_raw(
        0xDDA4,
        &[(b"TX00", b"textures/face.dds\0"), (b"DNAM", &dnam)],
    );
    let group = wrap_in_txst_group(&[txst]);

    let mut reader = EsmReader::new(&group);
    let header = reader.read_group_header().expect("group header");
    let end = reader.group_content_end(&header);
    let mut diffuse_only: HashMap<u32, String> = HashMap::new();
    let mut sets: HashMap<u32, TextureSet> = HashMap::new();
    parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

    let set = sets.get(&0xDDA4).expect("set missing for DNAM TXST");
    assert_eq!(set.flags, 0x05);
    // Sanity: bit 2 (HasModelSpaceNormals) must be readable.
    assert!(set.flags & 0x04 != 0, "HasModelSpaceNormals must be set");
}

/// Skyrim DNAM is 1 byte. The parser must accept the short form and
/// land it in the low byte of the captured u16.
#[test]
fn parse_txst_extracts_dnam_flags_skyrim_single_byte() {
    let dnam: [u8; 1] = [0x02]; // FaceGenTinting only
    let txst = build_txst_record_raw(
        0x5DA4,
        &[(b"TX00", b"textures/skin.dds\0"), (b"DNAM", &dnam)],
    );
    let group = wrap_in_txst_group(&[txst]);

    let mut reader = EsmReader::new(&group);
    let header = reader.read_group_header().expect("group header");
    let end = reader.group_content_end(&header);
    let mut diffuse_only: HashMap<u32, String> = HashMap::new();
    let mut sets: HashMap<u32, TextureSet> = HashMap::new();
    parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

    let set = sets.get(&0x5DA4).expect("set missing for Skyrim DNAM TXST");
    assert_eq!(set.flags, 0x02);
}

/// A full vanilla-shape TXST with TX slots + MNAM + DODT + DNAM
/// must populate every field on `TextureSet` without losing any.
#[test]
fn parse_txst_extracts_all_fields_together() {
    let dodt = dodt_payload(
        1.0,
        2.0,
        3.0,
        4.0,
        0.1,
        8.0,
        0.02,
        2,
        0x01,
        [128, 128, 128, 255],
    );
    let dnam = (0x07u16).to_le_bytes();
    let txst = build_txst_record_raw(
        0xFA110,
        &[
            (b"TX00", b"textures/diff.dds\0"),
            (b"TX01", b"textures/norm.dds\0"),
            (b"MNAM", b"Materials/Bundle.BGSM\0"),
            (b"DNAM", &dnam),
            (b"DODT", &dodt),
        ],
    );
    let group = wrap_in_txst_group(&[txst]);

    let mut reader = EsmReader::new(&group);
    let header = reader.read_group_header().expect("group header");
    let end = reader.group_content_end(&header);
    let mut diffuse_only: HashMap<u32, String> = HashMap::new();
    let mut sets: HashMap<u32, TextureSet> = HashMap::new();
    parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

    let set = sets.get(&0xFA110).expect("set missing");
    assert_eq!(set.diffuse.as_deref(), Some("textures/diff.dds"));
    assert_eq!(set.normal.as_deref(), Some("textures/norm.dds"));
    assert_eq!(set.material_path.as_deref(), Some("Materials/Bundle.BGSM"));
    assert_eq!(set.flags, 0x07);
    let dd = set.decal_data.expect("decal_data populated");
    assert_eq!(dd.min_width, 1.0);
    assert_eq!(dd.parallax_passes, 2);
    assert_eq!(dd.color, [128, 128, 128, 255]);
}

/// FO4-D4-NEW-06 follow-up: a DODT-only TXST (no TX slots, no MNAM)
/// must still survive the `if set != default()` insertion gate now
/// that `decal_data` carries authored data. Pre-fix, the 3 vanilla
/// DODT-only TXSTs hit the gate and were dropped from `texture_sets`.
#[test]
fn parse_txst_dodt_only_record_is_preserved() {
    let dodt = dodt_payload(
        1.0,
        2.0,
        3.0,
        4.0,
        0.5,
        16.0,
        0.04,
        4,
        0x01,
        [255, 0, 0, 255],
    );
    let txst = build_txst_record_raw(0xD0D7_0017, &[(b"DODT", &dodt)]);
    let group = wrap_in_txst_group(&[txst]);

    let mut reader = EsmReader::new(&group);
    let header = reader.read_group_header().expect("group header");
    let end = reader.group_content_end(&header);
    let mut diffuse_only: HashMap<u32, String> = HashMap::new();
    let mut sets: HashMap<u32, TextureSet> = HashMap::new();
    parse_txst_group(&mut reader, end, &mut diffuse_only, &mut sets).expect("parse");

    assert!(
        sets.get(&0xD0D7_0017).is_some(),
        "DODT-only TXST must survive the default-set rejection gate"
    );
    assert!(
        diffuse_only.get(&0xD0D7_0017).is_none(),
        "DODT-only TXST must not enter the diffuse-only legacy map"
    );
}

// ── #692 / O3-N-04 regression guards ──────────────────────────────
//
// CELL + REFR ownership tuple (XOWN / XRNK / XGLB). Pre-fix every
// arm dropped these on the `_` match — stealing-detection /
// property-crime gameplay had no input. Cross-game (Oblivion +
// FO3 + FNV + Skyrim+ all use the same wire format).

