//! Player inventory/equipment and persistent-settings diagnostics.

use super::shared::*;
use byroredux_core::ecs::components::{EquipmentSlots, EquippedWeapon, Inventory};
use byroredux_core::settings::{SettingValue, SettingsRegistry};

/// Select the saved condition flag; other Hardcore mechanics are still pending.
pub(crate) struct HardcoreCommand;

impl ConsoleCommand for HardcoreCommand {
    fn name(&self) -> &str {
        "hardcore"
    }
    fn description(&self) -> &str {
        "Inspect/select New Vegas Hardcore effect branches: hardcore [on|off] (hunger/thirst/sleep pending)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let requested = match args.trim() {
            "" => None,
            "on" | "1" => Some(true),
            "off" | "0" => Some(false),
            _ => return CommandOutput::error("usage: hardcore [on|off]"),
        };
        let Some(mut mode) =
            world.try_resource_mut::<byroredux_core::ecs::resources::HardcoreMode>()
        else {
            return CommandOutput::error("Hardcore mode resource unavailable");
        };
        if let Some(enabled) = requested {
            mode.enabled = enabled;
        }
        CommandOutput::line(format!(
            "Hardcore effect conditions: {} (hunger/thirst/sleep simulation pending)",
            if mode.enabled { "on" } else { "off" }
        ))
    }
}

/// `inv.add <form_id> [count]` — append a stack to the player's inventory
/// through the same append invariants the loot-transfer path keeps (whole
/// stack or same-base merge; equipment indices stay stable). A debug
/// frontend for P3 mid-life gear-import driving, never a separate
/// implementation.
pub(crate) struct InvAddCommand;

impl ConsoleCommand for InvAddCommand {
    fn name(&self) -> &str {
        "inv.add"
    }

    fn description(&self) -> &str {
        "Add items to the player inventory (usage: inv.add <form_id> [count=1])"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut parts = args.split_whitespace();
        let Some(form_id) = parts
            .next()
            .and_then(parse_console_form_id)
        else {
            return CommandOutput::error("usage: inv.add <form_id_hex> [count=1]");
        };
        let count: u32 = parts
            .next()
            .and_then(|raw| raw.parse().ok())
            .unwrap_or(1);
        if count == 0 {
            return CommandOutput::error("count must be >= 1");
        }
        match crate::inventory::add_item(world, form_id, count) {
            Some(index) => CommandOutput::line(format!(
                "inv.add: {count} x {form_id:08X} → row {index}",
                index = index.0
            )),
            None => CommandOutput::error("inv.add: no player inventory to append into"),
        }
    }
}

/// `inv.equip <form_id>` — toggle the equipment state of the player's row
/// carrying `form_id`. Queues the native menu's own
/// [`byroredux_debug_ui::InventoryAction::ToggleEquip`], drained through
/// `apply_action` on the main thread — one canonical mutation path, a
/// delayed frontend.
pub(crate) struct InvEquipCommand;

impl ConsoleCommand for InvEquipCommand {
    fn name(&self) -> &str {
        "inv.equip"
    }

    fn description(&self) -> &str {
        "Toggle the equipment state of a player inventory row (usage: inv.equip <form_id_hex>)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(form_id) = parse_console_form_id(args.trim())
        else {
            return CommandOutput::error("usage: inv.equip <form_id_hex>");
        };
        match crate::inventory::queue_equip_by_form_id(world, form_id) {
            Ok(message) => CommandOutput::line(message),
            Err(error) => CommandOutput::error(format!("inv.equip: {error}")),
        }
    }
}

/// `inv.use <form_id>` — use (consume) one of the player's items by the menu's own `Consume` action: a
/// stimpak restores, a chem with a script effect starts it.
pub(crate) struct InvUseCommand;

impl ConsoleCommand for InvUseCommand {
    fn name(&self) -> &str {
        "inv.use"
    }

    fn description(&self) -> &str {
        "Use (consume) a player inventory item (usage: inv.use <form_id_hex>)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(form_id) = parse_console_form_id(args.trim()) else {
            return CommandOutput::error("usage: inv.use <form_id_hex>");
        };
        match crate::inventory::queue_consume_by_form_id(world, form_id) {
            Ok(message) => CommandOutput::line(message),
            Err(error) => CommandOutput::error(format!("inv.use: {error}")),
        }
    }
}

/// `inv.drop <form_id>` — drop one of the player's items into the world by the menu's own `Drop`
/// action (a created reference at the player's feet that can be picked up again).
pub(crate) struct InvDropCommand;

impl ConsoleCommand for InvDropCommand {
    fn name(&self) -> &str {
        "inv.drop"
    }

    fn description(&self) -> &str {
        "Drop one of a player inventory item into the world (usage: inv.drop <form_id_hex>)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(form_id) = parse_console_form_id(args.trim()) else {
            return CommandOutput::error("usage: inv.drop <form_id_hex>");
        };
        match crate::inventory::queue_drop_by_form_id(world, form_id) {
            Ok(message) => CommandOutput::line(message),
            Err(error) => CommandOutput::error(format!("inv.drop: {error}")),
        }
    }
}

/// `craft.list` / `craft.make <recipe>` — the crafting page's two halves from the console, at the
/// placed bench nearest the player: the recipes it offers (with the ingredients held), and one craft
/// through the page's own action.
pub(crate) struct CraftCommand;

