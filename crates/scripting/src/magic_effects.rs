//! The caster runtime of M47.3: spells, ingestibles and abilities that put an *effect* on an actor, and
//! the effects that keep running on it — the `ScriptEffectStart` / `ScriptEffectUpdate` /
//! `ScriptEffectFinish` blocks of a scripted magic effect, value-modifier effects over their duration,
//! `OnMagicEffectHit`, and the ObScript commands that start and stop them (`Cast`, `CastImmediateOnSelf`,
//! `Dispel`, `DispelAllSpells`, `HasMagicEffect`, `ScriptEffectElapsedSeconds`).
//!
//! **What an effect is.** A `SPEL` / `ALCH` effect names a magic effect (`MGEF`), a magnitude and a
//! duration. Its script comes from one of two places: Fallout 3 and New Vegas author it on the `MGEF`
//! (archetype *Script*, whose "Assoc. Script" is the `SCPT` — xEdit `wbMGEFFAssocItemDecider`,
//! archetype 1), Oblivion on the spell's effect item (`SCIT`). Its value change, for the value-modifier
//! archetype, is `magnitude` on the actor value the effect names, negative when the effect is flagged
//! *Detrimental*.
//!
//! **How it runs** (the rules the CK / GECK give, and `magic.rs` already follows):
//! * a script effect runs `ScriptEffectStart` once when it begins, `ScriptEffectUpdate` every frame it is
//!   active, and `ScriptEffectFinish` once when it ends — the duration elapsed, or `Dispel` / death. An
//!   effect with no duration runs Start and Finish in the same frame;
//! * a value-modifier effect flagged *Recover* changes the value once at the start and back at the end
//!   ("a Recover effect will modify their actor value once at the start, then modify it back once the
//!   Effect expires"); one that is not changes the value every second it lasts, and an instant one (no
//!   duration) once;
//! * a constant effect (an ability's) has no duration and lasts until the spell is removed.
//!
//! The script's variables live with the effect, not with the actor: an actor can carry an object script and
//! several effect scripts that reuse a name. The calling reference of the script is the actor the effect
//! is on.
//!
//! Not modelled: an effect's archetype beyond script and value modifier (summons, light, paralysis,
//! cure, dispel-by-effect …), area and touch/target delivery (every effect lands on its target at once),
//! magic resistance, and the visual (`PlayMagicShaderVisuals` and friends have their own sink).

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::components::ActorValues;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{
    EsmIndex, ItemKind, MagicArchetype, MagicEffectItem, ScriptRecord, ScriptType, MGEF_FLAG_DETRIMENTAL, MGEF_FLAG_RECOVER,
};

use crate::obscript_quests::ObScriptCommandHost;
use crate::obscript_vm::{BlockInfo, ObScriptProgram, VmState, BLOCK_SCRIPT_EFFECT_FINISH, BLOCK_SCRIPT_EFFECT_START, BLOCK_SCRIPT_EFFECT_UPDATE};

/// The value change an effect makes on an actor value.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ValueEffect {
    /// The AVIF FormID keying `ActorValues`.
    pub actor_value: u32,
    /// Signed: negative for a detrimental effect.
    pub amount: f32,
    /// *Recover*: change once at the start, back at the end — otherwise every second.
    pub recover: bool,
}

/// One effect of a spell or ingestible as the runtime needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CastEffect {
    /// The `MGEF` the effect names (in Oblivion, the one its 4-char code resolves to).
    pub mgef: u32,
    /// The `SCPT` of a scripted effect, else 0.
    pub script: u32,
    /// Seconds; 0 is instant.
    pub duration: f32,
    pub magnitude: f32,
    pub value: Option<ValueEffect>,
}

impl CastEffect {
    /// Whether the effect does something this runtime models.
    pub fn is_modelled(&self) -> bool {
        self.script != 0 || self.value.is_some()
    }
}

