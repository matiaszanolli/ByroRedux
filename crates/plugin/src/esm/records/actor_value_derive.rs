//! NPC actor-value population (#1663 FNV/FO3 + CHARAL FO4).
//!
//! Produces the `(canonical actor-value key, value)` pairs that seed an `ActorValues`
//! component the `GetActorValue` condition reads. Two per-game mechanisms
//! converge on the same output shape:
//!
//! - **FNV / FO3 (auto-calc):** computes SPECIAL + base skills and Health
//!   from the NPC's class/level, per the documented GECK auto-calculate
//!   model, and resolves each to its `AVIF` FormID (the original #1663 path,
//!   below).
//! - **Skyrim (race + offset):** seeds Health from the race's starting
//!   Health plus the NPC's signed TES5 ACBS Health offset.
//! - **FO4+ (stored):** FO4 stores actor values rather than deriving them
//!   — the `NPC_` `PRPS` property array already *is* `(AVIF FormID, value)`
//!   pairs, with baked `Calculated Health`/`Action Points` in `DNAM`. The
//!   stored path returns those verbatim (see CHARAL §"NPC SPECIAL storage",
//!   `docs/engine/charal-fo4-ruleset.md`).
//!
//! ## Model (cited)
//!
//! - **SPECIAL**: an auto-calc NPC adopts its **class's base attributes**
//!   as its SPECIAL (geckwiki *Stats Tab - NPC* / *Class*). Those are the
//!   7 bytes of the class's `ATTR` subrecord (fopdoc `CLAS`), Str → Luck.
//! - **Skill base**: `skill = fAVDSkill<name>Base + governing_SPECIAL ×
//!   fAVDSkillPrimaryBonusMult + ceil(Luck × fAVDSkillLuckBonusMult)`
//!   (geckwiki *Derived Skill Settings*). Defaults: base **2**, primary
//!   mult **2**, luck mult **0.5**. Worked example (geckwiki): END 5 +
//!   Luck 5 → Unarmed = 2 + 5×2 + ceil(5×0.5) = 15.
//! - **Governing SPECIAL per skill**: fallout.fandom *New Vegas SPECIAL* /
//!   geckwiki *SPECIAL*. FO3 and FNV use distinct canonical 13-skill rosters;
//!   FNV's displayed Guns/Survival retain the `AVSmallGuns`/`AVThrowing`
//!   record identities.
//! - **Health**: FO3 `90 + 20·END + 10·Level`; FNV
//!   `100 + 20·END + 5·(Level−1)`, from the locked CHARAL ruleset capture.
//!
//! ## Deferred (intentionally, not guessed)
//!
//! - **Tag-skill bonus + per-level growth.** The 3 class tag skills get a
//!   flat `fAVDTagSkillBonus` (+15) and an Intelligence-scaled per-level
//!   bonus; the exact per-level formula is **not published anywhere
//!   citable** (geckwiki / Fallout wikis describe it only qualitatively),
//!   so it is left out rather than fabricated. The base values below are
//!   correct; tag skills will read a few points low until the level model
//!   is pinned against the engine.
//! - **Non-auto-calc NPCs — ~40 % of every FO3/FNV actor, not a tail
//!   (#2957).** The discriminator is `ACBS` flag bit 4 (`0x0010`,
//!   "Auto-calculate stats"): when it is *clear* the NPC stores its own
//!   SPECIAL and skills in the `NPC_` record's DNAM-era layout, and those
//!   authored values — not the class averages — are what the game reads.
//!   [`derive_autocalc_actor_values`] never consults `acbs_flags`; it goes
//!   straight to the class for every FO3/FNV actor.
//!
//!   Censused over the vanilla masters (`cargo run -p byroredux-plugin
//!   --example autocalc_census -- FalloutNV.esm Fallout3.esm`):
//!
//!   |                  |    FNV    |    FO3    |
//!   |------------------|-----------|-----------|
//!   | `NPC_` records   |   3816    |   1647    |
//!   | auto-calc **ON** | 2283 (59.8 %) | 935 (56.8 %) |
//!   | auto-calc **OFF**| **1533 (40.2 %)** | **712 (43.2 %)** |
//!
//!   So this is a bare-majority-correct path, not a near-universal one: ~1500
//!   FNV and ~700 FO3 actors — disproportionately the hand-authored named
//!   NPCs that quests and dialogue conditions target — currently receive
//!   class-averaged stats instead of their authored ones.
//!
//!   The deferral stands because the stored values are not merely unread,
//!   they are **unparsed**: the FO3/FNV `NPC_` DNAM skill/SPECIAL block has
//!   no parser arm at all (the only actor-value-capturing `b"DNAM"` arm is
//!   gated on `GameKind::uses_actor_value_properties`, i.e. FO4+). That parse
//!   is `/audit-esm` Dimension 4 work — `NPC_` record decoding, not CHARAL.
//!   Once it lands, [`derive_npc_actor_values`] gains a third arm gated on
//!   this bit, in the same resolve-or-skip shape the FO4 and Skyrim arms
//!   already use.
//! - **Derived attributes** beyond FO3/FNV/Skyrim Health and FO4's baked
//!   Health / Action Points (Carry Weight, regeneration, …).
//!
//! ## FormID space
//!
//! `index.classes` and `index.actor_values` are keyed in global load-order
//! space; the returned `AVIF` FormIDs are too (the same space a remapped
//! CTDA `param_1` / `GetActorValue` compares against). `NpcRecord.
//! class_form_id` (CNAM) is remapped to global load-order space at parse
//! time (`parse_npc`'s `remap` param — see #1996), so the `index.classes`
//! lookup is exact on multi-plugin loads too.

