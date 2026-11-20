//! The player's spell cast (player-spellcasting PRD, `docs/prd/player-spellcasting.md`; design in
//! `docs/engine/scripting.md`, "Player cast"): the readied spell, the gate, the spend.
//!
//! A cast has four stages: **ready** (a spell is selected: [`ReadiedSpell`]), **gate** (the player may cast it now,
//! each refusal named: [`CastRefusal`]), **resolve** (each effect finds its range), **settle** (Magicka and Fatigue
//! applied, the one [`cast_spell`](crate::magic_effects::cast_spell) call). Phase 1 resolves Self effects only: a spell
//! with a Touch or Target effect is refused with [`CastRefusal::Range`] until Phase 2 builds them (PSC-TGT-002, -003).
//!
//! The rules (cost, tier, Fatigue) are [`SpellCastModel`]'s (`crates/core/src/character/spell_cast.rs`), installed
//! from the player's rules profile; nothing here holds a constant. Only the player's path gates and spends: the
//! console's `magic.cast`, ObScript `Cast` and a perk's activate choice keep calling `cast_spell` unchanged (PSC-CST-007).

use byroredux_core::character::{EffectRange, SpellCastModel, SpellTier};
use byroredux_core::ecs::components::ActorValues;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::SpellType;

use crate::magic::{CanonicalSpell, SpellCatalog, SpellList};
use crate::magic_effects::cast_spell;

/// Oblivion's actor-value codes the cast reads (UESP *Oblivion:Actor Values*; the engine's
/// `OBLIVION_ACTOR_VALUES` roster): Luck, Magicka, Fatigue, and the six magic skills (20 Alteration to 25 Restoration).
const AV_LUCK: u32 = 7;
const AV_MAGICKA: u32 = 9;
const AV_FATIGUE: u32 = 10;
const AV_SCHOOL_FIRST: u32 = 20;

const SCHOOL_NAMES: [&str; 6] = ["Alteration", "Conjuration", "Destruction", "Illusion", "Mysticism", "Restoration"];
const TIER_NAMES: [&str; 5] = ["Novice", "Apprentice", "Journeyman", "Expert", "Master"];

/// The player's readied spell (PSC-RDY-001): the spell the cast action casts next, a form id in their [`SpellList`] or
/// none. Saved (PSC-RDY-004); a spell that leaves the list clears it ([`readied`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ReadiedSpell(pub Option<u32>);

impl Component for ReadiedSpell {
    type Storage = SparseSetStorage<Self>;
}

/// Why a cast did not start (PSC-CST-002). A refusal spends nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum CastRefusal {
    /// The player cannot act (dead, or not in character mode).
    CannotAct,
    /// The last cast is still inside the casting interval (PSC-CST-006).
    Cooldown { remaining: f32 },
    NothingReadied,
    /// The readied spell is not in the player's spell list.
    NotKnown,
    /// The game has no cast rules (everything but Oblivion), or the spell carries no cast data.
    NoRules,
    /// Not a Spell: a Power, Lesser Power, disease, ability or poison (PSC-NG-002).
    NotCastable(SpellType),
    /// The spell has a Touch or Target effect, which Phase 1 does not resolve.
    Range(EffectRange),
    /// The player's tier in the school is below the spell's mastery (PSC-CST-003).
    Mastery { school: Option<u8>, needs: SpellTier, has: SpellTier },
    /// The cost is greater than the player's current Magicka (PSC-CST-002).
    Magicka { cost: f32, has: f32 },
}

