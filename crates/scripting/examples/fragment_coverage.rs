//! Empirical b2 coverage — what fraction of the real vanilla fragment
//! corpus the [`lower_fragment`] effect table actually claims.
//!
//! The compositional-scaling design
//! ([`docs/engine/m47-2-recognizer-scaling.md`]) predicted, from the
//! primitive-frequency curve, that a small effect vocabulary covers a
//! large share of the 43,818 behavioral `Fragment_*` functions. This
//! example measures the *current* table's real claim rate end-to-end:
//! decompile every `.pex`, lower every fragment, tally claimed vs
//! declined (with the decline reasons, so the next primitives to add are
//! obvious).
//!
//! It is the b2 analog of `pex_corpus_shapes` and doubles as a
//! coverage-regression gate as the effect table grows.
//!
//! ```bash
//! cargo run --release -p byroredux-scripting --example fragment_coverage -- \
//!     "<Skyrim SE>/Data/Skyrim - Misc.bsa" \
//!     "<Fallout 4>/Data/Fallout4 - Misc.ba2" \
//!     "<Starfield>/Data/Starfield - Misc.ba2"
//! ```

use std::collections::BTreeMap;

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_papyrus::ast::ScriptItem;
use byroredux_pex::{decompile::decompile_script, parse};
use byroredux_scripting::translate::effects::{lower_fragment, Effect};

enum Archive {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl Archive {
    fn open(path: &str) -> std::io::Result<Self> {
        if path.to_ascii_lowercase().ends_with(".ba2") {
            Ok(Archive::Ba2(Ba2Archive::open(path)?))
        } else {
            Ok(Archive::Bsa(BsaArchive::open(path)?))
        }
    }
    fn list_files(&self) -> Vec<&str> {
        match self {
            Archive::Bsa(a) => a.list_files(),
            Archive::Ba2(a) => a.list_files(),
        }
    }
    fn extract(&self, path: &str) -> std::io::Result<Vec<u8>> {
        match self {
            Archive::Bsa(a) => a.extract(path),
            Archive::Ba2(a) => a.extract(path),
        }
    }
}

fn effect_kind(e: &Effect) -> &'static str {
    match e {
        Effect::SetStage { .. } => "SetStage",
        Effect::SetObjectiveDisplayed { .. } => "SetObjectiveDisplayed",
        Effect::SetObjectiveCompleted { .. } => "SetObjectiveCompleted",
        Effect::SetObjectiveFailed { .. } => "SetObjectiveFailed",
        Effect::CompleteAllObjectives { .. } => "CompleteAllObjectives",
        Effect::AddItem { .. } => "AddItem",
        Effect::MoveTo { .. } => "MoveTo",
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: fragment_coverage <archive.bsa|archive.ba2> [more...]");
        std::process::exit(2);
    }

    let mut behavioral = 0usize; // non-empty fragments
    let mut claimed = 0usize; // fully lowered
    let mut empty = 0usize; // empty fragments (trivially lowered)
    let mut effect_hist: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut claimed_effects = 0usize;

    for path in &args {
        let Ok(arch) = Archive::open(path) else {
            eprintln!("!! could not open {path}");
            continue;
        };
        let pex_files: Vec<String> = arch
            .list_files()
            .into_iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(".pex"))
            .map(|s| s.to_string())
            .collect();
        eprintln!("{path}: {} .pex", pex_files.len());

        for f in pex_files {
            let Ok(data) = arch.extract(&f) else { continue };
            let Ok(pex) = parse(&data) else { continue };
            // Catch a decompiler panic so one bad script can't abort the sweep.
            let Ok(Ok(script)) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decompile_script(&pex)))
            else {
                continue;
            };
            for item in &script.body {
                let ScriptItem::Function(func) = &item.node else {
                    continue;
                };
                if !func.name.node.0.to_ascii_lowercase().starts_with("fragment") {
                    continue;
                }
                if func.body.is_empty() {
                    empty += 1;
                    continue;
                }
                behavioral += 1;
                if let Some(effects) = lower_fragment(&func.body) {
                    claimed += 1;
                    claimed_effects += effects.len();
                    for e in &effects {
                        *effect_hist.entry(effect_kind(e)).or_default() += 1;
                    }
                }
            }
        }
    }

    let pct = |n: usize, d: usize| if d == 0 { 0.0 } else { 100.0 * n as f64 / d as f64 };
    println!("\n######## b2 fragment-lowerer coverage ########");
    println!("empty fragments (trivial no-op): {empty}");
    println!("behavioral fragments: {behavioral}");
    println!(
        "fully lowered (claimed): {claimed} ({:.1}% of behavioral)",
        pct(claimed, behavioral)
    );
    println!("declined: {} ({:.1}%)", behavioral - claimed, pct(behavioral - claimed, behavioral));
    println!("\ncanonical effects emitted: {claimed_effects}");
    for (k, n) in &effect_hist {
        println!("  {k:<24} {n}");
    }
}