use super::actor::{NpcRecord, ACBS_PC_LEVEL_MULT};
use super::index::EsmIndex;
use byroredux_core::character::{Attribute, AttributeSet, CharacterRulesProfile, NpcStatModel};

/// The governing SPECIAL's index into `class.base_attributes` for a canonical
/// [`Attribute`]. `None` for a non-SPECIAL attribute (never happens for a
/// Fallout skill governor).
///
/// #2934 — reads the roster from [`AttributeSet::FALLOUT`] rather than a local
/// copy. The array that used to live here duplicated CHARAL's attribute roster
/// in a second crate, so a canonical-order change would have desynced silently:
/// this is a *positional* lookup into `ClassRecord::base_attributes` (ATTR
/// order), and a mismatched index reads the wrong attribute for every
/// auto-calc skill. `fallout_roster_matches_attr_order` pins the two orders
/// against each other.
fn special_index(attr: Attribute) -> Option<usize> {
    AttributeSet::FALLOUT
        .members()
        .iter()
        .position(|a| *a == attr)
}

// Derived-skill game-setting defaults (geckwiki Derived Skill Settings).
//
// #2934 — DOCTRINE NOTE. These three coefficients are a per-game *rule*, and
// CHARAL's stated shape puts rules on `CharacterRuleset` (the spec's
// `skill_calc: SkillDerivation { base, attr_mult, luck_mult }` field), not in
// a consumer crate. They live here because nothing populates that field yet —
// it does not exist workspace-wide. Moving them is deliberately paired with
// sourcing them from GMSTs (#2942): the values would then have one place to be
// read into, and `derive_npc_actor_values` would take the ruleset rather than
// re-deriving the rule. Until then this is a known, recorded deviation rather
// than a silent one.
const SKILL_BASE: f32 = 2.0; // fAVDSkill<name>Base
const SKILL_ATTR_MULT: f32 = 2.0; // fAVDSkillPrimaryBonusMult
const SKILL_LUCK_MULT: f32 = 0.5; // fAVDSkillLuckBonusMult

/// `skill = 2 + 2 × governing + ceil(Luck × 0.5)`.
fn base_skill(governing: u8, luck: u8) -> f32 {
    SKILL_BASE + SKILL_ATTR_MULT * f32::from(governing) + (SKILL_LUCK_MULT * f32::from(luck)).ceil()
}

