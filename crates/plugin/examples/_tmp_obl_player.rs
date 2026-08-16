//! TEMP scratch (audit 2026-08-16): which NPC_ FormID is the player base
//! record on each game's master? Probes 0x07 and 0x14.
use byroredux_plugin::esm::records::parse_esm;

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("read fail {path}");
            continue;
        };
        let Ok(index) = parse_esm(&bytes) else {
            eprintln!("parse fail {path}");
            continue;
        };
        println!("== {path} game={:?} npcs={}", index.game, index.npcs.len());
        for id in [0x07u32, 0x14u32] {
            match index.npcs.get(&id) {
                Some(n) => println!(
                    "   NPC_ 0x{id:08X}: editor_id={:?} full={:?} inv_entries={} outfit={:?}",
                    n.editor_id,
                    n.full_name,
                    n.inventory.len(),
                    n.default_outfit
                ),
                None => println!("   NPC_ 0x{id:08X}: ABSENT"),
            }
        }
    }
}
