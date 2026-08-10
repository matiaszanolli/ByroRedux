//! Survey a real interior CELL for combat-fixture candidates.
//!
//! The playable vertical slice needs a direct `NPC_` placement whose runtime
//! inventory resolves to at least one concrete `WEAP`. `LVLN` placements are
//! reported separately because the current cell loader does not spawn them.
//!
//! Usage:
//!   cargo run -p byroredux-plugin --example probe_combat_fixture -- \
//!     <ESM> <CELL_EDID> [CELL_EDID ...]

use byroredux_plugin::equip::{expand_leveled_form_id, resolve_inherited_inventory};
use byroredux_plugin::esm::records::items::ItemKind;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let esm_path = args.next().ok_or_else(|| {
        anyhow::anyhow!("usage: probe_combat_fixture ESM CELL_EDID [CELL_EDID ...]")
    })?;
    let cell_ids: Vec<String> = args.collect();
    if cell_ids.is_empty() {
        anyhow::bail!("supply at least one interior CELL EditorID");
    }

    let bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes)?;

    for cell_id in cell_ids {
        let Some(cell) = index.cells.cells.get(&cell_id.to_ascii_lowercase()) else {
            println!("MISS {cell_id}");
            continue;
        };

        let direct_npcs: Vec<_> = cell
            .references
            .iter()
            .filter_map(|placed| {
                index
                    .npcs
                    .get(&placed.base_form_id)
                    .map(|npc| (placed, npc))
            })
            .collect();
        let leveled_npcs: Vec<_> = cell
            .references
            .iter()
            .filter_map(|placed| {
                index
                    .leveled_npcs
                    .get(&placed.base_form_id)
                    .map(|list| (placed, list))
            })
            .collect();

        println!(
            "CELL {} form={:08X} refs={} direct_npc={} lvln={}",
            cell.editor_id,
            cell.form_id,
            cell.references.len(),
            direct_npcs.len(),
            leveled_npcs.len()
        );

        for (placed, npc) in direct_npcs {
            let actor_level = npc.level.max(1);
            let inventory = resolve_inherited_inventory(npc, actor_level, &index);
            let actor_values =
                byroredux_plugin::esm::records::derive_npc_actor_values(npc, &index, index.game);
            let factions = npc
                .factions
                .iter()
                .map(|membership| {
                    let editor_id = index
                        .factions
                        .get(&membership.faction_form_id)
                        .map(|faction| faction.editor_id.as_str())
                        .unwrap_or("?");
                    format!(
                        "{:08X}:{}:rank={}",
                        membership.faction_form_id, editor_id, membership.rank
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let mut weapons = Vec::new();
            for entry in inventory {
                let mut resolved = Vec::new();
                expand_leveled_form_id(entry.item_form_id, actor_level, &index, &mut resolved);
                for form_id in resolved {
                    let Some(item) = index.items.get(&form_id) else {
                        continue;
                    };
                    if let ItemKind::Weapon { damage, .. } = item.kind {
                        weapons.push(format!(
                            "{:08X}:{}:damage={}:model={}",
                            form_id, item.common.editor_id, damage, item.common.model_path
                        ));
                    }
                }
            }
            weapons.sort();
            weapons.dedup();

            println!(
                "  NPC ref={:08X} base={:08X} edid={} level={} pos=({:.1},{:.1},{:.1}) \
                 factions=[{}] actor_values={} outfit={} death_item={} weapons=[{}]",
                placed.form_id,
                npc.form_id,
                npc.editor_id,
                npc.level,
                placed.position[0],
                placed.position[1],
                placed.position[2],
                factions,
                actor_values.len(),
                npc.default_outfit
                    .map(|form_id| format!("{form_id:08X}"))
                    .unwrap_or_else(|| "none".to_string()),
                if npc.death_item_form_id == 0 {
                    "none".to_string()
                } else {
                    format!("{:08X}", npc.death_item_form_id)
                },
                weapons.join(", ")
            );
        }

        for (placed, list) in leveled_npcs {
            println!(
                "  LVLN ref={:08X} base={:08X} edid={} entries={} (not runtime-spawned)",
                placed.form_id,
                list.form_id,
                list.editor_id,
                list.entries.len()
            );
        }
    }

    Ok(())
}