/// Derive an NPC's `(AVIF FormID, value)` actor-value pairs for the
/// [`ActorValues::from_pairs`] population. The index's canonical character
/// profile selects the mechanism:
///
/// - **FO4+** (`uses_actor_value_properties`): actor values are *stored*,
///   not derived — the `PRPS` property pairs are the SPECIAL + overrides
///   verbatim, plus the baked `DNAM` Calculated Health / Action Points.
///   See [`derive_stored_actor_values`].
/// - **Skyrim**: Health, Magicka, and Stamina are their respective
///   `RACE.DATA` starting values plus signed `NPC_.ACBS` offsets, resolved
///   through the load order's canonical AVIF records.
/// - **FNV / FO3**: actor values are *auto-calculated* from the NPC's class
///   base SPECIAL (the documented GECK model) — the 7 SPECIAL, the profile's
///   exact skill roster, and its sourced Health curve. See
///   [`derive_autocalc_actor_values`]. `TPLT`/`Use Stats` template
///   inheritance is resolved first (#2956,
///   [`crate::equip::resolve_inherited_stats`]) — a templated `Lvl*` shell's
///   own `class_form_id`/level are frequently not what the engine actually
///   uses.
///
/// Empty for every other game (Oblivion), a Skyrim NPC whose race has no
/// usable pool values, an FO4 NPC with no `PRPS`, or an FNV NPC whose class
/// wasn't parsed. Individual missing Skyrim AVIFs are skipped independently.
///
/// [`ActorValues::from_pairs`]: byroredux_core::ecs::components::ActorValues::from_pairs
pub fn derive_npc_actor_values(npc: &NpcRecord, index: &EsmIndex) -> Vec<(u32, f32)> {
    match index.character_rules.npc_stat_model() {
        NpcStatModel::Stored => derive_stored_actor_values(npc, index),
        NpcStatModel::RaceBaseOffsets => derive_skyrim_actor_values(npc, index),
        NpcStatModel::ClassAutoCalc { health } => {
            let stats_npc =
                crate::equip::resolve_inherited_stats(npc, effective_npc_level(npc) as i16, index);
            derive_autocalc_actor_values(stats_npc, index, index.character_rules, health)
        }
        NpcStatModel::None => Vec::new(),
    }
}

/// TES5 NPC resource pools are authored as race starting values plus signed
/// actor offsets. Each resolves independently through its authored AVIF.
fn derive_skyrim_actor_values(npc: &NpcRecord, index: &EsmIndex) -> Vec<(u32, f32)> {
    let Some(race) = index.races.get(&npc.race_form_id) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(3);
    for (name, starting, offset) in [
        ("Health", race.starting_health, npc.health_offset),
        ("Magicka", race.starting_magicka, npc.magicka_offset),
        ("Stamina", race.starting_stamina, npc.stamina_offset),
    ] {
        let Some((key, starting)) = index.actor_value_form_id(name).zip(starting) else {
            continue;
        };
        let value = starting + f32::from(offset);
        if value.is_finite() && value > 0.0 {
            out.push((key, value));
        }
    }
    out
}

/// FO4+ stored actor values: the `PRPS` `(AVIF FormID, value)` pairs
/// verbatim (SPECIAL + overrides — already in the right space and shape)
/// plus the baked `DNAM` derived stats resolved to their AVIF FormIDs. The
/// Health / Action Points lookups resolve-or-skip, matching the auto-calc
/// path's contract for an index missing an `AVIF`. One allocation; the
/// `PRPS` slice is `memcpy`'d, the ≤2 baked stats pushed.
fn derive_stored_actor_values(npc: &NpcRecord, index: &EsmIndex) -> Vec<(u32, f32)> {
    let mut out = Vec::with_capacity(npc.actor_value_props.len() + 2);
    out.extend_from_slice(&npc.actor_value_props);
    for (avif_editor_id, baked) in [
        ("Health", npc.calculated_health),
        ("ActionPoints", npc.calculated_action_points),
    ] {
        if baked > 0 {
            if let Some(fid) = index.actor_value_form_id(avif_editor_id) {
                out.push((fid, f32::from(baked)));
            }
        }
    }
    out
}