impl ConsoleCommand for CraftCommand {
    fn name(&self) -> &str {
        "craft"
    }

    fn description(&self) -> &str {
        "Craft at the nearest workbench: craft list | craft open | craft make <recipe_form_hex>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut parts = args.split_whitespace();
        let Some(bench) = crate::crafting::nearest_bench(world) else {
            return CommandOutput::error("craft: no workbench is loaded");
        };
        match parts.next() {
            Some("benches") => {
                let lines = crate::crafting::loaded_benches(world)
                    .into_iter()
                    .take(60)
                    .map(|(entity, base, at)| format!("bench {entity} base {base:08X} at ({:.1}, {:.1}, {:.1})", at.x, at.y, at.z))
                    .collect::<Vec<_>>();
                CommandOutput::lines(std::iter::once(format!("{} loaded bench(es)", lines.len())).chain(lines).collect())
            }
            Some("list") => {
                let recipes = crate::crafting::recipes_at(world, bench);
                let base = world.get::<byroredux_scripting::SceneAliasCandidate>(bench).map_or(0, |identity| identity.base_form_id);
                let at = world.get::<byroredux_core::ecs::components::GlobalTransform>(bench).map(|t| t.translation).unwrap_or_default();
                let mut lines = vec![format!(
                    "bench entity {bench}: {} recipe(s) (base {base:08X} at ({:.0}, {:.0}, {:.0}))",
                    recipes.len(),
                    at.x,
                    at.y,
                    at.z
                )];
                for recipe in recipes.iter().take(60) {
                    lines.push(format!(
                        "{:08X} {}{} [{}]",
                        recipe.form_id,
                        recipe.name,
                        if recipe.output_count > 1 { format!(" x{}", recipe.output_count) } else { String::new() },
                        if recipe.craftable { "can craft" } else { "missing ingredients" },
                    ));
                }
                CommandOutput::lines(lines)
            }
            // The activation's own door: the app opens the crafting page next frame.
            Some("open") => {
                crate::crafting::request_crafting(world, bench);
                CommandOutput::line(format!("requested the crafting page at bench entity {bench}"))
            }
            Some("make") => {
                let Some(recipe) = parts.next().and_then(parse_console_form_id) else {
                    return CommandOutput::error("usage: craft make <recipe_form_hex>");
                };
                match crate::crafting::craft(world, recipe, bench) {
                    Ok(line) => CommandOutput::line(line),
                    Err(reason) => CommandOutput::error(format!("craft: {reason}")),
                }
            }
            _ => CommandOutput::error("usage: craft list | craft open | craft make <recipe_form_hex>"),
        }
    }
}

/// `magic.cast <spell> [target|.]` — cast a spell on a reference (the player when none) through the
/// caster runtime ObScript `Cast` uses; `magic.effects [entity|.]` lists the effects running on one.
pub(crate) struct MagicCastCommand;

impl ConsoleCommand for MagicCastCommand {
    fn name(&self) -> &str {
        "magic.cast"
    }

    fn description(&self) -> &str {
        "Cast a spell like ObScript Cast: magic.cast <spell formid> [target entity|.] (the player when omitted)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut parts = args.split_whitespace();
        let Some(spell) = parts.next().and_then(parse_console_form_id) else {
            return CommandOutput::error("usage: magic.cast <spell formid> [target entity|.]");
        };
        let target = match parts.next() {
            Some(token) => match super::shared::resolve_console_entity(world, token) {
                Ok(entity) => entity,
                Err(message) => return CommandOutput::error(format!("magic.cast: {message}")),
            },
            None => match crate::systems::PlayerEntity::get(world) {
                Some(player) => player,
                None => return CommandOutput::error("magic.cast: no player"),
            },
        };
        // The console stands in for the player's own cast: the player is the caster, so the cast is
        // an event the Story Manager sees (`CAST`).
        let caster = crate::systems::PlayerEntity::get(world);
        if byroredux_scripting::magic_effects::cast_spell(world, caster, target, spell) {
            CommandOutput::line(format!("cast {spell:08X} on entity {target}"))
        } else {
            CommandOutput::error(format!("magic.cast: {spell:08X} is unknown or has no effect the runtime models"))
        }
    }
}

/// `actor.revive <entity|.>` — what ObScript `Resurrect` does, from the console: raise the request the
/// revive system acts on at the head of the next frame.
pub(crate) struct ActorReviveCommand;

impl ConsoleCommand for ActorReviveCommand {
    fn name(&self) -> &str {
        "actor.revive"
    }

    fn description(&self) -> &str {
        "Bring a dead or unconscious actor back like ObScript Resurrect: actor.revive <entity|.>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(token) = args.split_whitespace().next() else {
            return CommandOutput::error("usage: actor.revive <entity|.>");
        };
        let actor = match super::shared::resolve_console_entity(world, token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("actor.revive: {message}")),
        };
        let Some(mut requests) = world.query_mut::<byroredux_scripting::obscript_state::ReviveRequest>() else {
            return CommandOutput::error("actor.revive: no scripting state installed");
        };
        requests.insert(actor, byroredux_scripting::obscript_state::ReviveRequest);
        CommandOutput::line(format!("actor {actor}: revive queued"))
    }
}

