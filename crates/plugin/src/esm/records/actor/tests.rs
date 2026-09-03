//! Unit tests for the NPC_/RACE/CLAS/FACT record parsers in the parent
//! `actor` module. Extracted from `actor.rs` (#2055) to keep the
//! production half under the 2000-LOC file threshold; pulled in via
//! `#[cfg(test)] mod tests;`. Body preserved verbatim.

use super::*;
use crate::esm::reader::SubRecord;

fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
    SubRecord {
        sub_type: *typ,
        data: data.to_vec(),
    }
}

#[test]
fn npc_extracts_race_class_factions_inventory() {
    let mut acbs = Vec::new();
    acbs.extend_from_slice(&0x100u32.to_le_bytes()); // flags
    acbs.extend_from_slice(&[0u8; 4]); // fatigue + barter
    acbs.extend_from_slice(&5i16.to_le_bytes()); // level
    acbs.extend_from_slice(&[0u8; 14]); // pad to 24 bytes total

    let mut snam = Vec::new();
    snam.extend_from_slice(&0xAAAAu32.to_le_bytes());
    snam.push(2u8);
    snam.extend_from_slice(&[0u8; 3]);

    let mut cnto = Vec::new();
    cnto.extend_from_slice(&0xBBBBu32.to_le_bytes());
    cnto.extend_from_slice(&3i32.to_le_bytes());

    let subs = vec![
        sub(b"EDID", b"NpcTest\0"),
        sub(b"FULL", b"Test NPC\0"),
        sub(b"RNAM", &0xCCCCu32.to_le_bytes()),
        sub(b"CNAM", &0xDDDDu32.to_le_bytes()),
        sub(b"ACBS", &acbs),
        sub(b"SNAM", &snam),
        sub(b"CNTO", &cnto),
        sub(b"PKID", &0xEEEEu32.to_le_bytes()),
    ];
    let n = parse_npc(0x500, &subs, GameKind::Fallout3NV, &None);
    assert_eq!(n.editor_id, "NpcTest");
    assert_eq!(n.race_form_id, 0xCCCC);
    assert_eq!(n.class_form_id, 0xDDDD);
    assert_eq!(n.factions.len(), 1);
    assert_eq!(n.factions[0].faction_form_id, 0xAAAA);
    assert_eq!(n.factions[0].rank, 2);
    assert_eq!(n.inventory.len(), 1);
    assert_eq!(n.inventory[0].item_form_id, 0xBBBB);
    assert_eq!(n.inventory[0].count, 3);
    assert_eq!(n.ai_packages, vec![0xEEEE]);
    assert_eq!(n.acbs_flags, 0x100);
    assert_eq!(n.level, 5);
}

/// #1650 — Oblivion's 16-byte ACBS (no disposition / template field)
/// must parse via the `GameKind::Oblivion` arm. Pre-fix the 16-byte
/// payload never reached the `>= 24` FNV arm, so every Oblivion actor
/// kept `level = 1` / `acbs_flags = 0` → lowest leveled-list tier and
/// every actor (incl. all females) resolved Male. Pins a level > 1 and
/// the Female flag (bit 0).
#[test]
fn oblivion_16byte_acbs_parses_level_and_gender() {
    use crate::equip::Gender;
    // flags@0 = 1 (Female bit), baseSpell@4, fatigue@6, barterGold@8,
    // level@10 = 6, calcMin@12, calcMax@14 — 16 bytes total.
    let mut acbs = Vec::new();
    acbs.extend_from_slice(&1u32.to_le_bytes()); // flags: Female bit set
    acbs.extend_from_slice(&[0u8; 6]); // baseSpell + fatigue + barterGold
    acbs.extend_from_slice(&6i16.to_le_bytes()); // level @10
    acbs.extend_from_slice(&[0u8; 4]); // calcMin + calcMax → 16 bytes
    assert_eq!(acbs.len(), 16);

    let subs = vec![sub(b"EDID", b"OblivionGuard\0"), sub(b"ACBS", &acbs)];
    let n = parse_npc(0x0001_7000, &subs, GameKind::Oblivion, &None);
    assert_eq!(
        n.level, 6,
        "Oblivion ACBS level @10 must decode (not default 1)"
    );
    assert_eq!(n.acbs_flags, 1, "Oblivion ACBS flags @0 must decode");
    assert_eq!(
        Gender::from_acbs_flags(n.acbs_flags),
        Gender::Female,
        "ACBS flag bit 0 → Female (pre-fix every actor resolved Male)"
    );
}

/// The 16-byte ACBS layout is Oblivion-only: under FNV/FO3 the same
/// payload must NOT be mis-decoded — that arm requires `>= 24` bytes,
/// so a stray 16-byte ACBS is ignored and the defaults stand. Guards
/// the new GameKind gate from leaking into later titles.
#[test]
fn fnv_ignores_16byte_acbs() {
    let mut acbs = Vec::new();
    acbs.extend_from_slice(&1u32.to_le_bytes());
    acbs.extend_from_slice(&[0u8; 6]);
    acbs.extend_from_slice(&6i16.to_le_bytes());
    acbs.extend_from_slice(&[0u8; 4]);
    let subs = vec![sub(b"EDID", b"FnvNpc\0"), sub(b"ACBS", &acbs)];
    let n = parse_npc(0x0010_0001, &subs, GameKind::Fallout3NV, &None);
    assert_eq!(
        n.level, 1,
        "16-byte ACBS must not parse under FNV (stays default)"
    );
    assert_eq!(n.acbs_flags, 0);
}

/// Fallout 4 uses a 20-byte ACBS layout rather than FNV's 24-byte
/// configuration. This fixture is the byte shape authored on vanilla
/// Desdemona: Female flag, level 1, disposition 35, Use Stats template
/// bit. Dropping it makes female-only outfit ARMAs resolve through their
/// empty male path and leaves only the FaceGen head visible.
#[test]
fn fallout4_20byte_acbs_parses_gender_level_and_template_flags() {
    use crate::equip::Gender;

    let acbs = [
        0x21, 0x08, 0x00, 0x00, // flags = 0x821 (Female included)
        0x00, 0x00, // XP value offset
        0x01, 0x00, // level = 1
        0x00, 0x00, // calc min
        0x00, 0x00, // calc max
        0x23, 0x00, // disposition = 35
        0x02, 0x00, // template flags = Use Stats
        0x00, 0x00, // bleedout override
        0x00, 0x00, // unknown
    ];
    let subs = vec![sub(b"EDID", b"Desdemona\0"), sub(b"ACBS", &acbs)];

    let n = parse_npc(0x0004_5AD1, &subs, GameKind::Fallout4, &None);

    assert_eq!(n.acbs_flags, 0x821);
    assert_eq!(Gender::from_acbs_flags(n.acbs_flags), Gender::Female);
    assert_eq!(n.level, 1);
    assert_eq!(n.disposition_base, 35);
    assert_eq!(n.template_flags, 0x0002);
}

/// Skyrim shares FNV's 24-byte ACBS length but not its field layout. Pin all
/// three signed pool offsets plus the fields on either side of the Health
/// offset so the generic Fallout arm cannot silently reclaim this payload.
#[test]
fn skyrim_24byte_acbs_parses_tes5_resource_offsets() {
    let mut acbs = Vec::with_capacity(24);
    acbs.extend_from_slice(&0x0081u32.to_le_bytes()); // female + PC level mult
    acbs.extend_from_slice(&(-20i16).to_le_bytes()); // magicka offset @4
    acbs.extend_from_slice(&15i16.to_le_bytes()); // stamina offset @6
    acbs.extend_from_slice(&750u16.to_le_bytes()); // level multiplier @8
    acbs.extend_from_slice(&3u16.to_le_bytes()); // calc min @10
    acbs.extend_from_slice(&12u16.to_le_bytes()); // calc max @12
    acbs.extend_from_slice(&100u16.to_le_bytes()); // speed @14
    acbs.extend_from_slice(&(-10i16).to_le_bytes()); // disposition @16
    acbs.extend_from_slice(&0x0002u16.to_le_bytes()); // use stats @18
    acbs.extend_from_slice(&25i16.to_le_bytes()); // health offset @20
    acbs.extend_from_slice(&40u16.to_le_bytes()); // bleedout @22
    assert_eq!(acbs.len(), 24);

    let npc = parse_npc(0x45A0, &[sub(b"ACBS", &acbs)], GameKind::Skyrim, &None);
    assert_eq!(npc.acbs_flags, 0x0081);
    assert_eq!(npc.magicka_offset, -20);
    assert_eq!(npc.stamina_offset, 15);
    assert_eq!(npc.level, 750);
    assert_eq!(npc.calc_min, 3);
    assert_eq!(npc.disposition_base, -10);
    assert_eq!(npc.template_flags, 0x0002);
    assert_eq!(npc.health_offset, 25);
}

/// Regression for #1273 — `SCRI` attached-script FormID on NPC_
/// and CREA records was silently dropped. 24 % of FO3 named NPCs
/// and 27 % of FO3 creatures author SCRI; FNV similar. The audit
/// fixture mirrors the Three Dog (`MQGalaxyNewsRadio` broadcast
/// trigger) shape — a thin NPC record where the only meaningful
/// payload is the attached script.
#[test]
fn npc_extracts_scri_attached_script() {
    let subs = vec![
        sub(b"EDID", b"ThreeDog\0"),
        sub(b"SCRI", &0xDEAD_BEEFu32.to_le_bytes()),
    ];
    let n = parse_npc(0x000A_0001, &subs, GameKind::Fallout3NV, &None);
    assert_eq!(n.script_form_id, 0xDEAD_BEEF);
    assert_eq!(n.editor_id, "ThreeDog");
}

/// Same arm fires for CREA records: `parse_npc` is shared between
/// NPC_ and CREA (see `records/mod.rs:b"CREA"` dispatch). Asserts
/// the parser doesn't gate SCRI on a record-type discriminator
/// we don't carry.
#[test]
fn crea_extracts_scri_attached_script() {
    let subs = vec![
        sub(b"EDID", b"SuperMutantBrute\0"),
        sub(b"SCRI", &0xCAFE_0001u32.to_le_bytes()),
    ];
    let n = parse_npc(0x000B_0002, &subs, GameKind::Fallout3NV, &None);
    assert_eq!(n.script_form_id, 0xCAFE_0001);
}

/// Zero-byte SCRI (rare but legal in modded content) must NOT
/// fall through to a stale value; the field defaults to 0 and
/// the arm is gated on `>= 4`, so a 0-length SCRI no-ops.
#[test]
fn npc_short_scri_is_ignored() {
    let subs = vec![sub(b"EDID", b"NoScript\0"), sub(b"SCRI", &[])];
    let n = parse_npc(0x000A_0003, &subs, GameKind::Fallout3NV, &None);
    assert_eq!(n.script_form_id, 0);
}