/// The authored level used by NPC auto-calc. PC-level-multiplier actors carry
/// a fixed-point multiplier in `level`, so use their authored `calcMin` floor
/// until the player-relative half of that model exists.
fn effective_npc_level(npc: &NpcRecord) -> u16 {
    if npc.acbs_flags & ACBS_PC_LEVEL_MULT != 0 {
        npc.calc_min.max(1)
    } else {
        npc.level.max(1) as u16
    }
}

/// FNV / FO3 auto-calc: SPECIAL = the NPC's class base attributes, skills
/// derived via the GECK formula (`base_skill`), and Health derived from the
/// locked per-game END + level curve. The #1663 reference path.
fn derive_autocalc_actor_values(
    npc: &NpcRecord,
    index: &EsmIndex,
    profile: CharacterRulesProfile,
    health_curve: byroredux_core::character::NpcHealthCurve,
) -> Vec<(u32, f32)> {
    let Some(class) = index.classes.get(&npc.class_form_id) else {
        return Vec::new();
    };
    let special = class.base_attributes;
    let luck = special[6];

    let skills = profile.skills();
    let roster = AttributeSet::FALLOUT.members();
    let mut out = Vec::with_capacity(roster.len() + skills.len() + 1);
    for (i, attr) in roster.iter().enumerate() {
        if let Some(fid) = index.actor_value_form_id(attr.editor_id()) {
            out.push((fid, f32::from(special[i])));
        }
    }
    // Governing SPECIAL per skill comes from the canonical CHARAL roster —
    // the single source (no local duplicate). Luck governs no skill, so every
    // Fallout skill maps to a SPECIAL index; the `and_then` is total in
    // practice and simply skips any future ungoverned entry.
    for skill in skills.skills() {
        let Some(gov) = skill.governing.and_then(special_index) else {
            continue;
        };
        if let Some(fid) = index.actor_value_form_id(skill.editor_id) {
            out.push((fid, base_skill(special[gov], luck)));
        }
    }
    if let Some(fid) = index.health_actor_value_key() {
        let endurance = f32::from(special[2]);
        let level = f32::from(effective_npc_level(npc));
        let health = health_curve.evaluate(endurance, level);
        out.push((fid, health));
    }
    out
}

#[cfg(test)]
mod tests {

    /// #2934 — `special_index` is a POSITIONAL lookup into
    /// `ClassRecord::base_attributes`, which is stored in ATTR order
    /// (S-P-E-C-I-A-L). It now reads that order from `AttributeSet::FALLOUT`
    /// instead of a local duplicate, so the two must stay identical: a reordered
    /// canonical roster would silently shift every governing-attribute lookup and
    /// mis-derive all 13 auto-calc skills, with no type error and no panic.
    #[test]
    fn fallout_roster_matches_attr_order() {
        let roster: Vec<&str> = AttributeSet::FALLOUT
            .members()
            .iter()
            .map(|a| a.editor_id())
            .collect();
        assert_eq!(
            roster,
            vec![
                "Strength",
                "Perception",
                "Endurance",
                "Charisma",
                "Intelligence",
                "Agility",
                "Luck",
            ],
            "AttributeSet::FALLOUT must stay in ATTR / ClassRecord::base_attributes \
         order — special_index() indexes base_attributes by position (#2934)"
        );
        // And the positional contract itself, spelled out.
        assert_eq!(special_index(Attribute::Strength), Some(0));
        assert_eq!(special_index(Attribute::Luck), Some(6));
        assert_eq!(
            special_index(Attribute::Willpower),
            None,
            "a non-Fallout attribute must not resolve to a SPECIAL slot"
        );
    }

    use super::*;
    use crate::esm::records::{AvifRecord, ClassRecord, RaceRecord};

    fn avif(form_id: u32, editor_id: &str) -> AvifRecord {
        AvifRecord {
            form_id,
            editor_id: editor_id.to_string(),
            ..Default::default()
        }
    }

