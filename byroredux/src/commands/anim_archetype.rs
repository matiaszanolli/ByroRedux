//! `anim.archetype` - what a script's `Actor.ChangeAnimArchetype` does, from the console.
//!
//! Records the archetype keyword in `AnimArchetypeOverrides` for the actor's
//! reference (the same write the VM makes), so the actor's keywords, its subgraph
//! selection and - on Fallout 4 - its idle follow (`systems::archetype_idle`).

use super::shared::*;

/// `anim.archetype <entity|.> [keyword_formid|none]`: set (or, with `none`, clear
/// to Neutral) an actor's animation archetype; with no keyword, list the
/// archetype keywords and the actor's current override.
pub(crate) struct AnimArchetypeCommand;

impl ConsoleCommand for AnimArchetypeCommand {
    fn name(&self) -> &str {
        "anim.archetype"
    }

    fn description(&self) -> &str {
        "ChangeAnimArchetype from the console: anim.archetype <entity|.> [keyword_formid|none]"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut tokens = args.split_whitespace();
        let Some(entity_token) = tokens.next() else {
            return CommandOutput::error("usage: anim.archetype <entity|.> [keyword_formid|none]");
        };
        let entity = match resolve_console_entity(world, entity_token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("anim.archetype: {message}")),
        };
        let Some(key) = byroredux_scripting::condition::entity_global_form_id(world, entity) else {
            return CommandOutput::error("anim.archetype: the entity is not a placed reference");
        };
        let current = world
            .try_resource::<byroredux_scripting::AnimArchetypeOverrides>()
            .and_then(|overrides| overrides.get(key));
        let Some(keyword_token) = tokens.next() else {
            let ids: Vec<String> = world
                .try_resource::<byroredux_scripting::anim_subgraph::SubgraphTable>()
                .map(|table| table.archetype_ids().map(|id| format!("{id:08X}")).collect())
                .unwrap_or_default();
            return CommandOutput::lines(vec![
                format!("override of {key:08X}: {current:?}"),
                format!("archetype keywords: {}", ids.join(" ")),
            ]);
        };
        let keyword = if keyword_token.eq_ignore_ascii_case("none") {
            0
        } else {
            match parse_console_form_id(keyword_token) {
                Some(keyword) => keyword,
                None => return CommandOutput::error(format!("anim.archetype: bad keyword `{keyword_token}`")),
            }
        };
        let Some(mut overrides) = world.try_resource_mut::<byroredux_scripting::AnimArchetypeOverrides>() else {
            return CommandOutput::error("anim.archetype: no AnimArchetypeOverrides resource");
        };
        overrides.set(key, keyword);
        CommandOutput::line(format!("{key:08X} archetype set to {keyword:08X} (was {current:?})"))
    }
}