/// Regression for #377 (FNV F2-03): ACBS `disposition_base` is an
/// i16 at offset 20, not a u8. Pre-fix the parser pulled
/// `sub.data[20]` (one byte), so values outside 0..=127 got their
/// high byte dropped and the sign destroyed. Verify both a negative
/// disposition (Raider-tier) and a positive value > 127 round-trip.
#[test]
fn npc_acbs_disposition_base_reads_signed_i16() {
    // ACBS layout (FNV NPC_, 24 bytes): flags u32, fatigue u16,
    // barter u16, level i16, calc_min u16, calc_max u16, speed_mult
    // u16, karma f32, disposition_base i16, template_flags u16.
    fn acbs_with_disposition(d: i16) -> Vec<u8> {
        let mut a = Vec::with_capacity(24);
        a.extend_from_slice(&0u32.to_le_bytes()); // flags
        a.extend_from_slice(&[0u8; 4]); // fatigue + barter
        a.extend_from_slice(&1i16.to_le_bytes()); // level
        a.extend_from_slice(&[0u8; 10]); // calc_min + calc_max + speed_mult + karma
        a.extend_from_slice(&d.to_le_bytes()); // disposition_base
        a.extend_from_slice(&0u16.to_le_bytes()); // template_flags
        a
    }

    let neg = parse_npc(
        0x700,
        &[
            sub(b"EDID", b"Raider\0"),
            sub(b"ACBS", &acbs_with_disposition(-40)),
        ],
        GameKind::Fallout3NV,
        &None,
    );
    assert_eq!(
        neg.disposition_base, -40,
        "negative disposition must keep its sign"
    );

    let high = parse_npc(
        0x701,
        &[
            sub(b"EDID", b"Friendly\0"),
            sub(b"ACBS", &acbs_with_disposition(200)),
        ],
        GameKind::Fallout3NV,
        &None,
    );
    assert_eq!(
        high.disposition_base, 200,
        "values > 127 must not lose the high byte"
    );
}

#[test]
fn npc_vmad_flips_has_script() {
    // Regression: #369 — Skyrim NPCs with attached Papyrus scripts
    // were not discoverable. The presence-only `has_script` flag
    // is the audit's minimum-viable signal.
    let subs = vec![
        sub(b"EDID", b"ScriptedActor\0"),
        sub(b"VMAD", b"\x05\x00\x02\x00\x00\x00"),
    ];
    let n = parse_npc(0x501, &subs, GameKind::Skyrim, &None);
    assert!(n.has_script);
}

#[test]
fn npc_without_vmad_has_script_false() {
    // Sibling check — bare NPC must keep has_script at default.
    let subs = vec![sub(b"EDID", b"PlainActor\0")];
    let n = parse_npc(0x502, &subs, GameKind::Fallout3NV, &None);
    assert!(!n.has_script);
}

#[test]
fn fact_extracts_relations_and_ranks() {
    let mut xnam = Vec::new();
    xnam.extend_from_slice(&0x123u32.to_le_bytes());
    xnam.extend_from_slice(&(-50i32).to_le_bytes());
    xnam.extend_from_slice(&1u32.to_le_bytes()); // combat reaction = enemy

    let subs = vec![
        sub(b"EDID", b"NCR\0"),
        sub(b"FULL", b"NCR\0"),
        sub(b"DATA", &0x01u32.to_le_bytes()),
        sub(b"XNAM", &xnam),
        sub(b"MNAM", b"Recruit\0"),
        sub(b"MNAM", b"Trooper\0"),
        sub(b"MNAM", b"Veteran\0"),
    ];
    let f = parse_fact(0x42, &subs, &None);
    assert_eq!(f.editor_id, "NCR");
    assert_eq!(f.flags, 0x01);
    assert_eq!(f.relations.len(), 1);
    assert_eq!(f.relations[0].other_faction, 0x123);
    assert_eq!(f.relations[0].modifier, -50);
    assert_eq!(f.relations[0].combat_reaction, 1);
    // No RNAM in this fixture — titles alone still ladder 0/1/2 (#3338).
    assert_eq!(
        f.ranks
            .iter()
            .map(|r| (r.index, r.male.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "Recruit"), (1, "Trooper"), (2, "Veteran")]
    );
}

/// #3338 — `OmertaFaction` (0x10c6f8), transcribed from `FalloutNV.esm`:
/// `RNAM(0), RNAM(1), MNAM, FNAM, RNAM(2), MNAM, FNAM`. Rank 0 is authored
/// with no title at all, so the pre-fix parser — which pushed one entry per
/// `MNAM` and ignored `RNAM` entirely — returned rank 1's label for index 0.
/// 17 of FNV's 682 factions have `RNAM count != MNAM count`.
#[test]
fn fact_rank_ladder_keys_off_rnam_not_mnam_arrival_order() {
    let subs = vec![
        sub(b"EDID", b"OmertaFaction\0"),
        sub(b"RNAM", &0u32.to_le_bytes()),
        sub(b"RNAM", &1u32.to_le_bytes()),
        sub(b"MNAM", b"Thug\0"),
        sub(b"FNAM", b"Thugette\0"),
        sub(b"RNAM", &2u32.to_le_bytes()),
        sub(b"MNAM", b"Boss\0"),
        sub(b"FNAM", b"Madam\0"),
    ];
    let f = parse_fact(0x10c6f8, &subs, &None);
    assert_eq!(f.ranks.len(), 3, "one rung per authored RNAM");
    // The untitled rank 0 keeps its slot instead of being collapsed away.
    assert_eq!(f.ranks[0].index, 0);
    assert!(f.ranks[0].male.is_empty() && f.ranks[0].female.is_empty());
    assert_eq!(f.ranks[0].title(false), None);
    assert_eq!((f.ranks[1].index, f.ranks[1].male.as_str()), (1, "Thug"));
    assert_eq!(f.ranks[1].female, "Thugette");
    assert_eq!((f.ranks[2].index, f.ranks[2].male.as_str()), (2, "Boss"));
    // `FNAM` used to be discarded outright.
    assert_eq!(f.ranks[2].title(true), Some("Madam"));
    assert_eq!(f.ranks[2].title(false), Some("Boss"));
}

/// The complementary shapes from the same census: a rank ladder that is one
/// bare `RNAM` with nothing after it (`NCRCFPowderGangerFaction`, RNAM=1
/// MNAM=0), and a rank whose only title is the male one (FNV authors 94
/// `MNAM` against 53 `FNAM`, so this is the common case).
#[test]
fn fact_rank_handles_titleless_and_male_only_rungs() {
    let subs = vec![sub(b"RNAM", &0u32.to_le_bytes())];
    let f = parse_fact(0x8d395, &subs, &None);
    assert_eq!(f.ranks.len(), 1);
    assert_eq!(f.ranks[0].title(true), None);

    let subs = vec![sub(b"RNAM", &4u32.to_le_bytes()), sub(b"MNAM", b"Ranger\0")];
    let f = parse_fact(0x1, &subs, &None);
    // Non-dense ladders are why `index` is stored rather than implied by
    // position: this rank is 4, at ladder position 0.
    assert_eq!(f.ranks[0].index, 4);
    // The male title is the fallback when no FNAM is authored.
    assert_eq!(f.ranks[0].title(true), Some("Ranger"));
}

/// Regression for #482: the reaction field is a 4-byte u32 per
/// UESP spec, not a single byte. A typical u32 like `0x00000002`
/// (ally) must round-trip through the parser correctly — this is
/// the minimal "parser reads the right field width" check.
///
/// Pre-fix the parser read only `sub.data[8]` (the low byte). For
/// vanilla values 0..=3 the low byte happens to equal the full
/// value, so the test passes with the old code too — its job is
/// to document the spec and catch a future regression that goes
/// back to byte access.
#[test]
fn fact_xnam_combat_reaction_reads_full_u32() {
    let mut xnam = Vec::new();
    xnam.extend_from_slice(&0x999u32.to_le_bytes()); // other faction
    xnam.extend_from_slice(&0i32.to_le_bytes()); // modifier
    xnam.extend_from_slice(&2u32.to_le_bytes()); // combat reaction = ally (full 4 bytes)

    let subs = vec![
        sub(b"EDID", b"AllyFaction\0"),
        sub(b"DATA", &0x00u32.to_le_bytes()),
        sub(b"XNAM", &xnam),
    ];
    let f = parse_fact(0x77, &subs, &None);
    assert_eq!(f.relations.len(), 1);
    assert_eq!(
        f.relations[0].combat_reaction, 2,
        "ally (combat_reaction=2) must round-trip — parser must read 4 bytes"
    );
}

/// Regression for #3339: the decoded reaction is *stored* at its full
/// `u32` width, not truncated back to 8 bits.
///
/// #482 widened the read to 4 bytes to survive "a future mod that extends
/// the enum past 255", but the value landed in a `u8` field via `as u8` —
/// so the wider read only advanced the cursor and threw away exactly the
/// bits it was added to preserve. `fact_xnam_combat_reaction_reads_full_u32`
/// above can't catch this: it uses `2`, which fits either width. A value
/// above 255 is the only input that distinguishes them.
#[test]
fn fact_xnam_combat_reaction_survives_values_above_u8() {
    let mut xnam = Vec::new();
    xnam.extend_from_slice(&0x999u32.to_le_bytes()); // other faction
    xnam.extend_from_slice(&0i32.to_le_bytes()); // modifier
    xnam.extend_from_slice(&0x0001_0002u32.to_le_bytes()); // reaction, low byte == 2

    let subs = vec![
        sub(b"EDID", b"WideReactionFaction\0"),
        sub(b"DATA", &0x00u32.to_le_bytes()),
        sub(b"XNAM", &xnam),
    ];
    let f = parse_fact(0x78, &subs, &None);
    assert_eq!(f.relations.len(), 1);
    assert_eq!(
        f.relations[0].combat_reaction, 0x0001_0002,
        "the full u32 must be stored — an `as u8` cast would yield 2, which is \
         indistinguishable from the vanilla `ally` value"
    );
}

/// Regression for #481 (FNV-2-L1): FACT DATA is a single-byte
/// flags field on FO3 / FNV per UESP. Pre-fix the parser read 4
/// bytes, so any garbage in bytes 1..=3 of the DATA payload
/// (variable tail, neighbour padding) leaked into the high 24
/// bits. Only bits 0–2 are authoritative; verify the fix rejects
/// the high bytes.
#[test]
fn fact_data_reads_only_low_byte() {
    // Simulate a DATA sub-record whose first byte holds the real
    // flags (bit 0 = hidden) and whose remaining bytes are the
    // FNV tail (e.g. `unknown: u8 + crime_gold_multiplier: f32`)
    // or just padding. Pre-fix the parser treated all 4 bytes as
    // flags and reported `0x0EFF_FF01`; post-fix it reports `0x01`.
    let data = [
        0x01u8, // real flags — bit 0 = hidden
        0xFFu8, 0xFFu8, 0xEFu8, // tail / padding bytes; must NOT become flags
    ];
    let subs = vec![sub(b"EDID", b"SpookyFaction\0"), sub(b"DATA", &data)];
    let f = parse_fact(0x88, &subs, &None);
    assert_eq!(
        f.flags, 0x01,
        "only byte 0 of DATA carries flag bits on FO3 / FNV (#481)"
    );
}

/// Edge case: a zero-length DATA sub-record must not crash and
/// must leave flags at the default (0).
#[test]
fn fact_data_empty_leaves_flags_default() {
    let subs = vec![sub(b"EDID", b"PlaceholderFaction\0"), sub(b"DATA", &[])];
    let f = parse_fact(0x89, &subs, &None);
    assert_eq!(
        f.flags, 0,
        "empty DATA must not override the FactionRecord default"
    );
}

// ── #591 / FO4-DIM6-06 face-morph capture ──────────────────────────