    /// Build an index whose AVIF records cover the 7 SPECIAL + 13 FNV
    /// skills + Health, with deterministic FormIDs (0x100 + slot). The
    /// records use shipped-data `AV` prefixes and record identities rather
    /// than the canonical/display spellings consumed by CHARAL.
    fn fnv_index_with_class(class_form_id: u32, base: [u8; 7]) -> EsmIndex {
        let mut index = EsmIndex::default();
        index.character_rules = CharacterRulesProfile::FALLOUT_NEW_VEGAS;
        let roster: Vec<&str> = AttributeSet::FALLOUT
            .members()
            .iter()
            .map(|a| a.editor_id())
            .collect();
        for (fid, name) in (0x100u32..).zip(
            roster.iter().chain(
                [
                    "Barter",
                    "EnergyWeapons",
                    "Explosives",
                    "Lockpick",
                    "Medicine",
                    "MeleeWeapons",
                    "Repair",
                    "Science",
                    "SmallGuns",
                    "Sneak",
                    "Speech",
                    "Throwing",
                    "Unarmed",
                    "Health",
                ]
                .iter(),
            ),
        ) {
            index
                .actor_values
                .insert(fid, avif(fid, &format!("AV{name}")));
        }
        index.classes.insert(
            class_form_id,
            ClassRecord {
                form_id: class_form_id,
                base_attributes: base,
                ..Default::default()
            },
        );
        index
    }

    fn npc_with_class(class_form_id: u32) -> NpcRecord {
        NpcRecord {
            class_form_id,
            level: 1,
            ..Default::default()
        }
    }

    #[test]
    fn base_skill_matches_documented_example() {
        // geckwiki worked example: END 5, Luck 5 → 2 + 5*2 + ceil(2.5) = 15.
        assert_eq!(base_skill(5, 5), 15.0);
        // Luck rounds UP: Luck 7 → ceil(3.5) = 4 bonus.
        assert_eq!(base_skill(0, 7), 2.0 + 4.0);
        // Zero everything → the flat base of 2.
        assert_eq!(base_skill(0, 0), 2.0);
    }

    #[test]
    fn derives_special_and_skills_from_class() {
        // Str=5 Per=6 End=5 Cha=4 Int=7 Agi=6 Luck=5.
        let base = [5, 6, 5, 4, 7, 6, 5];
        let index = fnv_index_with_class(0x2000, base);
        let npc = npc_with_class(0x2000);
        let pairs = derive_npc_actor_values(&npc, &index);

        // Helper: value for a named AV via its resolved FormID.
        let val = |name: &str| -> f32 {
            let fid = index.actor_value_form_id(name).unwrap();
            pairs.iter().find(|(f, _)| *f == fid).unwrap().1
        };

        // SPECIAL copied straight through.
        assert_eq!(val("Strength"), 5.0);
        assert_eq!(val("Intelligence"), 7.0);
        assert_eq!(val("Luck"), 5.0);

        // Skills via 2 + 2*gov + ceil(Luck/2); Luck 5 → +3.
        assert_eq!(val("Unarmed"), 2.0 + 2.0 * 5.0 + 3.0, "END 5"); // 15
        assert_eq!(val("SmallGuns"), 2.0 + 2.0 * 6.0 + 3.0, "AGI 6"); // Guns = 17
        assert_eq!(val("Throwing"), 2.0 + 2.0 * 5.0 + 3.0, "END 5"); // Survival = 15
        assert_eq!(val("Science"), 2.0 + 2.0 * 7.0 + 3.0, "INT 7"); // 19
        assert_eq!(val("Barter"), 2.0 + 2.0 * 4.0 + 3.0, "CHA 4"); // 13
        assert_eq!(val("Health"), 200.0, "100 + 20·END at level 1");

        // 7 SPECIAL + 13 FNV skills + Health.
        assert_eq!(pairs.len(), 21);
    }