impl std::fmt::Display for CastRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CannotAct => write!(f, "cannot act"),
            Self::Cooldown { remaining } => write!(f, "cooldown {remaining:.2}s"),
            Self::NothingReadied => write!(f, "no spell readied"),
            Self::NotKnown => write!(f, "spell not in the spell list"),
            Self::NoRules => write!(f, "no cast rules for this game or spell"),
            Self::NotCastable(spell_type) => write!(f, "not castable ({spell_type:?})"),
            Self::Range(range) => write!(f, "{} range not supported yet", range.label()),
            Self::Mastery { school, needs, has } => write!(
                f,
                "mastery: needs {} in {}, has {}",
                TIER_NAMES[usize::from(*needs).min(4)],
                school_name(*school),
                TIER_NAMES[usize::from(*has).min(4)]
            ),
            Self::Magicka { cost, has } => write!(f, "magicka: costs {cost:.1}, has {has:.1}"),
        }
    }
}

/// The name of a school for a report.
pub fn school_name(school: Option<u8>) -> &'static str {
    school.and_then(|school| SCHOOL_NAMES.get(usize::from(school)).copied()).unwrap_or("None")
}

/// What a cast that started spent.
#[derive(Debug, Clone, PartialEq)]
pub struct CastStarted {
    /// The Magicka applied as damage.
    pub magicka: f32,
    /// The Fatigue applied as damage.
    pub fatigue: f32,
    pub school: Option<u8>,
    /// Whether `cast_spell` started any effect (an effect the runtime does not model starts none).
    pub effects_started: bool,
}

/// The result of one cast action, for `cast.status` and the log (PSC-DX-001, -003).
#[derive(Debug, Clone, PartialEq)]
pub struct CastReport {
    pub spell: u32,
    pub name: String,
    pub outcome: Result<CastStarted, CastRefusal>,
}

impl std::fmt::Display for CastReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.outcome {
            Ok(started) => write!(
                f,
                "started {} ({:08X}): magicka {:.1}, fatigue {:.1}, school {}, effects {}",
                self.name,
                self.spell,
                started.magicka,
                started.fatigue,
                school_name(started.school),
                if started.effects_started { "applied" } else { "none applied" }
            ),
            Err(refusal) => write!(f, "refused {} ({:08X}): {refusal}", self.name, self.spell),
        }
    }
}

/// Session state of the cast, not saved: the casting interval and the last report (`cast.status`).
#[derive(Debug, Clone, Default)]
pub struct CastState {
    /// Seconds until the next cast may start.
    pub cooldown_remaining: f32,
    /// The interval the last cast armed, for the report.
    pub interval: f32,
    pub last: Option<CastReport>,
}

impl Resource for CastState {}

/// Advance the casting interval by `dt` seconds.
pub fn tick_cast_state(world: &World, dt: f32) {
    if let Some(mut state) = world.try_resource_mut::<CastState>() {
        state.cooldown_remaining = (state.cooldown_remaining - dt.max(0.0)).max(0.0);
    }
}

/// An Oblivion actor-value code's AVIF form, when the load order resolves it.
fn av_form(world: &World, code: u32) -> Option<u32> {
    crate::actor_value_names::condition_actor_value_form(world, code)
}

/// The numbers a cast reads off the player: current Luck, current and base skill per school, current Magicka.
struct Caster {
    luck: f32,
    skills: [f32; 6],
    base_skills: [f32; 6],
    magicka: f32,
}

fn read_caster(world: &World, player: EntityId) -> Caster {
    let magicka_av = av_form(world, AV_MAGICKA);
    let luck_av = av_form(world, AV_LUCK);
    let mut caster = Caster { luck: 0.0, skills: [0.0; 6], base_skills: [0.0; 6], magicka: 0.0 };
    if let Some(values) = world.get::<ActorValues>(player) {
        caster.luck = luck_av.map_or(0.0, |av| values.current(av));
        caster.magicka = magicka_av.map_or(0.0, |av| values.current(av));
        for school in 0..6u32 {
            if let Some(av) = av_form(world, AV_SCHOOL_FIRST + school) {
                caster.skills[school as usize] = values.current(av);
                caster.base_skills[school as usize] = values.get_base(av).map_or(0.0, |entry| entry.base);
            }
        }
    }
    caster
}