/// Build a 36-byte FMRS payload from 9 floats.
fn fmrs_bytes(values: [f32; 9]) -> Vec<u8> {
    let mut out = Vec::with_capacity(36);
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// FMRI / FMRS appear in alternating order on the wire and pair
/// 1-to-1 inside the parsed record. Shape verified against vanilla
/// `Fallout4.esm` named-NPC sub-records (Hancock has 6 paired
/// FMRI/FMRS; MQ101KelloggScene player duplicate has 30).
#[test]
fn npc_pairs_fmri_with_fmrs_in_order() {
    let s0 = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
    let s1 = [-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0];
    let subs = vec![
        sub(b"EDID", b"NamedNpc\0"),
        sub(b"FMRI", &0xDEADu32.to_le_bytes()),
        sub(b"FMRS", &fmrs_bytes(s0)),
        sub(b"FMRI", &0xBEEFu32.to_le_bytes()),
        sub(b"FMRS", &fmrs_bytes(s1)),
    ];
    let n = parse_npc(0x600, &subs, GameKind::Fallout4, &None);
    let face = n
        .face_morphs
        .as_ref()
        .expect("face_morphs must be Some when FMRI/FMRS present");
    assert_eq!(face.morphs.len(), 2);
    assert_eq!(face.morphs[0].form_id, 0xDEAD);
    assert_eq!(face.morphs[0].setting, s0);
    assert_eq!(face.morphs[1].form_id, 0xBEEF);
    assert_eq!(face.morphs[1].setting, s1);
}

/// MSDK / MSDV are parallel arrays: u32 keys + matching f32 values.
/// One sub-record carries the full table on vanilla FO4 NPCs;
/// `chunks_exact` walks every entry without dropping a tail.
#[test]
fn npc_msdk_msdv_walk_full_table() {
    let mut msdk = Vec::new();
    msdk.extend_from_slice(&0x10u32.to_le_bytes());
    msdk.extend_from_slice(&0x20u32.to_le_bytes());
    msdk.extend_from_slice(&0x30u32.to_le_bytes());
    let mut msdv = Vec::new();
    msdv.extend_from_slice(&0.25f32.to_le_bytes());
    msdv.extend_from_slice(&0.5f32.to_le_bytes());
    msdv.extend_from_slice(&0.75f32.to_le_bytes());
    let subs = vec![
        sub(b"EDID", b"Slidered\0"),
        sub(b"MSDK", &msdk),
        sub(b"MSDV", &msdv),
    ];
    let n = parse_npc(0x601, &subs, GameKind::Fallout4, &None);
    let face = n.face_morphs.as_ref().unwrap();
    assert_eq!(face.slider_keys, vec![0x10, 0x20, 0x30]);
    assert_eq!(face.slider_values, vec![0.25, 0.5, 0.75]);
}

/// QNAM is 4 × f32 on FO4 NPCs (texture-lighting tint). HCLF / BCLF
/// each are u32 FormIDs; multiple PNAM head-part FormIDs accumulate.
#[test]
fn npc_captures_qnam_hclf_bclf_pnam() {
    let mut qnam = Vec::new();
    for v in [0.6f32, 0.7, 0.8, 1.0] {
        qnam.extend_from_slice(&v.to_le_bytes());
    }
    let subs = vec![
        sub(b"EDID", b"FullFace\0"),
        sub(b"QNAM", &qnam),
        sub(b"HCLF", &0x1111u32.to_le_bytes()),
        sub(b"BCLF", &0x2222u32.to_le_bytes()),
        sub(b"PNAM", &0xAAAAu32.to_le_bytes()),
        sub(b"PNAM", &0xBBBBu32.to_le_bytes()),
        sub(b"PNAM", &0xCCCCu32.to_le_bytes()),
    ];
    let n = parse_npc(0x602, &subs, GameKind::Fallout4, &None);
    let face = n.face_morphs.as_ref().unwrap();
    assert_eq!(face.texture_lighting, Some([0.6, 0.7, 0.8, 1.0]));
    assert_eq!(face.hair_color, Some(0x1111));
    assert_eq!(face.body_color, Some(0x2222));
    assert_eq!(face.head_parts, vec![0xAAAA, 0xBBBB, 0xCCCC]);
}

/// Face-morph block stays `None` for NPCs that ship none of the
/// covered sub-records — pre-FO4 NPCs and FO4 generic settlers
/// land in this branch. Regression pin so the
/// `if !face.is_empty()` gate doesn't drift to `Some(Default)`.
#[test]
fn npc_without_face_subs_leaves_face_morphs_none() {
    let subs = vec![sub(b"EDID", b"PlainSettler\0")];
    let n = parse_npc(0x603, &subs, GameKind::Fallout4, &None);
    assert!(n.face_morphs.is_none());
}

/// FO4 `PRPS` decodes to `(AVIF FormID, value)` pairs (8 bytes each)
/// and `DNAM`'s leading two u16 are the baked Calculated Health /
/// Action Points — the whole CHARAL FO4 NPC-stat decode in one record.
#[test]
fn npc_fo4_decodes_prps_pairs_and_dnam_baked_stats() {
    let mut prps = Vec::new();
    prps.extend_from_slice(&0x0000_02A0u32.to_le_bytes()); // Strength AVIF
    prps.extend_from_slice(&7.0f32.to_le_bytes());
    prps.extend_from_slice(&0x0000_02A6u32.to_le_bytes()); // Luck AVIF
    prps.extend_from_slice(&5.0f32.to_le_bytes());
    let mut dnam = Vec::new();
    dnam.extend_from_slice(&240u16.to_le_bytes()); // Calculated Health
    dnam.extend_from_slice(&90u16.to_le_bytes()); // Calculated Action Points
    dnam.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // far-dist + geared + unused
                                                       // Two PRKR perks: { PERK FormID u32, rank u8 } = 5 bytes each.
    let mut prkr_a = 0x0001_D245u32.to_le_bytes().to_vec();
    prkr_a.push(1);
    let mut prkr_b = 0x0001_D246u32.to_le_bytes().to_vec();
    prkr_b.push(3);
    let subs = vec![
        sub(b"EDID", b"Fo4Npc\0"),
        sub(b"PRPS", &prps),
        sub(b"DNAM", &dnam),
        sub(b"PRKR", &prkr_a),
        sub(b"PRKR", &prkr_b),
    ];
    let n = parse_npc(0x610, &subs, GameKind::Fallout4, &None);
    assert_eq!(n.actor_value_props, vec![(0x2A0, 7.0), (0x2A6, 5.0)]);
    assert_eq!(n.calculated_health, 240);
    assert_eq!(n.calculated_action_points, 90);
    assert_eq!(n.perks, vec![(0x1_D245, 1), (0x1_D246, 3)]);
}

/// The FO4 AV-property arms are gated on `uses_actor_value_properties`
/// (FO4+ only). An FNV NPC carrying a stray `DNAM` must NOT be read as
/// FO4 calculated stats — FNV `NPC_` `DNAM` is a different layout, and
/// FNV has no `PRPS`. Guards the predicate gate against drift.
#[test]
fn npc_fnv_ignores_fo4_av_property_arms() {
    let subs = vec![
        sub(b"EDID", b"FnvNpc\0"),
        sub(b"DNAM", &[0xFF; 8]),
        sub(b"PRKR", &[0xFF; 5]),
    ];
    let n = parse_npc(0x611, &subs, GameKind::Fallout3NV, &None);
    assert!(n.actor_value_props.is_empty());
    assert_eq!(n.calculated_health, 0);
    assert_eq!(n.calculated_action_points, 0);
    assert!(n.perks.is_empty(), "PRKR gated off for FNV");
}

/// #3158 — Skyrim `NPC_` carries `PRKZ`/`PRKR` (1620 of `Skyrim.esm`'s 5118
/// records, 7993 entries; censused with the `probe_npc_perks` example), but
/// the `PRKR` arm used to sit behind `uses_actor_value_properties`, which
/// excludes Skyrim. Every Skyrim NPC therefore parsed with an empty `perks`
/// list, `spawn_npc_entity` skipped the `Perks` component, and `HasPerk`
/// evaluated a structural `0.0` on the reference title.
///
/// Skyrim's `PRKR` is 8 bytes, not FO4's 5 — the extra three are unused
/// padding after the rank, so the FormID/rank offsets are shared. This pins
/// both the wider gate and the shared decode.
#[test]
fn npc_skyrim_decodes_eight_byte_prkr_perks() {
    let mut prkr_a = 0x0005_820Cu32.to_le_bytes().to_vec();
    prkr_a.extend_from_slice(&[2, 0, 0, 0]); // rank 2 + three unused bytes
    let mut prkr_b = 0x000C_44B7u32.to_le_bytes().to_vec();
    prkr_b.extend_from_slice(&[1, 0, 0, 0]);
    let subs = vec![
        sub(b"EDID", b"SkyrimNpc\0"),
        sub(b"PRKZ", &2u32.to_le_bytes()),
        sub(b"PRKR", &prkr_a),
        sub(b"PRKR", &prkr_b),
    ];
    let n = parse_npc(0x612, &subs, GameKind::Skyrim, &None);
    assert_eq!(
        n.perks,
        vec![(0x5_820C, 2), (0xC_44B7, 1)],
        "Skyrim PRKR must decode under uses_npc_perk_entries"
    );
    // The wider perk gate must NOT drag the FO4 actor-value arms along:
    // Skyrim has no PRPS and its DNAM is a different layout.
    assert!(n.actor_value_props.is_empty());
    assert_eq!(n.calculated_health, 0);
}

/// The perk gate stays closed for the three masters that ship no `PRKR` at
/// all (`Oblivion.esm` 2482 `NPC_`, `Fallout3.esm` 1647, `FalloutNV.esm`
/// 3816 — zero perk sub-records between them). A stray `PRKR` on those games
/// is malformed data, not perks.
#[test]
fn npc_perk_gate_stays_closed_for_pre_skyrim_games() {
    for game in [GameKind::Oblivion, GameKind::Fallout3NV] {
        let subs = vec![sub(b"EDID", b"LegacyNpc\0"), sub(b"PRKR", &[0xFF; 8])];
        let n = parse_npc(0x613, &subs, game, &None);
        assert!(n.perks.is_empty(), "PRKR must stay gated off for {game:?}");
    }
}

/// Mismatched FMRI/FMRS counts truncate to the shorter array
/// instead of panicking. Defensive against malformed mod records;
/// vanilla Bethesda content always pairs them 1-to-1.
#[test]
fn npc_mismatched_fmri_fmrs_truncates_to_shorter() {
    let s = [1.0; 9];
    // 3 FMRI but only 2 FMRS — should yield 2 paired entries.
    let subs = vec![
        sub(b"EDID", b"Malformed\0"),
        sub(b"FMRI", &0xA1u32.to_le_bytes()),
        sub(b"FMRI", &0xA2u32.to_le_bytes()),
        sub(b"FMRI", &0xA3u32.to_le_bytes()),
        sub(b"FMRS", &fmrs_bytes(s)),
        sub(b"FMRS", &fmrs_bytes(s)),
    ];
    let n = parse_npc(0x604, &subs, GameKind::Fallout4, &None);
    let face = n.face_morphs.as_ref().unwrap();
    assert_eq!(face.morphs.len(), 2);
    assert_eq!(face.morphs[0].form_id, 0xA1);
    assert_eq!(face.morphs[1].form_id, 0xA2);
}

/// FNV NPC `PNAM` carries a single eyebrow HDPT FormID, NOT an
/// FO4-style head-parts list. The `game`-aware gate keeps FNV
/// PNAMs out of `face_morphs.head_parts`; M41.0 Phase 1a now
/// captures them into `runtime_facegen.eyebrow_form_id` instead
/// of dropping them on the floor.
#[test]
fn npc_fnv_pnam_lands_in_runtime_facegen_eyebrow() {
    let subs = vec![
        sub(b"EDID", b"FnvNpc\0"),
        // FNV-style PNAM: a single 4-byte eyebrow HDPT FormID.
        sub(b"PNAM", &0xDEADu32.to_le_bytes()),
    ];
    let n = parse_npc(0x606, &subs, GameKind::Fallout3NV, &None);
    assert!(
        n.face_morphs.is_none(),
        "FNV PNAM must not populate face_morphs.head_parts (FO4 semantic)"
    );
    let recipe = n
        .runtime_facegen
        .as_ref()
        .expect("FNV PNAM must produce runtime_facegen");
    assert_eq!(recipe.eyebrow_form_id, Some(0xDEAD));
}

/// FGGS / FGGA / FGTS slider arrays land in fixed-size float
/// arrays. Pre-Phase-3b the parser is the only consumer; the
/// spawn-side morph evaluator picks them up from
/// `runtime_facegen.fggs` directly.
#[test]
fn npc_fnv_fggs_fgga_fgts_populate_runtime_facegen() {
    let mut fggs = Vec::with_capacity(50 * 4);
    for i in 0..50 {
        fggs.extend_from_slice(&(i as f32 * 0.1).to_le_bytes());
    }
    let mut fgga = Vec::with_capacity(30 * 4);
    for i in 0..30 {
        fgga.extend_from_slice(&(i as f32 * -0.05).to_le_bytes());
    }
    let mut fgts = Vec::with_capacity(50 * 4);
    for i in 0..50 {
        fgts.extend_from_slice(&(i as f32 * 0.02).to_le_bytes());
    }
    let subs = vec![
        sub(b"EDID", b"SunnyMockup\0"),
        sub(b"FGGS", &fggs),
        sub(b"FGGA", &fgga),
        sub(b"FGTS", &fgts),
    ];
    let n = parse_npc(0x607, &subs, GameKind::Fallout3NV, &None);
    let recipe = n
        .runtime_facegen
        .as_ref()
        .expect("FGGS/FGGA/FGTS must produce runtime_facegen");
    assert!((recipe.fggs[7] - 0.7).abs() < 1e-6);
    assert!((recipe.fgga[5] - -0.25).abs() < 1e-6);
    assert!((recipe.fgts[3] - 0.06).abs() < 1e-6);
    // Slot beyond the table stays at the default 0.0.
    assert_eq!(recipe.fggs[49], 4.9_f32);
    assert_eq!(recipe.fgga[29], -1.45_f32);
}

/// Short FGGS payload pads with zeros — the parser must not
/// over-read or panic on truncated mod records.
#[test]
fn npc_fnv_short_fggs_pads_with_zero() {
    // 5 × f32 = 20 bytes; far short of the canonical 200.
    let mut fggs = Vec::with_capacity(5 * 4);
    for v in [1.0f32, 2.0, 3.0, 4.0, 5.0] {
        fggs.extend_from_slice(&v.to_le_bytes());
    }
    let subs = vec![sub(b"EDID", b"TruncMod\0"), sub(b"FGGS", &fggs)];
    let n = parse_npc(0x608, &subs, GameKind::Fallout3NV, &None);
    let recipe = n.runtime_facegen.as_ref().unwrap();
    assert_eq!(recipe.fggs[0], 1.0);
    assert_eq!(recipe.fggs[4], 5.0);
    for v in &recipe.fggs[5..] {
        assert_eq!(*v, 0.0);
    }
}

/// HCLR / HNAM / LNAM / ENAM all land in `runtime_facegen` on
/// kf-era games. HCLR's optional 4th byte is dropped per UESP.
#[test]
fn npc_fnv_hclr_hnam_lnam_enam_populate_runtime_facegen() {
    let subs = vec![
        sub(b"EDID", b"FullRecipe\0"),
        sub(b"HCLR", &[0x33, 0x55, 0x77, 0xFF]), // 4-byte; alpha dropped
        sub(b"HNAM", &0xCAFEu32.to_le_bytes()),
        sub(b"LNAM", &0xBEEFu32.to_le_bytes()),
        sub(b"ENAM", &0xF00Du32.to_le_bytes()),
    ];
    let n = parse_npc(0x609, &subs, GameKind::Fallout3NV, &None);
    let recipe = n.runtime_facegen.as_ref().unwrap();
    assert_eq!(recipe.hair_color_rgb, Some([0x33, 0x55, 0x77]));
    assert_eq!(recipe.hair_form_id, Some(0xCAFE));
    assert_eq!(recipe.unused_lnam, Some(0xBEEF));
    assert_eq!(recipe.eyes_form_id, Some(0xF00D));
}

/// FO4 NPCs ship none of the kf-era recipe sub-records — and
/// even if a malformed mod adds an FGGS payload to an FO4 NPC,
/// the gate keeps `runtime_facegen` at `None`. Mirror property:
/// kf-era NPCs with FO4-shaped FMRI/FMRS don't populate
/// `face_morphs`. Both are pinned to keep the predicates honest.
#[test]
fn npc_runtime_facegen_and_face_morphs_are_mutually_exclusive() {
    let fggs = vec![0u8; 200];
    let subs_fo4 = vec![sub(b"EDID", b"Fo4Stray\0"), sub(b"FGGS", &fggs)];
    let n = parse_npc(0x60A, &subs_fo4, GameKind::Fallout4, &None);
    assert!(n.runtime_facegen.is_none(), "FO4 must not parse FGGS");

    let mut fmrs = Vec::with_capacity(36);
    for v in [0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9] {
        fmrs.extend_from_slice(&v.to_le_bytes());
    }
    let subs_fnv = vec![
        sub(b"EDID", b"FnvStray\0"),
        sub(b"FMRI", &0xDEADu32.to_le_bytes()),
        sub(b"FMRS", &fmrs),
    ];
    let n = parse_npc(0x60B, &subs_fnv, GameKind::Fallout3NV, &None);
    assert!(n.face_morphs.is_none(), "FNV must not parse FMRI/FMRS");
}

/// Wrong-sized FMRS (e.g. a Skyrim record that ships a smaller
/// payload, or a corrupt mod) is dropped silently — the length
/// gate `>= 36` keeps malformed bytes from being re-interpreted as
/// a partial setting array. The matched FMRI then becomes an
/// orphan and the truncation rule above drops it too.
#[test]
fn npc_undersized_fmrs_is_dropped() {
    let subs = vec![
        sub(b"EDID", b"BadBytes\0"),
        sub(b"FMRI", &0xF00Du32.to_le_bytes()),
        sub(b"FMRS", &[0u8; 16]), // < 36 bytes
    ];
    let n = parse_npc(0x605, &subs, GameKind::Fallout4, &None);
    // FMRI captured but FMRS dropped → mismatched (1 vs 0) →
    // truncate to 0 → no morphs → block is empty → None.
    assert!(n.face_morphs.is_none());
}

/// #1996 — every embedded FormID on `NpcRecord` must land in global
/// load-order space, matching how `EsmIndex.packages` / `.races` /
/// `.classes` are keyed (`read_record_header` remaps the record's own
/// header FormID unconditionally). Pre-fix, `parse_npc` never threaded
/// a remap at all, so `PKID`/`RNAM`/`CNAM` stayed plugin-local and any
/// multi-plugin load silently failed every `index.packages.get(pk)` /
/// `index.races.get(...)` / `index.classes.get(...)` lookup for an
/// override-plugin NPC — e.g. the spawn tail's `active_package`
/// resolve always coming back empty for that NPC's packages.
#[test]
fn npc_embedded_form_ids_remap_to_global_space() {
    // Plugin slot 2, one master at slot 0 (mirrors
    // `parse_pack_pldt_near_reference_remaps_form_id`'s fixture).
    let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
    // mod_index 1 == master_slots.len() → self-reference (a FormID
    // this override plugin defines itself, e.g. its own PACK/RACE).
    let self_ref = (1u32 << 24) | 0x0000_1234;
    // mod_index 0 → the master's slot (e.g. a base-game RACE/CLAS).
    let master_ref: u32 = 0x0000_5678;

    let subs = vec![
        sub(b"EDID", b"OverridePluginNpc\0"),
        sub(b"RNAM", &master_ref.to_le_bytes()),
        sub(b"CNAM", &master_ref.to_le_bytes()),
        sub(b"PKID", &self_ref.to_le_bytes()),
    ];
    let n = parse_npc(0x000A_0001, &subs, GameKind::Fallout3NV, &Some(remap));

    assert_eq!(
        n.race_form_id, master_ref,
        "master-slot reference (mod_index 0) stays at slot 0's byte"
    );
    assert_eq!(
        n.class_form_id, master_ref,
        "class FormID must remap the same way as race"
    );
    assert_eq!(
        n.ai_packages,
        vec![(2u32 << 24) | 0x0000_1234],
        "self-referential PKID must resolve to the plugin's own \
             global slot (2), not stay at its local self-ref top byte (1) — \
             this is the exact field the spawn tail looks up via \
             `index.packages.get(pk)` before `active_package`"
    );
}

/// #2080 / FNV-D4-02 — the FNV/FO3/Oblivion FaceGen-recipe fields
/// (HNAM/ENAM/PNAM-eyebrow) must remap the same way as the classic
/// fields #1996 already covered. Pre-fix, these arms read
/// `u32_or_default()` directly with no `remap_fid` wrapper despite
/// `remap` being in scope for the whole function — an override-
/// plugin NPC's own hair/eyes/eyebrow reference resolved against the
/// wrong `index.hair`/`index.eyes` entry (silently bald/browless).
#[test]
fn npc_facegen_recipe_form_ids_remap_to_global_space() {
    let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
    let self_ref = (1u32 << 24) | 0x0000_1234; // eyebrow: this plugin's own HDPT
    let master_ref: u32 = 0x0000_5678; // hair/eyes: a base-game HAIR/EYES

    let subs = vec![
        sub(b"EDID", b"OverridePluginFaceGenNpc\0"),
        sub(b"HNAM", &master_ref.to_le_bytes()),
        sub(b"ENAM", &master_ref.to_le_bytes()),
        sub(b"PNAM", &self_ref.to_le_bytes()),
    ];
    let n = parse_npc(0x000A_0002, &subs, GameKind::Fallout3NV, &Some(remap));
    let recipe = n
        .runtime_facegen
        .expect("HNAM/ENAM/PNAM populate the recipe");

    assert_eq!(
        recipe.hair_form_id,
        Some(master_ref),
        "master-slot HNAM reference (mod_index 0) stays at slot 0's byte"
    );
    assert_eq!(
        recipe.eyes_form_id,
        Some(master_ref),
        "ENAM must remap the same way as HNAM"
    );
    assert_eq!(
        recipe.eyebrow_form_id,
        Some((2u32 << 24) | 0x0000_1234),
        "self-referential eyebrow PNAM must resolve to the plugin's own global slot"
    );
}

/// #2080 / FNV-D4-02 — the FO4+ face-morph block (FMRI, HCLF, BCLF,
/// and the FO4 head-parts `PNAM`) shares the same unremapped-FormID
/// gap as the pre-FO4 recipe block. Same impact: an override-plugin
/// NPC's own hair-color/body-color/head-part reference resolves
/// against the wrong entry.
#[test]
fn npc_fo4_face_morph_form_ids_remap_to_global_space() {
    let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
    let self_ref = (1u32 << 24) | 0x0000_1234;
    let master_ref: u32 = 0x0000_5678;

    let subs = vec![
        sub(b"EDID", b"OverridePluginFo4FaceNpc\0"),
        sub(b"FMRI", &master_ref.to_le_bytes()),
        sub(b"FMRS", &[0u8; 36]),
        sub(b"HCLF", &master_ref.to_le_bytes()),
        sub(b"BCLF", &self_ref.to_le_bytes()),
        sub(b"PNAM", &master_ref.to_le_bytes()),
    ];
    let n = parse_npc(0x000A_0003, &subs, GameKind::Fallout4, &Some(remap));
    let face = n
        .face_morphs
        .expect("FMRI/HCLF/BCLF/PNAM populate face_morphs");

    assert_eq!(
        face.morphs[0].form_id, master_ref,
        "FMRI must remap the same way as the pre-FO4 recipe fields"
    );
    assert_eq!(face.hair_color, Some(master_ref));
    assert_eq!(face.body_color, Some((2u32 << 24) | 0x0000_1234));
    assert_eq!(
        face.head_parts,
        vec![master_ref],
        "FO4 head-parts PNAM must remap the same way as the FNV/FO3 eyebrow PNAM"
    );
}

// ── #967 / OBL-D3-NEW-03 — RACE Oblivion-shape DATA + subs ────────

/// Build a 36-byte Oblivion DATA payload: 8 × (u8 skill_index, u8
/// bonus) + heightM + heightF + weightM + weightF + raceFlags.
fn oblivion_data(
    pairs: [(u8, i8); 8],
    height: (f32, f32),
    weight: (f32, f32),
    flags: u32,
) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    for (skill, bonus) in pairs {
        data.push(skill);
        data.push(bonus as u8);
    }
    data.extend_from_slice(&height.0.to_le_bytes());
    data.extend_from_slice(&height.1.to_le_bytes());
    data.extend_from_slice(&weight.0.to_le_bytes());
    data.extend_from_slice(&weight.1.to_le_bytes());
    data.extend_from_slice(&flags.to_le_bytes());
    assert_eq!(data.len(), 36);
    data
}