/// Translate one authored effect item (see the module docs for where its script comes from).
pub fn cast_effect(index: &EsmIndex, item: &MagicEffectItem) -> CastEffect {
    let mgef = index.resolve_magic_effect(item.effect_form_id);
    let script = if item.script_form_id != 0 {
        item.script_form_id
    } else {
        mgef.filter(|mgef| mgef.archetype == Some(MagicArchetype::Other(1)))
            .map(|mgef| mgef.associated_item)
            .filter(|&script| index.scripts.get(&script).is_some_and(|record| record.script_type == ScriptType::MagicEffect))
            .unwrap_or(0)
    };
    let value = mgef.and_then(|mgef| {
        if !item.magnitude.is_finite() || !matches!(mgef.archetype, Some(MagicArchetype::ValueModifier)) {
            return None;
        }
        let actor_value = mgef.primary_actor_value.and_then(|reference| index.resolve_actor_value(reference))?;
        let sign = if mgef.effect_flags & MGEF_FLAG_DETRIMENTAL != 0 { -1.0 } else { 1.0 };
        Some(ValueEffect { actor_value, amount: sign * item.magnitude, recover: mgef.effect_flags & MGEF_FLAG_RECOVER != 0 })
    });
    CastEffect { mgef: mgef.map_or(item.effect_form_id, |mgef| mgef.form_id), script, duration: item.duration as f32, magnitude: item.magnitude, value }
}

/// The effects of an ingestible (`ALCH`): its authored effect list.
pub fn consumable_cast_effects(index: &EsmIndex, form_id: u32) -> Vec<CastEffect> {
    let Some(item) = index.items.get(&form_id) else { return Vec::new() };
    let ItemKind::Aid { authored_effects: Some(effects), .. } = &item.kind else { return Vec::new() };
    effects.iter().map(|entry| cast_effect(index, &entry.effect)).collect()
}

/// An effect that is on an actor. Saved.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ActiveEffect {
    /// The spell, ingestible or ability that put it there (`Dispel` takes a source).
    pub source: u32,
    pub mgef: u32,
    /// The caster's reference FormID, 0 for none.
    pub caster: u32,
    pub script: u32,
    pub duration: f32,
    /// A constant effect has no end.
    pub indefinite: bool,
    pub elapsed: f32,
    pub started: bool,
    /// Told to stop (`Dispel`): ends at the next pass whatever its duration.
    pub ending: bool,
    pub value: Option<ValueEffect>,
    /// Whole seconds of a per-second value change already applied.
    pub ticks: u32,
    /// The script's locals, by slot — the effect's own, not the actor's.
    pub locals: Vec<(u16, f32)>,
    /// Its reference variables, by slot (exact form ids).
    pub ref_locals: Vec<(u16, u32)>,
}

/// The effects running on an actor.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ActiveMagicEffects(pub Vec<ActiveEffect>);

impl Component for ActiveMagicEffects {
    type Storage = SparseSetStorage<Self>;
}

/// The load order's magic-effect scripts by `SCPT` FormID. `NOT_SAVED_BY_DESIGN`: rebuilt from the records
/// at every content load.
#[derive(Default)]
pub struct EffectScriptTable {
    scripts: HashMap<u32, (Arc<ScriptRecord>, Vec<BlockInfo>)>,
}

impl Resource for EffectScriptTable {}

/// Index the scripts of type *Magic Effect* that have bytecode and walk cleanly. Returns how many.
pub fn install_effect_scripts(world: &mut World, scripts: &HashMap<u32, ScriptRecord>) -> usize {
    let mut table = EffectScriptTable::default();
    for script in scripts.values() {
        if script.script_type != ScriptType::MagicEffect || script.compiled.is_empty() {
            continue;
        }
        let Some(blocks) = ObScriptProgram::new(script).blocks() else { continue };
        table.scripts.insert(script.form_id, (Arc::new(script.clone()), blocks));
    }
    let count = table.scripts.len();
    world.insert_resource(table);
    count
}

