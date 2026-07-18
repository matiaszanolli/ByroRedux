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
        assert_eq!(n.level, 6, "Oblivion ACBS level @10 must decode (not default 1)");
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
        assert_eq!(n.level, 1, "16-byte ACBS must not parse under FNV (stays default)");
        assert_eq!(n.acbs_flags, 0);
    }

    /// Regression for #1273 — `SCRI` attached-script FormID on NPC_
    /// and CREA records was silently dropped. 24 % of FO3 named NPCs
    /// + 27 % of FO3 creatures author SCRI; FNV similar. The audit
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
        let f = parse_fact(0x42, &subs);
        assert_eq!(f.editor_id, "NCR");
        assert_eq!(f.flags, 0x01);
        assert_eq!(f.relations.len(), 1);
        assert_eq!(f.relations[0].other_faction, 0x123);
        assert_eq!(f.relations[0].modifier, -50);
        assert_eq!(f.relations[0].combat_reaction, 1);
        assert_eq!(f.ranks, vec!["Recruit", "Trooper", "Veteran"]);
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
        let f = parse_fact(0x77, &subs);
        assert_eq!(f.relations.len(), 1);
        assert_eq!(
            f.relations[0].combat_reaction, 2,
            "ally (combat_reaction=2) must round-trip — parser must read 4 bytes"
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
        let f = parse_fact(0x88, &subs);
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
        let f = parse_fact(0x89, &subs);
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
    /// override-plugin NPC — e.g. `active_package_is_sandbox` always
    /// returning `false` for that NPC's sandbox packages.
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
            n.race_form_id,
            master_ref,
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
             this is the exact field `active_package_is_sandbox` looks up \
             via `index.packages.get(pk)`"
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
        let recipe = n.runtime_facegen.expect("HNAM/ENAM/PNAM populate the recipe");

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
        let face = n.face_morphs.expect("FMRI/HCLF/BCLF/PNAM populate face_morphs");

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
        let r = parse_race(0x10001, &subs, GameKind::Oblivion);
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
        let r = parse_race(0x10002, &subs, GameKind::Oblivion);
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
        let r = parse_race(0x10003, &subs, GameKind::Fallout3NV);
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
        let r = parse_race(0x10004, &subs, GameKind::Oblivion);
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
        let r = parse_race(0x10005, &subs, GameKind::Skyrim);
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
            let r = parse_race(0x10006, &subs, game);
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

    /// #1629 — a 128-byte Skyrim RACE DATA must NOT be decoded with the
    /// 36-byte TES4 layout. With the GameKind guard the arm is skipped, so
    /// skill_bonuses / base_height / base_weight / race_flags stay at their
    /// RaceRecord defaults instead of garbage from a mis-applied layout.
    #[test]
    fn skyrim_race_data_128b_is_not_decoded_as_tes4_layout() {
        // Non-zero bytes so a TES4 decode WOULD produce visible garbage.
        let data: Vec<u8> = (0..128).map(|i| (i as u8).wrapping_add(1)).collect();
        let subs = vec![sub(b"DATA", &data)];
        let r = parse_race(0x900, &subs, GameKind::Skyrim);
        assert!(
            r.skill_bonuses.is_empty(),
            "Skyrim 128-byte DATA must not yield TES4-layout skill bonuses"
        );
        assert_eq!(r.base_height, (1.0, 1.0), "height must stay default");
        assert_eq!(r.base_weight, (1.0, 1.0), "weight must stay default");
        assert_eq!(r.race_flags, 0, "flags must stay default");
    }

    /// The Oblivion path still decodes its 36-byte DATA (guard preserves
    /// existing behaviour for the TES4/FO3/FNV era).
    #[test]
    fn oblivion_race_data_36b_still_decodes() {
        let mut data = vec![0u8; 36];
        // base_height/weight live at bytes 16..32 (4 f32). Set height male = 2.0.
        data[16..20].copy_from_slice(&2.0f32.to_le_bytes());
        let subs = vec![sub(b"DATA", &data)];
        let r = parse_race(0x901, &subs, GameKind::Oblivion);
        assert_eq!(r.base_height.0, 2.0, "Oblivion DATA must still decode");
    }
