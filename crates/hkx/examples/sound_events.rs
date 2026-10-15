//! List the `SoundPlay.*` events the clip triggers of Skyrim SE behavior graphs
//! raise, with how many triggers carry each — the evidence for how a graph's
//! sound notifies name sounds.
//!
//! ```text
//! cargo run --release -p byroredux-hkx --example sound_events -- \
//!     "<Skyrim SE>/Data/Skyrim - Animations.bsa" [path-substring]
//! ```

use byroredux_bsa::BsaArchive;
use byroredux_hkx::decode_behavior_graph;
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let archive = BsaArchive::open(args.next().expect("archive path")).expect("open");
    let filter = args.next().map(|f| f.to_ascii_lowercase());
    let mut by_event: BTreeMap<String, usize> = BTreeMap::new();
    let mut graphs = 0;
    for path in archive.list_files() {
        let lower = path.to_ascii_lowercase();
        if !lower.contains("\\behaviors\\") || !lower.ends_with(".hkx") {
            continue;
        }
        if filter.as_ref().is_some_and(|f| !lower.contains(f)) {
            continue;
        }
        let Ok(bytes) = archive.extract(path) else { continue };
        let Ok(graph) = decode_behavior_graph(&bytes) else { continue };
        graphs += 1;
        let Some(machine) = &graph.machine else { continue };
        for state in &machine.states {
            let Some(clip) = &state.clip else { continue };
            for trigger in &clip.triggers {
                if let Some(name) = graph.event_name(trigger.event) {
                    if name.to_ascii_lowercase().starts_with("soundplay") {
                        *by_event.entry(name.to_owned()).or_default() += 1;
                    }
                }
            }
        }
    }
    println!("{graphs} graphs; {} distinct SoundPlay events", by_event.len());
    for (name, count) in by_event {
        println!("{count:5}  {name}");
    }
}
