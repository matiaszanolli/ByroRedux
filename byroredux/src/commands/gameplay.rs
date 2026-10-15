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
        let Some(player) = world
            .try_resource::<crate::systems::PlayerEntity>()
            .and_then(|player| player.0)
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
        "Spend level-up rewards (usage: levelup | levelup skill <Skill> <points> | levelup perk <perk_hex> | levelup xp <n>)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        use byroredux_core::character::{CharacterLevel, CharacterRuleset, LevelUpPoints, Perks};
        let Some(player) = world
            .try_resource::<crate::systems::PlayerEntity>()
            .and_then(|player| player.0)
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
                    "levelup: level={} xp={} skill_points={} (+{:.1}) perk_points={}",
                    level.level,
                    level.xp,
                    rewards.skill_points,
                    rewards.skill_point_fraction,
                    rewards.perk_points
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
                if let Some(mut values) =
                    world.query_mut::<byroredux_core::ecs::components::ActorValues>()
                {
                    if let Some(values) = values.get_mut(player) {
                        values.mod_permanent(avif, spend as f32);
                    }
                }
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
                let rank = {
                    let Some(mut perks) = world.query_mut::<Perks>() else {
                        return CommandOutput::error("levelup: no perk storage");
                    };
                    if perks.get_mut(player).is_none() {
                        perks.insert(player, Perks::default());
                    }
                    let owned = perks.get_mut(player).expect("present or inserted");
                    let rank = owned.rank(perk).saturating_add(1);
                    owned.set_rank(perk, rank);
                    rank
                };
                if let Some(mut stored) = world.query_mut::<LevelUpPoints>() {
                    if let Some(stored) = stored.get_mut(player) {
                        stored.perk_points -= 1;
                    }
                }
                CommandOutput::line(format!("levelup: perk {perk:08X} now rank {rank}"))
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
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<ActorValues>();
        world.register::<CharacterLevel>();
        world.register::<LevelUpPoints>();
        world.register::<Perks>();
        let mut index = EsmIndex {
            game: GameKind::Fallout3NV,
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
        world.insert(player, perks);

        // 200 XP reaches level 2 (150·1 + 50); 350 more reaches level 3.
        crate::combat::grant_experience(&world, player, 200);
        crate::combat::grant_experience(&world, player, 350);
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
        assert_eq!(world.get::<LevelUpPoints>(player).unwrap().skill_points, 28);
        let _ = LevelUpCommand.execute(&world, "perk 31DD8");
        assert_eq!(world.get::<Perks>(player).unwrap().rank(EDUCATED), 2);
        assert_eq!(world.get::<LevelUpPoints>(player).unwrap().perk_points, 0);

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
}