#[test]
fn race_oblivion_data_reads_8_skill_pairs_plus_heights() {
    // Nord-like sample: bonuses on Blade(0x0E) + Block(0x0F) +
    // HeavyArmor(0x12) + Restoration(0x19) + LightArmor(0x1B);
    // remaining slots = 0xFF (Skill_None sentinel, should drop).
    let pairs = [
        (0x0E_u8, 10_i8), // Blade +10
        (0x0F, 5),        // Block +5
        (0x12, 5),        // HeavyArmor +5
        (0x19, 5),        // Restoration +5
        (0x1B, 5),        // LightArmor +5
        (0xFF, 0),        // Skill_None — drop
        (0xFF, 0),
        (0xFF, 0),
    ];
    let data = oblivion_data(pairs, (1.04, 1.0), (1.0, 1.0), 0x01);
    let subs = vec![
        sub(b"EDID", b"Nord\0"),
        sub(b"FULL", b"Nord\0"),
        sub(b"DATA", &data),
    ];
    let r = parse_race(0x10001, &subs, GameKind::Oblivion, &None);
    // 5 real bonuses, 3 None-sentinel slots dropped.
    assert_eq!(r.skill_bonuses.len(), 5);
    assert_eq!(r.skill_bonuses[0], (0x0E, 10));
    assert_eq!(r.skill_bonuses[4], (0x1B, 5));
    assert!((r.base_height.0 - 1.04).abs() < 1e-6);
    assert!((r.base_height.1 - 1.0).abs() < 1e-6);
    assert_eq!(r.race_flags, 0x01);
}

