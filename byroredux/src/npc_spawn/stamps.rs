//! Component stamps from a ResolvedNpc: faction ranks, spells, actor values, creature attack, ruleset / VATS / melee builders.

use super::*;

/// Stamp a [`FactionRanks`] component on the NPC's placement root from its
/// `NPC_` `SNAM` faction list, so the M47.1 `GetFactionRank` condition can
/// read it (#1665). No-op when the NPC declares no factions. Faction ids are
/// carried verbatim from the record (NPC source space) — see `FactionRanks`.
/// #4093 (D5-02) — resolves the `Use Factions` TPLT chain before reading
/// membership, the same pattern `stamp_character_components` already uses
/// for `Use Stats`/`Use Traits`: a templated shell with `Use Factions` set
/// and its own (typically empty) `FACT` list previously never inherited
/// the template's membership.
pub(super) fn stamp_faction_ranks(world: &mut World, placement_root: EntityId, resolved: &ResolvedNpc<'_>) {
    if let Some(ranks) = faction_ranks_of(resolved) {
        world.insert(placement_root, ranks);
    }
}

/// #4414 — stamp the actor's `AIDT` combat disposition, read through the
/// resolved "Use AI Data" terminal, so ambient faction hostility can decide
/// whether it starts combat. No-op when the game's `AIDT` is not decoded
/// (Oblivion's 0–100 scale, Starfield) — such an actor never starts combat
/// on its own.
///
/// #4817 — called at job finalize, not when the placement root is created.
/// A resumable job yields across frames, and a root that already carried the
/// disposition was a perceiver for `faction_hostility_system` before its body
/// existed and before the caller restored its parked `Dead`: an evicted
/// corpse could start combat, move and strike while its cell re-streamed.
/// Finalize and the caller's restore run in one synchronous step, so no
/// system tick sees the disposition without the rest of the actor.
pub(super) fn arm_combat_disposition(
    world: &mut World,
    placement_root: EntityId,
    resolved: &ResolvedNpc<'_>,
) {
    if let Some(ai) = resolved.ai_data.ai_data {
        world.insert(placement_root, crate::systems::CombatDisposition(ai));
    }
}

/// The resolved record's faction membership, `None` when it declares none.
/// Shared by the NPC stamp above and the player's seed (#4699), so both
/// read membership through the same TPLT resolution.
pub(crate) fn faction_ranks_of(resolved: &ResolvedNpc<'_>) -> Option<FactionRanks> {
    // #4457 — the "Use Factions" terminal arrives pre-resolved from the
    // population boundary (`spawn_placement_root`); this cannot read the
    // shell's own list by accident.
    if resolved.factions.factions.is_empty() {
        return None;
    }
    Some(FactionRanks::from_pairs(
        resolved
            .factions
            .factions
            .iter()
            .map(|f| (f.faction_form_id, f.rank)),
    ))
}

/// #4415 — stamp the actor's [`SpellList`](byroredux_scripting::SpellList)
/// (own + race `SPLO`, leveled lists resolved at its level) and apply its
/// constant spells' permanent value changes to the `ActorValues`
/// [`stamp_actor_values`] just inserted — through the same canonical spell
/// translation a scripted `AddSpell` uses.
///
/// #4822 — inserted even when empty. `magic::add_spell` needs a `SpellList`
/// to add to, so an actor whose own and racial `SPLO` are empty (mostly
/// creatures) silently dropped every quest `AddSpell`, and nothing was saved.
/// The player gets one unconditionally for the same reason.
pub(super) fn stamp_spell_list(
    world: &mut World,
    placement_root: EntityId,
    resolved: &ResolvedNpc<'_>,
    index: &EsmIndex,
) {
    let spells = byroredux_plugin::equip::resolve_actor_spells(resolved, index);
    // #4415 — the racial subset on its own component, so the scripted
    // AddRaceSpells / RemoveRaceSpells pair can re-apply / clear exactly
    // it without an index at apply time.
    let racial = byroredux_plugin::equip::resolve_racial_spells(resolved, index);
    world.insert(placement_root, byroredux_scripting::RaceSpells(racial));
    if spells.is_empty() {
        world.insert(placement_root, byroredux_scripting::SpellList(spells));
        return;
    }
    if let Some(values) =
        world.get_mut::<byroredux_core::ecs::components::ActorValues>(placement_root)
    {
        for &spell in &spells {
            let canonical = byroredux_scripting::magic::canonical_spell(index, spell);
            byroredux_scripting::magic::apply_modifiers(values, &canonical.constant_modifiers, 1.0);
        }
    }
    world.insert(placement_root, byroredux_scripting::SpellList(spells));
}

