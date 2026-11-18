//! Portable actor-value keys at the save boundary.
use byroredux_core::ecs::components::{
    ActorValue, ActorValueAlias, ActorValues, ActorVitals, TimedRestoration, TimedRestorations,
};
use byroredux_core::ecs::World;
use byroredux_core::form_id::FormIdPair;
use byroredux_save::SaveError;
use byroredux_sdk::identity::FormRef;
use serde::{Deserialize, Serialize};

use crate::load_order::LoadOrderIdentity;

/// A sorted list avoids JSON object keys that cannot represent `FormIdPair`.
#[derive(Debug, Serialize, Deserialize)]
pub struct SavedActorValues(
    pub Vec<(FormIdPair, ActorValue)>,
    pub Vec<(FormIdPair, FormIdPair, FormIdPair)>,
);

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedActorVitals {
    pub health: FormIdPair,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedTimedRestoration {
    source_form_id: u32,
    actor_value: FormIdPair,
    per_second: f32,
    remaining: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedTimedRestorations(pub Vec<SavedTimedRestoration>);

pub fn save_key(
    world: &World,
    entity: u32,
    key: u32,
    column: &'static str,
) -> Result<FormIdPair, SaveError> {
    let identity = world
        .try_resource::<LoadOrderIdentity>()
        .ok_or(SaveError::MissingResource("LoadOrderIdentity"))?;
    identity
        .pair(key)
        .filter(|pair| pair.local.0 != 0)
        .ok_or(SaveError::UnownedFormId {
            column,
            entity,
            form: key,
        })
}

pub fn load_key(
    world: &World,
    entity: u32,
    pair: FormIdPair,
    column: &'static str,
) -> Result<Option<u32>, SaveError> {
    let identity = world
        .try_resource::<LoadOrderIdentity>()
        .ok_or(SaveError::MissingResource("LoadOrderIdentity"))?;
    let form = FormRef::new(pair.plugin.0.to_be_bytes(), pair.local.0);
    let key = identity.global_form_id(form);
    if key.is_none() {
        log::warn!("load: {column} entity {entity}: actor-value identity {pair:?} is unavailable; dropping it");
    }
    Ok(key)
}

pub fn save_values(
    world: &World,
    entity: u32,
    values: &ActorValues,
) -> Result<SavedActorValues, SaveError> {
    let mut rows = values
        .stored_iter()
        .map(|(key, value)| Ok((save_key(world, entity, key, "ActorValues")?, *value)))
        .collect::<Result<Vec<_>, SaveError>>()?;
    rows.sort_by_key(|(pair, _)| (pair.plugin, pair.local.0));
    let mut aliases = values
        .aliases()
        .map(|(key, alias)| {
            Ok((
                save_key(world, entity, key, "ActorValues alias")?,
                save_key(world, entity, alias.current, "ActorValues alias current")?,
                save_key(world, entity, alias.base, "ActorValues alias base")?,
            ))
        })
        .collect::<Result<Vec<_>, SaveError>>()?;
    aliases.sort_by_key(|(pair, _, _)| (pair.plugin, pair.local.0));
    Ok(SavedActorValues(rows, aliases))
}

pub fn load_values(
    world: &World,
    entity: u32,
    values: SavedActorValues,
) -> Result<Option<ActorValues>, SaveError> {
    let mut rows = Vec::with_capacity(values.0.len());
    for (pair, value) in values.0 {
        if let Some(key) = load_key(world, entity, pair, "ActorValues")? {
            rows.push((key, value));
        }
    }
    let mut restored = ActorValues::from_entries(rows);
    for (key, current, base) in values.1 {
        let (Some(key), Some(current), Some(base)) = (
            load_key(world, entity, key, "ActorValues alias")?,
            load_key(world, entity, current, "ActorValues alias current")?,
            load_key(world, entity, base, "ActorValues alias base")?,
        ) else {
            continue;
        };
        if !restored.set_alias(key, ActorValueAlias { current, base }) {
            log::warn!("load: ActorValues entity {entity}: alias {key:#010x} has unavailable or chained sources; dropping it");
        }
    }
    Ok(Some(restored))
}

pub fn save_vitals(
    world: &World,
    entity: u32,
    vitals: &ActorVitals,
) -> Result<SavedActorVitals, SaveError> {
    Ok(SavedActorVitals {
        health: save_key(world, entity, vitals.health, "ActorVitals")?,
    })
}

pub fn load_vitals(
    world: &World,
    entity: u32,
    vitals: SavedActorVitals,
) -> Result<Option<ActorVitals>, SaveError> {
    Ok(load_key(world, entity, vitals.health, "ActorVitals")?.map(|health| ActorVitals { health }))
}

pub fn save_restorations(
    world: &World,
    entity: u32,
    effects: &TimedRestorations,
) -> Result<SavedTimedRestorations, SaveError> {
    let rows = effects
        .effects
        .iter()
        .map(|effect| {
            Ok(SavedTimedRestoration {
                source_form_id: effect.source_form_id,
                actor_value: save_key(world, entity, effect.actor_value, "TimedRestorations")?,
                per_second: effect.per_second,
                remaining: effect.remaining,
            })
        })
        .collect::<Result<Vec<_>, SaveError>>()?;
    Ok(SavedTimedRestorations(rows))
}

pub fn load_restorations(
    world: &World,
    entity: u32,
    effects: SavedTimedRestorations,
) -> Result<Option<TimedRestorations>, SaveError> {
    let mut rows = Vec::with_capacity(effects.0.len());
    for effect in effects.0 {
        if let Some(actor_value) = load_key(world, entity, effect.actor_value, "TimedRestorations")?
        {
            rows.push(TimedRestoration {
                source_form_id: effect.source_form_id,
                actor_value,
                per_second: effect.per_second,
                remaining: effect.remaining,
            });
        }
    }
    Ok(Some(TimedRestorations { effects: rows }))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedSkillProgress(pub Vec<(FormIdPair, f32)>);

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedValueEffect {
    actor_value: FormIdPair,
    amount: f32,
    recover: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedActiveEffect {
    source: u32,
    mgef: u32,
    caster: u32,
    script: u32,
    duration: f32,
    indefinite: bool,
    elapsed: f32,
    started: bool,
    ending: bool,
    value: Option<SavedValueEffect>,
    ticks: u32,
    locals: Vec<(u16, f32)>,
    ref_locals: Vec<(u16, u32)>,
    suspended: bool,
    effect_index: u16,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedActiveMagicEffects(pub Vec<SavedActiveEffect>);

impl SavedActiveMagicEffects {
    pub fn value_count(&self) -> usize {
        self.0
            .iter()
            .filter(|effect| effect.value.is_some())
            .count()
    }
}

pub fn save_active_effects(
    world: &World,
    entity: u32,
    effects: &crate::magic_effects::ActiveMagicEffects,
) -> Result<SavedActiveMagicEffects, SaveError> {
    let mut rows = Vec::with_capacity(effects.0.len());
    for effect in &effects.0 {
        let value = match effect.value {
            Some(value) => Some(SavedValueEffect {
                actor_value: save_key(world, entity, value.actor_value, "ActiveMagicEffects")?,
                amount: value.amount,
                recover: value.recover,
            }),
            None => None,
        };
        rows.push(SavedActiveEffect {
            source: effect.source,
            mgef: effect.mgef,
            caster: effect.caster,
            script: effect.script,
            duration: effect.duration,
            indefinite: effect.indefinite,
            elapsed: effect.elapsed,
            started: effect.started,
            ending: effect.ending,
            value,
            ticks: effect.ticks,
            locals: effect.locals.clone(),
            ref_locals: effect.ref_locals.clone(),
            suspended: effect.suspended,
            effect_index: effect.effect_index,
        });
    }
    Ok(SavedActiveMagicEffects(rows))
}

pub fn load_active_effects(
    world: &World,
    entity: u32,
    effects: SavedActiveMagicEffects,
) -> Result<Option<crate::magic_effects::ActiveMagicEffects>, SaveError> {
    let mut rows = Vec::with_capacity(effects.0.len());
    for effect in effects.0 {
        let value = match effect.value {
            Some(value) => load_key(world, entity, value.actor_value, "ActiveMagicEffects")?.map(
                |actor_value| crate::magic_effects::ValueEffect {
                    actor_value,
                    amount: value.amount,
                    recover: value.recover,
                },
            ),
            None => None,
        };
        rows.push(crate::magic_effects::ActiveEffect {
            source: effect.source,
            mgef: effect.mgef,
            caster: effect.caster,
            script: effect.script,
            duration: effect.duration,
            indefinite: effect.indefinite,
            elapsed: effect.elapsed,
            started: effect.started,
            ending: effect.ending,
            value,
            ticks: effect.ticks,
            locals: effect.locals,
            ref_locals: effect.ref_locals,
            suspended: effect.suspended,
            effect_index: effect.effect_index,
        });
    }
    Ok(Some(crate::magic_effects::ActiveMagicEffects(rows)))
}

pub fn save_skill_progress(
    world: &World,
    progress: &crate::skills::SkillProgress,
) -> Result<SavedSkillProgress, SaveError> {
    let mut rows = progress
        .0
        .iter()
        .map(|(&key, &xp)| Ok((save_key(world, 0, key, "SkillProgress")?, xp)))
        .collect::<Result<Vec<_>, SaveError>>()?;
    rows.sort_by_key(|(pair, _)| (pair.plugin, pair.local.0));
    Ok(SavedSkillProgress(rows))
}

pub fn load_skill_progress(
    world: &World,
    progress: SavedSkillProgress,
) -> Result<crate::skills::SkillProgress, SaveError> {
    let mut rows = std::collections::BTreeMap::new();
    for (pair, xp) in progress.0 {
        if let Some(key) = load_key(world, 0, pair, "SkillProgress")? {
            rows.insert(key, xp);
        }
    }
    Ok(crate::skills::SkillProgress(rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::form_id::ENGINE_PLUGIN_ID;
    use byroredux_plugin::esm::reader::GlobalSlot;

    struct CaptureWarnings(std::sync::Mutex<Vec<String>>);
    impl log::Log for CaptureWarnings {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.level() <= log::Level::Warn
        }
        fn log(&self, record: &log::Record<'_>) {
            if self.enabled(record.metadata()) {
                self.0.lock().unwrap().push(record.args().to_string());
            }
        }
        fn flush(&self) {}
    }
    static WARNINGS: CaptureWarnings = CaptureWarnings(std::sync::Mutex::new(Vec::new()));

    #[test]
    fn values_follow_the_engine_identity_when_its_slot_moves() {
        let mut old = World::new();
        old.insert_resource(LoadOrderIdentity::new(vec![(
            GlobalSlot::Regular(1),
            ENGINE_PLUGIN_ID,
        )]));
        let mut values = ActorValues::new();
        values.set_base(0x0100_0020, 25.0);
        values.mod_permanent(0x0100_0020, 7.0);
        values.apply_damage(0x0100_0020, 2.0);
        values.mod_temporary(0x0100_0001, 3.0);
        values.set_base(0x0100_000d, 40.0);
        values.set_base(0x0100_001b, 20.0);
        assert!(values.set_alias(
            0x0100_001d,
            ActorValueAlias {
                current: 0x0100_000d,
                base: 0x0100_001b
            }
        ));
        let saved = save_values(&old, 9, &values).unwrap();
        let mut new = World::new();
        new.insert_resource(LoadOrderIdentity::new(vec![(
            GlobalSlot::Regular(2),
            ENGINE_PLUGIN_ID,
        )]));
        let restored = load_values(&new, 9, saved).unwrap().unwrap();
        assert_eq!(restored.current(0x0200_0020), 30.0);
        assert!(!restored.get(0x0200_0001).unwrap().base_authored);
        assert_eq!(restored.current(0x0200_001d), 40.0);
        assert_eq!(restored.get_base(0x0200_001d).unwrap().base, 20.0);
        assert_eq!(restored.current(0x0100_0020), 0.0);
    }

    #[test]
    fn missing_plugin_drops_only_its_values() {
        log::set_logger(&WARNINGS).unwrap();
        log::set_max_level(log::LevelFilter::Warn);
        let mut old = World::new();
        let other = byroredux_core::form_id::PluginId::from_filename("other.esp");
        old.insert_resource(LoadOrderIdentity::new(vec![
            (GlobalSlot::Regular(1), ENGINE_PLUGIN_ID),
            (GlobalSlot::Regular(2), other),
        ]));
        let mut values = ActorValues::from_pairs([(0x0100_0001, 12.0), (0x0200_0001, 99.0)]);
        assert!(values.set_alias(0x0100_0002, ActorValueAlias { current: 0x0200_0001, base: 0x0100_0001 }));
        let saved = save_values(&old, 7, &values).unwrap();
        let mut new = World::new();
        new.insert_resource(LoadOrderIdentity::new(vec![(
            GlobalSlot::Regular(2),
            ENGINE_PLUGIN_ID,
        )]));
        let restored = load_values(&new, 7, saved).unwrap().unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored.current(0x0200_0001), 12.0);
        assert!(restored.get(0x0200_0002).is_none(), "an alias with a missing source must be dropped");
        let warnings = WARNINGS.0.lock().unwrap();
        assert!(
            warnings
                .iter()
                .any(|message| message.contains("ActorValues entity 7")
                    && message.contains(&format!("{other:?}"))
                    && message.contains("LocalFormId(1)")),
            "missing owner must be named: {warnings:?}"
        );
    }

    #[test]
    fn saving_an_unowned_value_is_an_error() {
        let mut world = World::new();
        world.insert_resource(LoadOrderIdentity::new(vec![(
            GlobalSlot::Regular(1),
            ENGINE_PLUGIN_ID,
        )]));
        let error =
            save_values(&world, 7, &ActorValues::from_pairs([(0x0200_0001, 3.0)])).unwrap_err();
        assert!(matches!(
            error,
            SaveError::UnownedFormId {
                entity: 7,
                form: 0x0200_0001,
                ..
            }
        ));
    }

    #[test]
    fn running_effects_and_restorations_follow_their_value_identity() {
        let mut old = World::new();
        old.insert_resource(LoadOrderIdentity::new(vec![(
            GlobalSlot::Regular(1),
            ENGINE_PLUGIN_ID,
        )]));
        let mut new = World::new();
        new.insert_resource(LoadOrderIdentity::new(vec![(
            GlobalSlot::Regular(2),
            ENGINE_PLUGIN_ID,
        )]));
        let timed = TimedRestorations {
            effects: vec![TimedRestoration {
                source_form_id: 0x123,
                actor_value: 0x0100_0020,
                per_second: 2.0,
                remaining: 4.0,
            }],
        };
        let saved = save_restorations(&old, 9, &timed).unwrap();
        let restored = load_restorations(&new, 9, saved).unwrap().unwrap();
        assert_eq!(restored.effects[0].actor_value, 0x0200_0020);
        assert_eq!(restored.effects[0].remaining, 4.0);
        let effects =
            crate::magic_effects::ActiveMagicEffects(vec![crate::magic_effects::ActiveEffect {
                source: 1,
                mgef: 2,
                caster: 3,
                script: 4,
                duration: 8.0,
                indefinite: false,
                elapsed: 1.0,
                started: true,
                ending: false,
                value: Some(crate::magic_effects::ValueEffect {
                    actor_value: 0x0100_0020,
                    amount: 5.0,
                    recover: true,
                }),
                ticks: 0,
                locals: vec![(2, 3.0)],
                ref_locals: vec![],
                suspended: false,
                effect_index: 2,
            }]);
        let saved = save_active_effects(&old, 9, &effects).unwrap();
        let restored = load_active_effects(&new, 9, saved).unwrap().unwrap();
        assert_eq!(restored.0[0].value.unwrap().actor_value, 0x0200_0020);
        assert_eq!(restored.0[0].locals, vec![(2, 3.0)]);
        assert_eq!(restored.0[0].effect_index, 2, "the effect's position in its spell is saved");
    }
}