/// The seconds the running effect script has been active, for `ScriptEffectElapsedSeconds`. Set only while
/// an effect script's block runs. `NOT_SAVED_BY_DESIGN`: transient.
#[derive(Debug, Default, Clone, Copy)]
pub struct ScriptEffectRun {
    pub elapsed: Option<f32>,
}

impl Resource for ScriptEffectRun {}

/// `OnMagicEffectHit` events waiting for the target's object script: `(target, mgef, caster)`.
/// `NOT_SAVED_BY_DESIGN`: drained the frame they are raised.
#[derive(Debug, Default)]
pub struct PendingMagicHits(pub Vec<(EntityId, u32, u32)>);

impl Resource for PendingMagicHits {}

/// Put `effects` on `target`, cast by `caster`, from `source`. A constant effect (`indefinite`) has no end.
/// Returns how many the runtime took (the modelled ones).
pub fn start_effects(world: &World, target: EntityId, caster: Option<EntityId>, source: u32, effects: &[CastEffect], indefinite: bool) -> usize {
    let caster_form = caster.and_then(|caster| crate::obscript_state::reference_form(world, caster)).unwrap_or(0);
    let target_form = crate::obscript_state::reference_form(world, target).unwrap_or(0);
    let taken: Vec<ActiveEffect> = effects
        .iter()
        .filter(|effect| effect.is_modelled())
        .map(|effect| ActiveEffect {
            source,
            mgef: effect.mgef,
            caster: caster_form,
            script: effect.script,
            duration: effect.duration,
            indefinite,
            elapsed: 0.0,
            started: false,
            ending: false,
            value: effect.value,
            ticks: 0,
            locals: Vec::new(),
            ref_locals: Vec::new(),
        })
        .collect();
    if taken.is_empty() {
        return 0;
    }
    let count = taken.len();
    if let Some(mut hits) = world.try_resource_mut::<PendingMagicHits>() {
        hits.0.extend(taken.iter().map(|effect| (target, effect.mgef, caster_form)));
    }
    let _ = target_form;
    let Some(mut active) = world.query_mut::<ActiveMagicEffects>() else { return 0 };
    match active.get_mut(target) {
        Some(existing) => existing.0.extend(taken),
        None => {
            active.insert(target, ActiveMagicEffects(taken));
        }
    }
    count
}

/// Cast `spell` on `target`: every modelled effect of it lands. `false` when the spell is unknown or has
/// nothing this runtime models.
pub fn cast_spell(world: &World, caster: Option<EntityId>, target: EntityId, spell: u32) -> bool {
    let effects = match world.try_resource::<crate::magic::SpellCatalog>().and_then(|catalog| catalog.get(spell).map(|s| s.effects.clone())) {
        Some(effects) => effects,
        None => return false,
    };
    let started = start_effects(world, target, caster, spell, &effects, false) > 0;
    // M43 — the Story Manager's `CAST` (Cast Magic Event): `OnStoryCastMagic(akCastingActor,
    // akSpellTarget, akLocation, akSpell)`. R1 = the caster, L1 = the session location. The CK page:
    // "akSpellTarget always returns none" — so R2 stays unset whatever the cast's target (the quests'
    // `SpellTarget` alias is optional for the same reason). The spell is a form the event slots do not
    // carry; the real `CAST` quests read only the caster and the location. Only a cast with a caster
    // raises it.
    if let (true, Some(caster)) = (started, caster) {
        let location = world.try_resource::<crate::location::PlayerLocation>().and_then(|here| here.0);
        log::debug!("story manager: CAST raised (caster {caster}, location {location:?})");
        crate::story_manager::raise_story_event(
            world,
            caster,
            crate::story_manager::StoryEvent {
                mnemonic: *b"CAST",
                reference_1: caster,
                reference_2: None,
                location_1: location,
                location_2: None,
                scripted: None,
            },
        );
    }
    started
}