#[test]
fn race_oblivion_subrecords_captured() {
    let attr = [
        // male
        50, 40, 30, 40, 30, 50, 30, 50, //
        // female
        40, 40, 30, 50, 30, 50, 40, 50,
    ];
    let mut dnam = Vec::new();
    dnam.extend_from_slice(&0x000Au32.to_le_bytes()); // male hair
    dnam.extend_from_slice(&0x000Bu32.to_le_bytes()); // female hair
    let mut vnam = Vec::new();
    vnam.extend_from_slice(&0x0100u32.to_le_bytes());
    vnam.extend_from_slice(&0x0101u32.to_le_bytes());
    let pnam = 5.0_f32.to_le_bytes();
    let unam = 3.0_f32.to_le_bytes();
    let mut xnam_breton = Vec::new();
    xnam_breton.extend_from_slice(&0x10001u32.to_le_bytes()); // other race
    xnam_breton.extend_from_slice(&(-5_i32).to_le_bytes());
    let data = oblivion_data([(0xFF, 0); 8], (1.0, 1.0), (1.0, 1.0), 0);
    let subs = vec![
        sub(b"EDID", b"Breton\0"),
        sub(b"DATA", &data),
        sub(b"ATTR", &attr),
        sub(b"DNAM", &dnam),
        sub(b"VNAM", &vnam),
        sub(b"PNAM", &pnam),
        sub(b"UNAM", &unam),
        sub(b"XNAM", &xnam_breton),
    ];
    let r = parse_race(0x10002, &subs, GameKind::Oblivion, &None);
    let a = r.base_attributes.expect("ATTR captured");
    assert_eq!(a.male.strength, 50);
    assert_eq!(a.male.luck, 50);
    assert_eq!(a.female.strength, 40);
    assert_eq!(r.default_hair, Some((0x000A, 0x000B)));
    assert_eq!(r.voice_forms, Some((0x0100, 0x0101)));
    assert_eq!(r.facegen_main_clamp, Some(5.0));
    assert_eq!(r.facegen_face_clamp, Some(3.0));
    assert_eq!(r.race_reactions, vec![(0x10001, -5)]);
}

/// SIBLING gate (audit completeness check #1) — FNV-tagged RACE
/// reuses the 36-byte DATA shape per OpenMW, but the Oblivion-only
/// sub-records (ATTR / DNAM / VNAM / PNAM / UNAM / XNAM) MUST be
/// dropped when `game != GameKind::Oblivion`. Otherwise a future
/// loader walking the same arm on TES5 would mis-read VNAM's
/// 4-byte equipment-type-flags payload as 2 form IDs.
#[test]
fn race_oblivion_subrecords_skipped_on_non_oblivion_games() {
    let attr = [10u8; 16];
    let mut dnam = Vec::new();
    dnam.extend_from_slice(&0x000Au32.to_le_bytes());
    dnam.extend_from_slice(&0x000Bu32.to_le_bytes());
    let data = oblivion_data([(0xFF, 0); 8], (1.0, 1.0), (1.0, 1.0), 0);
    let subs = vec![
        sub(b"EDID", b"FnvHuman\0"),
        sub(b"DATA", &data),
        sub(b"ATTR", &attr),
        sub(b"DNAM", &dnam),
    ];
    let r = parse_race(0x10003, &subs, GameKind::Fallout3NV, &None);
    assert!(r.base_attributes.is_none());
    assert!(r.default_hair.is_none());
    // DATA path still runs — FNV shares the 36-byte shape.
    assert_eq!(r.race_flags, 0);
}

/// Multiple XNAM sub-records — each pair appends to the
/// `race_reactions` list in authoring order.
#[test]
fn race_multiple_xnam_pairs_collected() {
    let data = oblivion_data([(0xFF, 0); 8], (1.0, 1.0), (1.0, 1.0), 0);
    let mut x1 = Vec::new();
    x1.extend_from_slice(&0x10010u32.to_le_bytes());
    x1.extend_from_slice(&5_i32.to_le_bytes());
    let mut x2 = Vec::new();
    x2.extend_from_slice(&0x10011u32.to_le_bytes());
    x2.extend_from_slice(&(-3_i32).to_le_bytes());
    let subs = vec![
        sub(b"EDID", b"Imperial\0"),
        sub(b"DATA", &data),
        sub(b"XNAM", &x1),
        sub(b"XNAM", &x2),
    ];
    let r = parse_race(0x10004, &subs, GameKind::Oblivion, &None);
    assert_eq!(r.race_reactions.len(), 2);
    assert_eq!(r.race_reactions[0], (0x10010, 5));
    assert_eq!(r.race_reactions[1], (0x10011, -3));
}

// ── #2093 / SKY-D3-NEW-01 — RACE.WNAM default skin ───────────────

/// `WNAM` on a `uses_prebaked_facegen()` game captures the default
/// skin ARMO form ID. Without this the prebaked NPC-spawn path has
/// no way to give NPCs a body-mesh fallback when OTFT/CNTO doesn't
/// cover every biped region.
#[test]
fn race_skyrim_wnam_captured() {
    let subs = vec![
        sub(b"EDID", b"NordRace\0"),
        sub(b"WNAM", &0x0001_3746u32.to_le_bytes()),
    ];
    let r = parse_race(0x10005, &subs, GameKind::Skyrim, &None);
    assert_eq!(r.default_skin, Some(0x0001_3746));
}

/// `WNAM` must NOT be read on TES4/FO3/FNV — those games don't
/// author it, and treating a stray same-named sub-record as a skin
/// FormID would silently equip garbage.
#[test]
fn race_wnam_skipped_on_non_prebaked_games() {
    for game in [GameKind::Oblivion, GameKind::Fallout3NV] {
        let subs = vec![
            sub(b"EDID", b"SomeRace\0"),
            sub(b"WNAM", &0x0001_3746u32.to_le_bytes()),
        ];
        let r = parse_race(0x10006, &subs, game, &None);
        assert!(
            r.default_skin.is_none(),
            "{game:?} must not capture WNAM as a skin form"
        );
    }
}

// ── #968 / OBL-D3-NEW-04 — CLAS Oblivion-shape DATA ──────────────

/// Build a 52-byte Oblivion CLAS DATA payload per the empirical
/// vanilla layout (#968):
///   2 × u32 primary attributes (8 B)
///   u32 specialization         (4 B)
///   7 × u32 major skills       (28 B)
///   u32 flags                  (4 B)
///   u32 services               (4 B)
///   i8 trainer + u8 level + 2 B pad (4 B)
fn oblivion_clas_data(attrs: (u32, u32), spec: u32, majors: [u32; 7], flags: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity(52);
    data.extend_from_slice(&attrs.0.to_le_bytes());
    data.extend_from_slice(&attrs.1.to_le_bytes());
    data.extend_from_slice(&spec.to_le_bytes());
    for s in majors {
        data.extend_from_slice(&s.to_le_bytes());
    }
    data.extend_from_slice(&flags.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes()); // services
    data.extend_from_slice(&[0u8; 4]); // trainer skill + level + 2 pad
    assert_eq!(data.len(), 52);
    data
}

#[test]
fn clas_oblivion_knight_round_trips() {
    // Knight (form 0x836 in vanilla Oblivion.esm) — primary
    // attrs (Strength=0, Personality=6), specialization 0 = Combat,
    // 7 majors per the empirical probe.
    let data = oblivion_clas_data(
        (0, 6),
        0,
        [0x0F, 0x17, 0x12, 0x10, 0x0E, 0x20, 0x11],
        0x01, // Playable
    );
    let subs = vec![
        sub(b"EDID", b"Knight\0"),
        sub(b"FULL", b"Knight\0"),
        sub(b"DATA", &data),
    ];
    let c = parse_clas(0x836, &subs, GameKind::Oblivion);
    assert_eq!(c.primary_attributes, Some((0, 6)));
    assert_eq!(c.specialization, Some(0));
    assert_eq!(
        c.major_skills,
        vec![0x0F, 0x17, 0x12, 0x10, 0x0E, 0x20, 0x11]
    );
    assert_eq!(c.flags_oblivion, Some(0x01));
    // FNV-shape fields stay empty on Oblivion.
    assert!(c.tag_skills.is_empty());
    assert_eq!(c.base_attributes, [0u8; 7]);
}

