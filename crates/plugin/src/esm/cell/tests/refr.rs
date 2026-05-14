//! REFR (placed reference) parsing tests.
//!
//! Position / scale, XESP enable-parent (inverted + null-parent), XTEL
//! teleport, XLKR linked refs, XPRM primitives, XRDS radius, XMSP material
//! swap, XRMR rooms, XPOD portal pairs, FO4 texture overrides, ACRE
//! placement, ownership tuple.

use super::super::super::reader::EsmReader;
use super::super::walkers::parse_refr_group;
use super::super::*;

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

/// Regression for #971 / FO4-D4-NEW-08 — XMSP populates
/// `material_swap_ref` so the cell loader can resolve the per-REFR
/// MSWP table at spawn time. Pre-fix the arm was missing and every
/// vanilla Raider colour variant / station-wagon rust pattern / Vault
/// decay overlay rendered with the base mesh's textures.
#[test]
fn parse_refr_extracts_xmsp_material_swap_ref() {
    let xmsp = 0x0024_9A4Eu32.to_le_bytes();
    let record = build_refr_with_subs(0xBEEF, &[(b"XMSP", &xmsp)]);
    let r = parse_one_refr(&record);
    assert_eq!(r.material_swap_ref, Some(0x0024_9A4E));
}

/// REFR with no XMSP must leave `material_swap_ref` as `None` — the
/// cell loader's overlay builder fast-paths these by skipping the
/// `material_swaps` lookup entirely.
#[test]
fn parse_refr_without_xmsp_has_no_material_swap_ref() {
    let record = build_refr_with_subs(0xBEEF, &[]);
    let r = parse_one_refr(&record);
    assert!(r.material_swap_ref.is_none());
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