/// Stop the effects `source` put on `target` (every effect when `source` is `None`): their `Finish` blocks
/// run and recovered values are given back at the next [`magic_effect_system`] pass, which sees them
/// marked finished. Returns how many were ended.
pub fn dispel(world: &World, target: EntityId, source: Option<u32>) -> usize {
    let Some(mut active) = world.query_mut::<ActiveMagicEffects>() else { return 0 };
    let Some(effects) = active.get_mut(target) else { return 0 };
    let mut ended = 0;
    for effect in effects.0.iter_mut().filter(|effect| !effect.ending && source.is_none_or(|source| effect.source == source)) {
        effect.ending = true;
        ended += 1;
    }
    ended
}

/// Whether `target` has an active effect of `mgef` (`HasMagicEffect`).
pub fn has_effect(world: &World, target: EntityId, mgef: u32) -> bool {
    world.get::<ActiveMagicEffects>(target).is_some_and(|active| active.0.iter().any(|effect| effect.mgef == mgef))
}

fn change_value(world: &World, target: EntityId, change: impl FnOnce(&mut ActorValues)) {
    if let Some(mut values) = world.query_mut::<ActorValues>() {
        if let Some(values) = values.get_mut(target) {
            change(values);
        }
    }
}

/// Run one block of an effect script on `target`, keeping the effect's own locals.
fn run_effect_block(world: &World, target: EntityId, effect: &mut ActiveEffect, script: &ScriptRecord, block: BlockInfo, dt: f32) {
    let Some(target_form) = crate::obscript_state::reference_form(world, target) else { return };
    let mut state = VmState {
        locals: effect.locals.iter().copied().collect(),
        ref_locals: effect.ref_locals.iter().copied().collect(),
    };
    if let Some(mut run) = world.try_resource_mut::<ScriptEffectRun>() {
        run.elapsed = Some(effect.elapsed);
    }
    let mut host = ObScriptCommandHost::new(world, target_form, Some(target), dt, script);
    host.action_ref = (effect.caster != 0).then_some(effect.caster);
    let outcome = ObScriptProgram::new(script).run_at(&block, &mut state, &mut host);
    if let Some(mut run) = world.try_resource_mut::<ScriptEffectRun>() {
        run.elapsed = None;
    }
    if let crate::obscript_vm::BlockOutcome::Malformed(at) = outcome {
        log::warn!("ObScript effect script {:08X}: malformed bytecode at {at:#06x} (block {:#04x})", script.form_id, block.block_type);
    }
    let mut locals: Vec<(u16, f32)> = state.locals.into_iter().collect();
    locals.sort_by_key(|(slot, _)| *slot);
    effect.locals = locals;
    let mut refs: Vec<(u16, u32)> = state.ref_locals.into_iter().collect();
    refs.sort_by_key(|(slot, _)| *slot);
    effect.ref_locals = refs;
}

