//! Console doors onto the player actions that raise Story Manager events (M43): they call the same functions the
//! game's own paths call, so a live run can exercise the real data (lock levels, the pick item, terminals, the
//! jail table, the SM roots) without driving the interaction UI.
//!
//! - `lock.status` — locked references and how the player could get past each one.
//! - `lock.open <entity>` — open one the way activating it does (key, pick or hack); raises `LOCK` / `HACK`.
//! - `word.teach <wordofpower_hex>` — `Game.TeachWord`'s effect; raises `NVPE` for a new word.
//! - `jail.send <faction_hex> [fake]` — `Faction.SendPlayerToJail`'s effect; raises `ARRT`, and `JAIL` for a real jail.
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
