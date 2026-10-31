//! Console doors onto the player actions that raise Story Manager events (M43): they call the same functions the
//! game's own paths call, so a live run can exercise the real data (lock levels, the pick item, terminals, the
//! jail table, the SM roots) without driving the interaction UI.
//!
//! - `lock.status` — locked references and how the player could get past each one.
//! - `lock.open <entity>` — open one the way activating it does (key, pick or hack); raises `LOCK` / `HACK`.
//! - `word.teach <wordofpower_hex>` — `Game.TeachWord`'s effect; raises `NVPE` for a new word.
//! - `jail.send <faction_hex> [fake]` — `Faction.SendPlayerToJail`'s effect; raises `ARRT`, and `JAIL` for a real jail.
//! - `place.at <base_hex> [dx dy dz]` — create a reference of any base beside the player (the machinery `PlaceAtMe`
//!   and item drops use), to bring an actor and an object together at runtime.
//! - `sm.nodes [filter]` — the loaded Story Manager quest nodes that carry a node-level reset window (`HNAM`).
//! - `conv.status` / `entity.move <entity> <dx> <dy> <dz>` — the actors the random-conversation pass can use, and a nudge
//!   of one actor's position (renderer Y-up units), to stage a mover for the hello event.
//! - `jail.escape` — the player walks out of the cell to the jail's exit without serving: `ESJA`.

use super::shared::*;
use byroredux_core::ecs::components::{GlobalTransform, Locked};

pub(crate) struct LockStatusCommand;

impl ConsoleCommand for LockStatusCommand {
    fn name(&self) -> &str {
        "lock.status"
    }
    fn description(&self) -> &str {
        "List locked references with their level and how the player could bypass the lock (pick / hack / none)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let locked: Vec<(u32, u8, Option<u32>)> = world
            .query::<Locked>()
            .map(|all| all.iter().map(|(entity, lock)| (entity, lock.lock_level, lock.key_form_id)).collect())
            .unwrap_or_default();
        let mut lines = vec![format!("{} locked reference(s)", locked.len())];
        for (entity, level, key) in locked.iter().take(40) {
            let bypass = match crate::lockpick::bypass_of(world, *entity) {
                Some(crate::lockpick::Bypass::Pick) => "pick",
                Some(crate::lockpick::Bypass::Hack) => "hack",
                None => "none",
            };
            let base = world.get::<byroredux_scripting::SceneAliasCandidate>(*entity).map_or(0, |c| c.base_form_id);
            let at = world.get::<GlobalTransform>(*entity).map(|t| t.translation).unwrap_or_default();
            lines.push(format!(
                "lock {entity} base {base:08X} level {level} key {} bypass {bypass} at ({:.1}, {:.1}, {:.1})",
                key.map_or("none".to_owned(), |k| format!("{k:08X}")),
                at.x,
                at.y,
                at.z
            ));
        }
        CommandOutput::lines(lines)
    }
}

pub(crate) struct LockOpenCommand;

impl ConsoleCommand for LockOpenCommand {
    fn name(&self) -> &str {
        "lock.open"
    }
    fn description(&self) -> &str {
        "Open a locked reference the way activating it does (key, pick or hack): lock.open <entity>"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Ok(entity) = args.trim().parse::<u32>() else {
            return CommandOutput::error("usage: lock.open <entity>");
        };
        if crate::interaction::open_lock_for_console(world, entity) {
            CommandOutput::line(format!("lock {entity} opened"))
        } else {
            CommandOutput::error(format!("lock {entity} stays shut (no key, no pick / hack, or not locked)"))
        }
    }
}

pub(crate) struct WordTeachCommand;

impl ConsoleCommand for WordTeachCommand {
    fn name(&self) -> &str {
        "word.teach"
    }
    fn description(&self) -> &str {
        "Give the player a word of power (Game.TeachWord): word.teach <form_id_hex>"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(word) = parse_console_form_id(args.trim()) else {
            return CommandOutput::error("usage: word.teach <form_id_hex>");
        };
        if byroredux_scripting::words_of_power::obtain_word(world, word) {
            CommandOutput::line(format!("word {word:08X} obtained"))
        } else {
            CommandOutput::line(format!("word {word:08X} was already known"))
        }
    }
}

