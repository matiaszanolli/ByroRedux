//! TEMP audit probe (Dimension 4, checklist item 5) — not for commit.
//! Whole-master sweep: for every FNV NPC whose resolved inventory
//! references an LVLI, confirm `expand_leveled_form_id` yields at least
//! one concrete item (i.e. LVLI refs don't silently resolve to empty
//! gear).
use byroredux_plugin::equip::{expand_leveled_form_id, resolve_inherited_inventory};
use byroredux_plugin::esm::records::actor::effective_actor_level;

fn main() -> anyhow::Result<()> {
    let esm_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/FalloutNV.esm".to_string());
    let bytes = std::fs::read(&esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes)?;

    let mut npcs_with_lvli_ref = 0usize;
    let mut npcs_with_lvli_ref_empty_result = 0usize;
    let mut total_lvli_refs = 0usize;
    let mut total_lvli_refs_empty = 0usize;
    let mut examples = Vec::new();

    for npc in index.npcs.values() {
        let actor_level = effective_actor_level(npc);
        let inventory = resolve_inherited_inventory(npc, actor_level, &index);
        let mut this_npc_had_lvli = false;
        let mut this_npc_had_empty = false;
        for entry in inventory {
            if !index.leveled_items.contains_key(&entry.item_form_id) {
                continue;
            }
            this_npc_had_lvli = true;
            total_lvli_refs += 1;
            let mut resolved = Vec::new();
            expand_leveled_form_id(entry.item_form_id, actor_level, &index, &mut resolved);
            if resolved.is_empty() {
                total_lvli_refs_empty += 1;
                this_npc_had_empty = true;
                if examples.len() < 15 {
                    examples.push(format!(
                        "NPC {:08X} ({}) level={} -> LVLI {:08X} resolved EMPTY",
                        npc.form_id, npc.editor_id, actor_level, entry.item_form_id
                    ));
                }
            }
        }
        if this_npc_had_lvli {
            npcs_with_lvli_ref += 1;
        }
        if this_npc_had_empty {
            npcs_with_lvli_ref_empty_result += 1;
        }
    }

    println!(
        "NPCs total={} with_lvli_ref={} with_at_least_one_empty_lvli_result={}",
        index.npcs.len(),
        npcs_with_lvli_ref,
        npcs_with_lvli_ref_empty_result,
    );
    println!(
        "LVLI refs total={} resolved_empty={}",
        total_lvli_refs, total_lvli_refs_empty
    );
    for e in &examples {
        println!("  {e}");
    }

    Ok(())
}