/// The player's readied spell, cleared first when it has left their spell list (PSC-RDY-001).
pub fn readied(world: &World, player: EntityId) -> Option<u32> {
    let spell = world.get::<ReadiedSpell>(player).and_then(|readied| readied.0)?;
    let known = world.get::<SpellList>(player).is_some_and(|list| list.0.contains(&spell));
    if known {
        return Some(spell);
    }
    set_readied(world, player, None);
    None
}

fn set_readied(world: &World, player: EntityId, spell: Option<u32>) {
    if let Some(mut store) = world.query_mut::<ReadiedSpell>() {
        store.insert(player, ReadiedSpell(spell));
    }
}

/// The cast data of `spell` with the model that prices it, or why it has none.
fn cast_data(catalog: &SpellCatalog, spell: u32) -> Option<(&CanonicalSpell, &crate::magic::SpellCastData)> {
    let canonical = catalog.get(spell)?;
    Some((canonical, canonical.cast.as_ref()?))
}

/// What a spell costs the player now and which school it feeds, for `spell.ready` and `cast.status` (PSC-RDY-002).
#[derive(Debug, Clone, PartialEq)]
pub struct SpellPreview {
    pub name: String,
    pub school: Option<u8>,
    pub tier: SpellTier,
    pub cost: f32,
}

/// Price `spell` for the player.
pub fn preview(world: &World, player: EntityId, spell: u32) -> Result<SpellPreview, CastRefusal> {
    let model = *world.try_resource::<SpellCastModel>().ok_or(CastRefusal::NoRules)?;
    let catalog = world.try_resource::<SpellCatalog>().ok_or(CastRefusal::NoRules)?;
    let (_, data) = cast_data(&catalog, spell).ok_or(CastRefusal::NoRules)?;
    let caster = read_caster(world, player);
    Ok(SpellPreview {
        name: data.name.clone(),
        school: model.primary_school(&data.effects),
        tier: model.spell_tier(&data.effects, data.manual()),
        cost: model.actual_cost(&data.effects, data.manual().map(|(cost, _)| cost), |school| caster.skills[usize::from(school).min(5)], caster.luck),
    })
}

/// Ready a spell the player knows (PSC-RDY-002). Refuses a spell not in the list and one that is not castable.
pub fn ready_spell(world: &World, player: EntityId, spell: u32) -> Result<SpellPreview, CastRefusal> {
    if !world.get::<SpellList>(player).is_some_and(|list| list.0.contains(&spell)) {
        return Err(CastRefusal::NotKnown);
    }
    let catalog = world.try_resource::<SpellCatalog>().ok_or(CastRefusal::NoRules)?;
    let (_, data) = cast_data(&catalog, spell).ok_or(CastRefusal::NoRules)?;
    if data.spell_type != SpellType::Spell {
        return Err(CastRefusal::NotCastable(data.spell_type));
    }
    drop(catalog);
    let preview = preview(world, player, spell)?;
    set_readied(world, player, Some(spell));
    Ok(preview)
}

/// The player's castable spells in list order (PSC-RDY-003).
pub fn castable_spells(world: &World, player: EntityId) -> Vec<u32> {
    let Some(list) = world.get::<SpellList>(player) else { return Vec::new() };
    let Some(catalog) = world.try_resource::<SpellCatalog>() else { return Vec::new() };
    list.0.iter().copied().filter(|spell| cast_data(&catalog, *spell).is_some_and(|(_, data)| data.spell_type == SpellType::Spell)).collect()
}