/// Stamp an [`ActorValues`] component on the NPC's placement root, derived
/// from the active game's authored/derived character rules, so condition and
/// combat systems read the same values. No-op when the derivation yields
/// nothing or the index cannot resolve the required records.
pub(super) fn stamp_actor_values(
    world: &mut World,
    placement_root: EntityId,
    resolved: &ResolvedNpc<'_>,
    index: &EsmIndex,
) {
    let pairs = byroredux_plugin::esm::records::derive_resolved_actor_values(resolved, index);
    if pairs.is_empty() {
        return;
    }
    let health = index
        .health_actor_value_key()
        .filter(|health| pairs.iter().any(|(form_id, _)| form_id == health));
    world.insert(
        placement_root,
        byroredux_core::ecs::components::ActorValues::from_pairs(pairs),
    );
    if let Some(health) = health {
        world.insert(
            placement_root,
            byroredux_core::ecs::components::ActorVitals { health },
        );
    }
}

/// Stamp a [`CreatureAttack`] on the placement root from the `CREA`
/// record's authored `DATA.Damage` (#3762).
///
/// `CreatureStats::damage` reached the parser and stopped there. #3390 gave
/// creatures SPECIAL + Health, but the one number that defines a creature's
/// attack had no reader — this stamp makes it available to a future combat
/// consumer. **Corrected (#4105 / D5-03), updated for #4324:** that consumer
/// now exists. `attack_damage` is called by `combat_input_system` for the
/// player and by `npc_combat_ai_system` for any actor carrying an
/// `AiCombatState`, so `HitEvent` has two producers and a creature in combat
/// strikes with the `DATA.Damage` this stamps (692 FNV / 186 FO3 creatures
/// author a non-zero value). `AiCombatState` has two producers too: a quest
/// fragment's `StartCombat`, and, since #4414, `faction_hostility_system`,
/// which starts combat for any actor whose stamped `CombatDisposition` and
/// faction relations say it attacks what it sees — creatures included, wherever
/// their `AIDT` decodes. The value is therefore reached ambiently, not only
/// through a scripted `StartCombat`.
///
/// No-op for `NPC_` (no `creature_stats`) and for a creature whose `DATA`
/// leaves damage at zero or negative: absence means "no authored attack",
/// which keeps the existing `UNARMED_DAMAGE` baseline the answer rather
/// than materialising an actor that attacks for nothing.
///
/// #4091 (D1-01) — reads the `Use Stats`-resolved record, the same way its
/// three sibling stamps (`stamp_actor_values` via `derive_npc_actor_values`,
/// `stamp_character_components`) and `derive_creature_actor_values` already
/// do. `CREA.DATA` (SPECIAL/Health/damage) rides the same `Use Stats`
/// (`0x0002`) bit as the rest of the creature's stat block; reading the raw
/// shell here let one entity get its SPECIAL/Health from the template and
/// its attack damage from the shell — the fifth instance of this exact
/// defect class (#2956, #3381, #3382, #3480 were the first four).
pub(super) fn stamp_creature_attack(
    world: &mut World,
    placement_root: EntityId,
    resolved: &ResolvedNpc<'_>,
) {
    // #4091 (D1-01) — the "Use Stats" terminal arrives pre-resolved
    // (#4457). `CREA.DATA` (SPECIAL/Health/damage) rides the same `Use
    // Stats` (`0x0002`) bit as the rest of the creature's stat block;
    // reading the raw shell here would let one entity get its
    // SPECIAL/Health from the template and its attack damage from the
    // shell — the fifth instance of this exact defect class (#2956,
    // #3381, #3382, #3480 were the first four).
    let Some(stats) = resolved.stats.creature_stats else {
        return;
    };
    if stats.damage <= 0 {
        return;
    }
    world.insert(
        placement_root,
        byroredux_core::ecs::components::CreatureAttack {
            damage: f32::from(stats.damage),
        },
    );
}