/// Advance every active effect by `dt`: start the ones that have not, run their `Update` blocks, apply
/// per-second value changes, and end the ones whose time is up. Exclusive: scripts write the same state
/// the other Update systems read, so nothing is held across a run.
pub fn magic_effect_system(world: &World, dt: f32) {
    let targets: Vec<EntityId> = match world.query::<ActiveMagicEffects>() {
        Some(active) if !active.is_empty() => active.iter().map(|(entity, _)| entity).collect(),
        _ => return,
    };
    let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
    for target in targets {
        let mut index = 0;
        // One effect at a time, re-reading the list: a script may `Dispel` or cast.
        while let Some(mut effect) = world.get::<ActiveMagicEffects>(target).and_then(|active| active.0.get(index).cloned()) {
            let original = effect.clone();
            let script = world
                .try_resource::<EffectScriptTable>()
                .and_then(|table| table.scripts.get(&effect.script).map(|(record, blocks)| (Arc::clone(record), blocks.clone())));
            let run_blocks = |world: &World, effect: &mut ActiveEffect, block_type: u16| {
                if let Some((record, blocks)) = &script {
                    for block in blocks.iter().filter(|block| block.block_type == block_type) {
                        run_effect_block(world, target, effect, record, *block, dt);
                    }
                }
            };
            if !effect.started {
                effect.started = true;
                if let Some(value) = effect.value {
                    if effect.duration <= 0.0 && !effect.indefinite {
                        // Instant: one change of the value itself.
                        change_value(world, target, |values| {
                            if value.amount >= 0.0 {
                                values.restore(value.actor_value, value.amount);
                            } else {
                                values.apply_damage(value.actor_value, -value.amount);
                            }
                        });
                    } else if value.recover {
                        change_value(world, target, |values| values.mod_permanent(value.actor_value, value.amount));
                    }
                }
                run_blocks(world, &mut effect, BLOCK_SCRIPT_EFFECT_START);
            } else {
                effect.elapsed += dt;
            }
            let finished = effect.ending || (!effect.indefinite && effect.elapsed >= effect.duration);
            if !finished || effect.duration > 0.0 {
                run_blocks(world, &mut effect, BLOCK_SCRIPT_EFFECT_UPDATE);
            }
            // Per-second value changes: one for each whole second that has gone by.
            if let Some(value) = effect.value.filter(|value| !value.recover && (effect.duration > 0.0 || effect.indefinite)) {
                let due = effect.elapsed.floor().max(0.0) as u32;
                let owed = due.saturating_sub(effect.ticks);
                effect.ticks = effect.ticks.max(due);
                if owed > 0 {
                    change_value(world, target, |values| {
                        for _ in 0..owed {
                            if value.amount >= 0.0 {
                                values.restore(value.actor_value, value.amount);
                            } else {
                                values.apply_damage(value.actor_value, -value.amount);
                            }
                        }
                    });
                }
            }
            if finished {
                run_blocks(world, &mut effect, BLOCK_SCRIPT_EFFECT_FINISH);
                if let Some(value) = effect.value.filter(|value| value.recover && (effect.duration > 0.0 || effect.indefinite)) {
                    change_value(world, target, |values| values.mod_permanent(value.actor_value, -value.amount));
                }
            }
            // #6232 — any value change above (an instant or per-second damage, a
            // Drain, a Fortify given back) may have taken Health to zero, and
            // nothing else sweeps actors at Health <= 0. Queue the death every
            // other producer does; the caster is the killer.
            if effect.value.is_some() {
                let caster = effect.caster;
                crate::fragment::queue_health_death_by(world, target, || {
                    crate::condition::resolve_entity_by_global_form_id(world, caster)
                        .filter(|&killer| caster != 0 && killer != target)
                });
            }
            // Write back — unless a script changed the list under us (a Dispel), which wins.
            if let Some(mut active) = world.query_mut::<ActiveMagicEffects>() {
                if let Some(list) = active.get_mut(target) {
                    let same = list.0.get(index).is_some_and(|current| *current == original);
                    if same {
                        if finished {
                            list.0.remove(index);
                            continue;
                        }
                        list.0[index] = effect;
                    }
                }
            }
            index += 1;
        }
        if let Some(mut active) = world.query_mut::<ActiveMagicEffects>() {
            if active.get(target).is_some_and(|list| list.0.is_empty()) {
                active.remove(target);
            }
        }
    }
}

/// Deliver the `OnMagicEffectHit` events effects raised when they landed, to the target's object script:
/// the block written for an effect runs for that effect (`Begin OnMagicEffectHit SomeEffect`), one with
/// no parameter for any. The caster is the action reference.
pub fn magic_hit_system(world: &World, dt: f32) {
    let hits = match world.try_resource_mut::<PendingMagicHits>() {
        Some(mut pending) if !pending.0.is_empty() => std::mem::take(&mut pending.0),
        _ => return,
    };
    for (target, mgef, caster) in hits {
        crate::obscript_objects::deliver_magic_effect_hit(world, target, mgef, caster, dt);
    }
}