/// FNV CLAS (fopdoc layout): tag skills come from the 28-byte `DATA`
/// block; the 7 base SPECIAL attributes come from the separate `ATTR`
/// subrecord — NOT appended to `DATA` (the pre-#1663 assumption). The
/// game gate keeps it off the Oblivion 52-byte arm.
#[test]
fn clas_fnv_tag_skills_and_attr_special() {
    // 28-byte DATA: 4 × i32 tag skills + flags + services + teaches
    // (i8) + max-training (u8) + 2 unused. No attributes here.
    let mut data = Vec::with_capacity(28);
    data.extend_from_slice(&0xC0DE_0001u32.to_le_bytes());
    data.extend_from_slice(&0xC0DE_0002u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes()); // (filtered by != 0)
    data.extend_from_slice(&0xC0DE_0003u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes()); // flags
    data.extend_from_slice(&0u32.to_le_bytes()); // buys/sells + services
    data.extend_from_slice(&[0u8; 4]); // teaches + max + 2 unused
    assert_eq!(data.len(), 28);
    // ATTR: one 7-byte struct (Str, Per, End, Cha, Int, Agi, Luck).
    let attr = [1u8, 2, 3, 4, 5, 6, 7];
    let subs = vec![
        sub(b"EDID", b"NCRTrooper\0"),
        sub(b"DATA", &data),
        sub(b"ATTR", &attr),
    ];
    let c = parse_clas(0x600, &subs, GameKind::Fallout3NV);
    assert_eq!(c.tag_skills, vec![0xC0DE_0001, 0xC0DE_0002, 0xC0DE_0003]);
    assert_eq!(c.base_attributes, [1, 2, 3, 4, 5, 6, 7]);
    // Oblivion-only fields stay None.
    assert!(c.primary_attributes.is_none());
    assert!(c.specialization.is_none());
    assert!(c.major_skills.is_empty());
    assert!(c.flags_oblivion.is_none());
}

/// FO3 splits the 7 base attributes across 7 single-byte `ATTR`
/// subrecords; they must accumulate in order into `base_attributes`.
#[test]
fn clas_fo3_split_attr_subrecords_accumulate() {
    let mut data = Vec::with_capacity(28);
    data.extend_from_slice(&0u32.to_le_bytes()); // tag1 (filtered)
    data.extend_from_slice(&[0u8; 24]); // remaining DATA
    let mut subs = vec![sub(b"EDID", b"FO3Class\0"), sub(b"DATA", &data)];
    for v in [5u8, 6, 7, 4, 8, 6, 5] {
        subs.push(sub(b"ATTR", &[v]));
    }
    let c = parse_clas(0x601, &subs, GameKind::Fallout3NV);
    assert_eq!(c.base_attributes, [5, 6, 7, 4, 8, 6, 5]);
}

/// Boundary: a malformed Oblivion CLAS with < 52-byte DATA must
/// fall through cleanly (no panic, no off-the-end read). Both
/// game-specific arms gate on length.
#[test]
fn clas_oblivion_short_data_drops_silently() {
    let data = vec![0u8; 40]; // less than 52
    let subs = vec![sub(b"EDID", b"BadClass\0"), sub(b"DATA", &data)];
    let c = parse_clas(0x837, &subs, GameKind::Oblivion);
    // No arm fired; nothing crashed; all Oblivion-only fields stay None.
    assert!(c.primary_attributes.is_none());
    assert!(c.major_skills.is_empty());
    // FNV arm would have fired at >= 35 — but we're game=Oblivion,
    // so the gate skipped it.
    assert!(c.tag_skills.is_empty());
}

/// #1629 / #2455 — a Skyrim RACE DATA must not be read with the 36-byte
/// TES4 layout. #1629 achieved that by skipping the arm entirely; #2455
/// replaced the skip with the real TES5 decode, so the guarantee is now
/// "decoded with the *TES5* layout" rather than "left at defaults".
///
/// TES5 puts 7 skill pairs + 2 padding bytes where TES4 puts 8 pairs, which
/// shifts height/weight/flags by nothing but changes the pair count — so a
/// TES4 read of this fixture would produce a different, wrong bonus list.
#[test]
fn skyrim_race_data_uses_the_tes5_layout_not_tes4() {
    let mut data = vec![0u8; 128];
    // 7 pairs: one real bonus then Skill_None for the rest.
    data[0] = 0x07; // Two-Handed
    data[1] = 10;
    for slot in 1..7 {
        data[slot * 2] = 0xFF;
        data[slot * 2 + 1] = 0;
    }
    // Bytes 14..16 are TES5 padding — deliberately non-zero, so a TES4 read
    // (which would treat them as an eighth skill pair) is distinguishable.
    data[14] = 0xAB;
    data[15] = 0xCD;
    data[16..20].copy_from_slice(&1.03f32.to_le_bytes());
    data[20..24].copy_from_slice(&1.0f32.to_le_bytes());
    data[24..28].copy_from_slice(&0.9f32.to_le_bytes());
    data[28..32].copy_from_slice(&1.1f32.to_le_bytes());
    data[32..36].copy_from_slice(&0x50a0_8943u32.to_le_bytes());
    data[36..40].copy_from_slice(&50.0f32.to_le_bytes());
    data[40..44].copy_from_slice(&75.0f32.to_le_bytes());
    data[44..48].copy_from_slice(&100.0f32.to_le_bytes());

    let r = parse_race(0x900, &[sub(b"DATA", &data)], GameKind::Skyrim, &None);
    assert_eq!(
        r.skill_bonuses,
        vec![(0x07, 10)],
        "exactly the 7-pair TES5 array, Skill_None dropped — a TES4 read \
         would have consumed the padding as an eighth pair"
    );
    assert_eq!(r.base_height, (1.03, 1.0));
    assert_eq!(r.base_weight, (0.9, 1.1));
    assert_eq!(r.race_flags, 0x50a0_8943);
    assert_eq!(r.starting_health, Some(50.0));
    assert_eq!(r.starting_magicka, Some(75.0));
    assert_eq!(r.starting_stamina, Some(100.0));
}

/// #2455 — the real thing. Vanilla `Skyrim.esm` `NordRace` DATA, first 36
/// bytes verbatim (the record is 164 bytes; the tail is TES5-only fields no
/// consumer reads). The decoded bonuses must match Nord's documented racials,
/// which is what validates the *skill-index mapping*, not just the offsets.
#[test]
fn vanilla_skyrim_nordrace_data_decodes_to_its_documented_racials() {
    let mut data = vec![0u8; 164];
    data[..36].copy_from_slice(&[
        0x07, 0x0a, 0x06, 0x05, 0x09, 0x05, 0x0a, 0x05, 0x11, 0x05, 0x0c, 0x05, 0xff, 0x00, 0x00,
        0x00, 0x0a, 0xd7, 0x83, 0x3f, 0x0a, 0xd7, 0x83, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00,
        0x80, 0x3f, 0x43, 0x89, 0xa0, 0x50,
    ]);

    let r = parse_race(0x13746, &[sub(b"DATA", &data)], GameKind::Skyrim, &None);

    // Nord: Two-Handed +10; One-Handed, Block, Smithing, Speech, Light
    // Armor +5 each. Skyrim actor-value skill indices, not TES4's.
    assert_eq!(
        r.skill_bonuses,
        vec![(7, 10), (6, 5), (9, 5), (10, 5), (17, 5), (12, 5)],
        "decoded bonuses must match vanilla Nord's racial skill list"
    );
    assert!(
        (r.base_height.0 - 1.03).abs() < 1e-6,
        "Nord male height 1.03"
    );
    assert!((r.base_height.1 - 1.03).abs() < 1e-6);
    assert_eq!(r.base_weight, (1.0, 1.0));
    assert_eq!(r.race_flags, 0x50a0_8943);
    assert!(r.race_flags & 1 == 1, "Nord is a playable race");
}

/// #2455 — vanilla `Skyrim.esm` `ElderRace` grants no skill bonuses, so all
/// seven slots are the `0xFF` Skill_None sentinel. Distinguishes "decoded and
/// genuinely empty" from "never decoded", which the old default-asserting
/// test could not.
#[test]
fn vanilla_skyrim_elderrace_decodes_to_no_skill_bonuses() {
    let mut data = vec![0u8; 164];
    for slot in 0..7 {
        data[slot * 2] = 0xFF;
    }
    data[16..20].copy_from_slice(&1.0f32.to_le_bytes());
    data[20..24].copy_from_slice(&1.0f32.to_le_bytes());

    let r = parse_race(0x13744, &[sub(b"DATA", &data)], GameKind::Skyrim, &None);
    assert!(r.skill_bonuses.is_empty());
    assert_eq!(r.base_height, (1.0, 1.0), "height still decodes");
}

/// #2455 — FO4 / FO76 use a *third* layout: floats from offset 0 and no
/// skill array at all (neither game has skills). Fixture bytes are the first
/// 36 of vanilla `Fallout4.esm`'s `HumanRace` and `HumanChildRace`.
///
/// The child race is the field-identifying evidence: it is the one that must
/// come out shorter. Reading these bytes with the TES5 layout would instead
/// yield the bogus pairs `(0,0) (128,63) …` and a height of 0.5.
#[test]
fn fallout4_race_data_decodes_height_from_offset_zero() {
    let human: [u8; 36] = [
        0x00, 0x00, 0x80, 0x3f, 0x48, 0xe1, 0x7a, 0x3f, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x00,
        0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00,
        0x00, 0x00, 0x43, 0x89, 0xa0, 0x50,
    ];
    let child: [u8; 36] = [
        0x33, 0x33, 0x53, 0x3f, 0x33, 0x33, 0x53, 0x3f, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x00,
        0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x00, 0x3f, 0x00, 0x00,
        0x00, 0x00, 0x47, 0x89, 0x20, 0x50,
    ];

    let mut data = vec![0u8; 200];
    data[..36].copy_from_slice(&human);
    let r = parse_race(0x13746, &[sub(b"DATA", &data)], GameKind::Fallout4, &None);
    assert!((r.base_height.0 - 1.0).abs() < 1e-6);
    assert!((r.base_height.1 - 0.98).abs() < 1e-6);

    let mut data = vec![0u8; 200];
    data[..36].copy_from_slice(&child);
    let child_race = parse_race(0x1, &[sub(b"DATA", &data)], GameKind::Fallout4, &None);
    assert!(
        (child_race.base_height.0 - 0.825).abs() < 1e-6,
        "the child race must decode shorter than the adult — this is what \
         identifies offset 0 as height"
    );
    assert!(child_race.base_height.0 < r.base_height.0);

    // No skills in FO4, and the weight morph is deliberately not surfaced.
    assert!(child_race.skill_bonuses.is_empty());
    assert_eq!(child_race.base_weight, (1.0, 1.0));
    assert_eq!(child_race.race_flags, 0);
}

/// #2455 — FO76's 216-byte DATA shares FO4's leading-height shape.
#[test]
fn fallout76_race_data_shares_the_fallout4_height_offsets() {
    let mut data = vec![0u8; 216];
    data[0..4].copy_from_slice(&1.15f32.to_le_bytes());
    data[4..8].copy_from_slice(&0.95f32.to_le_bytes());
    let r = parse_race(0x2, &[sub(b"DATA", &data)], GameKind::Fallout76, &None);
    assert_eq!(r.base_height, (1.15, 0.95));
    assert!(r.skill_bonuses.is_empty());
}