pub(crate) struct JailSendCommand;

impl ConsoleCommand for JailSendCommand {
    fn name(&self) -> &str {
        "jail.send"
    }
    fn description(&self) -> &str {
        "Send the player to jail as a guard's dialogue does (Faction.SendPlayerToJail): jail.send <faction_hex> [fake]"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut parts = args.split_whitespace();
        let Some(faction) = parts.next().and_then(parse_console_form_id) else {
            return CommandOutput::error("usage: jail.send <faction_hex> [fake]");
        };
        let real = parts.next() != Some("fake");
        let sent = byroredux_scripting::jail::send_player_to_jail(world, faction, None, true, real);
        CommandOutput::line(format!("SendPlayerToJail({faction:08X}, real={real}): {}", if sent { "sent to a cell" } else { "arrest raised, no cell" }))
    }
}

pub(crate) struct JailEscapeCommand;

impl ConsoleCommand for JailEscapeCommand {
    fn name(&self) -> &str {
        "jail.escape"
    }
    fn description(&self) -> &str {
        "Take the player out to the jail's exit without serving the sentence (a jailbreak): raises ESJA once outside"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(sentence) = world
            .try_resource::<byroredux_scripting::obscript_ledger::ObScriptLedger>()
            .and_then(|ledger| ledger.jail)
        else {
            return CommandOutput::error("the player is not serving a sentence (jail.send starts one)");
        };
        match world.try_resource_mut::<byroredux_scripting::PlayerMoveRequest>() {
            Some(mut request) => {
                request.0 = Some(sentence.exit_marker);
                CommandOutput::line(format!("moving the player to the exit marker {:08X} with the sentence unserved", sentence.exit_marker))
            }
            None => CommandOutput::error("no move request resource"),
        }
    }
}

pub(crate) struct StartReleaseCommand;

impl ConsoleCommand for StartReleaseCommand {
    fn name(&self) -> &str {
        "start.release"
    }
    fn description(&self) -> &str {
        "Release the launch's explicit start pin, as the first key or mouse press does, so scripted player moves take effect"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        crate::components::ExplicitStartPin::release(world);
        CommandOutput::line("explicit start released: scripted player moves now take control")
    }
}

pub(crate) struct PlaceAtCommand;

impl ConsoleCommand for PlaceAtCommand {
    fn name(&self) -> &str {
        "place.at"
    }
    fn description(&self) -> &str {
        "Create a reference of a base beside the player (PlaceAtMe's machinery): place.at <base_hex> [dx dy dz] (Z-up offsets)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut parts = args.split_whitespace();
        let Some(base) = parts.next().and_then(parse_console_form_id) else {
            return CommandOutput::error("usage: place.at <base_hex> [dx dy dz]");
        };
        let offset: Vec<f32> = parts.filter_map(|part| part.parse().ok()).collect();
        let offset = [offset.first().copied().unwrap_or(0.0), offset.get(1).copied().unwrap_or(0.0), offset.get(2).copied().unwrap_or(0.0)];
        let Some(player) = crate::systems::PlayerEntity::get(world) else {
            return CommandOutput::error("place.at: no player");
        };
        let Some(at) = world.get::<GlobalTransform>(player).map(|t| t.translation) else {
            return CommandOutput::error("place.at: the player has no position");
        };
        let zup = byroredux_core::math::coord::yup_to_zup_pos(at.to_array());
        let position = [zup[0] + offset[0], zup[1] + offset[1], zup[2] + offset[2]];
        let Some(placement) = crate::cell_loader::created::placement_at_position(world, position) else {
            return CommandOutput::error("place.at: no current cell to place into");
        };
        let Some(ids) = world.try_resource::<byroredux_scripting::CreatedReferences>().map(|created| created.scripted_ids()) else {
            return CommandOutput::error("place.at: no created-reference registry");
        };
        let Some(reference) = ids.next() else {
            return CommandOutput::error("place.at: created form ids are exhausted");
        };
        if let Some(mut created) = world.try_resource_mut::<byroredux_scripting::CreatedReferences>() {
            created.create_scripted(reference, base, byroredux_scripting::PLAYER_REFERENCE, false);
            created.place(reference, placement);
        }
        CommandOutput::line(format!("placed base {base:08X} as reference {reference:08X} at ({:.0}, {:.0}, {:.0})", position[0], position[1], position[2]))
    }
}