/// `actor.look <entity> <target|none>` — what ObScript `Look` / `StopLook` do, from the console.
pub(crate) struct ActorLookCommand;

impl ConsoleCommand for ActorLookCommand {
    fn name(&self) -> &str {
        "actor.look"
    }

    fn description(&self) -> &str {
        "Force an actor's head to track a reference like ObScript Look: actor.look <entity|.> <target entity|none>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut parts = args.split_whitespace();
        let (Some(actor_token), Some(target_token)) = (parts.next(), parts.next()) else {
            return CommandOutput::error("usage: actor.look <entity|.> <target entity|none>");
        };
        let actor = match super::shared::resolve_console_entity(world, actor_token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("actor.look: {message}")),
        };
        if target_token.eq_ignore_ascii_case("none") {
            let stopped = byroredux_scripting::obscript_anim::stop_look(world, actor);
            return CommandOutput::line(format!("actor {actor}: look {}", if stopped { "stopped" } else { "was not set" }));
        }
        let target = match super::shared::resolve_console_entity(world, target_token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("actor.look: {message}")),
        };
        if byroredux_scripting::obscript_anim::start_look(world, actor, target) {
            CommandOutput::line(format!("actor {actor}: head tracks entity {target}"))
        } else {
            CommandOutput::error("actor.look: no scripting state installed")
        }
    }
}

pub(crate) struct MagicEffectsCommand;

impl ConsoleCommand for MagicEffectsCommand {
    fn name(&self) -> &str {
        "magic.effects"
    }

    fn description(&self) -> &str {
        "List the magic effects running on a reference: magic.effects [entity|.] (the player when omitted)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let target = match args.split_whitespace().next() {
            Some(token) => match super::shared::resolve_console_entity(world, token) {
                Ok(entity) => entity,
                Err(message) => return CommandOutput::error(format!("magic.effects: {message}")),
            },
            None => match crate::systems::PlayerEntity::get(world) {
                Some(player) => player,
                None => return CommandOutput::error("magic.effects: no player"),
            },
        };
        let Some(active) = world.get::<byroredux_scripting::magic_effects::ActiveMagicEffects>(target) else {
            return CommandOutput::line(format!("entity {target}: no magic effects"));
        };
        let mut lines = vec![format!("entity {target}: {} magic effect(s)", active.0.len())];
        for effect in &active.0 {
            lines.push(format!(
                "  source {:08X} effect {:08X} script {:08X} elapsed {:.2}/{}{}{} locals {:?}",
                effect.source,
                effect.mgef,
                effect.script,
                effect.elapsed,
                if effect.indefinite { "constant".to_owned() } else { format!("{:.0}s", effect.duration) },
                if effect.started { "" } else { " (not started)" },
                if effect.ending { " (ending)" } else { "" },
                effect.locals
            ));
        }
        CommandOutput::lines(lines)
    }
}

/// `inventory.status` — expose the live player loadout used by combat.
pub(crate) struct InventoryStatusCommand;

impl ConsoleCommand for InventoryStatusCommand {
    fn name(&self) -> &str {
        "inventory.status"
    }

    fn description(&self) -> &str {
        "Show the player's inventory, occupied equipment slots, and combat weapon"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(player) = crate::systems::PlayerEntity::get(world)
        else {
            return CommandOutput::lines(vec![
                "Inventory status:".to_owned(),
                "  player=none".to_owned(),
            ]);
        };

        // Read each sparse component independently so no command holds
        // unrelated storage locks at the same time.
        let (stack_rows, item_count) = world.get::<Inventory>(player).map_or((0, 0), |inventory| {
            (
                inventory.items.len(),
                inventory.items.iter().map(|stack| stack.count as u64).sum(),
            )
        });
        // #3112 — counts the weapon slot alongside the biped occupants; it is
        // a separate field, not `occupants[31]`.
        let occupied_slots = world
            .get::<EquipmentSlots>(player)
            .map_or(0, |slots| slots.equipped_indices().count());
        let weapon = world.get::<EquippedWeapon>(player).map(|weapon| *weapon);
        // #4711 — resolve through the same helpers combat swings with, so the
        // printed damage includes the CHARAL Melee Damage bonus (FO3/FNV
        // `STR × 0.5`) instead of the weapon's raw authored field. Called
        // after the `EquippedWeapon` snapshot above, so no guard is held
        // across the helpers' own resource/component acquisitions.
        let damage = crate::combat::attack_damage(world, player);
        let reach = crate::combat::attack_reach_bu(world, player);
        let cooldown = crate::combat::attack_cooldown_seconds(world, player);

        let mut lines = vec!["Inventory status:".to_owned()];
        lines.push(format!(
            "  player={player} stack_rows={stack_rows} item_count={item_count} occupied_slots={occupied_slots}"
        ));
        match weapon {
            Some(weapon) => lines.push(format!(
                "  equipped_weapon=0x{:08X} inventory_index={} damage={damage:.1} source=weapon reach_bu={reach:.1} cooldown_s={cooldown:.2}",
                weapon.base_form_id, weapon.inventory_index.0
            )),
            None => lines.push(format!(
                "  equipped_weapon=none damage={damage:.1} source=unarmed reach_bu={reach:.1} cooldown_s={cooldown:.2}"
            )),
        }
        CommandOutput::lines(lines)
    }
}

