use byroredux_plugin::esm::records::actor::effective_actor_level;
use byroredux_plugin::equip::resolve_inherited_inventory;

fn main() -> anyhow::Result<()> {
    let esm_path = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/FalloutNV.esm";
    let bytes = std::fs::read(esm_path)?;
    let index = byroredux_plugin::esm::parse_esm(&bytes)?;

    for fid in [0x00166CD6u32, 0x0013A643, 0x00165760, 0x00165761, 0x00141135, 0x000295F7] {
        if let Some(l) = index.leveled_items.get(&fid) {
            println!("LVLI {:08X} '{}' flags={:#04x} chance_none={} entries:", fid, l.editor_id, l.flags, l.chance_none);
            for e in &l.entries {
                let kind = if index.items.contains_key(&e.form_id) { "ITEM" }
                    else if index.leveled_items.contains_key(&e.form_id) { "LVLI" }
                    else { "UNRESOLVED" };
                println!("   entry level={} form_id={:08X} kind={}", e.level, e.form_id, kind);
            }
        } else {
            println!("LVLI {:08X} NOT FOUND in leveled_items", fid);
        }
    }

    // Check one specific NPC in detail
    if let Some(npc) = index.npcs.get(&0x0017A2F4) {
        let lvl = effective_actor_level(npc);
        println!("\nNPC {} level(raw)={} effective_level={}", npc.editor_id, npc.level, lvl);
        for entry in resolve_inherited_inventory(npc, lvl, &index) {
            println!("  inv entry form_id={:08X} count={}", entry.item_form_id, entry.count);
        }
    }
    Ok(())
}