/// Cycle the readied spell by `step` (+1 next, -1 previous) through the castable spells in list order, wrapping; with
/// nothing readied the first (next) or last (previous) is chosen (PSC-RDY-003).
pub fn cycle_spell(world: &World, player: EntityId, step: i32) -> Result<SpellPreview, CastRefusal> {
    let spells = castable_spells(world, player);
    if spells.is_empty() {
        return Err(CastRefusal::NothingReadied);
    }
    let len = spells.len() as i32;
    let next = match readied(world, player).and_then(|spell| spells.iter().position(|candidate| *candidate == spell)) {
        Some(at) => (at as i32 + step).rem_euclid(len) as usize,
        None if step >= 0 => 0,
        None => spells.len() - 1,
    };
    ready_spell(world, player, spells[next])
}

/// One cast action (PSC-CST-001..009): gate in order, then settle. `can_act` is the engine's "the player may act" verdict
/// (alive, in character mode) and `interval` the casting interval a started cast arms (PSC-CST-006). The report is also
/// stored in [`CastState`].
pub fn cast_readied(world: &World, player: EntityId, can_act: bool, interval: f32) -> CastReport {
    let spell = readied(world, player);
    let name = spell.and_then(|spell| world.try_resource::<SpellCatalog>().and_then(|catalog| cast_data(&catalog, spell).map(|(_, data)| data.name.clone()))).unwrap_or_default();
    let outcome = match spell {
        _ if !can_act => Err(CastRefusal::CannotAct),
        None => Err(CastRefusal::NothingReadied),
        Some(spell) => settle(world, player, spell, interval),
    };
    let report = CastReport { spell: spell.unwrap_or(0), name, outcome };
    if let Some(mut state) = world.try_resource_mut::<CastState>() {
        state.last = Some(report.clone());
    }
    log::info!("cast: {report}");
    report
}

