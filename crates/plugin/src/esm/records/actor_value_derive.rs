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
//! - **Skyrim (race + offset + class skills):** seeds Health, Magicka and
//!   Stamina each from its race's `RACE.DATA` starting value plus the
//!   NPC's signed `ACBS` offset, and all 18 skills by the UESP `CLAS`
//!   formula (`derive_skyrim_skills`); a non-auto-calc NPC keeps its
//!   authored `DNAM` values as written.
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
//! - **Skill base**: `skill = fAVDSkill<DisplayName>Base + governing_SPECIAL ×
//!   fAVDSkillPrimaryBonusMult + ceil(Luck × fAVDSkillLuckBonusMult)`
//!   (geckwiki *Derived Skill Settings*). Defaults: base **2**, primary
//!   mult **2**, luck mult **0.5**. Worked example (geckwiki): END 5 +
//!   Luck 5 → Unarmed = 2 + 5×2 + ceil(5×0.5) = 15. Of these, only the base
//!   is actually authored (per-skill, by display name — 13 FNV GMSTs, all
//!   `2.0`); the two mults are geckwiki-documented but unauthored by either
//!   master, so they're engine defaults, not a pending GMST read (#3173).
//! - **Governing SPECIAL per skill**: fallout.fandom *New Vegas SPECIAL* /
//!   geckwiki *SPECIAL*. FO3 and FNV use distinct canonical 13-skill rosters;
//!   FNV's displayed Guns/Survival retain the `AVSmallGuns`/`AVThrowing`
//!   record identities.
//! - **Health (NPC)**: `DATA` Base Health + the sourced
//!   `NpcHealthCurve` (`base + 5·(END−1) + 5·(L−1)` — see
//!   `crates/core/src/character/profile.rs`, #5238/#5340). The player
//!   formulas (FO3 `90 + 20·END + 10·L`, FNV `100 + 20·END + 5·(L−1)`)
//!   are the `Health (player)` row of the capture and never seed NPCs.
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
//! - **Non-auto-calc NPCs' authored *skills* (~40 % of every FO3/FNV
//!   actor).** The discriminator is `ACBS` flag bit 4 (`0x0010`,
//!   "Auto-calculate stats"): when clear the NPC stores its own SPECIAL
//!   and skills in the `NPC_` record. #5338 reads the **SPECIAL half** —
//!   the 7 `DATA` bytes the #5005 arm already held — so the authored
//!   attributes (and the Health their Endurance drives) now seed
//!   correctly. The **skill half** is still deferred: the FO3/FNV
//!   `NPC_` DNAM skill/SPECIAL block has no parser arm (the only
//!   actor-value-capturing `b"DNAM"` arm is gated on
//!   `GameKind::uses_actor_value_properties`, i.e. FO4+), so skills
//!   derive from the authored SPECIAL through the GECK formula instead
//!   of the authored per-skill values. That parse is `/audit-esm`
//!   Dimension 4 work — `NPC_` record decoding, not CHARAL.
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

use super::actor::{effective_actor_level, NpcRecord, RaceRecord};
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
// it does not exist workspace-wide. Until then this is a known, recorded
// deviation rather than a silent one.
//
// #3173 — GMST authoring, corrected. `SKILL_BASE` is per-skill in vanilla:
// FNV authors thirteen `fAVDSkill<DisplayName>Base` GMSTs (Barter, BigGuns,
// EnergyWeapons, Explosives, Lockpick, Medicine, MeleeWeapons, Repair,
// Science, SmallGuns, Sneak, Speech, Survival — keyed on the *display* name,
// the inverse of the `AVIF` record-identity convention), all `2.0` in
// vanilla and collapsed into this one shared constant. `fAVDSkillPrimaryBonusMult`
// and `fAVDSkillLuckBonusMult` do NOT exist in either `FalloutNV.esm` or
// `Fallout3.esm` (verified by raw byte search) — they are geckwiki-documented
// but unauthored; there is nothing to source them from, so when `skill_calc`
// lands on `CharacterRuleset` these two stay engine constants and only
// `SKILL_BASE` becomes a per-skill GMST read (by display name, `2.0` fallback).
const SKILL_BASE: f32 = 2.0; // fAVDSkill<DisplayName>Base (per-skill; see #3173)
const SKILL_ATTR_MULT: f32 = 2.0; // geckwiki fAVDSkillPrimaryBonusMult — unauthored, engine default
const SKILL_LUCK_MULT: f32 = 0.5; // geckwiki fAVDSkillLuckBonusMult — unauthored, engine default

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
///   [`derive_autocalc_actor_values`].
///
/// `TPLT` template inheritance is resolved first for **every** stat model
/// (#2956, #3381, #3382, [`crate::equip::resolve_inherited_stats`]) — a
/// templated shell's own `class_form_id`/level/`PRPS`/`DNAM`/race offsets are
/// frequently not what the engine actually uses, and
/// `stamp_character_components` resolves the same chains for the
/// `CharacterLevel`/`Background` it writes on the same entity.
///
/// **Two** chains, not one (#3480): `Use Stats` (`0x0002`) for the stat
/// fields and `Use Traits` (`0x0001`) for race, gated by independently-set
/// bits and walked by [`crate::equip::resolve_inherited_traits`]. Each arm is
/// handed the record whose fields it reads. An *absent* field on the resolved
/// record falls back down the chain rather than erasing a value some other
/// record on it authored (#3481, then #4086 for the intermediates a
/// two-endpoint comparison skipped) — see
/// [`crate::equip::resolve_inherited_field`].
///
/// Empty for every other game (Oblivion), a Skyrim NPC whose race has no
/// usable pool values, an FO4 NPC with no `PRPS`, or an FNV NPC whose class
/// wasn't parsed. Individual missing Skyrim AVIFs are skipped independently.
///
/// - **Creatures (`CREA`, FO3/FNV)**: SPECIAL and Health read straight off
///   the record's own `DATA` (#3390). They share `NpcRecord` and the whole
///   spawn tail with `NPC_` (#442/#2567) but not the stat model: `CNAM` is
///   not a CLAS FormID on `CREA` (0/1578 FNV and 0/533 FO3 resolve to one,
///   #3383), so the auto-calc arm's class lookup can never hit. The model
///   is selected per record — via [`CharacterRulesProfile::creature_stat_model`]
///   rather than a game branch — so `is_creature` is the only thing this
///   function asks about the record's class.
///
/// [`ActorValues::from_pairs`]: byroredux_core::ecs::components::ActorValues::from_pairs
pub fn derive_npc_actor_values(npc: &NpcRecord, index: &EsmIndex) -> Vec<(u32, f32)> {
    derive_resolved_actor_values(&crate::equip::ResolvedNpc::resolve(npc, index), index)
}

/// #4457 — the population-boundary form of [`derive_npc_actor_values`]:
/// the caller resolves the record once ([`crate::equip::ResolvedNpc`])
/// and hands it to every stamp, so the TPLT chain walks once per spawn
/// instead of once per consuming helper. The two-argument form remains
/// for callers outside a spawn boundary (the player template, tests).
///
/// #3381 / #3382 — `TPLT` + ACBS "Use Stats" inheritance is applied for
/// EVERY stat model, not just auto-calc. `template_flags` is parsed on all
/// three families and `stamp_character_components` consumes the same
/// resolved records for `CharacterLevel`/`Background` on the same entity, so
/// deriving actor values off the unresolved shell contradicted the
/// components sitting beside them.
pub fn derive_resolved_actor_values(
    resolved: &crate::equip::ResolvedNpc<'_>,
    index: &EsmIndex,
) -> Vec<(u32, f32)> {
    let stats = resolved.stats;
    // #3480 — race is NOT a stat. It rides the independently-set "Use
    // Traits" bit (`TEMPLATE_FLAG_USE_TRAITS`), which `ResolvedNpc`
    // resolves on its own chain and which `stamp_character_components`
    // uses for the `Background` it writes on this same entity. Reading
    // `race_form_id` off the *stats* record made 1,180 vanilla Skyrim
    // NPCs contradict their own `Background`, 118 of them landing on a
    // different `RACE.DATA` triple (and `00109C7C`, one of the wrong
    // races, authors no starting Magicka at all — those actors lost the
    // pool outright). The two chains are resolved separately and each arm
    // is handed the record whose fields it actually reads.
    let traits = resolved.r#traits;
    // #3390 — creatures and NPCs are different stat models *within* one
    // game, so the model is chosen by record kind and the profile still
    // owns which model that is. Consumers never branch on game identity.
    let model = if stats.is_creature {
        index.character_rules.creature_stat_model()
    } else {
        index.character_rules.npc_stat_model()
    };
    let mut values = match model {
        NpcStatModel::Stored => derive_stored_actor_values(resolved, index),
        NpcStatModel::RaceBaseOffsets => derive_skyrim_actor_values(stats, traits, index),
        NpcStatModel::ClassAutoCalc { health } => derive_autocalc_actor_values(
            stats,
            index,
            index.character_rules,
            // #5238 — retune the curve from the load-order GMSTs; both
            // vanilla masters author the mult pair, mods may retune any of
            // the three settings.
            health.with_gmst(|name| index.game_setting_float(name)),
        ),
        NpcStatModel::CreatureData => derive_creature_actor_values(stats, index),
        NpcStatModel::None => Vec::new(),
    };
    // Skyrim's AIDT responsibility is the authored Morality AV consumed by
    // dialogue/bribe formulas. Resolve it through the Use-AI-Data chain, just
    // like the combat disposition stamp, and preserve a more specific value
    // if the stat model already supplied Morality.
    if let (Some(ai), Some(morality)) = (
        resolved.ai_data.ai_data,
        index.actor_value_form_id("Morality"),
    ) {
        if let Some(responsibility) = ai.responsibility {
            if !values.iter().any(|(id, _)| *id == morality) {
                values.push((morality, responsibility as u8 as f32));
            }
        }
    }
    // #4447 — the body-condition seeding base is profile data
    // (`CharacterRulesProfile::body_condition_base`: FO3/FNV = 100 per the
    // GECK Stats List, every other family = none), not a `game ==` kind
    // compare. A kind branch cannot distinguish FO3 from FNV — the split
    // the profile exists to carry — and follows the broad kind onto any
    // future Fallout3NV-kind master, where the profile row is the reviewed
    // answer. Still gated on Health landing so the "unpopulated, not zero"
    // contract holds: a derivation that produced no Health is a shell and
    // gets no conditions either.
    if let Some(condition_base) = index.character_rules.body_condition_base() {
        if index
            .health_actor_value_key()
            .is_some_and(|health| values.iter().any(|&(id, _)| id == health))
        {
            // GECK Stats List: body-condition AVs start at 100, independently of
            // actor health. Damage/restoration then use the same saved AV layers.
            for name in crate::consumables::BODY_CONDITION_VALUES {
                if let Some(id) = index.actor_value_form_id(name) {
                    if !values.iter().any(|&(key, _)| key == id) {
                        values.push((id, condition_base));
                    }
                }
            }
        }
    }
    values
}