/// This subsystem's resources and components.
pub fn install(world: &mut World) {
    world.register::<ActiveMagicEffects>();
    world.init_resource::<ScriptEffectRun>();
    world.init_resource::<PendingMagicHits>();
    world.init_resource::<EffectScriptTable>();
}

/// This subsystem's saved column. Called by the binary's `build_save_registry` in its row order.
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    registry.register_replacing_component::<ActiveMagicEffects>("ActiveMagicEffects");
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::{ActorValueRef, AvifRecord, MgefRecord, SpelRecord};

    const HEALTH: u32 = 0x450;

    fn world_with(mut index: EsmIndex) -> (World, EntityId) {
        index.game = GameKind::Fallout3NV;
        let mut world = World::new();
        crate::register(&mut world);
        crate::install_actor_value_names(&mut world, &index);
        world.insert_resource(crate::magic::SpellCatalog::from_index(&index));
        install_effect_scripts(&mut world, &index.scripts);
        let mut pool = byroredux_core::form_id::FormIdPool::new();
        let id = pool.intern(byroredux_core::form_id::FormIdPair {
            plugin: byroredux_core::form_id::PluginId::from_filename("FalloutNV.esm"),
            local: byroredux_core::form_id::LocalFormId(0x1234),
        });
        world.insert_resource(pool);
        let target = world.spawn();
        world.insert(target, byroredux_core::ecs::components::FormIdComponent(id));
        world.insert(target, ActorValues::from_pairs([(HEALTH, 100.0)]));
        (world, target)
    }

    fn index_with_value_effect(flags: u32, magnitude: f32, duration: u32) -> EsmIndex {
        let mut index = EsmIndex { game: GameKind::Fallout3NV, ..Default::default() };
        index.actor_values.insert(HEALTH, AvifRecord { form_id: HEALTH, editor_id: "AVHealth".into(), ..Default::default() });
        index.magic_effects.insert(
            0x77,
            MgefRecord {
                form_id: 0x77,
                effect_flags: flags,
                archetype: Some(MagicArchetype::ValueModifier),
                primary_actor_value: Some(ActorValueRef::Form(HEALTH)),
                ..Default::default()
            },
        );
        index.spells.insert(
            0x500,
            SpelRecord {
                form_id: 0x500,
                effects: vec![MagicEffectItem { effect_form_id: 0x77, magnitude, area: 0, duration, script_form_id: 0 }],
                ..Default::default()
            },
        );
        index
    }

    fn health(world: &World, target: EntityId) -> f32 {
        world.get::<ActorValues>(target).unwrap().current(HEALTH)
    }

    #[test]
    fn an_instant_effect_changes_the_value_once_and_leaves_nothing_running() {
        let (world, target) = world_with(index_with_value_effect(0, -30.0 * -1.0, 0));
        // Magnitude 30, not detrimental: restores — the actor is at full, so damage first.
        world.query_mut::<ActorValues>().unwrap().get_mut(target).unwrap().apply_damage(HEALTH, 50.0);
        assert!(cast_spell(&world, None, target, 0x500));
        magic_effect_system(&world, 0.016);
        assert_eq!(health(&world, target), 80.0);
        assert!(world.get::<ActiveMagicEffects>(target).is_none());
    }

    #[test]
    fn a_per_second_effect_ticks_each_whole_second_and_ends_at_its_duration() {
        // Detrimental 10 a second for 3 seconds.
        let (world, target) = world_with(index_with_value_effect(MGEF_FLAG_DETRIMENTAL, 10.0, 3));
        assert!(cast_spell(&world, None, target, 0x500));
        magic_effect_system(&world, 0.0);
        assert_eq!(health(&world, target), 100.0, "nothing before the first second");
        magic_effect_system(&world, 1.0);
        assert_eq!(health(&world, target), 90.0);
        magic_effect_system(&world, 2.0);
        assert_eq!(health(&world, target), 70.0);
        assert!(world.get::<ActiveMagicEffects>(target).is_none(), "ended at 3 s");
    }

    fn pending_kills(world: &World) -> Vec<(EntityId, Option<EntityId>, bool)> {
        world.resource::<crate::story_manager::PendingScriptKills>().0.clone()
    }

    /// M43 — a cast with a caster raises `CAST` (caster `R1`; the CK: the spell target "always returns
    /// none", so `R2` stays unset); a cast with no caster raises nothing.
    #[test]
    fn a_cast_raises_the_cast_magic_story_event() {
        let (mut world, target) = world_with(index_with_value_effect(MGEF_FLAG_DETRIMENTAL, 5.0, 0));
        crate::story_manager::register(&mut world);
        let caster = world.spawn();
        let casts = |world: &World| -> Vec<crate::story_manager::StoryEvent> {
            world
                .query::<crate::story_manager::StoryEvent>()
                .map(|events| events.iter().map(|(_, e)| *e).filter(|e| e.mnemonic == *b"CAST").collect())
                .unwrap_or_default()
        };
        assert!(cast_spell(&world, None, target, 0x500));
        assert!(casts(&world).is_empty(), "no caster, no event");
        assert!(cast_spell(&world, Some(caster), target, 0x500));
        let events = casts(&world);
        assert_eq!(events.len(), 1);
        let slots = events[0].slots();
        assert_eq!((slots.reference_1, slots.reference_2), (Some(caster), None), "the target is never reported");
    }

    /// #6232 — a lethal instant effect queues the death every other Health
    /// writer does, crediting the caster; a survivable one queues nothing.
    #[test]
    fn a_lethal_instant_effect_queues_one_death_crediting_the_caster() {
        let (mut world, target) = world_with(index_with_value_effect(MGEF_FLAG_DETRIMENTAL, 150.0, 0));
        world.insert(target, byroredux_core::ecs::components::ActorVitals { health: HEALTH });
        let caster = world.spawn();
        let mut pool = world.resource_mut::<byroredux_core::form_id::FormIdPool>();
        let caster_id = pool.intern(byroredux_core::form_id::FormIdPair {
            plugin: byroredux_core::form_id::PluginId::from_filename("FalloutNV.esm"),
            local: byroredux_core::form_id::LocalFormId(0x5678),
        });
        drop(pool);
        world.insert(caster, byroredux_core::ecs::components::FormIdComponent(caster_id));
        assert!(cast_spell(&world, Some(caster), target, 0x500));
        magic_effect_system(&world, 0.016);
        assert!(health(&world, target) <= 0.0);
        assert_eq!(pending_kills(&world), vec![(target, Some(caster), false)]);
    }

    #[test]
    fn a_lethal_per_second_effect_queues_the_death_on_the_killing_tick() {
        let (mut world, target) = world_with(index_with_value_effect(MGEF_FLAG_DETRIMENTAL, 60.0, 3));
        world.insert(target, byroredux_core::ecs::components::ActorVitals { health: HEALTH });
        assert!(cast_spell(&world, None, target, 0x500));
        magic_effect_system(&world, 0.0);
        magic_effect_system(&world, 1.0);
        assert!(pending_kills(&world).is_empty(), "40 health left after the first tick");
        magic_effect_system(&world, 1.0);
        assert_eq!(pending_kills(&world), vec![(target, None, false)]);
    }

    #[test]
    fn a_recover_effect_changes_the_value_at_the_start_and_gives_it_back_at_the_end() {
        // Fortify (not detrimental, Recover) +25 for 5 s.
        let (world, target) = world_with(index_with_value_effect(MGEF_FLAG_RECOVER, 25.0, 5));
        assert!(cast_spell(&world, None, target, 0x500));
        magic_effect_system(&world, 0.0);
        assert_eq!(health(&world, target), 125.0);
        magic_effect_system(&world, 4.0);
        assert_eq!(health(&world, target), 125.0, "still active");
        magic_effect_system(&world, 1.5);
        assert_eq!(health(&world, target), 100.0);
        assert!(world.get::<ActiveMagicEffects>(target).is_none());
    }

    fn statement(op: u16, payload: &[u8]) -> Vec<u8> {
        let mut out = op.to_le_bytes().to_vec();
        out.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    /// `set counter to counter + 1` (a local in slot 1).
    fn counting_body() -> Vec<u8> {
        let mut expr = vec![b' ', b's', 1, 0];
        expr.extend_from_slice(&[b' ', b'n', 1, 0, 0, 0]);
        expr.extend_from_slice(b"+ ");
        let mut set = vec![b's', 1, 0];
        set.extend_from_slice(&(expr.len() as u16).to_le_bytes());
        set.extend_from_slice(&expr);
        statement(0x15, &set)
    }

    fn effect_script(blocks: &[u16]) -> ScriptRecord {
        let mut compiled = statement(0x1d, &[]);
        for &block in blocks {
            let mut begin = block.to_le_bytes().to_vec();
            begin.extend_from_slice(&0u32.to_le_bytes());
            compiled.extend(statement(0x10, &begin));
            compiled.extend(counting_body());
            compiled.extend(statement(0x11, &[]));
        }
        ScriptRecord {
            form_id: 0x900,
            editor_id: "TestEffectScript".into(),
            script_type: ScriptType::MagicEffect,
            compiled_size: compiled.len() as u32,
            compiled,
            locals: vec![byroredux_plugin::esm::records::ScriptLocalVar { index: 1, var_type: 2, name: "counter".into() }],
            ..Default::default()
        }
    }

    #[test]
    fn a_script_effect_runs_start_once_update_each_frame_and_finish_at_the_end() {
        let mut index = EsmIndex { game: GameKind::Fallout3NV, ..Default::default() };
        // An effect whose script the spell names (Oblivion's SCIT shape).
        index.spells.insert(
            0x501,
            SpelRecord {
                form_id: 0x501,
                effects: vec![MagicEffectItem { effect_form_id: 0x78, magnitude: 1.0, area: 0, duration: 2, script_form_id: 0x900 }],
                ..Default::default()
            },
        );
        index.scripts.insert(0x900, effect_script(&[BLOCK_SCRIPT_EFFECT_START, BLOCK_SCRIPT_EFFECT_UPDATE, BLOCK_SCRIPT_EFFECT_FINISH]));
        let (world, target) = world_with(index);
        let counter = |world: &World| {
            world
                .get::<ActiveMagicEffects>(target)
                .and_then(|active| active.0.first().and_then(|effect| effect.locals.iter().find(|(slot, _)| *slot == 1).map(|(_, v)| *v)))
        };
        assert!(cast_spell(&world, None, target, 0x501));
        magic_effect_system(&world, 0.5);
        assert_eq!(counter(&world), Some(2.0), "Start then this frame's Update");
        magic_effect_system(&world, 0.5);
        assert_eq!(counter(&world), Some(3.0), "one more Update");
        magic_effect_system(&world, 1.5);
        assert!(world.get::<ActiveMagicEffects>(target).is_none(), "Finish ran and the effect ended at 2 s");
    }

    #[test]
    fn dispel_ends_a_running_effect_and_gives_a_recovered_value_back() {
        let (world, target) = world_with(index_with_value_effect(MGEF_FLAG_RECOVER, 25.0, 60));
        assert!(cast_spell(&world, None, target, 0x500));
        magic_effect_system(&world, 0.0);
        assert!(has_effect(&world, target, 0x77));
        assert_eq!(dispel(&world, target, Some(0x500)), 1);
        magic_effect_system(&world, 0.0);
        assert_eq!(health(&world, target), 100.0);
        assert!(!has_effect(&world, target, 0x77));
        // A spell the target is not under has nothing to dispel.
        assert_eq!(dispel(&world, target, Some(0x999)), 0);
    }
}