    /// #2956 — a templated `Lvl*` shell's own `class_form_id` must be
    /// IGNORED once `Use Stats` is set; the template's class (and level)
    /// govern the whole SPECIAL + skill derivation instead. Give the
    /// shell a class_form_id that doesn't even resolve in the index, so a
    /// pass-through bug would degrade to `empty_without_class_or_
    /// unsupported_game`'s empty-result path rather than silently
    /// producing plausible-looking wrong numbers — either way, a
    /// regression here is loud, not silent.
    #[test]
    fn derive_npc_actor_values_follows_use_stats_template_to_the_correct_class() {
        let template_base = [8, 6, 9, 4, 7, 5, 3]; // the class the engine actually uses
        let mut index = fnv_index_with_class(0x2000, template_base);

        let mut template_npc = npc_with_class(0x2000);
        template_npc.form_id = 0x0010_0001;
        template_npc.level = 20;
        index.npcs.insert(template_npc.form_id, template_npc);

        let mut shell = npc_with_class(0x9999); // unresolvable — proves it's never read
        shell.form_id = 0x0010_0000;
        shell.level = 1;
        shell.template_form_id = 0x0010_0001;
        shell.template_flags = crate::equip::TEMPLATE_FLAG_USE_STATS;

        let pairs = derive_npc_actor_values(&shell, &index);
        assert!(
            !pairs.is_empty(),
            "must resolve through the template, not empty out on the shell's own \
             unresolvable class"
        );

        let val = |name: &str| -> f32 {
            let fid = index.actor_value_form_id(name).unwrap();
            pairs.iter().find(|(f, _)| *f == fid).unwrap().1
        };
        assert_eq!(val("Strength"), 8.0, "template's SPECIAL, not the shell's");
        assert_eq!(val("Endurance"), 9.0);
        assert_eq!(
            val("Health"),
            95.0 + 20.0 * 9.0 + 5.0 * 20.0,
            "template's level (20) feeds the Health curve, not the shell's (1)"
        );
    }

    #[test]
    fn empty_without_class_or_unsupported_game() {
        let mut index = fnv_index_with_class(0x2000, [5; 7]);
        // NPC referencing an unparsed class → empty.
        assert!(derive_npc_actor_values(&npc_with_class(0x9999), &index).is_empty());
        // Right NPC, unsupported profile → empty.
        index.character_rules = CharacterRulesProfile::NONE;
        assert!(derive_npc_actor_values(&npc_with_class(0x2000), &index).is_empty());
    }

    #[test]
    fn skyrim_pools_are_race_starts_plus_signed_npc_offsets() {
        let mut index = EsmIndex::default();
        index.character_rules = CharacterRulesProfile::SKYRIM;
        index.actor_values.insert(0x3E8, avif(0x3E8, "AVHealth"));
        index.actor_values.insert(0x3E9, avif(0x3E9, "AVMagicka"));
        index.actor_values.insert(0x3EA, avif(0x3EA, "AVStamina"));
        index.races.insert(
            0x13746,
            RaceRecord {
                form_id: 0x13746,
                starting_health: Some(50.0),
                starting_magicka: Some(75.0),
                starting_stamina: Some(100.0),
                ..Default::default()
            },
        );
        let npc = NpcRecord {
            race_form_id: 0x13746,
            health_offset: -15,
            magicka_offset: 10,
            stamina_offset: -20,
            ..Default::default()
        };

        assert_eq!(
            derive_npc_actor_values(&npc, &index),
            vec![(0x3E8, 35.0), (0x3E9, 85.0), (0x3EA, 80.0)]
        );
    }

    #[test]
    fn skyrim_health_skips_when_race_health_is_missing_or_invalid() {
        let npc = NpcRecord {
            race_form_id: 0x13746,
            health_offset: 10,
            ..Default::default()
        };
        let no_race = EsmIndex {
            character_rules: CharacterRulesProfile::SKYRIM,
            ..EsmIndex::default()
        };
        assert!(derive_npc_actor_values(&npc, &no_race).is_empty());

        let mut invalid_race = EsmIndex::default();
        invalid_race.character_rules = CharacterRulesProfile::SKYRIM;
        invalid_race
            .actor_values
            .insert(0x3E8, avif(0x3E8, "AVHealth"));
        invalid_race.races.insert(
            0x13746,
            RaceRecord {
                form_id: 0x13746,
                starting_health: Some(f32::NAN),
                ..Default::default()
            },
        );
        assert!(derive_npc_actor_values(&npc, &invalid_race).is_empty());
    }

