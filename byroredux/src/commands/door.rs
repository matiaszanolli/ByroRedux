//! The in-cell door runtime, from the console (PRD door-runtime DRT-DX-001): `door.state`. The teleport doors keep their
//! own `door.list` / `door.teleport`; this lists the doors that open in place.

use byroredux_core::ecs::components::{GlobalTransform, Locked, Transform};
use byroredux_core::ecs::{EntityId, World};
use byroredux_scripting::TwoStateActivator;

use super::shared::*;
use crate::components::{Door, DoorMotion};

/// `door.state [entity]` — the in-cell doors of the loaded cells, nearest the camera first, or the one named: the reference, the
/// state (`open`, `closed`, `opening`, `closing`), the lock, the default state and the record's flags, and the sounds it plays.
pub(crate) struct DoorStatusCommand;

impl ConsoleCommand for DoorStatusCommand {
    fn name(&self) -> &str {
        "door.state"
    }

    fn description(&self) -> &str {
        "List the in-cell doors (or the one entity) with their state, lock, default state, flags, motion and sounds: door.state [entity]"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let only: Option<EntityId> = match args.split_whitespace().next() {
            None => None,
            Some(text) => match text.parse() {
                Ok(entity) => Some(entity),
                Err(_) => return CommandOutput::error("usage: door.state [entity]"),
            },
        };
        let origin = world
            .try_resource::<ActiveCamera>()
            .and_then(|camera| world.query::<Transform>()?.get(camera.0).map(|t| t.translation))
            .unwrap_or_default();
        let mut found: Vec<(f32, EntityId)> = Vec::new();
        if let Some(doors) = world.query::<Door>() {
            for (entity, _) in doors.iter() {
                if only.is_some_and(|only| only != entity) {
                    continue;
                }
                let distance = world
                    .get::<GlobalTransform>(entity)
                    .map_or(0.0, |transform| (transform.translation - origin).length());
                found.push((distance, entity));
            }
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut lines = vec![format!("{} in-cell doors:", found.len())];
        for (distance, entity) in found.iter().take(200) {
            // Everything else is read before `Door` is taken, and nothing is acquired while it is held:
            // `door_motion_system` takes `TwoStateActivator` before `Door` (#7093).
            let state = world.get::<TwoStateActivator>(*entity).map_or("no state", |state| {
                match (state.is_open, state.is_animating) {
                    (true, false) => "open",
                    (false, false) => "closed",
                    (true, true) => "opening",
                    (false, true) => "closing",
                }
            });
            let lock = world.get::<Locked>(*entity).map_or("unlocked".to_owned(), |lock| match lock.key_form_id {
                Some(key) => format!("locked (level {}, key {key:08X})", lock.lock_level),
                None => format!("locked (level {}, no key)", lock.lock_level),
            });
            let reference = byroredux_scripting::condition::entity_global_form_id(world, *entity)
                .map_or("-".to_owned(), |form| format!("{form:08X}"));
            let Some(door) = world.get::<Door>(*entity) else { continue };
            let DoorMotion::Sequence { open, close } = door.motion;
            let named = [
                (door.flags.oblivion_gate, "oblivion-gate"),
                (door.flags.automatic, "automatic"),
                (door.flags.hidden, "hidden"),
                (door.flags.minimal_use, "minimal-use"),
                (door.flags.sliding, "sliding"),
                (door.flags.no_open_in_combat_search, "no-open-in-combat-search"),
            ]
            .iter()
            .filter(|(set, _)| *set)
            .map(|(_, name)| *name)
            .collect::<Vec<_>>();
            let sound = |id: Option<u32>| id.map_or("-".to_owned(), |id| format!("{id:08X}"));
            lines.push(format!(
                "{distance:>8.1}  door {entity:>6} ref {reference}  {state}  {lock}  default {}  sequence clips {open}/{close}  flags {:02X} [{}]  sounds open {} close {} loop {}",
                if door.default_open { "open" } else { "closed" },
                door.data.fnam,
                named.join(" "),
                sound(door.data.open_sound),
                sound(door.data.close_sound),
                sound(door.data.loop_sound),
            ));
        }
        CommandOutput::lines(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::cell::DoorData;

    fn door(world: &mut World, state: TwoStateActivator, lock: Option<Locked>) -> EntityId {
        let entity = world.spawn();
        let data = DoorData { fnam: 0x02, open_sound: Some(0x10), ..Default::default() };
        world.insert(
            entity,
            Door {
                motion: DoorMotion::Sequence { open: 3, close: 4 },
                data,
                flags: data.flags(byroredux_plugin::esm::reader::GameKind::Oblivion),
                default_open: true,
                remaining: 0.0,
                posed: None,
            },
        );
        world.insert(entity, state);
        if let Some(lock) = lock {
            world.insert(entity, lock);
        }
        entity
    }

    /// DRT-DX-001 — the command names each door's state, lock, default, flags, motion and sounds, and `door.state <entity>`
    /// narrows to one.
    #[test]
    fn door_state_reports_state_lock_default_flags_motion_and_sounds() {
        let mut world = World::new();
        world.register::<Door>();
        world.register::<Locked>();
        world.register::<TwoStateActivator>();
        let closed = door(&mut world, TwoStateActivator::default(), Some(Locked { lock_level: 50, key_form_id: Some(0x9DAC6) }));
        let opening = door(&mut world, TwoStateActivator { is_open: true, is_animating: true, ..Default::default() }, None);

        let all = DoorStatusCommand.execute(&world, "").lines.join("\n");
        assert!(all.starts_with("2 in-cell doors:"), "{all}");
        assert!(all.contains("closed  locked (level 50, key 0009DAC6)"), "{all}");
        assert!(all.contains("opening  unlocked"), "{all}");
        assert!(all.contains("default open"), "{all}");
        assert!(all.contains("sequence clips 3/4"), "{all}");
        assert!(all.contains("flags 02 [automatic]"), "{all}");
        assert!(all.contains("sounds open 00000010 close - loop -"), "{all}");

        let one = DoorStatusCommand.execute(&world, &closed.to_string()).lines.join("\n");
        assert!(one.starts_with("1 in-cell doors:") && !one.contains("opening"), "{one} ({opening})");
        assert!(DoorStatusCommand.execute(&world, "nope").lines[0].contains("usage"));
    }

    /// #7093 — `door.state` and the motion tick take `Door` and `TwoStateActivator` in one order. The command used to
    /// hold `Door` across `TwoStateActivator`, the reverse of `door_motion_system`'s sorted pair; with
    /// `BYRO_LOCK_ORDER_CHECK=1` this test closes that cycle on its own.
    #[test]
    fn door_state_and_door_motion_share_the_lock_order() {
        let mut world = World::new();
        world.register::<Door>();
        world.register::<Locked>();
        world.register::<TwoStateActivator>();
        world.insert_resource(byroredux_core::animation::AnimationClipRegistry::new());
        door(&mut world, TwoStateActivator { is_open: true, ..Default::default() }, None);

        let report = DoorStatusCommand.execute(&world, "").lines.join("\n");
        crate::systems::door::door_motion_system(&world, 0.0);
        assert!(report.starts_with("1 in-cell doors:"), "{report}");
    }
}
