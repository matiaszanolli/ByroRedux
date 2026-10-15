//! Print the decoded behavior graph(s) of an animation archive whose path
//! contains the given substring: events, states, transitions and clips.
//!
//! ```text
//! cargo run --release -p byroredux-hkx --example graph_dump -- \
//!     "<Skyrim SE>/Data/Skyrim - Animations.bsa" <path-substring> [save-dir]
//! ```

use byroredux_bsa::BsaArchive;
use byroredux_hkx::decode_behavior_graph;

fn main() {
    let mut args = std::env::args().skip(1);
    let archive = BsaArchive::open(args.next().expect("archive path")).expect("open");
    let filter = args.next().expect("a path substring").to_ascii_lowercase();
    // Optional third argument: also write each matching file's bytes here.
    let save_dir = args.next();
    for path in archive.list_files() {
        let lower = path.to_ascii_lowercase();
        if !lower.contains(&filter) || !lower.ends_with(".hkx") {
            continue;
        }
        let Ok(bytes) = archive.extract(path) else { continue };
        if let Some(dir) = &save_dir {
            let name = path.rsplit('\\').next().unwrap_or(path);
            std::fs::write(std::path::Path::new(dir).join(name), &bytes).expect("write");
        }
        let Ok(graph) = decode_behavior_graph(&bytes) else { continue };
        println!("== {path}");
        println!("events: {:?}", graph.events);
        let Some(machine) = &graph.machine else {
            println!("(no state machine)");
            continue;
        };
        fn show(graph: &byroredux_hkx::BehaviorGraph, machine: &byroredux_hkx::StateMachine, indent: usize) {
            let pad = " ".repeat(indent);
            let wildcard: Vec<_> = machine.wildcard.iter().map(|t| (graph.event_name(t.event), t.to_state)).collect();
            println!("{pad}start {} ; wildcard {wildcard:?}", machine.start_state);
            for state in &machine.states {
                println!("{pad}state {} '{}'", state.id, state.name);
                for t in &state.transitions {
                    println!("{pad}    on {:?} -> {} (flags {:#x}, priority {})", graph.event_name(t.event), t.to_state, t.flags, t.priority);
                }
                if let Some(clip) = &state.clip {
                    let triggers: Vec<_> = clip.triggers.iter().map(|t| (graph.event_name(t.event), t.local_time, t.relative_to_end)).collect();
                    println!("{pad}    clip {:?} mode {:?} speed {} triggers {triggers:?}", clip.animation, clip.mode, clip.playback_speed);
                }
                for clip in &state.content.blended_clips {
                    println!("{pad}    blended clip {:?} mode {:?}", clip.animation, clip.mode);
                }
                for sequence in &state.content.sequences {
                    println!("{pad}    sequence {sequence:?}");
                }
                for inner in &state.content.machines {
                    println!("{pad}    nested machine:");
                    show(graph, inner, indent + 8);
                }
            }
        }
        show(&graph, machine, 0);
    }
}
