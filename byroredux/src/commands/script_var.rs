//! `script.var` - read a script variable the Papyrus VM wrote, from the console.

use super::shared::*;
use byroredux_scripting::quest_stages::QuestFormId;
use byroredux_scripting::QuestScriptVariables;

/// `script.call <entity|.> <function>`: run a function of the Papyrus scripts the VM
/// runs on a reference (the console twin of a script calling its own function, e.g.
/// `fireTrap` on a `PhysicsTrap`).
pub(crate) struct ScriptCallCommand;

impl ConsoleCommand for ScriptCallCommand {
    fn name(&self) -> &str {
        "script.call"
    }

    fn description(&self) -> &str {
        "Call a function of the Papyrus scripts on a reference: script.call <entity|.> <function>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut tokens = args.split_whitespace();
        let (Some(entity_token), Some(function)) = (tokens.next(), tokens.next()) else {
            return CommandOutput::error("usage: script.call <entity|.> <function>");
        };
        let entity = match resolve_console_entity(world, entity_token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("script.call: {message}")),
        };
        let queued = byroredux_scripting::raise_reference_call(world, entity, function, Vec::new());
        if queued == 0 {
            return CommandOutput::error("script.call: no Papyrus script runs on that reference");
        }
        CommandOutput::line(format!("queued {function} on {queued} script(s) of entity {entity}"))
    }
}

/// `script.qcall <script> <function> [entity]`: run a function of a quest script now,
/// with an actor argument (the console twin of a dialogue fragment calling into its quest
/// script, e.g. `FavorDialogueScript` `Bribe`).
pub(crate) struct ScriptQuestCallCommand;

impl ConsoleCommand for ScriptQuestCallCommand {
    fn name(&self) -> &str {
        "script.qcall"
    }

    fn description(&self) -> &str {
        "Run a quest script function now: script.qcall <script> <function> [actor entity]"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut tokens = args.split_whitespace();
        let (Some(script), Some(function)) = (tokens.next(), tokens.next()) else {
            return CommandOutput::error("usage: script.qcall <script> <function> [actor entity]");
        };
        let mut call_args = Vec::new();
        if let Some(entity_token) = tokens.next() {
            let entity = match resolve_console_entity(world, entity_token) {
                Ok(entity) => entity,
                Err(message) => return CommandOutput::error(format!("script.qcall: {message}")),
            };
            let Some(form) = byroredux_scripting::condition::entity_global_form_id(world, entity) else {
                return CommandOutput::error("script.qcall: the entity is not a placed reference");
            };
            call_args.push(byroredux_scripting::translate::effects::VmStored::Form(form));
        }
        match byroredux_scripting::run_quest_script_function(world, script, function, call_args) {
            Ok(quest) => CommandOutput::line(format!("ran {script}.{function} on quest {quest:08X}")),
            Err(message) => CommandOutput::error(format!("script.qcall: {message}")),
        }
    }
}

/// `script.exec <papyrus statement>`: run one statement through the Papyrus VM now
/// (`Game.GetPlayer().AddItem(Game.GetCredits() as Form, 5)`).
pub(crate) struct ScriptExecCommand;

impl ConsoleCommand for ScriptExecCommand {
    fn name(&self) -> &str {
        "script.exec"
    }

    fn description(&self) -> &str {
        "Run one Papyrus statement now: script.exec <statement>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        if args.trim().is_empty() {
            return CommandOutput::error("usage: script.exec <papyrus statement>");
        }
        match byroredux_scripting::run_debug_statement(world, args.trim()) {
            Ok(()) => CommandOutput::line("ran (a statement the VM cannot run logs a `Papyrus VM ... stopped` warning)"),
            Err(message) => CommandOutput::error(format!("script.exec: {message}")),
        }
    }
}

/// `scan.info <entity|.|0xFORM>`: the scanned flag and map-marker state a script left on a
/// reference (`SetScanned`, `AddToMapScanned`, `AddToMap`).
pub(crate) struct ScanInfoCommand;

impl ConsoleCommand for ScanInfoCommand {
    fn name(&self) -> &str {
        "scan.info"
    }

    fn description(&self) -> &str {
        "Scanned flag and map-marker state of a reference: scan.info <entity|.>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(token) = args.split_whitespace().next() else {
            return CommandOutput::error("usage: scan.info <entity|.>");
        };
        let entity = match resolve_console_entity(world, token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("scan.info: {message}")),
        };
        let player = world
            .try_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
            .map(|player| player.0);
        let form = if Some(entity) == player {
            Some(byroredux_scripting::PLAYER_REFERENCE)
        } else {
            byroredux_scripting::condition::entity_global_form_id(world, entity)
        };
        let Some(form) = form else {
            return CommandOutput::error("scan.info: the entity is not a placed reference");
        };
        let scanned = world
            .try_resource::<byroredux_scripting::ScannedReferences>()
            .is_some_and(|scanned| scanned.is_scanned(form));
        let (visible, fast_travel, star_map) = world.try_resource::<byroredux_scripting::MapMarkers>().map_or(
            (false, false, false),
            |markers| (markers.is_visible(form), markers.is_fast_travel(form), markers.is_star_map_visible(form)),
        );
        CommandOutput::line(format!(
            "{form:08X}: scanned {scanned}, map-visible {visible}, fast-travel {fast_travel}, star-map-visible {star_map}"
        ))
    }
}