/// Stamp the CHARAL structural components — [`CharacterLevel`],
/// [`Background`], and [`Perks`] — on the NPC's placement root. Level +
/// race/class come from every game's `NPC_`; perks from FO4+ `PRKR`.
/// Complements [`stamp_actor_values`] (the numeric substrate): together they
/// land the full canonical character state an actor carries at spawn.
///
/// #2956 — `Use Stats`/`Use Traits` `TPLT` template inheritance is resolved
/// first, the same way [`resolve_inherited_inventory`] already resolves
/// `Use Inventory`: a `Lvl*` shell NPC's own level/class/race are frequently
/// not what the engine actually uses once its `template_flags` says to
/// inherit them.
///
/// [`resolve_inherited_inventory`]: byroredux_plugin::equip::resolve_inherited_inventory
pub(super) fn stamp_character_components(world: &mut World, placement_root: EntityId, resolved: &ResolvedNpc<'_>) {
    use byroredux_core::character::{Background, CharacterLevel, PerkRank, Perks};

    // #4457 — both terminals arrive pre-resolved from the population
    // boundary; the #2956 `Use Stats`/`Use Traits` resolution cannot be
    // skipped or re-walked here.
    let stats_npc = resolved.stats;
    let traits_npc = resolved.r#traits;

    // Level: the resolved stats source's level (its own when `Use Stats`
    // isn't set or doesn't resolve). NPCs carry no XP.
    //
    // #2955 — routed through `effective_actor_level` so a PC-level-multiplier
    // record contributes its ACBS `calcMin`, not the raw multiplier. Writing
    // the multiplier here put 268 FNV / ~190 FO3 base actors two to three
    // orders of magnitude out, and this field feeds `DerivedInput::LEVEL`,
    // the leveling model and the M45 save snapshot.
    world.insert(
        placement_root,
        CharacterLevel {
            level: effective_actor_level(stats_npc).max(0) as u16,
            xp: 0,
        },
    );
    // Provenance: race (from the Use-Traits source) + class (from the
    // Use-Stats source; 0 = none), reused by runtime leveling.
    world.insert(
        placement_root,
        Background {
            race_form_id: traits_npc.race_form_id,
            class_form_id: stats_npc.class_form_id,
        },
    );
    // Perks (FO4+ `PRKR`) — skip the component entirely when the NPC has none.
    // Perks ride no template flag, so they come from the shell.
    if !resolved.shell.perks.is_empty() {
        world.insert(
            placement_root,
            Perks {
                entries: resolved
                    .shell
                    .perks
                    .iter()
                    .map(|&(perk_form_id, rank)| PerkRank { perk_form_id, rank })
                    .collect(),
            },
        );
    }
}

/// Build the per-game [`CharacterRuleset`](byroredux_core::character::CharacterRuleset)
/// from the parsed AVIF set. The parser-selected character profile owns the
/// per-game ruleset choice; this consumer only supplies authored FormID
/// resolution.
pub fn build_character_ruleset(
    index: &EsmIndex,
) -> Option<byroredux_core::character::CharacterRuleset> {
    let resolve = |editor_id: &str| index.actor_value_form_id(editor_id);
    let gmst = |editor_id: &str| index.game_setting_float(editor_id);
    index.character_rules.build_ruleset(resolve, gmst, &index.plugin_files)
}

/// Build the per-game VATS resources (#6350): the load-order-priced
/// [`VatsModel`](byroredux_core::character::VatsModel), and — when the
/// load order resolves an `ActionPoints` AVIF — the regen lane for
/// [`PoolRegenConfig`](byroredux_core::character::regen::PoolRegenConfig)
/// (its first production consumer; the TES lanes stay inert because no
/// Fallout load order resolves Fatigue/Magicka AVIFs). `None` on every
/// game whose profile carries no VATS row.
pub fn build_vats_model(
    index: &EsmIndex,
) -> Option<(
    byroredux_core::character::VatsModel,
    byroredux_core::character::regen::PoolRegenConfig,
)> {
    // The model — including which FO3-vs-FNV restore-rate dialect applies —
    // is the profile's own row (#6423): no game test, no whole-profile
    // comparison. `None` on every game with no sourced and wired VATS.
    let gmst = |editor_id: &str| index.game_setting_float(editor_id);
    let model = index.character_rules.vats()?.with_gmst(gmst);
    let action_points = index
        .actor_value_form_id("ActionPoints")
        .map(|avif| byroredux_core::character::regen::ActionPointsRegen {
            avif,
            per_second: model.ap_regen_per_second(),
        });
    // Fatigue/Magicka/Willpower are TES pools: no Fallout load order
    // resolves them, so the per-actor AV gates keep those lanes inert —
    // but they still must name *something* for the Copy struct.
    Some((
        model,
        byroredux_core::character::regen::PoolRegenConfig {
            fatigue_avif: 0,
            magicka_avif: 0,
            willpower_avif: 0,
            action_points,
        },
    ))
}

/// Build the per-game [`MeleeDamageConfig`](byroredux_core::character::MeleeDamageConfig)
/// resource — the resolved AVIF id for the AVIF-backed additive Melee Damage
/// row (`STR × 0.5` on FO3/FNV, #3092). `None` means this game authors no
/// `MeleeDamage` AVIF (FO4, TES) — the combat consumer treats that exactly
/// like a missing [`CharacterRuleset`](byroredux_core::character::CharacterRuleset)
/// row: fall back to the flat weapon/unarmed baseline.
pub fn build_melee_damage_config(
    index: &EsmIndex,
) -> Option<byroredux_core::character::MeleeDamageConfig> {
    let resolve = |editor_id: &str| index.actor_value_form_id(editor_id);
    byroredux_core::character::melee_damage_config(resolve)
}
