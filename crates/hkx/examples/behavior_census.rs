//! Decode every behavior graph in a Skyrim SE animation archive and report how
//! many decode, what they contain and which fail — the check behind the 64-bit
//! Havok 2010.2 field offsets in `behavior.rs`.
//!
//! ```text
//! cargo run --release -p byroredux-hkx --example behavior_census -- \
//!     "<Skyrim SE>/Data/Skyrim - Animations.bsa" [path-substring]
//! ```

use byroredux_bsa::BsaArchive;
use byroredux_hkx::decode_behavior_graph;
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let archive = BsaArchive::open(args.next().expect("archive path")).expect("open");
    let filter = args.next().map(|f| f.to_ascii_lowercase());
    let (mut ok, mut failed, mut with_machine, mut with_clips) = (0, 0, 0, 0);
    let mut errors: BTreeMap<String, usize> = BTreeMap::new();
    let mut inconsistent = 0;
    for path in archive.list_files() {
        let lower = path.to_ascii_lowercase();
        if !lower.contains("\\behaviors\\") || !lower.ends_with(".hkx") {
            continue;
        }
        if filter.as_ref().is_some_and(|f| !lower.contains(f)) {
            continue;
        }
        let Ok(bytes) = archive.extract(path) else { continue };
        match decode_behavior_graph(&bytes) {
            Ok(graph) => {
                ok += 1;
                let Some(machine) = &graph.machine else { continue };
                with_machine += 1;
                let states: std::collections::HashSet<i32> = machine.states.iter().map(|s| s.id).collect();
                let mut bad = !states.contains(&machine.start_state);
                for state in &machine.states {
                    for t in state.transitions.iter().chain(&machine.wildcard) {
                        bad |= !states.contains(&t.to_state) || graph.event_name(t.event).is_none() && t.event >= 0;
                    }
                    if let Some(clip) = &state.clip {
                        with_clips += 1;
                        bad |= clip.animation.is_empty() || !clip.playback_speed.is_finite();
                        for trigger in &clip.triggers {
                            bad |= graph.event_name(trigger.event).is_none();
                        }
                    }
                }
                if bad {
                    inconsistent += 1;
                    println!("inconsistent: {path}");
                }
            }
            Err(error) => {
                failed += 1;
                *errors.entry(error.to_string()).or_default() += 1;
            }
        }
    }
    println!("decoded {ok}, failed {failed}; {with_machine} with a state machine, {with_clips} state clips, {inconsistent} inconsistent");
    for (error, count) in errors {
        println!("  {count:5}  {error}");
    }
}
