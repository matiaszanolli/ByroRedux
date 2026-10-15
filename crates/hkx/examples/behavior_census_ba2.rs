//! Decode every behavior graph in a Fallout 4 animation archive (BA2, Havok 2014
//! packfiles) and report how
//! many decode, what they contain and which fail — the check behind the 64-bit
//! Havok 2014 field offsets in `behavior.rs` (`LAYOUT_2014`); the Skyrim twin is
//! `behavior_census`. Each inconsistent file prints what is wrong with it.
//!
//! ```text
//! cargo run --release -p byroredux-hkx --example behavior_census_ba2 -- \
//!     "<Fallout 4>/Data/Fallout4 - Animations.ba2" [path-substring]
//! ```

use byroredux_bsa::Ba2Archive;
use byroredux_hkx::decode_behavior_graph;
use std::collections::BTreeMap;

fn main() {
    let mut args = std::env::args().skip(1);
    let archive = Ba2Archive::open(args.next().expect("archive path")).expect("open");
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
                let mut why = String::new();
                if bad { why.push_str(&format!("start={} states={:?}; ", machine.start_state, machine.states.iter().map(|s| s.id).collect::<Vec<_>>())); }
                for state in &machine.states {
                    for t in state.transitions.iter().chain(&machine.wildcard) {
                        let b = !states.contains(&t.to_state) || graph.event_name(t.event).is_none() && t.event >= 0; if b { why.push_str(&format!("trans ev={} to={}; ", t.event, t.to_state)); } bad |= b;
                    }
                    if let Some(clip) = &state.clip {
                        with_clips += 1;
                        let b = clip.animation.is_empty() || !clip.playback_speed.is_finite(); if b { why.push_str(&format!("clip {:?} speed {}; ", clip.animation, clip.playback_speed)); } bad |= b;
                        for trigger in &clip.triggers {
                            let b = graph.event_name(trigger.event).is_none(); if b { why.push_str(&format!("trigger ev {}; ", trigger.event)); } bad |= b;
                        }
                    }
                }
                if bad {
                    inconsistent += 1;
                    println!("inconsistent: {path}: {}", &why[..why.len().min(200)]);
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