/// #2455 — a shape no arm claims must not be silently swallowed. Starfield
/// ships no RACE DATA at all, so an unexpected one is worth a breadcrumb;
/// the record still parses and simply keeps its defaults.
#[test]
fn unhandled_race_data_shape_leaves_defaults_intact() {
    let data = vec![0x5Au8; 40];
    let r = parse_race(0x3, &[sub(b"DATA", &data)], GameKind::Starfield, &None);
    assert!(r.skill_bonuses.is_empty());
    assert_eq!(r.base_height, (1.0, 1.0));
    assert_eq!(r.base_weight, (1.0, 1.0));
    assert_eq!(r.race_flags, 0);
}

/// The Oblivion path still decodes its 36-byte DATA (guard preserves
/// existing behaviour for the TES4/FO3/FNV era).
#[test]
fn oblivion_race_data_36b_still_decodes() {
    let mut data = vec![0u8; 36];
    // base_height/weight live at bytes 16..32 (4 f32). Set height male = 2.0.
    data[16..20].copy_from_slice(&2.0f32.to_le_bytes());
    let subs = vec![sub(b"DATA", &data)];
    let r = parse_race(0x901, &subs, GameKind::Oblivion, &None);
    assert_eq!(r.base_height.0, 2.0, "Oblivion DATA must still decode");
}

/// #2955 — `calcMin` must decode on every ACBS layout that carries it.
///
/// The field sits immediately after `level` in all four layouts and was
/// previously skipped. It is the level floor for PC-level-multiplier actors
/// (`ACBS_PC_LEVEL_MULT`), whose `level` field is a multiplier rather than a
/// level — so without `calcMin` there is no sourced level for 268 vanilla FNV
/// and ~190 FO3 base actors, and the consumer is left reading the multiplier.
///
/// Byte offsets are pinned per layout because the skip lengths around the read
/// are what a future edit gets wrong: shifting the cursor by two bytes decodes
/// `calcMax` (or half of `karma`) as the floor, silently.
#[test]
fn acbs_calc_min_decodes_on_every_layout() {
    // ── FNV / FO3: flags(4) fatigue(2) barter(2) level(2) calcMin@10 ──
    let mut fnv = Vec::new();
    fnv.extend_from_slice(&super::ACBS_PC_LEVEL_MULT.to_le_bytes()); // flags@0
    fnv.extend_from_slice(&[0u8; 4]); // fatigue + barter
    fnv.extend_from_slice(&1000i16.to_le_bytes()); // level@8 = the multiplier
    fnv.extend_from_slice(&6u16.to_le_bytes()); // calcMin@10
    fnv.extend_from_slice(&30u16.to_le_bytes()); // calcMax@12
    fnv.extend_from_slice(&[0u8; 10]); // speed_mult + karma → 24 bytes
    assert_eq!(fnv.len(), 24);
    let n = parse_npc(0x0001, &[sub(b"ACBS", &fnv)], GameKind::Fallout3NV, &None);
    assert_eq!(n.level, 1000, "the raw multiplier still decodes as-is");
    assert_eq!(n.calc_min, 6, "FNV calcMin@10 must decode");
    assert_ne!(
        n.calc_min, 30,
        "reading calcMax as calcMin is the two-byte cursor slip this pins"
    );
    assert!(
        n.acbs_flags & super::ACBS_PC_LEVEL_MULT != 0,
        "the multiplier bit must survive so consumers can gate on it"
    );

    // ── Oblivion: flags(4) baseSpell(2) fatigue(2) barter(2) level(2) calcMin@12 ──
    let mut obl = Vec::new();
    obl.extend_from_slice(&0u32.to_le_bytes());
    obl.extend_from_slice(&[0u8; 6]);
    obl.extend_from_slice(&6i16.to_le_bytes()); // level@10
    obl.extend_from_slice(&4u16.to_le_bytes()); // calcMin@12
    obl.extend_from_slice(&9u16.to_le_bytes()); // calcMax@14
    assert_eq!(obl.len(), 16);
    let n = parse_npc(0x0002, &[sub(b"ACBS", &obl)], GameKind::Oblivion, &None);
    assert_eq!(n.level, 6);
    assert_eq!(n.calc_min, 4, "Oblivion calcMin@12 must decode");

    // ── FO4: flags(4) xp_offset(2) level(2) calcMin@8 calcMax(2) disposition(2) ──
    let mut fo4 = Vec::new();
    fo4.extend_from_slice(&0u32.to_le_bytes());
    fo4.extend_from_slice(&0i16.to_le_bytes()); // xp offset
    fo4.extend_from_slice(&15i16.to_le_bytes()); // level@6
    fo4.extend_from_slice(&11u16.to_le_bytes()); // calcMin@8
    fo4.extend_from_slice(&40u16.to_le_bytes()); // calcMax@10
    fo4.extend_from_slice(&50i16.to_le_bytes()); // disposition@12
    fo4.extend_from_slice(&[0u8; 6]); // template_flags + bleedout + unknown
    assert_eq!(fo4.len(), 20);
    let n = parse_npc(0x0003, &[sub(b"ACBS", &fo4)], GameKind::Fallout4, &None);
    assert_eq!(n.level, 15);
    assert_eq!(n.calc_min, 11, "FO4 calcMin@8 must decode");
    assert_eq!(
        n.disposition_base, 50,
        "the fields AFTER calcMin must not shift — the skip length was \
         narrowed from 4 to 2 when calcMin started being read"
    );
}

/// #3325 — `WMI1` is the FACT → REPU edge, and it was dropped everywhere.
/// Without it `EsmIndex::reputations` is an orphan map: 13 parsed `REPU`
/// records with nothing able to say which faction moves which meter, so no
/// reputation runtime can be built on the index no matter what lands on top.
///
/// The FormID must arrive in **global** load-order space, like every other
/// embedded FormID, or `index.reputations` lookups miss on any multi-plugin
/// load — the exact failure #1996 fixed for NPC_.
#[test]
fn fact_wmi1_binds_the_faction_to_its_reputation_record() {
    let subs = vec![
        sub(b"EDID", b"GoodspringsFaction\0"),
        sub(b"WMI1", &0x000F_43DEu32.to_le_bytes()),
    ];
    let f = parse_fact(0x0010_4C6E, &subs, &None);
    assert_eq!(
        f.reputation,
        Some(0x000F_43DE),
        "WMI1 must land on FactionRecord::reputation"
    );

    // Plugin slot 2, one master at slot 0 — same fixture shape as
    // `npc_embedded_form_ids_remap_to_global_space`.
    let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
    let self_ref = (1u32 << 24) | 0x0000_43DE;
    let subs = vec![
        sub(b"EDID", b"OverridePluginFaction\0"),
        sub(b"WMI1", &self_ref.to_le_bytes()),
    ];
    let f = parse_fact(0x000A_0002, &subs, &Some(remap));
    assert_eq!(
        f.reputation,
        Some((2u32 << 24) | 0x0000_43DE),
        "a self-referential WMI1 must resolve to the plugin's own global slot"
    );
}

/// A faction without `WMI1` carries no reputation — `Some(0)` would be a
/// null FormID masquerading as a binding, and every `index.reputations`
/// lookup for it would miss confusingly rather than obviously.
#[test]
fn fact_without_wmi1_has_no_reputation_binding() {
    let subs = vec![sub(b"EDID", b"PlainFaction\0")];
    assert_eq!(parse_fact(0x43, &subs, &None).reputation, None);

    let subs = vec![
        sub(b"EDID", b"NullWmi1Faction\0"),
        sub(b"WMI1", &0u32.to_le_bytes()),
    ];
    assert_eq!(
        parse_fact(0x44, &subs, &None).reputation,
        None,
        "a null WMI1 payload is 'no binding', not a binding to FormID 0"
    );
}

// ── RACE head-part section gating (#3419) + per-game index table (#3420) ──

/// #3419 (FNV-2026-08-27-D4-03). FO3 / FNV RACE records re-use `INDX`
/// — with its own 0..3 numbering and its own `MNAM` / `FNAM` markers —
/// for the *body* section opened by `NAM1`. Pre-fix the accumulator
/// pushed every `INDX`+`MODL` pair into `head_parts`, so
/// `UpperBody.nif` landed under the Head role, `RightHand.nif` under
/// FNV's Mouth, and an `.egt` texture path under Teeth (lower).
/// Layout mirrors `CaucasianOldAged` (`000987DF`).
#[test]
fn race_head_parts_stop_at_the_body_section_marker() {
    let subs = vec![
        sub(b"EDID", b"CaucasianOldAged\0"),
        sub(b"NAM0", b""),
        sub(b"MNAM", b""),
        sub(b"INDX", &0u32.to_le_bytes()),
        sub(b"MODL", b"Characters\\Head\\HeadOld.NIF\0"),
        sub(b"INDX", &6u32.to_le_bytes()),
        sub(b"MODL", b"Characters\\Head\\EyeLeftHuman.NIF\0"),
        sub(b"FNAM", b""),
        sub(b"INDX", &0u32.to_le_bytes()),
        sub(b"MODL", b"Characters\\Head\\HeadOldFemale.NIF\0"),
        sub(b"NAM1", b""),
        sub(b"MNAM", b""),
        sub(b"INDX", &0u32.to_le_bytes()),
        sub(b"MODL", b"characters\\_Male\\UpperBody.nif\0"),
        sub(b"INDX", &2u32.to_le_bytes()),
        sub(b"MODL", b"characters\\_Male\\RightHand.nif\0"),
        sub(b"INDX", &3u32.to_le_bytes()),
        sub(b"MODL", b"Characters\\_Male\\UpperBodyHumanMale.egt\0"),
    ];
    let race = parse_race(0x000987DF, &subs, GameKind::Fallout3NV, &None);

    assert_eq!(
        race.head_parts,
        vec![
            (0, "Characters\\Head\\HeadOld.NIF".to_string(), Some(0)),
            (6, "Characters\\Head\\EyeLeftHuman.NIF".to_string(), Some(0)),
            (
                0,
                "Characters\\Head\\HeadOldFemale.NIF".to_string(),
                Some(1)
            ),
        ],
        "only the NAM0 head section may reach head_parts (#3419)",
    );
    assert_eq!(
        race.body_models.len(),
        6,
        "body_models stays the flat every-MODL list it has always been",
    );
}

/// Oblivion authors one ungendered head run (`NAM0` with no MNAM /
/// FNAM at all) and a body section whose `INDX` entries carry only
/// `ICON`. The section gate must leave that arm's entries untagged —
/// the spawner's `section.is_none_or(...)` rule depends on it.
#[test]
fn race_oblivion_head_section_stays_untagged() {
    let subs = vec![
        sub(b"EDID", b"Imperial\0"),
        sub(b"NAM0", b""),
        sub(b"INDX", &0u32.to_le_bytes()),
        sub(b"MODL", b"Characters\\Imperial\\HeadHuman.nif\0"),
        sub(b"INDX", &7u32.to_le_bytes()),
        sub(b"MODL", b"Characters\\Imperial\\EyeLeftHuman.nif\0"),
        sub(b"NAM1", b""),
        sub(b"MNAM", b""),
        sub(b"INDX", &0u32.to_le_bytes()),
        sub(b"ICON", b"Characters\\Imperial\\UpperBody.dds\0"),
    ];
    let race = parse_race(0x00000907, &subs, GameKind::Oblivion, &None);

    assert_eq!(
        race.head_parts,
        vec![
            (0, "Characters\\Imperial\\HeadHuman.nif".to_string(), None),
            (
                7,
                "Characters\\Imperial\\EyeLeftHuman.nif".to_string(),
                None
            ),
        ],
    );
}