pub(crate) struct SmNodesCommand;

impl ConsoleCommand for SmNodesCommand {
    fn name(&self) -> &str {
        "sm.nodes"
    }
    fn description(&self) -> &str {
        "List the loaded Story Manager quest nodes with a node-level reset window (HNAM): sm.nodes [editor-id filter]"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(tree) = world.try_resource::<byroredux_scripting::story_manager::SmTree>() else {
            return CommandOutput::error("sm.nodes: no Story Manager tree installed");
        };
        let filter = args.trim().to_ascii_lowercase();
        let mut windowed: Vec<(&str, f32, usize)> = tree
            .nodes
            .iter()
            .filter(|node| node.reset_hours > 0.0 && node.editor_id.to_ascii_lowercase().contains(&filter))
            .map(|node| (node.editor_id.as_str(), node.reset_hours, node.quests.len()))
            .collect();
        windowed.sort_by(|a, b| a.0.cmp(b.0));
        let mut lines = vec![format!("{} of {} node(s) carry a reset window (HNAM)", windowed.len(), tree.nodes.len())];
        lines.extend(windowed.iter().take(40).map(|(name, hours, quests)| format!("node {name} reset {hours} h quests {quests}")));
        CommandOutput::lines(lines)
    }
}

pub(crate) struct ConvStatusCommand;

impl ConsoleCommand for ConvStatusCommand {
    fn name(&self) -> &str {
        "conv.status"
    }
    fn description(&self) -> &str {
        "List the actors the random-conversation pass can use (a live package that allows random conversations)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let participants = crate::systems::random_conversation::collect_participants(world);
        let mut lines = vec![format!("{} conversation participant(s)", participants.len())];
        for p in participants.iter().take(40) {
            lines.push(format!(
                "participant {} at ({:.1}, {:.1}, {:.1}) initiates={}",
                p.entity, p.position.x, p.position.y, p.position.z, p.can_initiate
            ));
        }
        CommandOutput::lines(lines)
    }
}

pub(crate) struct EntityMoveCommand;

impl ConsoleCommand for EntityMoveCommand {
    fn name(&self) -> &str {
        "entity.move"
    }
    fn description(&self) -> &str {
        "Nudge an entity's position by an offset in renderer Y-up units: entity.move <entity> <dx> <dy> <dz>"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let parts: Vec<&str> = args.split_whitespace().collect();
        let (Some(entity), Some(dx), Some(dy), Some(dz)) = (
            parts.first().and_then(|p| p.parse::<u32>().ok()),
            parts.get(1).and_then(|p| p.parse::<f32>().ok()),
            parts.get(2).and_then(|p| p.parse::<f32>().ok()),
            parts.get(3).and_then(|p| p.parse::<f32>().ok()),
        ) else {
            return CommandOutput::error("usage: entity.move <entity> <dx> <dy> <dz>");
        };
        let delta = byroredux_core::math::Vec3::new(dx, dy, dz);
        let mut moved = false;
        if let Some(mut transforms) = world.query_mut::<byroredux_core::ecs::components::Transform>() {
            if let Some(transform) = transforms.get_mut(entity) {
                transform.translation += delta;
                moved = true;
            }
        }
        if let Some(mut globals) = world.query_mut::<GlobalTransform>() {
            if let Some(global) = globals.get_mut(entity) {
                global.translation += delta;
                moved = true;
            }
        }
        if moved {
            CommandOutput::line(format!("entity {entity} moved by ({dx}, {dy}, {dz})"))
        } else {
            CommandOutput::error(format!("entity {entity} has no position"))
        }
    }
}