/// FO3 / FNV `CREA`: the seven SPECIAL attributes and Health, verbatim from
/// the creature's own `DATA` block (#3390).
///
/// No auto-calc and no class: `CREA` references no CLAS at all, and the
/// three aggregate skills it authors (`Combat` / `Magic` / `Stealth`) are
/// deliberately **not** emitted — FO3/FNV publish no `AVIF` those map onto,
/// and inventing a mapping would be a guess. `DATA.Damage` is likewise
/// parsed but not an actor value in these games; both stay on
/// [`CreatureStats`] for a future consumer.
///
/// Empty when the record carries no `DATA` (a shell that inherits
/// everything, whose `TPLT` chain `resolve_inherited_stats` has already
/// followed) — the same "unpopulated, not zero" contract the other arms
/// use. Health is skipped when non-positive: `i16` is the authored type,
/// and a shell can leave it at 0.
fn derive_creature_actor_values(npc: &NpcRecord, index: &EsmIndex) -> Vec<(u32, f32)> {
    let Some(stats) = npc.creature_stats else {
        return Vec::new();
    };
    let roster = AttributeSet::FALLOUT.members();
    let mut out = Vec::with_capacity(roster.len() + 1);
    for (i, attr) in roster.iter().enumerate() {
        if let Some(fid) = index.actor_value_form_id(attr.editor_id()) {
            out.push((fid, f32::from(stats.attributes[i])));
        }
    }
    if stats.health > 0 {
        if let Some(fid) = index.health_actor_value_key() {
            out.push((fid, f32::from(stats.health)));
        }
    }
    out
}

/// TES5 NPC resource pools: race starting values plus signed actor offsets, and — for an
/// auto-calc NPC — the class term (UESP *CLAS*, Weight Calculations / Attributes):
/// `10·(level−1)` points distributed over Health / Magicka / Stamina by the class's attribute
/// weights ([`skyrim_class_pool_points`]) and, for Health alone, `5·(level−1)` regardless of
/// weight. Each resolves independently through its authored AVIF. A non-auto-calc NPC is
/// `racial + offset` only (same page).
///
/// #5914 — this closed the #4454 documented gap ("captured nowhere"): the page documents the
/// term two paragraphs below the skills formula the sibling [`derive_skyrim_skills`] already
/// implements, and it reproduces the baked `DNAM` pools of every vanilla auto-calc NPC checked
/// (`skyrim_autocalc_pools_match_the_baked_dnam_pools`).
///
/// #3480 — the two inputs come off **different** `TPLT` chains and the
/// caller resolves both: `stats` (the "Use Stats" record) carries the signed
/// `ACBS` offsets, which really are stats, while `traits` (the "Use Traits"
/// record) carries `race_form_id`. The bits are independently set, so a shell
/// routinely has one without the other — reading race off the stats record
/// put 1,180 vanilla Skyrim actors' pools on a race their own `Background`
/// disagreed with. `Background.race_form_id` is stamped from
/// `resolve_inherited_traits` too, so the two now agree by construction.
fn derive_skyrim_actor_values(
    stats: &NpcRecord,
    traits: &NpcRecord,
    index: &EsmIndex,
) -> Vec<(u32, f32)> {
    let Some(race) = index.races.get(&traits.race_form_id) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(3 + SKYRIM_SKILLS.len());
    // ACBS "Auto-calc stats": only such an NPC levels its pools by class.
    let class_points = if stats.acbs_flags & SKYRIM_AUTO_CALC != 0 {
        let weights = index
            .classes
            .get(&stats.class_form_id)
            .and_then(|class| class.skyrim_pool_weights)
            .unwrap_or([0; 3]);
        skyrim_class_pool_points(&weights, i32::from(effective_actor_level(stats)))
    } else {
        [0; 3]
    };
    for (slot, (name, starting, offset)) in [
        ("Health", race.starting_health, stats.health_offset),
        ("Magicka", race.starting_magicka, stats.magicka_offset),
        ("Stamina", race.starting_stamina, stats.stamina_offset),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(key) = index.actor_value_form_id(name) else {
            continue;
        };
        // The race parser keeps a starting value only when it is positive, so `None` is a race
        // that starts the pool at zero (a Magicka-less race): the NPC's offset and class term
        // still apply, and the `> 0` check below drops a pool that nets out empty. Seven
        // vanilla NPCs (race `0xD53`, ...) carry baked Magicka that only this reproduces.
        let value = starting.unwrap_or(0.0) + f32::from(offset) + class_points[slot] as f32;
        if value.is_finite() && value > 0.0 {
            out.push((key, value));
        }
    }
    out.extend(derive_skyrim_skills(stats, race, index));
    out
}

/// Skyrim's 18 skills in actor-value-index order (One-handed = 6 .. Enchanting = 23): the
/// order of the class's skill weights, the NPC's `DNAM` skill values and the race's skill
/// bonus indices, and the editor ids of their `AVIF` records.
pub const SKYRIM_SKILLS: [&str; 18] = [
    "OneHanded",
    "TwoHanded",
    "Marksman",
    "Block",
    "Smithing",
    "HeavyArmor",
    "LightArmor",
    "Pickpocket",
    "Lockpicking",
    "Sneak",
    "Alchemy",
    "Speechcraft",
    "Alteration",
    "Conjuration",
    "Destruction",
    // The Illusion skill's AVIF keeps its pre-release editor id (`AVMysticism`); `Skyrim.esm`
    // has no `AVIllusion` (it names only the `...Mod` / `...SkillAdvance` / `...PowerMod`
    // helpers), and CHARAL records the same rename (`SkillSet::SKYRIM`).
    "Mysticism",
    "Restoration",
    "Enchanting",
];

/// The actor-value index of Skyrim's first skill (One-handed): RACE skill bonuses name
/// their skill by actor-value index.
const SKYRIM_FIRST_SKILL_INDEX: u8 = 6;
/// What every Skyrim NPC's skill starts at (UESP *CLAS*, Weight Calculations: "15 + racial
/// bonus + ...").
pub const SKYRIM_BASE_SKILL: i32 = 15;
/// The skill points a Skyrim NPC gains per level above the first (UESP *CLAS*: "the
/// fixed 8 skill points per level").
const SKYRIM_SKILL_POINTS_PER_LEVEL: i32 = 8;
/// The most a skill reaches.
const SKYRIM_SKILL_CAP: i32 = 100;

