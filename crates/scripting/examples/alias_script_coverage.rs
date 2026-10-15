//! M47.2 metric — how many vanilla quest alias scripts the Papyrus VM runs: every
//! script the QUST `VMAD` alias sections attach, checked as the host does
//! (`check_alias_script`). `VM_REASONS=1` tallies why the VM declines the rest,
//! weighted by the alias instances carrying each script; `FAMILY=<text>` lists
//! the scripts whose decline reason contains `text`.
//!
//! ```bash
//! cargo run --release -p byroredux-scripting --example alias_script_coverage -- \
//!     "<Skyrim SE>/Data/Skyrim.esm" "<Skyrim SE>/Data/Skyrim - Misc.bsa"
//! ```

use std::collections::HashMap;

#[path = "common/archives.rs"]
mod archives;
use archives::{Archive, ArchiveLibrary};
use byroredux_plugin::esm::parse_esm;
use byroredux_scripting::translate::effects::ScriptLibrary;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let esm = args.next().expect("usage: alias_script_coverage <ESM> <scripts archive>");
    let archives = [Archive::open(&args.next().expect("scripts archive"))?];
    let library = ArchiveLibrary::new(&archives);
    let index = parse_esm(&std::fs::read(&esm)?)?;
    let family = std::env::var("FAMILY").ok();

    let mut checks: HashMap<String, Result<Vec<String>, String>> = HashMap::new();
    let mut instances: HashMap<String, usize> = HashMap::new();
    for quest in index.quests.values() {
        for alias in &quest.alias_scripts {
            for script in &alias.scripts.scripts {
                *instances.entry(script.name.to_ascii_lowercase()).or_default() += 1;
            }
        }
    }
    let (mut total, mut running) = (0usize, 0usize);
    let mut reasons: HashMap<String, usize> = HashMap::new();
    let mut handlers: HashMap<String, usize> = HashMap::new();
    for (name, count) in &instances {
        total += count;
        let outcome = checks
            .entry(name.clone())
            .or_insert_with(|| {
                let script = library.script(name).ok_or_else(|| "no .pex".to_owned())?;
                byroredux_scripting::check_alias_script(
                    script,
                    &byroredux_scripting::DELIVERED_ALIAS_EVENTS,
                    Some(&library),
                )
                .map(|(events, _)| events)
            })
            .clone();
        match outcome {
            Ok(events) => {
                running += count;
                for event in events {
                    *handlers.entry(event).or_default() += count;
                }
            }
            Err(reason) => {
                if family.as_deref().is_some_and(|f| reason.contains(f)) {
                    println!("{count:>5}  {name}: {reason}");
                }
                *reasons.entry(reason).or_default() += count;
            }
        }
    }
    if std::env::var("EVENTS").is_ok() {
        // The events the scripts the VM declines for "handles no delivered
        // event" handle instead, weighted by instances.
        let mut census: HashMap<String, usize> = HashMap::new();
        for (name, count) in &instances {
            if checks.get(name).is_some_and(|c| c.as_ref().err().is_some_and(|r| r.contains("no delivered event"))) {
                if let Some(script) = library.script(name) {
                    for item in &script.body {
                        match &item.node {
                            byroredux_papyrus::ast::ScriptItem::Event(e) => {
                                *census.entry(e.name.node.0.clone()).or_default() += count;
                            }
                            byroredux_papyrus::ast::ScriptItem::State(state) => {
                                for item in &state.body {
                                    if let byroredux_papyrus::ast::StateItem::Event(e) = &item.node {
                                        *census.entry(e.name.node.0.clone()).or_default() += count;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        let mut ranked: Vec<_> = census.into_iter().collect();
        ranked.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        println!("events of scripts with no delivered event: {:?}", &ranked[..ranked.len().min(25)]);
    }
    println!(
        "alias script instances: {total}; distinct scripts: {}; the VM runs {running} ({:.1}%)",
        instances.len(),
        100.0 * running as f64 / total.max(1) as f64
    );
    let mut handled: Vec<_> = handlers.into_iter().collect();
    handled.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    println!("events handled by running instances: {handled:?}");
    if std::env::var("VM_REASONS").is_ok() {
        let mut ranked: Vec<_> = reasons.into_iter().collect();
        ranked.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        for (reason, count) in ranked.into_iter().take(40) {
            println!("{count:>6}  {reason}");
        }
    }
    Ok(())
}