fn settle(world: &World, player: EntityId, spell: u32, interval: f32) -> Result<CastStarted, CastRefusal> {
    if let Some(state) = world.try_resource::<CastState>()
        && state.cooldown_remaining > 0.0
    {
        return Err(CastRefusal::Cooldown { remaining: state.cooldown_remaining });
    }
    let model = *world.try_resource::<SpellCastModel>().ok_or(CastRefusal::NoRules)?;
    let (school, cost) = {
        let catalog = world.try_resource::<SpellCatalog>().ok_or(CastRefusal::NoRules)?;
        let (_, data) = cast_data(&catalog, spell).ok_or(CastRefusal::NoRules)?;
        if data.spell_type != SpellType::Spell {
            return Err(CastRefusal::NotCastable(data.spell_type));
        }
        if let Some(effect) = data.effects.iter().find(|effect| effect.range != EffectRange::SelfRange) {
            return Err(CastRefusal::Range(effect.range));
        }
        let caster = read_caster(world, player);
        let school = model.primary_school(&data.effects);
        let needs = model.spell_tier(&data.effects, data.manual());
        let has = model.player_tier(school.map_or(0.0, |school| caster.base_skills[usize::from(school).min(5)]));
        if has < needs {
            return Err(CastRefusal::Mastery { school, needs, has });
        }
        let cost = model.actual_cost(&data.effects, data.manual().map(|(cost, _)| cost), |school| caster.skills[usize::from(school).min(5)], caster.luck);
        if cost > caster.magicka {
            return Err(CastRefusal::Magicka { cost, has: caster.magicka });
        }
        (school, cost)
    };
    let fatigue = model.fatigue_cost(cost);
    // Settle: the spend first (one write, the lock released before the effect start), then the shared effect start.
    let (magicka_av, fatigue_av) = (av_form(world, AV_MAGICKA), av_form(world, AV_FATIGUE));
    if let Some(mut values) = world.query_mut::<ActorValues>()
        && let Some(entry) = values.get_mut(player)
    {
        if let Some(av) = magicka_av {
            entry.apply_damage(av, cost);
        }
        if let Some(av) = fatigue_av {
            entry.apply_damage(av, fatigue);
        }
    }
    let effects_started = cast_spell(world, Some(player), player, spell);
    if let Some(mut state) = world.try_resource_mut::<CastState>() {
        state.cooldown_remaining = interval.max(0.0);
        state.interval = interval;
    }
    Ok(CastStarted { magicka: cost, fatigue, school, effects_started })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::magic::{CanonicalSpell, SpellCastData};
    use crate::magic_effects::{ActiveMagicEffects, CastEffect};
    use byroredux_core::character::EffectCostInput;
    use std::collections::HashMap;

    const SELF_SPELL: u32 = 0x10;
    const EXPERT_SPELL: u32 = 0x11;
    const TARGET_SPELL: u32 = 0x12;
    const POWER: u32 = 0x13;
    const INTERVAL: f32 = 0.45;

    fn effect(base_cost: f32, magnitude: f32, range: EffectRange, school: u8) -> EffectCostInput {
        EffectCostInput { base_cost, magnitude, duration: 0.0, area: 0.0, range, mgef_flags: 0, school: Some(school) }
    }

    fn spell(name: &str, spell_type: SpellType, effects: Vec<EffectCostInput>) -> CanonicalSpell {
        CanonicalSpell {
            // A lasting effect lands, so `cast_spell` starts it on the caster.
            effects: vec![CastEffect { mgef: 1, script: 0, duration: 30.0, magnitude: 1.0, value: None, conditions: None }],
            cast: Some(SpellCastData { name: name.into(), spell_type, effects, ..Default::default() }),
            ..Default::default()
        }
    }

    /// A player with Magicka 50, Fatigue 100, Luck 50 and every school at `skill`, knowing four spells:
    /// a cheap Self spell (cost 9.5 base, 13 at skill 0), a Journeyman Self spell (95), a Target spell and a Power.
    fn world_with(skill: f32, magicka: f32) -> (World, EntityId) {
        let mut world = World::new();
        world.register::<ReadiedSpell>();
        world.register::<ActiveMagicEffects>();
        let mut spells = HashMap::new();
        spells.insert(SELF_SPELL, spell("Heat Shell", SpellType::Spell, vec![effect(5.0, 10.0, EffectRange::SelfRange, 0)]));
        spells.insert(EXPERT_SPELL, spell("Big Shell", SpellType::Spell, vec![effect(50.0, 10.0, EffectRange::SelfRange, 3)]));
        spells.insert(TARGET_SPELL, spell("Flare", SpellType::Spell, vec![effect(7.5, 6.0, EffectRange::Target, 2)]));
        spells.insert(POWER, spell("Lover's Kiss", SpellType::Power, vec![effect(5.0, 10.0, EffectRange::SelfRange, 3)]));
        world.insert_resource(SpellCatalog::from_canonical(spells));
        world.insert_resource(SpellCastModel::OBLIVION);
        world.insert_resource(CastState::default());
        let player = world.spawn();
        let mut pairs = vec![(AV_LUCK, 50.0), (AV_MAGICKA, magicka), (AV_FATIGUE, 100.0)];
        pairs.extend((0..6).map(|school| (AV_SCHOOL_FIRST + school, skill)));
        world.insert(player, ActorValues::from_pairs(pairs));
        world.insert(player, SpellList(vec![SELF_SPELL, EXPERT_SPELL, TARGET_SPELL, POWER]));
        (world, player)
    }

    fn magicka(world: &World, player: EntityId) -> f32 {
        world.get::<ActorValues>(player).unwrap().current(AV_MAGICKA)
    }

    fn refusal(report: &CastReport) -> &CastRefusal {
        report.outcome.as_ref().expect_err("the cast was refused")
    }

    #[test]
    fn cast_refuses_each_gate_in_order() {
        let (world, player) = world_with(0.0, 50.0);
        assert_eq!(refusal(&cast_readied(&world, player, false, INTERVAL)), &CastRefusal::CannotAct);
        assert_eq!(refusal(&cast_readied(&world, player, true, INTERVAL)), &CastRefusal::NothingReadied);
        // A spell the player does not know cannot be readied, and one that left the list clears.
        assert_eq!(ready_spell(&world, player, 0x99).unwrap_err(), CastRefusal::NotKnown);
        ready_spell(&world, player, SELF_SPELL).unwrap();
        world.query_mut::<SpellList>().unwrap().get_mut(player).unwrap().0.retain(|spell| *spell != SELF_SPELL);
        assert_eq!(readied(&world, player), None);
        assert_eq!(refusal(&cast_readied(&world, player, true, INTERVAL)), &CastRefusal::NothingReadied);
        // A Power is not castable here.
        assert_eq!(ready_spell(&world, player, POWER).unwrap_err(), CastRefusal::NotCastable(SpellType::Power));
        // A Target spell is refused until Phase 2.
        ready_spell(&world, player, TARGET_SPELL).unwrap();
        assert_eq!(refusal(&cast_readied(&world, player, true, INTERVAL)), &CastRefusal::Range(EffectRange::Target));
        // Mastery before Magicka: a Journeyman spell with skill 0.
        ready_spell(&world, player, EXPERT_SPELL).unwrap();
        assert_eq!(refusal(&cast_readied(&world, player, true, INTERVAL)), &CastRefusal::Mastery { school: Some(3), needs: 2, has: 0 });
        // Nothing was spent by any refusal.
        assert_eq!(magicka(&world, player), 50.0);
        assert_eq!(world.get::<ActorValues>(player).unwrap().current(AV_FATIGUE), 100.0);
    }

    #[test]
    fn a_self_cast_spends_magicka_and_one_fatigue_and_starts_its_effect() {
        let (world, player) = world_with(0.0, 50.0);
        ready_spell(&world, player, SELF_SPELL).unwrap();
        let report = cast_readied(&world, player, true, INTERVAL);
        let started = report.outcome.clone().expect("the cast started");
        // 0.1 * 5 * 10^1.28 = 9.52, times 1.4 at skill 0 and Luck 50: floor(13.3) = 13.
        assert_eq!(started.magicka, 13.0);
        assert_eq!(started.fatigue, 1.0);
        assert_eq!(started.school, Some(0));
        assert!(started.effects_started);
        assert_eq!(magicka(&world, player), 37.0);
        assert_eq!(world.get::<ActorValues>(player).unwrap().current(AV_FATIGUE), 99.0);
        assert!(world.get::<ActiveMagicEffects>(player).is_some());
        assert_eq!(world.try_resource::<CastState>().unwrap().last.as_ref(), Some(&report));
    }

    #[test]
    fn a_repeat_inside_the_interval_is_refused_with_the_seconds_left() {
        let (world, player) = world_with(0.0, 50.0);
        ready_spell(&world, player, SELF_SPELL).unwrap();
        assert!(cast_readied(&world, player, true, INTERVAL).outcome.is_ok());
        assert_eq!(refusal(&cast_readied(&world, player, true, INTERVAL)), &CastRefusal::Cooldown { remaining: INTERVAL });
        tick_cast_state(&world, 0.3);
        assert!(matches!(refusal(&cast_readied(&world, player, true, INTERVAL)), CastRefusal::Cooldown { remaining } if (*remaining - 0.15).abs() < 1e-5));
        tick_cast_state(&world, 0.2);
        assert!(cast_readied(&world, player, true, INTERVAL).outcome.is_ok());
        assert_eq!(magicka(&world, player), 24.0, "two casts of 13");
    }

    #[test]
    fn the_skill_lowers_the_cost_and_the_base_skill_sets_the_tier() {
        // The Journeyman spell (base cost 95) needs skill 50; at skill 50 it costs floor(95 * (1.4 - 0.6)) = 76.
        let (world, player) = world_with(50.0, 200.0);
        ready_spell(&world, player, EXPERT_SPELL).unwrap();
        let started = cast_readied(&world, player, true, INTERVAL).outcome.expect("a Journeyman casts it");
        assert_eq!(started.magicka, 76.0);
        // A Fortify raises the modified skill (the cost) and not the base skill (the tier).
        let (world, player) = world_with(40.0, 200.0);
        world.query_mut::<ActorValues>().unwrap().get_mut(player).unwrap().mod_temporary(AV_SCHOOL_FIRST + 3, 30.0);
        ready_spell(&world, player, EXPERT_SPELL).unwrap();
        assert_eq!(refusal(&cast_readied(&world, player, true, INTERVAL)), &CastRefusal::Mastery { school: Some(3), needs: 2, has: 1 });
    }

    #[test]
    fn a_cost_equal_to_magicka_is_allowed_and_a_greater_one_is_refused() {
        let (world, player) = world_with(0.0, 13.0);
        ready_spell(&world, player, SELF_SPELL).unwrap();
        assert!(cast_readied(&world, player, true, INTERVAL).outcome.is_ok());
        assert_eq!(magicka(&world, player), 0.0);
        let (world, player) = world_with(0.0, 12.9);
        ready_spell(&world, player, SELF_SPELL).unwrap();
        assert_eq!(refusal(&cast_readied(&world, player, true, INTERVAL)), &CastRefusal::Magicka { cost: 13.0, has: 12.9 });
        assert_eq!(magicka(&world, player), 12.9);
    }

    #[test]
    fn ready_reports_the_name_school_tier_and_actual_cost() {
        let (world, player) = world_with(0.0, 50.0);
        let preview = ready_spell(&world, player, EXPERT_SPELL).unwrap();
        assert_eq!(preview, SpellPreview { name: "Big Shell".into(), school: Some(3), tier: 2, cost: 133.0 });
        assert_eq!(readied(&world, player), Some(EXPERT_SPELL));
    }

    #[test]
    fn next_and_previous_cycle_the_castable_spells_in_list_order_and_wrap() {
        let (world, player) = world_with(0.0, 50.0);
        assert_eq!(castable_spells(&world, player), vec![SELF_SPELL, EXPERT_SPELL, TARGET_SPELL]);
        let name = |preview: SpellPreview| preview.name;
        assert_eq!(name(cycle_spell(&world, player, 1).unwrap()), "Heat Shell");
        assert_eq!(name(cycle_spell(&world, player, 1).unwrap()), "Big Shell");
        assert_eq!(name(cycle_spell(&world, player, 1).unwrap()), "Flare");
        assert_eq!(name(cycle_spell(&world, player, 1).unwrap()), "Heat Shell", "wraps past the last");
        assert_eq!(name(cycle_spell(&world, player, -1).unwrap()), "Flare", "wraps before the first");
        let (world, player) = world_with(0.0, 50.0);
        assert_eq!(name(cycle_spell(&world, player, -1).unwrap()), "Flare", "previous with nothing readied is the last");
    }

    /// PSC-CST-007: the console's `magic.cast`, ObScript `Cast` and a perk's activate choice call `cast_spell` directly;
    /// it starts the effects and spends nothing, so only the player's path charges.
    #[test]
    fn the_shared_effect_start_charges_nothing() {
        let (world, player) = world_with(0.0, 50.0);
        assert!(cast_spell(&world, Some(player), player, SELF_SPELL));
        assert_eq!(magicka(&world, player), 50.0);
        assert_eq!(world.get::<ActorValues>(player).unwrap().current(AV_FATIGUE), 100.0);
        assert!(world.get::<ActiveMagicEffects>(player).is_some(), "the effect started");
    }

    #[test]
    fn a_game_without_cast_rules_refuses_every_cast() {
        let (mut world, player) = world_with(0.0, 50.0);
        ready_spell(&world, player, SELF_SPELL).unwrap();
        world.remove_resource::<SpellCastModel>();
        assert_eq!(refusal(&cast_readied(&world, player, true, INTERVAL)), &CastRefusal::NoRules);
    }
}