    #[test]
    fn actor_value_lookup_normalizes_av_prefix_and_rejects_null_form_ids() {
        let mut index = EsmIndex::default();
        index.actor_values.insert(0x3E8, avif(0x3E8, "AVHealth"));
        index.actor_values.insert(0, avif(0, "AVStrength"));
        assert_eq!(index.actor_value_form_id("Health"), Some(0x3E8));
        assert_eq!(index.actor_value_form_id("AVHealth"), Some(0x3E8));
        assert_eq!(index.actor_value_form_id("health"), Some(0x3E8));
        assert_eq!(index.actor_value_form_id("Strength"), None);

        index.actor_values.insert(0x500, avif(0x500, "Health"));
        assert_eq!(
            index.actor_value_form_id("Health"),
            Some(0x500),
            "an exact canonical spelling wins deterministically"
        );
        index.actor_values.insert(u32::MAX, avif(u32::MAX, "Luck"));
        index.actor_values.insert(0x3EE, avif(0x3EE, "AVLuck"));
        assert_eq!(
            index.actor_value_form_id("Luck"),
            Some(0x3EE),
            "an invalid exact match must not hide a usable AV-prefixed record"
        );
    }

    #[test]
    fn fo4_stored_returns_prps_verbatim_plus_baked_derived() {
        // FO4 stores AVs: PRPS pairs pass through unchanged; the baked
        // DNAM Health/AP resolve via their AVIF EditorIDs.
        let mut index = EsmIndex::default();
        index.character_rules = CharacterRulesProfile::FALLOUT4;
        index.actor_values.insert(0x900, avif(0x900, "Health"));
        index
            .actor_values
            .insert(0x901, avif(0x901, "ActionPoints"));

        let npc = NpcRecord {
            actor_value_props: vec![(0x2A0, 7.0), (0x2A6, 5.0)], // Strength 7, Luck 5
            calculated_health: 240,
            calculated_action_points: 90,
            ..Default::default()
        };
        let pairs = derive_npc_actor_values(&npc, &index);

        assert!(pairs.contains(&(0x2A0, 7.0)), "Strength prop passthrough");
        assert!(pairs.contains(&(0x2A6, 5.0)), "Luck prop passthrough");
        assert!(
            pairs.contains(&(0x900, 240.0)),
            "Calculated Health → Health AVIF"
        );
        assert!(
            pairs.contains(&(0x901, 90.0)),
            "Calculated AP → ActionPoints AVIF"
        );
        assert_eq!(pairs.len(), 4, "2 PRPS + 2 baked derived");
    }

    #[test]
    fn fo4_zero_baked_stats_skipped_and_no_class_needed() {
        // 0 = absent: no Health/AP appended. And FO4 needs no class record
        // (unlike the FNV auto-calc path) — PRPS alone populates.
        let index = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT4,
            ..EsmIndex::default()
        }; // no AVIF needed for PRPS passthrough
        let npc = NpcRecord {
            actor_value_props: vec![(0x2A0, 7.0)],
            ..Default::default()
        };
        let pairs = derive_npc_actor_values(&npc, &index);
        assert_eq!(pairs, vec![(0x2A0, 7.0)]);
    }

    #[test]
    fn later_creation_profiles_retain_stored_actor_value_population() {
        for profile in [
            CharacterRulesProfile::FALLOUT76,
            CharacterRulesProfile::STARFIELD,
        ] {
            let index = EsmIndex {
                character_rules: profile,
                ..EsmIndex::default()
            };
            let npc = NpcRecord {
                actor_value_props: vec![(0x2A0, 7.0)],
                ..NpcRecord::default()
            };
            assert_eq!(
                derive_npc_actor_values(&npc, &index),
                vec![(0x2A0, 7.0)],
                "{} must keep the stored PRPS path",
                profile.name()
            );
        }
    }
}