/// `settings.status` — expose the live universal registry and persistence path.
pub(crate) struct SettingsStatusCommand;

impl ConsoleCommand for SettingsStatusCommand {
    fn name(&self) -> &str {
        "settings.status"
    }

    fn description(&self) -> &str {
        "Show live universal settings and the settings.toml persistence path"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let persistence_path = world
            .try_resource::<crate::settings_io::SettingsPersistence>()
            .map(|persistence| persistence.path().display().to_string())
            .unwrap_or_else(|| "none".to_owned());
        let Some(settings) = world.try_resource::<SettingsRegistry>() else {
            return CommandOutput::lines(vec![
                "Settings status:".to_owned(),
                format!("  entries=0 persistence_path={persistence_path}"),
            ]);
        };
        let mut lines = Vec::with_capacity(settings.entries().len() + 2);
        lines.push("Settings status:".to_owned());
        lines.push(format!(
            "  entries={} persistence_path={persistence_path}",
            settings.entries().len()
        ));
        for entry in settings.entries() {
            lines.push(format!(
                "  {}={} restart_required={}",
                entry.id,
                setting_value(&entry.value),
                entry.restart_required
            ));
        }
        CommandOutput::lines(lines)
    }
}

fn setting_value(value: &SettingValue) -> String {
    match value {
        SettingValue::Bool(value) => value.to_string(),
        SettingValue::Number(value) => format!("{value:.3}"),
        SettingValue::Choice(value) => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{InventoryIndex, ItemStack};
    use byroredux_core::settings::SettingEntry;

    #[test]
    fn inventory_status_reports_the_combat_weapon_contract() {
        let mut world = World::new();
        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        world.insert(
            player,
            Inventory {
                items: vec![ItemStack::new(0x0001_CB64, 2)],
            },
        );
        let mut slots = EquipmentSlots::new();
        slots.equip(1 << 5, InventoryIndex(0));
        world.insert(player, slots);
        world.insert(
            player,
            EquippedWeapon {
                inventory_index: InventoryIndex(0),
                base_form_id: 0x0001_CB64,
                damage: 18.0,
                reach: 0.0,
                speed: 0.0,
            },
        );

        let output = InventoryStatusCommand.execute(&world, "").lines.join("\n");
        assert!(output.contains("stack_rows=1 item_count=2 occupied_slots=1"));
        assert!(output
            .contains("equipped_weapon=0x0001CB64 inventory_index=0 damage=18.0 source=weapon"));
    }

    /// #4711 — with a nonzero CHARAL Melee Damage bonus the printed damage
    /// is what `attack_damage` resolves (weapon + STR × 0.5), not the raw
    /// `EquippedWeapon::damage` field.
    #[test]
    fn inventory_status_damage_matches_attack_damage_with_str_bonus() {
        use byroredux_core::character::{
            CharacterRuleset, DerivedInput, DerivedStatFormula, LevelingModel, MeleeDamageConfig,
        };
        use byroredux_core::ecs::components::ActorValues;
        const STRENGTH: u32 = 0x05;
        const MELEE_DAMAGE: u32 = 0x2D2;

        let mut world = World::new();
        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        let mut ruleset = CharacterRuleset::new(LevelingModel::FNV);
        ruleset.push_derived(
            MELEE_DAMAGE,
            DerivedStatFormula::affine(DerivedInput::actor_value(STRENGTH), 0.5, 0.0),
        );
        world.insert_resource(ruleset);
        world.insert_resource(MeleeDamageConfig {
            melee_damage_avif: MELEE_DAMAGE,
        });
        world.insert(player, ActorValues::from_pairs([(STRENGTH, 10.0)]));
        world.insert(
            player,
            EquippedWeapon {
                inventory_index: InventoryIndex(0),
                base_form_id: 0x0001_CB64,
                damage: 18.0,
                reach: 0.0,
                speed: 0.0,
            },
        );

        let resolved = crate::combat::attack_damage(&world, player);
        assert_eq!(resolved, 23.0, "18.0 weapon + 10 STR × 0.5");
        let output = InventoryStatusCommand.execute(&world, "").lines.join("\n");
        assert!(
            output.contains(&format!("damage={resolved:.1} source=weapon")),
            "{output}"
        );
    }

    #[test]
    fn inventory_status_names_the_unarmed_fallback() {
        let mut world = World::new();
        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        world.insert(player, Inventory::new());
        world.insert(player, EquipmentSlots::new());

        let output = InventoryStatusCommand.execute(&world, "").lines.join("\n");
        assert!(output.contains("equipped_weapon=none damage=8.0 source=unarmed"));
    }

    #[test]
    fn settings_status_lists_live_registry_values() {
        let mut world = World::new();
        let mut settings = SettingsRegistry::default();
        settings
            .register(SettingEntry::toggle(
                "interface.crosshair",
                "Interface",
                "Crosshair",
                "",
                true,
            ))
            .unwrap();
        world.insert_resource(settings);

        let output = SettingsStatusCommand.execute(&world, "").lines.join("\n");
        assert!(output.contains("entries=1 persistence_path=none"));
        assert!(output.contains("interface.crosshair=true restart_required=false"));
    }
}

/// M47.2 — `levelup` — the level-up menu's work, until a menu exists: show
/// the player's banked level-up rewards, spend skill points on a skill of the
/// game's roster (one point raises it one, never past 100 — the Fallout
/// level-up menu's cap), or spend a perk point on a perk rank. Perk
/// requirements are not checked here.
pub(crate) struct LevelUpCommand;

impl ConsoleCommand for LevelUpCommand {
    fn name(&self) -> &str {
        "levelup"
    }

    fn description(&self) -> &str {
        "Spend level-up rewards (usage: levelup | levelup skill <Skill> <points> | levelup perk <perk_hex> | levelup pool <health|magicka|stamina> | levelup xp <n>)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        use byroredux_core::character::{CharacterLevel, CharacterRuleset, LevelUpPoints, Perks};
        let Some(player) = crate::systems::PlayerEntity::get(world)
        else {
            return CommandOutput::error("levelup: no player");
        };
        let mut parts = args.split_whitespace();
        let rewards = world
            .get::<LevelUpPoints>(player)
            .map(|rewards| *rewards)
            .unwrap_or_default();
        match parts.next() {
            None | Some("status") => {
                let level = world
                    .get::<CharacterLevel>(player)
                    .map(|level| *level)
                    .unwrap_or_default();
                CommandOutput::line(format!(
                    "levelup: level={} xp={} skill_points={} (+{:.1}) perk_points={} pool_picks={}",
                    level.level,
                    level.xp,
                    rewards.skill_points,
                    rewards.skill_point_fraction,
                    rewards.perk_points,
                    rewards.pool_picks
                ))
            }
            Some("skill") => {
                let (Some(name), Some(points)) =
                    (parts.next(), parts.next().and_then(|raw| raw.parse::<u32>().ok()))
                else {
                    return CommandOutput::error("usage: levelup skill <Skill> <points>");
                };
                let Some(skill) = world
                    .try_resource::<CharacterRuleset>()
                    .and_then(|ruleset| ruleset.skills.get_ignore_case(name))
                else {
                    return CommandOutput::error(format!("levelup: `{name}` is not a skill here"));
                };
                let Some(avif) = world
                    .try_resource::<byroredux_scripting::ActorValueNames>()
                    .and_then(|names| names.resolve(skill.editor_id))
                else {
                    return CommandOutput::error("levelup: the skill has no actor value");
                };
                let current = byroredux_scripting::condition::evaluate_function(
                    byroredux_scripting::condition::ConditionFunction::GetActorValue,
                    &byroredux_plugin::esm::records::condition::Condition {
                        param_1: avif,
                        ..Default::default()
                    },
                    player,
                    world,
                );
                let room = (100.0 - current).max(0.0).floor() as u32;
                let spend = points.min(rewards.skill_points).min(room);
                if spend == 0 {
                    return CommandOutput::error(format!(
                        "levelup: nothing to spend ({} unspent, {} is at {current})",
                        rewards.skill_points, skill.editor_id
                    ));
                }
                let Some(mut value_query) =
                    world.query_mut::<byroredux_core::ecs::components::ActorValues>()
                else {
                    return CommandOutput::error("levelup: no actor-value storage");
                };
                let Some(values) = value_query.get_mut(player) else {
                    return CommandOutput::error("levelup: player has no actor values");
                };
                let base = values.get(avif).map_or(0.0, |value| value.base);
                values.set_base(avif, base + spend as f32);
                drop(value_query);
                if let Some(mut stored) = world.query_mut::<LevelUpPoints>() {
                    if let Some(stored) = stored.get_mut(player) {
                        stored.skill_points -= spend;
                    }
                }
                CommandOutput::line(format!(
                    "levelup: {} {current} -> {} ({} skill points left)",
                    skill.editor_id,
                    current + spend as f32,
                    rewards.skill_points - spend
                ))
            }
            Some("perk") => {
                let Some(perk) = parts
                    .next()
                    .and_then(parse_console_form_id)
                else {
                    return CommandOutput::error("usage: levelup perk <perk_hex>");
                };
                if rewards.perk_points == 0 {
                    return CommandOutput::error("levelup: no perk points");
                }
                let Some(max_rank) = world
                    .try_resource::<byroredux_scripting::perk_entry_points::PerkEntryTable>()
                    .and_then(|table| table.num_ranks(perk))
                else {
                    return CommandOutput::error(format!("levelup: {perk:08X} is not a known PERK"));
                };
                let rank = {
                    let Some(mut perks) = world.query_mut::<Perks>() else {
                        return CommandOutput::error("levelup: no perk storage");
                    };
                    let current_rank = perks.get(player).map_or(0, |owned| owned.rank(perk));
                    let Some(rank) = current_rank.checked_add(1) else {
                        return CommandOutput::error(format!(
                            "levelup: perk {perk:08X} rank cannot increase further"
                        ));
                    };
                    if rank > max_rank {
                        return CommandOutput::error(format!(
                            "levelup: perk {perk:08X} is already at its maximum rank ({max_rank})"
                        ));
                    }
                    if perks.get_mut(player).is_none() {
                        perks.insert(player, Perks::default());
                    }
                    let owned = perks.get_mut(player).expect("present or inserted");
                    if !owned.try_set_rank(perk, rank, max_rank) {
                        return CommandOutput::error(format!(
                            "levelup: perk {perk:08X} rejected rank {rank} (maximum {max_rank})"
                        ));
                    }
                    rank
                };
                if let Some(mut stored) = world.query_mut::<LevelUpPoints>() {
                    if let Some(stored) = stored.get_mut(player) {
                        stored.perk_points -= 1;
                    }
                }
                CommandOutput::line(format!("levelup: perk {perk:08X} now rank {rank}"))
            }
            // Skyrim's level-up pool pick (#5916): `pool_pick_gain` points into
            // the chosen Health / Magicka / Stamina base, one pick per level.
            Some("pool") => {
                let Some(pool) = parts.next().and_then(|raw| {
                    ["Health", "Magicka", "Stamina"]
                        .into_iter()
                        .find(|name| name.eq_ignore_ascii_case(raw))
                }) else {
                    return CommandOutput::error("usage: levelup pool <health|magicka|stamina>");
                };
                let Some(gain) = world
                    .try_resource::<CharacterRuleset>()
                    .and_then(|ruleset| ruleset.leveling.pool_pick_gain())
                else {
                    return CommandOutput::error("levelup: this game has no pool picks");
                };
                if rewards.pool_picks == 0 {
                    return CommandOutput::error("levelup: no pool picks");
                }
                let Some(avif) = world
                    .try_resource::<byroredux_scripting::ActorValueNames>()
                    .and_then(|names| names.resolve(pool))
                else {
                    return CommandOutput::error(format!("levelup: {pool} has no actor value"));
                };
                let Some(mut value_query) =
                    world.query_mut::<byroredux_core::ecs::components::ActorValues>()
                else {
                    return CommandOutput::error("levelup: no actor-value storage");
                };
                let Some(values) = value_query.get_mut(player) else {
                    return CommandOutput::error("levelup: player has no actor values");
                };
                let base = values.get(avif).map_or(0.0, |value| value.base);
                values.set_base(avif, base + gain);
                drop(value_query);
                if let Some(mut stored) = world.query_mut::<LevelUpPoints>() {
                    if let Some(stored) = stored.get_mut(player) {
                        stored.pool_picks -= 1;
                    }
                }
                CommandOutput::line(format!(
                    "levelup: {pool} {base} -> {} ({} pool picks left)",
                    base + gain,
                    rewards.pool_picks - 1
                ))
            }
            // The vanilla console's `rewardxp`: experience through the same
            // level roll a kill takes.
            Some("xp") => {
                let Some(xp) = parts.next().and_then(|raw| raw.parse::<u32>().ok()) else {
                    return CommandOutput::error("usage: levelup xp <n>");
                };
                crate::combat::grant_experience(world, player, xp);
                CommandOutput::line(format!("levelup: +{xp} XP"))
            }
            Some(other) => CommandOutput::error(format!("levelup: unknown `{other}`")),
        }
    }
}

#[cfg(test)]
mod level_up_tests {
    use super::*;

    /// M47.2 — level-ups bank FO3/FNV skill points (New Vegas `10 + INT/2`,
    /// composed by `Educated`'s hook-10 +2) and perks on the cadence, and
    /// `levelup` spends them on a skill, capped at 100, and on a perk rank.
    #[test]
    fn level_ups_bank_skill_points_and_levelup_spends_them() {
        use byroredux_core::character::{
            falloutnv_ruleset, CharacterLevel, LevelUpPoints, Perks,
        };
        use byroredux_core::ecs::components::ActorValues;
        use byroredux_plugin::esm::reader::GameKind;
        use byroredux_plugin::esm::records::{
            AvifRecord, EsmIndex, PerkEntry, PerkEntryBody, PerkFunctionData, PerkRecord,
        };
        const INTELLIGENCE: u32 = 0x2C6;
        const SCIENCE: u32 = 0x2DB;
        const EDUCATED: u32 = 0x0003_1DD8;
        const RANKED_PERK: u32 = 0x0000_1234;
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<ActorValues>();
        world.register::<CharacterLevel>();
        world.register::<LevelUpPoints>();
        world.register::<Perks>();
        let mut index = EsmIndex {
            game: GameKind::Fallout3NV,
            character_rules: byroredux_core::character::CharacterRulesProfile::FALLOUT_NEW_VEGAS,
            ..Default::default()
        };
        for (form_id, editor_id) in [(INTELLIGENCE, "AVIntelligence"), (SCIENCE, "AVScience")] {
            index.actor_values.insert(
                form_id,
                AvifRecord {
                    form_id,
                    editor_id: editor_id.into(),
                    ..Default::default()
                },
            );
        }
        index.perks.insert(
            EDUCATED,
            PerkRecord {
                form_id: EDUCATED,
                num_ranks: 1,
                entries: vec![PerkEntry {
                    rank: 0,
                    priority: 0,
                    body: PerkEntryBody::EntryPoint {
                        entry_point_index: 10,
                        function: 2,
                        tab_count: 1,
                        param_type: 1,
                        function_data: PerkFunctionData::Float(2.0),
                        formatter: Vec::new(),
                        extra_flags: Vec::new(),
                    },
                    condition_tabs: Vec::new(),
                }],
                ..Default::default()
            },
        );
        index.perks.insert(
            RANKED_PERK,
            PerkRecord {
                form_id: RANKED_PERK,
                num_ranks: 2,
                ..Default::default()
            },
        );
        byroredux_scripting::install_actor_value_names(&mut world, &index);
        byroredux_scripting::perk_entry_points::install_perk_entry_points(&mut world, &index);
        world.insert_resource(falloutnv_ruleset(|name| match name {
            "Intelligence" => Some(INTELLIGENCE),
            "Science" => Some(SCIENCE),
            _ => None,
        }));
        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        world.insert(player, ActorValues::from_pairs([(INTELLIGENCE, 9.0), (SCIENCE, 95.0)]));
        world.insert(player, CharacterLevel { level: 1, xp: 0 });
        let mut perks = Perks::default();
        perks.set_rank(EDUCATED, 1);
        perks.set_rank(RANKED_PERK, 1);
        world.insert(player, perks);

        // 200 XP reaches level 2 (150·1 + 50); 350 more reaches level 3.
        world.insert_resource(crate::notifications::PlayerNotifications::default());
        // #5917 — a chem's +3 INT at the moment of the roll must not change the
        // banked points: Skill Rate reads the base INT (9), not the current 12.
        world
            .get_mut::<ActorValues>(player)
            .unwrap()
            .mod_temporary(INTELLIGENCE, 3.0);
        crate::combat::grant_experience(&world, player, 200);
        crate::combat::grant_experience(&world, player, 350);
        world
            .get_mut::<ActorValues>(player)
            .unwrap()
            .mod_temporary(INTELLIGENCE, -3.0);
        // #5874 — XP, each level reached and the banked rewards are announced.
        let notices = crate::notifications::drain(&world);
        assert!(notices.iter().any(|n| n == "+200 XP"), "{notices:?}");
        assert!(notices.iter().any(|n| n == "Level up! You reached level 2"), "{notices:?}");
        assert!(notices.iter().any(|n| n == "Level up! You reached level 3"), "{notices:?}");
        assert!(notices.iter().any(|n| n.contains("skill points")), "{notices:?}");
        assert!(notices.iter().any(|n| n.contains("perk point")), "{notices:?}");
        assert_eq!(world.get::<CharacterLevel>(player).unwrap().level, 3);
        let banked = *world.get::<LevelUpPoints>(player).unwrap();
        assert_eq!(
            (banked.skill_points, banked.perk_points),
            (33, 1),
            "2 × (14.5 + 2), a perk at level 2 only"
        );

        let _ = LevelUpCommand.execute(&world, "skill science 10");
        assert_eq!(
            world.get::<ActorValues>(player).unwrap().current(SCIENCE),
            100.0,
            "capped at 100"
        );
        let (science_base, science_permanent) = {
            let values = world.get::<ActorValues>(player).unwrap();
            let science = values.get(SCIENCE).unwrap();
            (science.base, science.permanent_mod)
        };
        assert_eq!(science_base, 100.0, "spent points are part of the AV base");
        assert_eq!(science_permanent, 0.0, "skill points are not perk modifiers");
        assert_eq!(world.get::<LevelUpPoints>(player).unwrap().skill_points, 28);
        let _ = LevelUpCommand.execute(&world, "perk 1234");
        assert_eq!(world.get::<Perks>(player).unwrap().rank(RANKED_PERK), 2);
        assert_eq!(world.get::<Perks>(player).unwrap().rank(EDUCATED), 1);
        assert_eq!(world.get::<LevelUpPoints>(player).unwrap().perk_points, 0);

        let mut rewards = *world.get::<LevelUpPoints>(player).unwrap();
        rewards.perk_points = 2;
        world.insert(player, rewards);
        let unknown = LevelUpCommand.execute(&world, "perk DEADBEEF").lines.join("\n");
        assert!(unknown.contains("not a known PERK"), "{unknown}");
        assert_eq!(world.get::<Perks>(player).unwrap().rank(0xDEAD_BEEF), 0);
        assert_eq!(world.get::<LevelUpPoints>(player).unwrap().perk_points, 2);

        let capped = LevelUpCommand.execute(&world, "perk 1234").lines.join("\n");
        assert!(capped.contains("maximum rank (2)"), "{capped}");
        assert_eq!(world.get::<Perks>(player).unwrap().rank(RANKED_PERK), 2);
        assert_eq!(world.get::<LevelUpPoints>(player).unwrap().perk_points, 2);

        // `Game.RewardPlayerXP(100)` with FO4's modifier (fXPModBase 1.0 +
        // fXPModMult 0.03 · INT 9) grants 127; `abDirect` grants 100.
        use byroredux_plugin::esm::records::{GameSetting, SettingValue};
        for (form_id, editor_id, value) in [(1, "fXPModBase", 1.0), (2, "fXPModMult", 0.03)] {
            index.game_settings.insert(
                form_id,
                GameSetting {
                    form_id,
                    editor_id: editor_id.into(),
                    value: SettingValue::Float(value),
                },
            );
        }
        assert!(crate::combat::install_xp_modifier_rules(&mut world, &index));
        let before = world.get::<CharacterLevel>(player).unwrap().xp;
        world
            .resource_mut::<byroredux_scripting::story_manager::PendingPlayerXp>()
            .0
            .extend([(100, false), (100, true)]);
        crate::combat::scripted_experience_system(&world, 0.0);
        assert_eq!(world.get::<CharacterLevel>(player).unwrap().xp, before + 227);
    }

    /// #5916 — a Skyrim level rolled through the shared XP path banks its
    /// perk and its Health / Magicka / Stamina pick (it used to `return`
    /// before either), and `levelup pool` spends the pick as +10 on the base.
    #[test]
    fn skyrim_levels_bank_a_perk_and_a_pool_pick() {
        use byroredux_core::character::{skyrim_ruleset, CharacterLevel, LevelUpPoints};
        use byroredux_core::ecs::components::ActorValues;
        use byroredux_plugin::esm::records::{AvifRecord, EsmIndex};
        const HEALTH: u32 = 0x3E8;
        const STAMINA: u32 = 0x3EA;

        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<ActorValues>();
        world.register::<CharacterLevel>();
        world.register::<LevelUpPoints>();
        let mut index = EsmIndex::default();
        for (form_id, editor_id) in [(HEALTH, "AVHealth"), (STAMINA, "AVStamina")] {
            index.actor_values.insert(
                form_id,
                AvifRecord {
                    form_id,
                    editor_id: editor_id.into(),
                    ..Default::default()
                },
            );
        }
        byroredux_scripting::install_actor_value_names(&mut world, &index);
        world.insert_resource(skyrim_ruleset(|name| match name {
            "Health" => Some(HEALTH),
            "Stamina" => Some(STAMINA),
            _ => None,
        }));
        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        world.insert(player, ActorValues::from_pairs([(HEALTH, 100.0), (STAMINA, 100.0)]));
        world.insert(player, CharacterLevel { level: 1, xp: 0 });
        world.insert_resource(crate::notifications::PlayerNotifications::default());

        // 25·1 + 75 = 100 XP per level at level 1, 125 at level 2: two levels.
        crate::combat::grant_experience(&world, player, 225);
        assert_eq!(world.get::<CharacterLevel>(player).unwrap().level, 3);
        let banked = *world.get::<LevelUpPoints>(player).unwrap();
        assert_eq!(
            (banked.perk_points, banked.pool_picks, banked.skill_points),
            (2, 2, 0),
            "each Skyrim level banks a perk and a pool pick, no skill points"
        );
        let notices = crate::notifications::drain(&world);
        assert!(notices.iter().any(|n| n.contains("pick")), "{notices:?}");

        let status = LevelUpCommand.execute(&world, "status").lines.join("\n");
        assert!(status.contains("pool_picks=2"), "{status}");
        let _ = LevelUpCommand.execute(&world, "pool stamina");
        assert_eq!(world.get::<ActorValues>(player).unwrap().current(STAMINA), 110.0);
        assert_eq!(world.get::<ActorValues>(player).unwrap().current(HEALTH), 100.0);
        assert_eq!(world.get::<LevelUpPoints>(player).unwrap().pool_picks, 1);
        let _ = LevelUpCommand.execute(&world, "pool health");
        let empty = LevelUpCommand.execute(&world, "pool health").lines.join("\n");
        assert!(empty.contains("no pool picks"), "{empty}");
        assert_eq!(world.get::<ActorValues>(player).unwrap().current(HEALTH), 110.0);
        let bad = LevelUpCommand.execute(&world, "pool carryweight").lines.join("\n");
        assert!(bad.contains("usage"), "{bad}");
    }
}


/// `mine.status` — the placed proximity mines (M43 `TMEE`): how many the load order defines, and each placed
/// mine's position (renderer Y-up) and state, for the live gate.
pub(crate) struct MineStatusCommand;

impl ConsoleCommand for MineStatusCommand {
    fn name(&self) -> &str {
        "mine.status"
    }
    fn description(&self) -> &str {
        "List the placed proximity mines and their state (waiting / armed / spent)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let defined = world.try_resource::<crate::systems::mines::MineBook>().map_or(0, |book| book.len());
        let mines = crate::systems::mines::placed_mines(world);
        let mut lines = vec![format!("mine types defined: {defined}; placed mines: {}", mines.len())];
        for mine in mines.iter().take(40) {
            lines.push(format!(
                "mine {} base {:08X} at ({:.1}, {:.1}, {:.1}) {}",
                mine.entity, mine.base, mine.position.x, mine.position.y, mine.position.z, mine.state
            ));
        }
        CommandOutput::lines(lines)
    }
}


/// `attraction.status` — the attraction system (M43 `AOBJ`): the rules and the object / NPC bases the load
/// order gives it, and how many placed objects and live actors currently carry one.
pub(crate) struct AttractionStatusCommand;

impl ConsoleCommand for AttractionStatusCommand {
    fn name(&self) -> &str {
        "attraction.status"
    }
    fn description(&self) -> &str {
        "Show the attraction rules, the object / NPC bases that carry them, and how many are placed now"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(book) = world.try_resource::<crate::systems::attraction::AttractionBook>() else {
            return CommandOutput::line("attraction: no book installed");
        };
        let (rules, object_bases, actor_bases) = book.counts();
        drop(book);
        let (objects, actors) = crate::systems::attraction::tracked(world);
        CommandOutput::line(format!(
            "attraction rules: {rules}; object bases: {object_bases}; npc bases: {actor_bases}; placed objects: {objects}; live actors: {actors}"
        ))
    }
}
