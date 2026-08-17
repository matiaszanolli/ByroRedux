//! Player inventory/equipment and persistent-settings diagnostics.

use super::shared::*;
use byroredux_core::ecs::components::{EquipmentSlots, EquippedWeapon, Inventory};
use byroredux_core::settings::{SettingValue, SettingsRegistry};

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
        let occupied_slots = world.get::<EquipmentSlots>(player).map_or(0, |slots| {
            slots
                .occupants
                .iter()
                .filter(|occupant| occupant.is_some())
                .count()
        });
        let weapon = world.get::<EquippedWeapon>(player).map(|weapon| *weapon);

        let mut lines = vec!["Inventory status:".to_owned()];
        lines.push(format!(
            "  player={player} stack_rows={stack_rows} item_count={item_count} occupied_slots={occupied_slots}"
        ));
        match weapon {
            Some(weapon) => lines.push(format!(
                "  equipped_weapon=0x{:08X} inventory_index={} damage={:.1} source=weapon",
                weapon.base_form_id, weapon.inventory_index.0, weapon.damage
            )),
            None => lines.push(format!(
                "  equipped_weapon=none damage={:.1} source=unarmed",
                crate::combat::UNARMED_DAMAGE
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
            },
        );

        let output = InventoryStatusCommand.execute(&world, "").lines.join("\n");
        assert!(output.contains("stack_rows=1 item_count=2 occupied_slots=1"));
        assert!(output
            .contains("equipped_weapon=0x0001CB64 inventory_index=0 damage=18.0 source=weapon"));
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