/// #3420. The raw `INDX` number is not portable: Oblivion carries a
/// second ear slot that FO3 / FNV drop, shifting every role below it.
/// Hard-coding the Fallout eye pair (6 / 7) selected the *tongue* and
/// the *left* eye on Oblivion.
#[test]
fn head_part_index_table_differs_per_game() {
    use head_part::{index_of, Role};

    assert_eq!(index_of(GameKind::Oblivion, Role::Head), Some(0));
    assert_eq!(index_of(GameKind::Oblivion, Role::Mouth), Some(3));
    assert_eq!(index_of(GameKind::Oblivion, Role::LeftEye), Some(7));
    assert_eq!(index_of(GameKind::Oblivion, Role::RightEye), Some(8));

    assert_eq!(index_of(GameKind::Fallout3NV, Role::Head), Some(0));
    assert_eq!(index_of(GameKind::Fallout3NV, Role::Mouth), Some(2));
    assert_eq!(index_of(GameKind::Fallout3NV, Role::LeftEye), Some(6));
    assert_eq!(index_of(GameKind::Fallout3NV, Role::RightEye), Some(7));
    assert_eq!(
        index_of(GameKind::Fallout3NV, Role::EarMale),
        index_of(GameKind::Fallout3NV, Role::EarFemale),
        "FNV splits ears by MNAM/FNAM section, not by index",
    );

    // Skyrim+ moved head parts out to standalone HDPT records.
    assert_eq!(index_of(GameKind::Skyrim, Role::Head), None);
    assert_eq!(index_of(GameKind::Starfield, Role::LeftEye), None);
}

/// Real-data pin for #3418 + #3419 over every `FalloutNV.esm` RACE:
/// no head-part entry may be a body mesh or an `.egt` texture, and the
/// `Head` role must resolve to a *different* mesh per gender (all 22
/// vanilla races author a distinct female head, which pre-#3418 no
/// female NPC ever received).
#[test]
#[ignore = "needs FNV game data on disk"]
fn parse_real_fnv_race_head_parts_exclude_the_body_section() {
    let path = crate::esm::test_paths::fnv_esm();
    if !path.exists() {
        eprintln!("Skipping: FalloutNV.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let index = crate::esm::records::parse_esm(&data).expect("parse_esm");
    assert!(!index.races.is_empty(), "FNV must ship RACE records");

    let head_idx = head_part::index_of(GameKind::Fallout3NV, head_part::Role::Head).unwrap();
    let mut gendered_heads = 0usize;
    for race in index.races.values() {
        for (idx, path, section) in &race.head_parts {
            let lower = path.to_ascii_lowercase();
            assert!(
                !lower.ends_with(".egt"),
                "RACE {:08X} head part {idx} is an .egt texture: {path} (#3419)",
                race.form_id,
            );
            assert!(
                !lower.starts_with("characters\\_male\\"),
                "RACE {:08X} head part {idx} is a body mesh: {path} (#3419)",
                race.form_id,
            );
            let _ = section;
        }
        let head_for = |tag: u8| {
            race.head_parts
                .iter()
                .find(|(idx, path, section)| {
                    *idx == head_idx && !path.is_empty() && section.is_none_or(|s| s == tag)
                })
                .map(|(_, path, _)| path.to_ascii_lowercase())
        };
        if let (Some(male), Some(female)) = (head_for(0), head_for(1)) {
            if male != female {
                gendered_heads += 1;
            }
        }
    }
    assert!(
        gendered_heads >= 20,
        "FNV authors a distinct female head on ~all races; found {gendered_heads} (#3418)",
    );
}

/// Real-data pin for #3420's Oblivion arm: `Imperial` (`00000907`)
/// authors the nine-slot head table, so the eyes sit at 7 / 8 and the
/// tongue — not an eye — sits at 6.
#[test]
#[ignore = "needs Oblivion game data on disk"]
fn parse_real_oblivion_race_head_part_indices() {
    let path = crate::esm::test_paths::oblivion_esm();
    if !path.exists() {
        eprintln!("Skipping: Oblivion.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let index = crate::esm::records::parse_esm(&data).expect("parse_esm");
    let race = index
        .races
        .get(&0x0000_0907)
        .expect("Oblivion.esm must ship Imperial 00000907");

    let path_at = |role| {
        let want = head_part::index_of(GameKind::Oblivion, role).unwrap();
        race.head_parts
            .iter()
            .find(|(idx, _, _)| *idx == want)
            .map(|(_, path, _)| path.to_ascii_lowercase())
            .unwrap_or_default()
    };
    assert!(path_at(head_part::Role::Head).ends_with("headhuman.nif"));
    assert!(path_at(head_part::Role::Mouth).ends_with("mouthhuman.nif"));
    assert!(path_at(head_part::Role::Tongue).ends_with("tonguehuman.nif"));
    assert!(path_at(head_part::Role::LeftEye).ends_with("eyelefthuman.nif"));
    assert!(path_at(head_part::Role::RightEye).ends_with("eyerighthuman.nif"));
}

// ── CREA DATA + creature actor values (#3390) ─────────────────────────

/// Wire-level pin for the sourced `CREA` `DATA` layout — xEdit
/// `Core/wbDefinitionsFNV.pas` `wbRecord(CREA, …) wbStruct(DATA, …)`,
/// byte-identical in `wbDefinitionsFO3.pas`. Payload is the real
/// `FalloutNV.esm` `VCrTier3GiantRadscorpionMedPers` (`00167EA7`) block.
#[test]
fn crea_data_decodes_the_sourced_seventeen_byte_layout() {
    let data: [u8; 17] = [
        0x02, // Type — Mutated Insect
        0x41, // Combat Skill 65
        0x32, // Magic Skill 50
        0x32, // Stealth Skill 50
        0x96, 0x00, // Health 150 (i16)
        0x00, 0x00, // unused
        0x3C, 0x00, // Damage 60 (i16)
        0x09, 0x06, 0x06, 0x06, 0x05, 0x03, 0x08, // S P E C I A L
    ];
    let subs = vec![
        sub(b"EDID", b"VCrTier3GiantRadscorpionMedPers\0"),
        sub(b"DATA", &data),
    ];
    let crea = parse_npc(0x0016_7EA7, &subs, GameKind::Fallout3NV, &None);

    assert_eq!(
        crea.creature_stats,
        Some(CreatureStats {
            creature_type: 2,
            combat_skill: 65,
            magic_skill: 50,
            stealth_skill: 50,
            health: 150,
            damage: 60,
            attributes: [9, 6, 6, 6, 5, 3, 8],
        }),
    );
}

/// `parse_npc` is shared by `NPC_` and `CREA` and does not know which
/// group it read, so the creature arm keys on the exact 17-byte length.
/// FNV `NPC_` `DATA` is a different struct — `i32` Base Health + the same
/// 7 attributes, 11 bytes (25 with the legacy unused tail) — and must not
/// be decoded as a creature block.
#[test]
fn npc_data_is_not_mistaken_for_a_creature_stat_block() {
    let mut npc_data = Vec::new();
    npc_data.extend_from_slice(&250i32.to_le_bytes()); // Base Health
    npc_data.extend_from_slice(&[5, 5, 5, 5, 5, 5, 5]); // Attributes
    assert_eq!(npc_data.len(), 11);
    let npc = parse_npc(
        0x0000_0001,
        &[sub(b"DATA", &npc_data)],
        GameKind::Fallout3NV,
        &None,
    );
    assert_eq!(npc.creature_stats, None);

    // The legacy long form.
    npc_data.extend_from_slice(&[0u8; 14]);
    assert_eq!(npc_data.len(), 25);
    let legacy = parse_npc(
        0x0000_0002,
        &[sub(b"DATA", &npc_data)],
        GameKind::Fallout3NV,
        &None,
    );
    assert_eq!(legacy.creature_stats, None);
}

/// Skyrim onward folded creatures into `NPC_` and author no `CREA` at
/// all; a 17-byte `DATA` on another game must not reach this decoder.
#[test]
fn crea_data_arm_is_gated_to_the_fallout3_fnv_era() {
    let data = [0u8; 17];
    for game in [
        GameKind::Oblivion,
        GameKind::Skyrim,
        GameKind::Fallout4,
        GameKind::Starfield,
    ] {
        let record = parse_npc(0x0000_0003, &[sub(b"DATA", &data)], game, &None);
        assert_eq!(
            record.creature_stats, None,
            "{game:?} must not decode a CREA stat block",
        );
    }
}

/// Real-data guard for #3390 over every `FalloutNV.esm` `CREA`: the whole
/// bestiary must decode a stat block and derive actor values from it.
///
/// Pre-fix the count on both halves was zero — `CREA` `DATA` was never
/// parsed and the auto-calc arm's class lookup could not hit, so all 1 578
/// creatures spawned with no `ActorValues`, hence no `ActorVitals`, hence
/// untargetable by the P2 melee slice. Spot-checks are vanilla values.
#[test]
#[ignore = "needs FNV game data on disk"]
fn parse_real_fnv_creatures_derive_actor_values() {
    let path = crate::esm::test_paths::fnv_esm();
    if !path.exists() {
        eprintln!("Skipping: FalloutNV.esm not found at {}", path.display());
        return;
    }
    let data = std::fs::read(&path).unwrap();
    let index = crate::esm::records::parse_esm(&data).expect("parse_esm");
    assert!(!index.creatures.is_empty(), "FNV must ship CREA records");

    let health_key = index
        .actor_value_form_id("Health")
        .expect("FNV must author a Health AVIF");
    let mut without_data = Vec::new();
    let mut derived = 0usize;
    let mut with_health = 0usize;
    for crea in index.creatures.values() {
        if crea.creature_stats.is_none() {
            without_data.push(crea.form_id);
        }
        let pairs = crate::esm::records::derive_npc_actor_values(crea, &index);
        if !pairs.is_empty() {
            derived += 1;
        }
        if pairs.iter().any(|(k, _)| *k == health_key) {
            with_health += 1;
        }
    }
    assert!(
        without_data.is_empty(),
        "every FNV CREA authors a 17-byte DATA; {} did not: {:08X?}",
        without_data.len(),
        &without_data[..without_data.len().min(8)],
    );
    assert_eq!(
        derived,
        index.creatures.len(),
        "every creature must derive a non-empty actor-value set",
    );
    // The remainder are `*DEAD` corpse props authored at health 0, which
    // correctly stay without `ActorVitals`.
    assert!(
        with_health * 100 / index.creatures.len() >= 85,
        "only {with_health}/{} creatures derived Health",
        index.creatures.len(),
    );

    let by_edid = |edid: &str| {
        index
            .creatures
            .values()
            .find(|c| c.editor_id == edid)
            .unwrap_or_else(|| panic!("{edid} missing"))
    };
    let health_of = |crea: &NpcRecord| {
        crate::esm::records::derive_npc_actor_values(crea, &index)
            .iter()
            .find(|(k, _)| *k == health_key)
            .map(|(_, v)| *v)
    };
    assert_eq!(health_of(by_edid("VCrDeathclawTier1TypeA")), Some(250.0));
    assert_eq!(health_of(by_edid("VCrTier1RadroachMed")), Some(12.0));
}