/// `quest.force <quest> <alias_id> <entity|.|0xREFR>`: `ReferenceAlias.ForceRefTo` from the console
/// (an alias override, as the VM records; dropped when the quest stops).
pub(crate) struct QuestForceAliasCommand;

impl ConsoleCommand for QuestForceAliasCommand {
    fn name(&self) -> &str {
        "quest.force"
    }

    fn description(&self) -> &str {
        "ForceRefTo from the console: quest.force <quest_formid> <alias_id> <entity|.|0xREFR>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut tokens = args.split_whitespace();
        let (Some(quest_token), Some(alias_token), Some(entity_token)) = (tokens.next(), tokens.next(), tokens.next())
        else {
            return CommandOutput::error("usage: quest.force <quest_formid> <alias_id> <entity|.|0xREFR>");
        };
        let (Some(quest), Ok(alias)) = (parse_console_u32(quest_token), alias_token.parse::<i32>()) else {
            return CommandOutput::error("quest.force: bad quest or alias id");
        };
        // `0x…` names the placed reference by FormID; anything else is an entity id.
        let entity = if let Some(reference) = entity_token.strip_prefix("0x").and_then(parse_console_form_id) {
            let found = world.query::<byroredux_scripting::SceneAliasCandidate>().and_then(|candidates| {
                candidates
                    .iter()
                    .find(|(_, candidate)| candidate.reference_form_id == reference)
                    .map(|(entity, _)| entity)
            });
            match found {
                Some(entity) => entity,
                None => return CommandOutput::error(format!("quest.force: reference {reference:08X} is not loaded")),
            }
        } else {
            match resolve_console_entity(world, entity_token) {
                Ok(entity) => entity,
                Err(message) => return CommandOutput::error(format!("quest.force: {message}")),
            }
        };
        let player = world
            .try_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
            .map(|player| player.0);
        let form = if Some(entity) == player {
            Some(byroredux_scripting::PLAYER_REFERENCE)
        } else {
            byroredux_scripting::condition::entity_global_form_id(world, entity)
        };
        let Some(form) = form else {
            return CommandOutput::error("quest.force: the entity is not a placed reference");
        };
        // An alias only ever binds a loaded candidate; a mesh child that merely
        // carries the reference's FormId is not one.
        if Some(entity) != player && world.get::<byroredux_scripting::SceneAliasCandidate>(entity).is_none() {
            return CommandOutput::error("quest.force: the entity is not a loaded alias candidate (a placed reference's root)");
        }
        let Some(mut overrides) = world.try_resource_mut::<byroredux_scripting::AliasOverrides>() else {
            return CommandOutput::error("quest.force: no AliasOverrides resource");
        };
        overrides.set(byroredux_scripting::QuestFormId(quest), alias, byroredux_scripting::AliasOverride::Forced(form));
        drop(overrides);
        // The next scene pass re-fills the aliases only when told they changed.
        byroredux_scripting::scene::mark_scene_actor_bindings_dirty(world);
        CommandOutput::line(format!("alias {alias} of {quest:08X} forced to {form:08X}"))
    }
}

/// `script.var <entity|.> <name>`: the value the VM's scripts wrote to `<name>`
/// (a property or variable) on the reference's script, else its authored value.
pub(crate) struct ScriptVarCommand;

impl ConsoleCommand for ScriptVarCommand {
    fn name(&self) -> &str {
        "script.var"
    }

    fn description(&self) -> &str {
        "Read a Papyrus script variable the VM wrote: script.var <entity|.> <name>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut tokens = args.split_whitespace();
        let (Some(entity_token), Some(name)) = (tokens.next(), tokens.next()) else {
            return CommandOutput::error("usage: script.var <entity|.> <name>");
        };
        let entity = match resolve_console_entity(world, entity_token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("script.var: {message}")),
        };
        let Some(form) = byroredux_scripting::condition::entity_global_form_id(world, entity) else {
            return CommandOutput::error("script.var: the entity is not a placed reference");
        };
        let key = QuestScriptVariables::property_key(name);
        let value = byroredux_scripting::quest_variable(world, QuestFormId(form), key);
        CommandOutput::line(format!("{form:08X}.{name} = {value:?}"))
    }
}
