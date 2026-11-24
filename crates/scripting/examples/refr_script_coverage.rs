//! M47.2 Phase 3 metric — how many vanilla *scripted references* the
//! recognizer chain gives working ECS behavior: every placed REFR / ACHR whose
//! own `VMAD`, or whose base record's `VMAD`, attaches a Papyrus script; a
//! reference counts as covered when at least one of its scripts is
//! recognized. The per-family table (script name → references carrying it) is
//! what the "top-N families" recognizer work is driven by.
//!
//! A script no recognizer takes counts as covered by the Papyrus VM when
//! `check_reference_script` accepts it, as the cell loader would attach it;
//! `VM_REASONS=1` tallies why the VM declines the rest, weighted by the
//! references carrying each script; `VM_REASON_FAMILIES=<text>` lists the
//! script families whose decline reason contains `text`. `VM_REPORT_ATTACHMENTS=1`
//! emits one tab-separated row per unique placed FormID and script name, with
//! `attachment`, FormID, script name, admission path and decline reason columns.
//!
//! ```bash
//! cargo run --release -p byroredux-scripting --example refr_script_coverage -- \
//!     "<Skyrim SE>/Data/Skyrim.esm" "<Skyrim SE>/Data/Skyrim - Misc.bsa"
//! ```

use std::collections::{HashMap, HashSet};

#[path = "common/archives.rs"]
mod archives;
use archives::{Archive, ArchiveLibrary};
use byroredux_plugin::esm::parse_esm;
use byroredux_plugin::esm::records::script_instance::ScriptInstanceData;
use byroredux_scripting::translate::effects::ScriptLibrary;
use byroredux_scripting::translate_pex;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let esm = args.next().expect("usage: refr_script_coverage <ESM> <scripts archive>");
    let archives = [Archive::open(&args.next().expect("scripts archive"))?];
    let archive = &archives[0];
    let library = ArchiveLibrary::new(&archives);
    let index = parse_esm(&std::fs::read(&esm)?)?;
    // Script name (lowercased) → whether the VM runs it, or why not.
    let mut vm_checks: HashMap<String, Result<(), String>> = HashMap::new();
    let mut vm_check = |name: &str| -> Result<(), String> {
        vm_checks
            .entry(name.to_ascii_lowercase())
            .or_insert_with(|| {
                let script = library.script(name).ok_or_else(|| "no .pex".to_owned())?;
                byroredux_scripting::check_reference_script(script, &byroredux_scripting::DELIVERED_EVENTS, Some(&library))
                    .map(|_| ())
            })
            .clone()
    };
    let mut vm_covered = 0usize;
    let mut vm_reasons: HashMap<String, usize> = HashMap::new();
    let report_attachments = std::env::var_os("VM_REPORT_ATTACHMENTS").is_some();
    let mut reported = HashSet::new();

    // Script name (lowercased) → (references carrying it, recognized?).
    let mut families: HashMap<String, (usize, Option<bool>)> = HashMap::new();
    let (mut scripted, mut covered) = (0usize, 0usize);
    let mut recognized_cache: HashMap<String, bool> = HashMap::new();
    let mut recognize = |name: &str, vmad: &ScriptInstanceData| -> bool {
        let key = name.to_ascii_lowercase();
        if let Some(&hit) = recognized_cache.get(&key) {
            return hit;
        }
        let path = format!("scripts\\{}.pex", key.replace(':', "\\"));
        let hit = archive
            .extract(&path)
            .ok()
            .and_then(|bytes| translate_pex(&bytes, index.game, Some(vmad), None))
            .is_some();
        recognized_cache.insert(key, hit);
        hit
    };

    let cells = index
        .cells
        .cells
        .values()
        .chain(index.cells.exterior_cells.values().flat_map(|grid| grid.values()));
    for cell in cells {
        for placed in &cell.references {
            let mut vmads: Vec<&ScriptInstanceData> = Vec::new();
            vmads.extend(placed.script_instance.as_ref());
            vmads.extend(index.base_record_script_instance(placed.base_form_id));
            if vmads.iter().all(|vmad| vmad.scripts.is_empty()) {
                continue;
            }
            scripted += 1;
            let mut any = false;
            let mut by_vm = false;
            for vmad in vmads {
                for script in &vmad.scripts {
                    let hit = recognize(&script.name, vmad);
                    any |= hit;
                    let (admission, reason) = if hit {
                        ("recognizer", String::new())
                    } else {
                        match vm_check(&script.name) {
                            Ok(()) => {
                                by_vm = true;
                                ("vm", String::new())
                            }
                            Err(why) => {
                                *vm_reasons.entry(why.clone()).or_default() += 1;
                                ("declined", why)
                            }
                        }
                    };
                    let name = script.name.to_ascii_lowercase();
                    if report_attachments && reported.insert((placed.form_id, name.clone())) {
                        println!("attachment\t{:08X}\t{name}\t{admission}\t{reason}", placed.form_id);
                    }
                    let family = families.entry(name).or_default();
                    family.0 += 1;
                    family.1 = Some(hit);
                }
            }
            covered += usize::from(any);
            vm_covered += usize::from(any || by_vm);
        }
    }
    let pct = 100.0 * covered as f64 / scripted.max(1) as f64;
    println!("scripted references: {scripted}; covered by a recognizer: {covered} ({pct:.1}%)");
    let vm_pct = 100.0 * vm_covered as f64 / scripted.max(1) as f64;
    println!("  with the Papyrus VM: {vm_covered} ({vm_pct:.1}%)");
    // `VM_REASON_FAMILIES=<text>`: the script families whose VM decline
    // reason contains `text`, by references carrying them.
    if let Ok(needle) = std::env::var("VM_REASON_FAMILIES") {
        let mut hits: Vec<(&String, usize)> = families
            .iter()
            .filter(|(name, _)| vm_checks.get(*name).is_some_and(|check| check.as_ref().is_err_and(|why| why.contains(&needle))))
            .map(|(name, (count, _))| (name, *count))
            .collect();
        hits.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        for (name, count) in hits.into_iter().take(25) {
            println!("    {needle}: {count:>6}  {name}  ({})", vm_checks[name].as_ref().unwrap_err());
        }
    }
    if std::env::var("VM_REASONS").is_ok() {
        let mut reasons: Vec<_> = vm_reasons.iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(a.1));
        for (reason, count) in reasons.into_iter().take(40) {
            println!("    vm: {reason:60} {count}");
        }
    }
    let mut ranked: Vec<_> = families.into_iter().collect();
    ranked.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then_with(|| a.0.cmp(&b.0)));
    println!("top script families (references, recognized):");
    for (name, (count, hit)) in ranked.iter().take(40) {
        let how = if *hit == Some(true) {
            "yes"
        } else if vm_checks.get(name).is_some_and(Result::is_ok) {
            " vm"
        } else {
            " no"
        };
        println!("  {count:>6}  {how}  {name}");
    }
    Ok(())
}