/// The points a class's skill `weights` distribute among its skills for an NPC of `level`
/// (UESP *CLAS*, Weight Calculations / Skills): first every complete set,
/// `floor(8 x (level - 1) / sum of weights) x weight`; then the leftover points one at a
/// time, looping over the skills ordered by weight (higher first) then actor-value index
/// (lower first), a skill taking no more leftovers than its weight. The page says the
/// assignment changes once a skill reaches 100 and does not give how; the result is capped
/// at the skill maximum instead.
pub fn skyrim_class_skill_points(weights: &[u8; 18], level: i32) -> [i32; 18] {
    let mut points = [0i32; 18];
    let total = SKYRIM_SKILL_POINTS_PER_LEVEL * (level - 1).max(0);
    let sum: i32 = weights.iter().map(|&w| i32::from(w)).sum();
    if total == 0 || sum == 0 {
        return points;
    }
    let sets = total / sum;
    for (slot, &weight) in points.iter_mut().zip(weights) {
        *slot = sets * i32::from(weight);
    }
    let mut leftover = total - sets * sum;
    let mut order: Vec<usize> = (0..18).filter(|&i| weights[i] > 0).collect();
    // Skyrim's class-skill allocation breaks equal weights by descending
    // actor-value index, as confirmed against Skyrim.esm's NPC DNAM values.
    order.sort_by_key(|&i| (std::cmp::Reverse(weights[i]), std::cmp::Reverse(i)));
    let mut taken = [0i32; 18];
    while leftover > 0 {
        let mut progressed = false;
        for &i in &order {
            if leftover == 0 {
                break;
            }
            if taken[i] < i32::from(weights[i]) {
                points[i] += 1;
                taken[i] += 1;
                leftover -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    points
}

/// ACBS "Auto-calc stats" - the NPC's skills and pools derive from race, class and level.
const SKYRIM_AUTO_CALC: u32 = 0x10;
/// The pool points a Skyrim NPC gains per level above the first, distributed by the class's
/// attribute weights (UESP *CLAS*: "the fixed 10 attribute points per level").
const SKYRIM_POOL_POINTS_PER_LEVEL: i32 = 10;
/// The Health a Skyrim NPC gains per level above the first regardless of weight (UESP
/// *CLAS*: "health always receives an additional 5 points per level").
const SKYRIM_HEALTH_PER_LEVEL: i32 = 5;

/// The points a class's attribute `weights` (Health, Magicka, Stamina) add to an NPC of
/// `level` (UESP *CLAS*, Weight Calculations / Attributes): `5·(level−1)` Health on top, then
/// `10·(level−1)` points distributed exactly like the skills — first every complete set,
/// `floor(10·(level−1) / sum of weights)·weight`, then the leftovers one at a time over the
/// pools ordered by weight (higher first) and, for equal weights, in **reverse** actor-value
/// order (Stamina, Magicka, Health), a pool taking no more leftovers than its weight.
pub fn skyrim_class_pool_points(weights: &[u8; 3], level: i32) -> [i32; 3] {
    let levels = (level - 1).max(0);
    let mut points = [SKYRIM_HEALTH_PER_LEVEL * levels, 0, 0];
    let total = SKYRIM_POOL_POINTS_PER_LEVEL * levels;
    let sum: i32 = weights.iter().map(|&w| i32::from(w)).sum();
    if total == 0 || sum == 0 {
        return points;
    }
    let sets = total / sum;
    for (slot, &weight) in points.iter_mut().zip(weights) {
        *slot += sets * i32::from(weight);
    }
    let mut leftover = total - sets * sum;
    let mut order: Vec<usize> = (0..3).filter(|&i| weights[i] > 0).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(weights[i]), std::cmp::Reverse(i)));
    let mut taken = [0i32; 3];
    while leftover > 0 {
        let mut progressed = false;
        for &i in &order {
            if leftover == 0 {
                break;
            }
            if taken[i] < i32::from(weights[i]) {
                points[i] += 1;
                taken[i] += 1;
                leftover -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    points
}

/// An NPC's 18 skills. Auto-calc NPCs follow the UESP formula: 15, plus the race's skill
/// bonus, plus the class's weighted share of 8 points per level
/// ([`skyrim_class_skill_points`]). An NPC without the auto-calc flag stores its skills in
/// `DNAM`, used as written; how its race bonus and its `DNAM` skill offsets combine with
/// them is not documented, so they are not added. Empty when neither source exists.
fn derive_skyrim_skills(stats: &NpcRecord, race: &RaceRecord, index: &EsmIndex) -> Vec<(u32, f32)> {
    let values: [i32; 18] = if stats.acbs_flags & SKYRIM_AUTO_CALC != 0 {
        // An NPC with no class weights no skill: it keeps 15 plus its race's bonus.
        let weights = index
            .classes
            .get(&stats.class_form_id)
            .and_then(|class| class.skyrim_skill_weights)
            .unwrap_or([0; 18]);
        let points = skyrim_class_skill_points(&weights, i32::from(effective_actor_level(stats)));
        let mut values = [0i32; 18];
        for (slot, point) in values.iter_mut().zip(points) {
            *slot = SKYRIM_BASE_SKILL + point;
        }
        for &(skill, bonus) in &race.skill_bonuses {
            if let Some(slot) = skill.checked_sub(SKYRIM_FIRST_SKILL_INDEX).and_then(|i| values.get_mut(usize::from(i))) {
                *slot += i32::from(bonus);
            }
        }
        values
    } else if let Some(stored) = stats.skill_values {
        stored.map(i32::from)
    } else {
        return Vec::new();
    };
    SKYRIM_SKILLS
        .iter()
        .zip(values)
        .filter_map(|(name, value)| Some((index.actor_value_form_id(name)?, value.clamp(0, SKYRIM_SKILL_CAP) as f32)))
        .collect()
}

/// FO4+ stored actor values: the `PRPS` `(AVIF FormID, value)` pairs
/// verbatim (SPECIAL + overrides — already in the right space and shape)
/// plus the baked `DNAM` derived stats resolved to their AVIF FormIDs. The
/// Health / Action Points lookups resolve-or-skip, matching the auto-calc
/// path's contract for an index missing an `AVIF`. One allocation; the
/// `PRPS` slice is `memcpy`'d, the ≤2 baked stats pushed.
fn derive_stored_actor_values(
    resolved: &crate::equip::ResolvedNpc<'_>,
    index: &EsmIndex,
) -> Vec<(u32, f32)> {
    // #3481 — template precedence is only correct for a field the template
    // actually carries. `0` is the documented "absent" sentinel on both baked
    // `DNAM` values (`NpcRecord::calculated_health`), and an empty `PRPS` is
    // the same statement for the property array, so an absent value must fall
    // back down the chain exactly the way the terminal resolution falls
    // back to the shell when the flag or the template is missing. Taking the
    // resolved record unconditionally cost 54 vanilla FO4 actors their own
    // authored Health — and `stamp_actor_values` only inserts `ActorVitals`
    // when the Health key is present, so those actors spawned undamageable.
    //
    // #4086 — and the fallback has to consult the whole chain, not its two
    // ends. The terminal leaves an intermediate `NPC_` that authors Health
    // invisible: the value fell all the way back to the shell, which is very
    // likely `0` as well, reproducing the exact undamageable-actor symptom
    // #3481 was filed for. Each field asks for the deepest record that
    // authors it — #4457 folds that walk behind the resolved record's
    // cached Use-Stats chain, so it cannot drift from the terminal walk.
    //
    // What "authors it" means per field: `0` on a baked `DNAM` stat means
    // *absent*, not zero — no live FO4 actor has 0 base Health or 0 base
    // Action Points, which is what makes the sentinel unambiguous and lets
    // `NpcRecord` skip an `Option` discriminant. An empty `PRPS` says the same
    // thing for the property array.
    let inherited = |authored: fn(&NpcRecord) -> bool, pick: fn(&NpcRecord) -> u16| {
        resolved
            .authored_stat_field(|record| authored(record).then(|| pick(record)))
            .unwrap_or(0)
    };
    let props = resolved
        .authored_stat_field(|record| {
            (!record.actor_value_props.is_empty()).then_some(record.actor_value_props.as_slice())
        })
        .unwrap_or(&[]);
    let mut out = Vec::with_capacity(props.len() + 2);
    // #4677 (CHAR-2026-09-21-D4-02) — when a baked DNAM value exists, the
    // PRPS pair for the same key is DROPPED rather than overridden by push
    // order. The collision is not hypothetical: on `Fallout4.esm`, PRPS
    // authors Health on 2,848 of 3,015 NPCs (values include 0.0 ×350 and
    // −10.0 ×185) and DNAM carries a positive calc_health on 2,525 of them
    // — 2,490 disagreeing pairs kept correct only by the DNAM push coming
    // second and `from_pairs` being last-write-wins. Making the precedence
    // explicit means a reorder, sort-by-key or dedup cannot silently
    // resurrect dead-on-spawn (0) or undamageable (−10) base Health.
    let baked_pairs: Vec<(u32, f32)> = [
        (
            "Health",
            inherited(|r| r.calculated_health > 0, |r| r.calculated_health),
        ),
        (
            "ActionPoints",
            inherited(
                |r| r.calculated_action_points > 0,
                |r| r.calculated_action_points,
            ),
        ),
    ]
    .into_iter()
    .filter_map(|(avif_editor_id, baked)| {
        (baked > 0).then_some(avif_editor_id).and_then(|id| {
            index
                .actor_value_form_id(id)
                .map(|fid| (fid, f32::from(baked)))
        })
    })
    .collect();
    out.extend_from_slice(
        &props
            .iter()
            .filter(|(fid, _)| !baked_pairs.iter().any(|(baked_fid, _)| baked_fid == fid))
            .copied()
            .collect::<Vec<_>>(),
    );
    out.extend(baked_pairs);
    out
}

/// `ACBS` flag bit 4 (`0x0010`) — "Auto-calculate stats" (GECK *Stats
/// Tab — NPC*). When clear, the NPC's SPECIAL is authored in its own
/// `DATA` (see [`NpcRecord::data_attributes`]); when set, it comes from
/// the class.
const ACBS_FLAG_AUTO_CALCULATE: u32 = 0x0010;

/// FNV / FO3 auto-calc: SPECIAL = the NPC's class base attributes, skills
/// derived via the GECK formula (`base_skill`), and Health derived from the
/// locked per-game END + level curve. The #1663 reference path.
///
/// #5338 — with the `ACBS` auto-calculate bit *clear*, the SPECIAL comes
/// from the record's own authored `DATA` bytes instead of the class
/// (~40 % of every FO3/FNV actor, disproportionately the named NPCs quests
/// target). The class still supplies the skill *roster* derivation inputs:
/// the authored `DNAM` skill half remains unparsed, so skills derive from
/// the authored SPECIAL through the same GECK formula.
fn derive_autocalc_actor_values(
    npc: &NpcRecord,
    index: &EsmIndex,
    profile: CharacterRulesProfile,
    health_curve: byroredux_core::character::NpcHealthCurve,
) -> Vec<(u32, f32)> {
    let Some(class) = index.classes.get(&npc.class_form_id) else {
        return Vec::new();
    };
    // #5338 — authored beats class-averaged: GECK "Attributes may not be
    // changed if Autocalc is checked", so with the box clear the seven
    // `DATA` bytes are the actor's real SPECIAL and the class ATTR is the
    // wrong table. Falls back to the class when the record ships no DATA
    // (a shell whose Use-Stats terminal authored none).
    let special = if npc.acbs_flags & ACBS_FLAG_AUTO_CALCULATE == 0 {
        npc.data_attributes.unwrap_or(class.base_attributes)
    } else {
        class.base_attributes
    };
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
        let level = f32::from(effective_actor_level(npc));
        // #5238 — the GECK-authored Base Health term: "Health is calculated
        // with Endurance and level. This value is then added to that
        // result." (GECK Stats Tab — NPC.) Non-positive values are the
        // authored starts-dead convention (#5005's corpse rule) and are
        // clamped for the seeding path; those actors are dead regardless.
        let base_health = npc.data_base_health.unwrap_or(0).max(0) as f32;
        let health = health_curve.evaluate(base_health, endurance, level);
        out.push((fid, health));
    }
    out
}

#[cfg(test)]
mod tests {
    use byroredux_core::character::SkillSet;

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
    use crate::esm::records::actor::CreatureStats;
    use crate::esm::records::{AvifRecord, ClassRecord, RaceRecord};

    fn avif(form_id: u32, editor_id: &str) -> AvifRecord {
        AvifRecord {
            form_id,
            editor_id: editor_id.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn plugin_skyrim_skill_avif_roster_matches_charal() {
        let mut plugin_roster = SKYRIM_SKILLS.to_vec();
        plugin_roster.sort_unstable();

        let mut charal_roster: Vec<_> = SkillSet::SKYRIM
            .skills()
            .iter()
            .map(|skill| skill.editor_id)
            .collect();
        charal_roster.sort_unstable();

        assert_eq!(plugin_roster.len(), 18);
        assert_eq!(charal_roster.len(), 18);
        assert_eq!(
            plugin_roster, charal_roster,
            "plugin AVIF lookup names must stay set-equal to CHARAL's Skyrim skill roster"
        );
    }

    /// Build an index whose AVIF records cover the 7 SPECIAL + 13 FNV
    /// skills + Health, with deterministic FormIDs (0x100 + slot). The
    /// records use shipped-data `AV` prefixes and record identities rather
    /// than the canonical/display spellings consumed by CHARAL.
    fn fnv_index_with_class(class_form_id: u32, base: [u8; 7]) -> EsmIndex {
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT_NEW_VEGAS,
            ..Default::default()
        };
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
        // #5238 — the sourced NPC curve: base (unset → 0) + 5·(END−1) +
        // 5·(L−1); END 5, L 1 → 20.
        assert_eq!(val("Health"), 20.0, "base 0 + 5·(END−1) + 5·(L−1)");

        // 7 SPECIAL + 13 FNV skills + Health.
        assert_eq!(pairs.len(), 21);
    }

    /// #3171 — the Health curve's level term and the `CharacterLevel`
    /// component must be the same number.
    ///
    /// This path used to run through a private `effective_npc_level` copy
    /// whose non-multiplier branch floored at `1` while the binary's
    /// `effective_actor_level` floored at `0` — so a non-mult actor recorded
    /// at level `0` derived its Health one level above the `CharacterLevel`
    /// stamped on the very same entity (+5 on FNV, +10 on FO3), and resolved
    /// its `Use Stats` template against a different `LVLN` tier. 30 FNV
    /// `NPC_` records are in that set. The copy is gone; this pins the
    /// agreement so a new one fails here.
    #[test]
    fn level_zero_actor_derives_health_at_the_level_its_record_states() {
        let base = [5, 5, 5, 5, 5, 5, 5];
        let index = fnv_index_with_class(0x2000, base);
        let mut npc = npc_with_class(0x2000);
        npc.level = 0;
        npc.acbs_flags = 0; // NOT a PC-level-multiplier record

        // The one level every consumer sees — `CharacterLevel { level }` in
        // `stamp_character_components` reads exactly this.
        assert_eq!(effective_actor_level(&npc), 0);

        let pairs = derive_npc_actor_values(&npc, &index);
        let health_fid = index.health_actor_value_key().unwrap();
        let health = pairs.iter().find(|(f, _)| *f == health_fid).unwrap().1;
        // #5238 — sourced NPC curve: base 0 + 5·(END−1) + 5·(L−1); END 5,
        // L 0 → 20 − 5 = 15. The level term keeps feeding at the record's
        // own level (0), which is the agreement this test pins.
        assert_eq!(health, 15.0);
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
        // #5238 — sourced NPC curve, base unset → 0: 5·(9−1) + 5·(20−1) = 135.
        assert_eq!(
            val("Health"),
            5.0 * (9.0 - 1.0) + 5.0 * (20.0 - 1.0),
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

    /// #4447 — body-condition seeding follows
    /// `CharacterRulesProfile::body_condition_base`, not a `game ==` kind
    /// compare. Pre-fix this was a raw `GameKind::Fallout3NV` branch that
    /// no unit fixture could exercise (the fixtures author no
    /// body-condition AVIFs), so its only cover was an `#[ignore]`d
    /// real-master test. Both legs are old-code killers: the FO4 leg sets
    /// the exact kind the old branch matched, with a profile that owns no
    /// body-condition base — the old code seeds anyway; the new code must
    /// not.
    #[test]
    fn body_condition_seeding_follows_the_profile_not_the_game_kind() {
        // (a) FNV profile + one authored body-condition AVIF → seeded at
        // the profile's base (100).
        let mut index = fnv_index_with_class(0x2000, [5; 7]);
        index
            .actor_values
            .insert(0x500, avif(0x500, "AVLeftAttackCondition"));
        let pairs = derive_npc_actor_values(&npc_with_class(0x2000), &index);
        let condition = index.actor_value_form_id("LeftAttackCondition").unwrap();
        assert_eq!(
            pairs.iter().find(|(k, _)| *k == condition).map(|(_, v)| *v),
            Some(100.0),
            "the FNV profile seeds body conditions at its authored base"
        );

        // (b) FO4 profile under the exact kind the old branch matched —
        // kind says Fallout3NV, profile owns no body-condition base: no
        // seeding. The profile is the reviewed answer, not the kind.
        let mut fo4 = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT4,
            game: crate::esm::reader::GameKind::Fallout3NV,
            ..EsmIndex::default()
        };
        fo4.actor_values.insert(0x3E8, avif(0x3E8, "Health"));
        fo4.actor_values
            .insert(0x500, avif(0x500, "AVLeftAttackCondition"));
        let npc = NpcRecord {
            calculated_health: 100,
            ..NpcRecord::default()
        };
        let pairs = derive_npc_actor_values(&npc, &fo4);
        assert!(
            !pairs.iter().any(|(k, _)| *k == 0x500),
            "an FO4 profile must not seed body conditions, even under a \
             Fallout3NV kind — the gate is the profile row, not the kind"
        );
        assert!(
            pairs.iter().any(|(k, _)| *k == 0x3E8),
            "the Health gate itself still passes — only the seeding is absent"
        );
    }

    /// #3381 — the Skyrim (`RaceBaseOffsets`) arm must resolve `TPLT` +
    /// "Use Stats" like the auto-calc arm already did. Pre-fix it read the
    /// shell `NPC_`'s own race and offsets, contradicting the
    /// `CharacterLevel`/`Background` that `stamp_character_components`
    /// resolves through the same chain onto the same entity.
    ///
    /// #3480 — the shell now sets **both** template bits. Race rides "Use
    /// Traits", so with only "Use Stats" set the template's race is no longer
    /// the right answer; the divergent case is pinned separately by
    /// [`skyrim_race_follows_use_traits_while_offsets_follow_use_stats`].
    #[test]
    fn skyrim_pools_follow_use_stats_template() {
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::SKYRIM,
            ..Default::default()
        };
        index.actor_values.insert(0x3E8, avif(0x3E8, "AVHealth"));
        index.actor_values.insert(0x3E9, avif(0x3E9, "AVMagicka"));
        index.actor_values.insert(0x3EA, avif(0x3EA, "AVStamina"));
        // The race the template points at — the one the engine uses.
        index.races.insert(
            0x1000,
            RaceRecord {
                form_id: 0x1000,
                starting_health: Some(100.0),
                starting_magicka: Some(100.0),
                starting_stamina: Some(100.0),
                ..Default::default()
            },
        );
        // The shell's own race, which must never be read.
        index.races.insert(
            0x2000,
            RaceRecord {
                form_id: 0x2000,
                starting_health: Some(5.0),
                starting_magicka: Some(5.0),
                starting_stamina: Some(5.0),
                ..Default::default()
            },
        );

        let template = NpcRecord {
            form_id: 0x0010_0001,
            race_form_id: 0x1000,
            health_offset: 20,
            magicka_offset: 10,
            stamina_offset: 5,
            ..Default::default()
        };
        index.npcs.insert(template.form_id, template);

        let shell = NpcRecord {
            form_id: 0x0010_0000,
            race_form_id: 0x2000,
            health_offset: -1,
            magicka_offset: -1,
            stamina_offset: -1,
            template_form_id: 0x0010_0001,
            template_flags: crate::equip::TEMPLATE_FLAG_USE_STATS
                | crate::equip::TEMPLATE_FLAG_USE_TRAITS,
            ..Default::default()
        };

        let pairs = derive_npc_actor_values(&shell, &index);
        let val = |fid: u32| pairs.iter().find(|(f, _)| *f == fid).map(|(_, v)| *v);
        assert_eq!(
            val(0x3E8),
            Some(120.0),
            "Health must come from the template"
        );
        assert_eq!(
            val(0x3E9),
            Some(110.0),
            "Magicka must come from the template"
        );
        assert_eq!(
            val(0x3EA),
            Some(105.0),
            "Stamina must come from the template"
        );
    }

    /// #3480 — the two `TPLT` chains are independent, and each arm must read
    /// the record its fields actually live on. A shell with `Use Stats` set
    /// and `Use Traits` clear keeps its **own** `RNAM` race while still taking
    /// the template's signed `ACBS` offsets.
    ///
    /// Measured on vanilla `Skyrim.esm`: 1,180 of 5,118 `NPC_` resolve a
    /// different race through the two chains, 118 of them onto a different
    /// `RACE.DATA` (H, M, S) triple. `MS07LvlBlackbloodMissileNordM`
    /// (`000DC8DC`) is the shape below — stats-race `00109C7C` authors no
    /// starting Magicka at all, so before this fix the actor lost the pool
    /// outright rather than merely getting a wrong number.
    #[test]
    fn skyrim_race_follows_use_traits_while_offsets_follow_use_stats() {
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::SKYRIM,
            ..Default::default()
        };
        index.actor_values.insert(0x3E8, avif(0x3E8, "AVHealth"));
        index.actor_values.insert(0x3E9, avif(0x3E9, "AVMagicka"));
        index.actor_values.insert(0x3EA, avif(0x3EA, "AVStamina"));
        // The shell's own race — a NordRace stand-in, the one `Background`
        // will carry because `Use Traits` is clear.
        index.races.insert(
            0x13746,
            RaceRecord {
                form_id: 0x13746,
                starting_health: Some(50.0),
                starting_magicka: Some(50.0),
                starting_stamina: Some(50.0),
                ..Default::default()
            },
        );
        // The stats template's race — `00109C7C`'s shape, Magicka unauthored.
        index.races.insert(
            0x109C7C,
            RaceRecord {
                form_id: 0x109C7C,
                starting_health: Some(12.0),
                starting_magicka: None,
                starting_stamina: Some(200.0),
                ..Default::default()
            },
        );

        let template = NpcRecord {
            form_id: 0x0010_0001,
            race_form_id: 0x109C7C,
            health_offset: 20,
            magicka_offset: 10,
            stamina_offset: 5,
            ..Default::default()
        };
        index.npcs.insert(template.form_id, template);

        let shell = NpcRecord {
            form_id: 0x0010_0000,
            race_form_id: 0x13746,
            health_offset: -1,
            magicka_offset: -1,
            stamina_offset: -1,
            template_form_id: 0x0010_0001,
            // Use Stats WITHOUT Use Traits — 23.1 % of vanilla Skyrim's
            // templated actors are in exactly this state.
            template_flags: crate::equip::TEMPLATE_FLAG_USE_STATS,
            ..Default::default()
        };

        let pairs = derive_npc_actor_values(&shell, &index);
        let val = |fid: u32| pairs.iter().find(|(f, _)| *f == fid).map(|(_, v)| *v);
        assert_eq!(
            val(0x3E8),
            Some(70.0),
            "race start from the shell's own RNAM (50) + the template's \
             Use Stats offset (+20)"
        );
        assert_eq!(
            val(0x3E9),
            Some(60.0),
            "Magicka must survive: the stats-chain race authors none, but race \
             is not a stat and the traits chain resolves to the shell's own"
        );
        assert_eq!(val(0x3EA), Some(55.0), "same split for Stamina");

        // The same-entity contract the whole finding is about: the race behind
        // the pools is the race `stamp_character_components` stamps onto
        // `Background`, which resolves the traits chain.
        assert_eq!(
            crate::equip::resolve_inherited_traits(&shell, effective_actor_level(&shell), &index)
                .race_form_id,
            0x13746,
            "Background's race and the pools' race must be the same FormID"
        );
    }

    /// #3480, the other half: when the shell *does* set `Use Traits`, race
    /// follows the chain — the fix must not simply pin race to the shell.
    #[test]
    fn skyrim_race_follows_the_template_when_use_traits_is_set() {
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::SKYRIM,
            ..Default::default()
        };
        index.actor_values.insert(0x3E8, avif(0x3E8, "AVHealth"));
        index.races.insert(
            0x1000,
            RaceRecord {
                form_id: 0x1000,
                starting_health: Some(100.0),
                ..Default::default()
            },
        );
        index.races.insert(
            0x2000,
            RaceRecord {
                form_id: 0x2000,
                starting_health: Some(5.0),
                ..Default::default()
            },
        );
        let template = NpcRecord {
            form_id: 0x0010_0001,
            race_form_id: 0x1000,
            ..Default::default()
        };
        index.npcs.insert(template.form_id, template);
        let shell = NpcRecord {
            form_id: 0x0010_0000,
            race_form_id: 0x2000,
            health_offset: 7,
            template_form_id: 0x0010_0001,
            // Traits only: the offsets stay the shell's own.
            template_flags: crate::equip::TEMPLATE_FLAG_USE_TRAITS,
            ..Default::default()
        };

        let pairs = derive_npc_actor_values(&shell, &index);
        assert_eq!(
            pairs.iter().find(|(f, _)| *f == 0x3E8).map(|(_, v)| *v),
            Some(107.0),
            "template race start (100) + the shell's own offset (+7)"
        );
    }

    /// #3481 — `0` is the "absent" sentinel on the baked `DNAM` pair, so a
    /// template that authors neither must not erase the shell's own values.
    ///
    /// Measured on vanilla `Fallout4.esm`: 54 of 3,015 actors author a
    /// `DNAM` Health their `Use Stats` template does not, and lost it. That
    /// matters beyond the number — `stamp_actor_values` only inserts
    /// `ActorVitals` when the Health key is present, so those actors spawned
    /// with no vitals and could not be damaged or killed at all.
    #[test]
    fn fo4_absent_baked_stats_fall_back_to_the_shell() {
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT4,
            ..Default::default()
        };
        index.actor_values.insert(0x3E8, avif(0x3E8, "Health"));
        index
            .actor_values
            .insert(0x3E9, avif(0x3E9, "ActionPoints"));
        index.actor_values.insert(0x3EA, avif(0x3EA, "Strength"));

        // A template that authors no DNAM pair and no PRPS at all.
        let template = NpcRecord {
            form_id: 0x0010_0001,
            ..Default::default()
        };
        index.npcs.insert(template.form_id, template);

        let shell = NpcRecord {
            form_id: 0x0010_0000,
            actor_value_props: vec![(0x3EA, 4.0)],
            calculated_health: 130,
            calculated_action_points: 70,
            template_form_id: 0x0010_0001,
            template_flags: crate::equip::TEMPLATE_FLAG_USE_STATS,
            ..Default::default()
        };

        let pairs = derive_npc_actor_values(&shell, &index);
        let val = |fid: u32| pairs.iter().find(|(f, _)| *f == fid).map(|(_, v)| *v);
        assert_eq!(
            val(0x3E8),
            Some(130.0),
            "an absent template DNAM Health must not erase the shell's own — \
             without Health there is no ActorVitals and the actor is unkillable"
        );
        assert_eq!(
            val(0x3E9),
            Some(70.0),
            "the identical sentinel governs ActionPoints"
        );
        assert_eq!(
            val(0x3EA),
            Some(4.0),
            "an empty template PRPS is the same 'absent' statement"
        );
    }

    /// #3481, the precedence half: a template that *does* author the field
    /// still wins. `1` is a live authored value, not the sentinel.
    #[test]
    fn fo4_authored_template_baked_stats_still_win_over_the_shell() {
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT4,
            ..Default::default()
        };
        index.actor_values.insert(0x3E8, avif(0x3E8, "Health"));
        index
            .actor_values
            .insert(0x3E9, avif(0x3E9, "ActionPoints"));

        let template = NpcRecord {
            form_id: 0x0010_0001,
            calculated_health: 1,
            // AP absent on the template — mixed, so one field falling back
            // cannot drag the other with it.
            ..Default::default()
        };
        index.npcs.insert(template.form_id, template);

        let shell = NpcRecord {
            form_id: 0x0010_0000,
            calculated_health: 999,
            calculated_action_points: 42,
            template_form_id: 0x0010_0001,
            template_flags: crate::equip::TEMPLATE_FLAG_USE_STATS,
            ..Default::default()
        };

        let pairs = derive_npc_actor_values(&shell, &index);
        let val = |fid: u32| pairs.iter().find(|(f, _)| *f == fid).map(|(_, v)| *v);
        assert_eq!(val(0x3E8), Some(1.0), "authored template Health wins");
        assert_eq!(
            val(0x3E9),
            Some(42.0),
            "and the absent AP independently falls back"
        );
    }

    /// #3382 — the FO4 (`Stored`) arm has the same obligation: `PRPS` and the
    /// baked `DNAM` pair come from the `Use Stats` source, not the shell.
    /// Measured against vanilla `Fallout4.esm`, 1,222 of 3,015 actors author
    /// a `PRPS` set differing from their template's and 1,201 a differing
    /// `DNAM`, so this is the common case, not an edge one.
    #[test]
    fn fo4_stored_values_follow_use_stats_template() {
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT4,
            ..Default::default()
        };
        index.actor_values.insert(0x3E8, avif(0x3E8, "Health"));
        index
            .actor_values
            .insert(0x3E9, avif(0x3E9, "ActionPoints"));
        index.actor_values.insert(0x3EA, avif(0x3EA, "Strength"));

        let template = NpcRecord {
            form_id: 0x0010_0001,
            actor_value_props: vec![(0x3EA, 8.0)],
            calculated_health: 250,
            calculated_action_points: 90,
            ..Default::default()
        };
        index.npcs.insert(template.form_id, template);

        let shell = NpcRecord {
            form_id: 0x0010_0000,
            actor_value_props: vec![(0x3EA, 1.0)],
            calculated_health: 5,
            calculated_action_points: 5,
            template_form_id: 0x0010_0001,
            template_flags: crate::equip::TEMPLATE_FLAG_USE_STATS,
            ..Default::default()
        };

        let pairs = derive_npc_actor_values(&shell, &index);
        let val = |fid: u32| pairs.iter().find(|(f, _)| *f == fid).map(|(_, v)| *v);
        assert_eq!(val(0x3EA), Some(8.0), "PRPS must come from the template");
        assert_eq!(
            val(0x3E8),
            Some(250.0),
            "baked DNAM Health from the template"
        );
        assert_eq!(val(0x3E9), Some(90.0), "baked DNAM AP from the template");
    }

    /// Without "Use Stats" the shell's own values still win — the resolver is
    /// resolve-or-fall-back, and hoisting it must not change that.
    #[test]
    fn fo4_stored_values_keep_the_shell_when_use_stats_is_clear() {
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT4,
            ..Default::default()
        };
        index.actor_values.insert(0x3EA, avif(0x3EA, "Strength"));

        let template = NpcRecord {
            form_id: 0x0010_0001,
            actor_value_props: vec![(0x3EA, 8.0)],
            ..Default::default()
        };
        index.npcs.insert(template.form_id, template);

        let shell = NpcRecord {
            form_id: 0x0010_0000,
            actor_value_props: vec![(0x3EA, 1.0)],
            template_form_id: 0x0010_0001,
            template_flags: 0,
            template_actors: [0; 13], // Use Stats NOT set
            ..Default::default()
        };

        let pairs = derive_npc_actor_values(&shell, &index);
        assert_eq!(
            pairs.iter().find(|(f, _)| *f == 0x3EA).map(|(_, v)| *v),
            Some(1.0),
            "no Use Stats flag → the shell's own PRPS stands"
        );
    }

    #[test]
    fn skyrim_pools_are_race_starts_plus_signed_npc_offsets() {
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::SKYRIM,
            ..Default::default()
        };
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

    /// #5914 - UESP *CLAS*, Weight Calculations / Attributes: Health always gains 5 per
    /// level; the 10 points per level split by weight in complete sets, then the leftovers
    /// one at a time by weight with equal weights in reverse actor-value order (Stamina,
    /// Magicka, Health), each pool taking no more leftovers than its weight.
    #[test]
    fn skyrim_class_pool_points_follow_the_documented_assignment() {
        // Level 4 = 30 points over weights 5/3/2 (sum 10): three complete sets.
        assert_eq!(skyrim_class_pool_points(&[5, 3, 2], 4), [15 + 15, 9, 6]);
        // Level 2 = 10 points over weights 1/1/1: three sets of 3, one leftover that
        // goes to Stamina (the reverse-order tie), plus Health's flat 5.
        assert_eq!(skyrim_class_pool_points(&[1, 1, 1], 2), [5 + 3, 3, 4]);
        // Weights 2/2/2: one complete set of 6, four leftovers dealt Stamina, Magicka,
        // Health, then Stamina again (each may take up to its weight of 2).
        assert_eq!(skyrim_class_pool_points(&[2, 2, 2], 2), [5 + 3, 3, 4]);
        // A heavier pool takes its leftover first, whatever its index.
        assert_eq!(skyrim_class_pool_points(&[1, 3, 0], 2), [5 + 3, 7, 0]);
        // Level 1 gains nothing; a class with no weights still grants Health's flat 5.
        assert_eq!(skyrim_class_pool_points(&[5, 3, 2], 1), [0, 0, 0]);
        assert_eq!(skyrim_class_pool_points(&[0, 0, 0], 7), [30, 0, 0]);
        // Every level distributes exactly 10 points, plus Health's 5.
        for level in 1..40 {
            let points = skyrim_class_pool_points(&[3, 1, 2], level);
            assert_eq!(points.iter().sum::<i32>(), 15 * (level - 1), "level {level}");
        }
    }

    /// #5914 - an auto-calc Skyrim NPC's pools are race start + offset + the class term; a
    /// non-auto-calc one is race start + offset only (UESP *CLAS*).
    #[test]
    fn skyrim_pools_gain_the_class_term_only_when_auto_calc() {
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::SKYRIM,
            ..Default::default()
        };
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
        index.classes.insert(
            0x2000,
            ClassRecord {
                form_id: 0x2000,
                skyrim_pool_weights: Some([5, 3, 2]),
                ..Default::default()
            },
        );
        let npc = |acbs_flags| NpcRecord {
            race_form_id: 0x13746,
            class_form_id: 0x2000,
            level: 4,
            acbs_flags,
            health_offset: -15,
            magicka_offset: 10,
            stamina_offset: -20,
            ..Default::default()
        };

        // Level 4: Health +30 (15 flat + 15 weighted), Magicka +9, Stamina +6.
        assert_eq!(
            derive_npc_actor_values(&npc(SKYRIM_AUTO_CALC), &index),
            vec![(0x3E8, 35.0 + 30.0), (0x3E9, 85.0 + 9.0), (0x3EA, 80.0 + 6.0)]
        );
        assert_eq!(
            derive_npc_actor_values(&npc(0), &index),
            vec![(0x3E8, 35.0), (0x3E9, 85.0), (0x3EA, 80.0)],
            "without auto-calc the pools are racial + offset only"
        );
    }

    /// #5914 - the pool formula against the ground truth: every non-templated, auto-calc,
    /// fixed-level `Skyrim.esm` NPC carries the pools the Creation Kit baked into its `DNAM`
    /// (bytes 36-41), and the derivation must reproduce them exactly.
    #[test]
    #[ignore = "needs Skyrim SE game data on disk"]
    fn skyrim_autocalc_pools_match_the_baked_dnam_pools() {
        use crate::esm::records::actor::ACBS_PC_LEVEL_MULT;
        let path = crate::esm::test_paths::skyrim_se_esm();
        if !path.exists() {
            eprintln!("Skipping skyrim_autocalc_pools_match_the_baked_dnam_pools: {} not found", path.display());
            return;
        }
        let index = crate::esm::parse_esm(&std::fs::read(&path).unwrap()).unwrap();
        let keys = ["Health", "Magicka", "Stamina"].map(|name| index.actor_value_form_id(name).expect("AVIF"));
        let (mut checked, mut mismatched) = (0usize, Vec::new());
        for npc in index.npcs.values() {
            let Some(baked) = npc.skyrim_calculated_pools else { continue };
            if npc.acbs_flags & SKYRIM_AUTO_CALC == 0
                || npc.acbs_flags & ACBS_PC_LEVEL_MULT != 0
                || npc.template_flags != 0
                || !index.races.contains_key(&npc.race_form_id)
            {
                continue;
            }
            let derived = derive_npc_actor_values(npc, &index);
            let pool = |key| derived.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
            checked += 1;
            // The derivation drops a pool that is not positive (a race with no Magicka, a
            // negative offset past the start), and the baked word is a signed i16 there.
            let got = keys.map(|key| Some(pool(key).unwrap_or(0.0)));
            let want = baked.map(|b| Some(f32::from((b as i16).max(0))));
            if got != want {
                mismatched.push((npc.form_id, npc.level, got, want));
            }
        }
        eprintln!("{checked} auto-calc NPCs checked, {} mismatched", mismatched.len());
        for m in mismatched.iter().take(12) {
            eprintln!("  {m:?}");
        }
        assert!(checked > 500, "the census should cover the vanilla auto-calc NPCs, got {checked}");
        assert!(mismatched.is_empty(), "{} pools differ from the baked DNAM", mismatched.len());
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

        let mut invalid_race = EsmIndex {
            character_rules: CharacterRulesProfile::SKYRIM,
            ..Default::default()
        };
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

    /// #4106 (D5-04) — `derive_skyrim_actor_values` resolves Health, Magicka
    /// and Stamina independently (2026-08-24): each is its own `zip(Option)`
    /// resolve-or-skip, not gated behind the other two. Nothing previously
    /// falsified that independence; `skyrim_health_skips_when_race_health_
    /// is_missing_or_invalid` only ever populated Health. A regression to an
    /// older all-or-nothing gate (return Health alone, and only when every
    /// pool is present) would keep every other Skyrim test green.
    #[test]
    fn skyrim_pools_resolve_independently_when_one_is_missing() {
        // (a) Race carries no `starting_magicka` — Health/Stamina must still
        // resolve; only Magicka is absent from the output.
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::SKYRIM,
            ..Default::default()
        };
        index.actor_values.insert(0x3E8, avif(0x3E8, "AVHealth"));
        index.actor_values.insert(0x3E9, avif(0x3E9, "AVMagicka"));
        index.actor_values.insert(0x3EA, avif(0x3EA, "AVStamina"));
        index.races.insert(
            0x13746,
            RaceRecord {
                form_id: 0x13746,
                starting_health: Some(50.0),
                starting_magicka: None,
                starting_stamina: Some(100.0),
                ..Default::default()
            },
        );
        let npc = NpcRecord {
            race_form_id: 0x13746,
            ..Default::default()
        };
        assert_eq!(
            derive_npc_actor_values(&npc, &index),
            vec![(0x3E8, 50.0), (0x3EA, 100.0)],
            "a race missing starting_magicka must not suppress Health/Stamina"
        );

        // (b) Race carries all three starts, but the load order is missing
        // the Magicka AVIF — Health/Stamina must still resolve.
        let mut no_magicka_avif = EsmIndex {
            character_rules: CharacterRulesProfile::SKYRIM,
            ..Default::default()
        };
        no_magicka_avif
            .actor_values
            .insert(0x3E8, avif(0x3E8, "AVHealth"));
        no_magicka_avif
            .actor_values
            .insert(0x3EA, avif(0x3EA, "AVStamina"));
        no_magicka_avif.races.insert(
            0x13746,
            RaceRecord {
                form_id: 0x13746,
                starting_health: Some(50.0),
                starting_magicka: Some(75.0),
                starting_stamina: Some(100.0),
                ..Default::default()
            },
        );
        assert_eq!(
            derive_npc_actor_values(&npc, &no_magicka_avif),
            vec![(0x3E8, 50.0), (0x3EA, 100.0)],
            "a load order missing the Magicka AVIF must not suppress Health/Stamina"
        );
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
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT4,
            ..Default::default()
        };
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

    /// #4677 (CHAR-2026-09-21-D4-02) — the PRPS/DNAM collision is now
    /// EXPLICIT precedence, not push-order luck. The audit censused
    /// Fallout4.esm: 2,490 NPCs whose PRPS Health disagrees with a
    /// positive DNAM calc_health, 185 authoring −10.0 (undamageable if
    /// it ever won) and 350 authoring 0.0 (dead on spawn). DNAM wins,
    /// by dropping the colliding PRPS pair rather than by ordering.
    #[test]
    fn baked_dnam_beats_a_colliding_prps_pair_regardless_of_order() {
        let mut index = EsmIndex {
            character_rules: CharacterRulesProfile::FALLOUT4,
            ..Default::default()
        };
        index
            .actor_values
            .insert(0x900, avif(0x900, "Health"));
        index
            .actor_values
            .insert(0x901, avif(0x901, "ActionPoints"));

        let npc = NpcRecord {
            // PRPS authors the collision: negative Health, zero AP — both
            // would be fatal if the pair outlived the baked push.
            actor_value_props: vec![(0x900, -10.0), (0x901, 0.0), (0x2A0, 7.0)],
            calculated_health: 150,
            calculated_action_points: 100,
            ..Default::default()
        };
        let pairs = derive_npc_actor_values(&npc, &index);

        assert!(
            !pairs.contains(&(0x900, -10.0)),
            "the colliding PRPS Health pair must be dropped, not overridden"
        );
        assert!(
            !pairs.contains(&(0x901, 0.0)),
            "the colliding PRPS AP pair must be dropped"
        );
        assert_eq!(
            pairs.iter().find(|&&(fid, _)| fid == 0x900),
            Some(&(0x900, 150.0)),
            "the baked DNAM Health is the one survivor"
        );
        assert_eq!(
            pairs.iter().find(|&&(fid, _)| fid == 0x901),
            Some(&(0x901, 100.0))
        );
        assert!(pairs.contains(&(0x2A0, 7.0)), "non-colliding props stay");
        assert_eq!(pairs.len(), 3, "1 PRPS + 2 baked");
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

    /// #4453 — inverted: FO76/Starfield no longer inherit FO4's `Stored`
    /// PRPS/DNAM decoder on presumption. No capture line says their
    /// `NPC_` records carry that wire layout, so their profiles claim
    /// `NpcStatModel::None` and the derivation must yield nothing — the
    /// AVIF facts a future capture-backed wiring will need stay pinned by
    /// the `fo76/starfield_stored_avif_outputs_resolve_on_shipped_master`
    /// corpus tests.
    #[test]
    fn later_creation_profiles_claim_no_population_until_captured() {
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
                Vec::new(),
                "{} claims no NPC stat model until its wire layout is captured (#4453)",
                profile.name()
            );
        }
    }

    // ── Creatures (#3390) ────────────────────────────────────────────

    fn creature(stats: CreatureStats) -> NpcRecord {
        NpcRecord {
            form_id: 0x0016_7EA7,
            is_creature: true,
            creature_stats: Some(stats),
            ..NpcRecord::default()
        }
    }

    /// The `VCrTier3GiantRadscorpionMedPers` stat block from
    /// `FalloutNV.esm`, decoded.
    fn radscorpion_stats() -> CreatureStats {
        CreatureStats {
            creature_type: 2,
            combat_skill: 65,
            magic_skill: 50,
            stealth_skill: 50,
            health: 150,
            damage: 60,
            attributes: [9, 6, 6, 6, 5, 3, 8],
        }
    }

    /// #3390 — a creature derives its seven SPECIAL and its Health from
    /// its own `DATA`, with no class involved. Pre-fix the whole FO3/FNV
    /// bestiary fell through the auto-calc arm's class lookup (`CNAM` is
    /// not a CLAS FormID on `CREA`) and derived nothing at all, which cost
    /// it `ActorVitals` and therefore made it untargetable by the melee
    /// slice.
    #[test]
    fn creature_derives_special_and_health_from_its_own_data() {
        // Note the class exists in the index and is deliberately NOT the
        // creature's — a creature must never reach the auto-calc arm.
        let index = fnv_index_with_class(0x2000, [1, 1, 1, 1, 1, 1, 1]);
        let pairs = derive_npc_actor_values(&creature(radscorpion_stats()), &index);

        let val = |name: &str| {
            let fid = index.actor_value_form_id(name).expect("AVIF");
            pairs
                .iter()
                .find(|(k, _)| *k == fid)
                .map(|(_, v)| *v)
                .unwrap_or_else(|| panic!("{name} missing from {pairs:?}"))
        };
        assert_eq!(val("Strength"), 9.0);
        assert_eq!(val("Perception"), 6.0);
        assert_eq!(val("Endurance"), 6.0);
        assert_eq!(val("Charisma"), 6.0);
        assert_eq!(val("Intelligence"), 5.0);
        assert_eq!(val("Agility"), 3.0);
        assert_eq!(val("Luck"), 8.0);
        assert_eq!(val("Health"), 150.0, "authored, not the NPC_ END curve");

        // 7 SPECIAL + Health. The three aggregate skills and Damage are
        // deliberately not emitted — no AVIF maps onto them.
        assert_eq!(pairs.len(), 8, "{pairs:?}");
    }

    /// An `NPC_` in the same index still takes the auto-calc path — the
    /// model is chosen per record, not per game.
    #[test]
    fn an_npc_beside_a_creature_still_auto_calcs() {
        let index = fnv_index_with_class(0x2000, [5, 5, 5, 5, 5, 5, 5]);
        let mut npc = npc_with_class(0x2000);
        npc.level = 1;

        let pairs = derive_npc_actor_values(&npc, &index);
        assert_eq!(pairs.len(), 21, "7 SPECIAL + 13 FNV skills + Health");
    }

    /// #5338 — with the ACBS auto-calculate bit clear, the actor's SPECIAL
    /// comes from its own authored `DATA` bytes, not the class ATTR. The
    /// vanilla shape is GhoulJasonBright: DATA END 7 against a class END 4,
    /// Base Health 80, level 6 → the wiki's 135, not the class-derived 120.
    #[test]
    fn authored_data_special_beats_the_class_when_auto_calc_is_off() {
        // Class END 4; authored DATA SPECIAL END 7 (Jason Bright's row).
        let index = fnv_index_with_class(0x2000, [5, 5, 4, 5, 5, 5, 5]);
        let mut npc = npc_with_class(0x2000);
        npc.acbs_flags = 0; // auto-calculate CLEAR
        npc.data_attributes = Some([6, 5, 7, 2, 6, 6, 3]);
        npc.data_base_health = Some(80);
        npc.level = 6;

        let pairs = derive_npc_actor_values(&npc, &index);
        let val = |name: &str| -> f32 {
            let fid = index.actor_value_form_id(name).unwrap();
            pairs.iter().find(|(f, _)| *f == fid).unwrap().1
        };
        assert_eq!(val("Strength"), 6.0, "authored DATA Strength");
        assert_eq!(val("Endurance"), 7.0, "authored DATA Endurance, not the class's 4");
        assert_eq!(
            val("Health"),
            135.0,
            "base 80 + 5·(DATA END 7 − 1) + 5·(L 6 − 1) — the wiki's value, \
             not the 120 the class's END 4 derives"
        );

        // The bit is the discriminator: with auto-calc set, the same DATA
        // bytes are the GECK's copy of the class and the class governs.
        let mut auto = npc.clone();
        auto.acbs_flags = 0x0010;
        let pairs = derive_npc_actor_values(&auto, &index);
        let fid = index.actor_value_form_id("Endurance").unwrap();
        assert_eq!(
            pairs.iter().find(|(f, _)| *f == fid).unwrap().1,
            4.0,
            "auto-calc ON → the class ATTR is the SPECIAL"
        );

        // A shell with the bit clear but no DATA falls back to the class —
        // resolve-or-fall-back, not resolve-or-empty.
        let mut shell = npc.clone();
        shell.data_attributes = None;
        let pairs = derive_npc_actor_values(&shell, &index);
        let fid = index.actor_value_form_id("Endurance").unwrap();
        assert_eq!(
            pairs.iter().find(|(f, _)| *f == fid).unwrap().1,
            4.0,
            "no authored DATA → the class remains the fallback"
        );
    }

    /// #5338 — the real-master pin the audit asked for: two hand-authored
    /// non-auto-calc FNV NPCs whose `DATA` SPECIAL differs from their
    /// class's, checked against the fandom infobox Hit Points values the
    /// audit cited. GhoulJasonBright 135, MartinaGroesbeck 80; Easy Pete
    /// (DATA == class) is the invariance control at 65.
    #[test]
    #[ignore = "needs FNV game data on disk"]
    fn fnv_authored_data_special_reproduces_the_wiki_hit_points() {
        let path = crate::esm::test_paths::fnv_esm();
        if !path.exists() {
            eprintln!("Skipping: FalloutNV.esm not found at {}", path.display());
            return;
        }
        let data = std::fs::read(&path).unwrap();
        let index = crate::esm::records::parse_esm(&data).expect("parse_esm");
        let health = index.health_actor_value_key().expect("Health AVIF");

        let expected = [
            ("GhoulJasonBright", 135.0, 7.0),
            ("MartinaGroesbeck", 80.0, 4.0),
            ("GSEasyPete", 65.0, 4.0),
        ];
        for (editor_id, want_health, want_endurance) in expected {
            let npc = index
                .npcs
                .values()
                .find(|npc| npc.editor_id == editor_id)
                .unwrap_or_else(|| panic!("{editor_id} must exist in FalloutNV.esm"));
            assert_eq!(
                npc.acbs_flags & 0x0010,
                0,
                "{editor_id} is the non-auto-calc shape this pin is about"
            );
            assert!(
                npc.data_attributes.is_some(),
                "{editor_id} must author the DATA SPECIAL block"
            );
            let pairs = derive_npc_actor_values(npc, &index);
            let val = |name: &str| -> f32 {
                let fid = index.actor_value_form_id(name).unwrap();
                pairs
                    .iter()
                    .find(|(f, _)| *f == fid)
                    .unwrap_or_else(|| panic!("{name} missing from {editor_id}'s pairs"))
                    .1
            };
            assert_eq!(val("Endurance"), want_endurance, "{editor_id} DATA END");
            assert_eq!(
                pairs.iter().find(|(f, _)| *f == health).unwrap().1,
                want_health,
                "{editor_id}: the wiki Hit Points value, via the authored DATA SPECIAL"
            );
        }
    }

    /// A `*DEAD` corpse prop authors `health = 0`; skipping Health there
    /// keeps the "unpopulated, not zero" contract and leaves the entity
    /// without `ActorVitals`, which is correct for scenery.
    #[test]
    fn creature_with_non_positive_health_still_derives_special() {
        let index = fnv_index_with_class(0x2000, [5; 7]);
        let stats = CreatureStats {
            health: 0,
            ..radscorpion_stats()
        };
        let pairs = derive_npc_actor_values(&creature(stats), &index);

        let health_key = index.actor_value_form_id("Health").unwrap();
        assert!(!pairs.iter().any(|(k, _)| *k == health_key));
        assert_eq!(pairs.len(), 7, "the seven SPECIAL survive");
    }

    /// A creature record with no `DATA` at all yields an empty set, the
    /// same "unpopulated" signal every other arm uses.
    #[test]
    fn creature_without_a_data_block_derives_nothing() {
        let index = fnv_index_with_class(0x2000, [5; 7]);
        let npc = NpcRecord {
            is_creature: true,
            ..NpcRecord::default()
        };
        assert!(derive_npc_actor_values(&npc, &index).is_empty());
    }

    /// Oblivion's `CREA` `DATA` is a different struct that has not been
    /// sourced, so its profile selects no creature model and the arm stays
    /// closed rather than mis-decoding.
    #[test]
    fn oblivion_creatures_select_no_stat_model() {
        assert_eq!(
            CharacterRulesProfile::OBLIVION.creature_stat_model(),
            byroredux_core::character::NpcStatModel::None,
        );
        let index = EsmIndex {
            character_rules: CharacterRulesProfile::OBLIVION,
            ..EsmIndex::default()
        };
        assert!(derive_npc_actor_values(&creature(radscorpion_stats()), &index).is_empty());
    }

    /// #3390 — `CREA.TPLT` points at `[CREA, LVLC]`, never `NPC_`. Before
    /// the walker learned those two maps it matched nothing for creatures,
    /// so 815 of 1578 FNV and 399 of 533 FO3 templated creatures derived
    /// their own shell block (health 50, SPECIAL all 5) instead of the
    /// base's authored stats.
    #[test]
    fn creature_use_stats_follows_a_crea_template() {
        let mut index = fnv_index_with_class(0x2000, [5; 7]);
        index
            .creatures
            .insert(0x3000, creature(radscorpion_stats()));

        let shell = NpcRecord {
            is_creature: true,
            template_form_id: 0x3000,
            template_flags: crate::equip::TEMPLATE_FLAG_USE_STATS,
            creature_stats: Some(CreatureStats {
                health: 50,
                attributes: [5; 7],
                ..CreatureStats::default()
            }),
            ..NpcRecord::default()
        };

        let pairs = derive_npc_actor_values(&shell, &index);
        let health_key = index.actor_value_form_id("Health").unwrap();
        assert_eq!(
            pairs
                .iter()
                .find(|(k, _)| *k == health_key)
                .map(|(_, v)| *v),
            Some(150.0),
            "the template's authored Health, not the shell's 50",
        );
        let str_key = index.actor_value_form_id("Strength").unwrap();
        assert_eq!(
            pairs.iter().find(|(k, _)| *k == str_key).map(|(_, v)| *v),
            Some(9.0),
        );
    }

    /// The `LVLC` half of the same chain — 429 of FNV's templated
    /// creatures and 130 of FO3's route through a leveled-creature list.
    #[test]
    fn creature_use_stats_follows_an_lvlc_template() {
        use crate::esm::records::{LeveledEntry, LeveledList};

        let mut index = fnv_index_with_class(0x2000, [5; 7]);
        index
            .creatures
            .insert(0x3000, creature(radscorpion_stats()));
        index.leveled_creatures.insert(
            0x4000,
            LeveledList {
                form_id: 0x4000,
                editor_id: "LvlRadscorpion".to_owned(),
                chance_none: 0,
                flags: 0,
                entries: vec![LeveledEntry {
                    level: 1,
                    form_id: 0x3000,
                    count: 1,
                }],
            },
        );

        let shell = NpcRecord {
            is_creature: true,
            level: 5,
            template_form_id: 0x4000,
            template_flags: crate::equip::TEMPLATE_FLAG_USE_STATS,
            creature_stats: Some(CreatureStats::default()),
            ..NpcRecord::default()
        };

        let health_key = index.actor_value_form_id("Health").unwrap();
        let pairs = derive_npc_actor_values(&shell, &index);
        assert_eq!(
            pairs
                .iter()
                .find(|(k, _)| *k == health_key)
                .map(|(_, v)| *v),
            Some(150.0),
            "LVLC leaf's Health must reach the shell",
        );
    }

    /// UESP *CLAS*: the priest's weights (Restoration 4, Speech 3, Enchanting / Illusion /
    /// Alchemy / Smithing 2): at level 2 (8 points, no complete set of 15) the leftover goes
    /// one per skill in weight order, then lower index first - a first round of one point to
    /// all six, a second to the two heaviest.
    #[test]
    fn skyrim_class_points_follow_the_documented_assignment_order() {
        // Level 2 earns 8 points. With three weight-2 skills the 2-point
        // remainder cuts the tied round after the two highest AVIFs.
        let mut tied_weights = [0u8; 18];
        tied_weights[0] = 2;
        tied_weights[1] = 2;
        tied_weights[2] = 2;
        let tied = skyrim_class_skill_points(&tied_weights, 2);
        assert_eq!((tied[0], tied[1], tied[2]), (2, 3, 3));

        let mut weights = [0u8; 18];
        for (index, weight) in [(22, 4), (17, 3), (23, 2), (21, 2), (16, 2), (10, 2)] {
            weights[index - 6] = weight;
        }
        let points = skyrim_class_skill_points(&weights, 2);
        assert_eq!(points[22 - 6], 2, "Restoration");
        assert_eq!(points[17 - 6], 2, "Speech");
        for index in [23, 21, 16, 10] {
            assert_eq!(points[index - 6], 1, "weight-2 skill {index}");
        }
        assert_eq!(points.iter().sum::<i32>(), 8);
        // Complete sets first: level 16 is 120 points = 8 sets of 15.
        let sets = skyrim_class_skill_points(&weights, 16);
        assert_eq!((sets[22 - 6], sets[17 - 6], sets[10 - 6]), (32, 24, 16));
        // Level 1 and an empty class gain nothing.
        assert_eq!(skyrim_class_skill_points(&weights, 1), [0; 18]);
        assert_eq!(skyrim_class_skill_points(&[0; 18], 30), [0; 18]);
    }

    /// An auto-calc Skyrim NPC's skills are 15 + the race's bonus + the class's points; one
    /// without the flag keeps its `DNAM` values as written.
    #[test]
    fn skyrim_skills_are_fifteen_plus_race_plus_class_points_or_the_stored_dnam() {
        let mut index = EsmIndex { character_rules: CharacterRulesProfile::SKYRIM, ..Default::default() };
        index.actor_values.insert(0x3E8, avif(0x3E8, "AVHealth"));
        for (offset, name) in SKYRIM_SKILLS.iter().enumerate() {
            index.actor_values.insert(0x400 + offset as u32, avif(0x400 + offset as u32, name));
        }
        let speech = 0x400 + 11;
        let restoration = 0x400 + 16;
        index.races.insert(
            0x13746,
            RaceRecord {
                form_id: 0x13746,
                starting_health: Some(50.0),
                // Speech +10 (actor-value index 17), Restoration +5 (22).
                skill_bonuses: vec![(17, 10), (22, 5)],
                ..Default::default()
            },
        );
        let mut weights = [0u8; 18];
        weights[17 - 6] = 3;
        weights[22 - 6] = 4;
        index.classes.insert(
            0x2000,
            crate::esm::records::ClassRecord { form_id: 0x2000, skyrim_skill_weights: Some(weights), ..Default::default() },
        );
        let value_of = |values: &[(u32, f32)], id: u32| values.iter().find(|(key, _)| *key == id).map(|(_, v)| *v);
        let auto = NpcRecord {
            race_form_id: 0x13746,
            class_form_id: 0x2000,
            level: 1,
            acbs_flags: 0x10,
            ..Default::default()
        };
        let values = derive_npc_actor_values(&auto, &index);
        assert_eq!(value_of(&values, speech), Some(25.0), "15 + 10 racial at level 1");
        assert_eq!(value_of(&values, restoration), Some(20.0));
        assert_eq!(value_of(&values, 0x400), Some(15.0), "an unweighted skill without a bonus");
        let leveled = NpcRecord { level: 8, ..auto.clone() };
        let values = derive_npc_actor_values(&leveled, &index);
        // 56 points over weights 3 + 4: 8 sets, no leftover: Speech +24, Restoration +32.
        assert_eq!(value_of(&values, speech), Some(15.0 + 10.0 + 24.0));
        assert_eq!(value_of(&values, restoration), Some(15.0 + 5.0 + 32.0));
        let mut stored_values = [0u8; 18];
        stored_values[17 - 6] = 41;
        let stored = NpcRecord { acbs_flags: 0, skill_values: Some(stored_values), ..auto };
        let values = derive_npc_actor_values(&stored, &index);
        assert_eq!(value_of(&values, speech), Some(41.0), "the stored value, no bonus added");
    }

    /// Every one of Skyrim's 18 skills names an `AVIF` in `Skyrim.esm`, and a real
    /// auto-calc NPC (Saadia) derives all 18 from her class.
    #[test]
    #[ignore = "needs Skyrim.esm on disk (~0.5 GB resident)"]
    fn vanilla_skyrim_skill_names_all_resolve_and_a_vanilla_npc_derives_them() {
        let path = crate::esm::test_paths::skyrim_se_esm();
        if !path.is_file() {
            if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
                panic!("BYROREDUX_REQUIRE_GAME_DATA is set, but Skyrim.esm was not found at {path:?}");
            }
            return;
        }
        let index = crate::esm::parse_esm(&std::fs::read(&path).unwrap()).unwrap();
        for name in SKYRIM_SKILLS {
            assert!(index.actor_value_form_id(name).is_some(), "no AVIF resolves {name}");
        }
        let with_weights = index.classes.values().filter(|class| class.skyrim_skill_weights.is_some()).count();
        assert!(with_weights > 50, "{with_weights} classes parsed skill weights");
        let npc = index.npcs.values().find(|npc| npc.editor_id == "Saadia").expect("Saadia");
        let values = derive_npc_actor_values(npc, &index);
        let skills = SKYRIM_SKILLS.iter().filter(|name| {
            let id = index.actor_value_form_id(name).unwrap();
            values.iter().any(|(key, _)| *key == id)
        });
        assert_eq!(skills.count(), 18, "all 18 skills derived for an auto-calc NPC: {values:?}");
    }
}
