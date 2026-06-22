//! `.pex` corpus smoke test — parse every compiled script in one or more
//! real game archives and report coverage, to shake out format edge cases
//! before the Phase 2 decompiler is built on top of the reader.
//!
//! Handles both BSA (Skyrim) and BA2 (FO4 / FO76 / Starfield) by file
//! extension. For each `.pex` it runs [`byroredux_pex::parse`] and tallies
//! successes/failures, the script-type distribution, an opcode histogram
//! (so we can see which opcodes vanilla content actually exercises), and
//! the first handful of failures verbatim.
//!
//! Usage:
//! ```bash
//! cargo run --release -p byroredux-pex --example pex_corpus_smoke -- \
//!     "/path/to/Skyrim Special Edition/Data/Skyrim - Misc.bsa" \
//!     "/path/to/Fallout 4/Data/Fallout4 - Misc.ba2"
//! ```

use std::collections::BTreeMap;

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_pex::{decompile::decompile_script, parse, OpCode, MAX_OPCODE, ScriptType};

/// Minimal archive abstraction over the two container formats — both
/// expose `list_files` + `extract`.
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

#[derive(Default)]
struct Stats {
    total: usize,
    ok: usize,
    failed: usize,
    by_type: BTreeMap<&'static str, usize>,
    opcode_hist: BTreeMap<u8, u64>,
    failures: Vec<(String, String)>,
    max_instr_fn: (String, usize),
    objects_with_no_object: usize,
    // Decompiler pass (parse → CFG → lift → reconstruct → lower to AST).
    decompiled_ok: usize,
    decompiled_err: usize,
    decompiled_panic: usize,
    decompile_failures: Vec<(String, String)>,
}

fn type_name(t: ScriptType) -> &'static str {
    match t {
        ScriptType::Skyrim => "Skyrim",
        ScriptType::Fallout4 => "Fallout4",
        ScriptType::Fallout76 => "Fallout76",
        ScriptType::Starfield => "Starfield",
    }
}

fn tally(pex: &byroredux_pex::Pex, name: &str, stats: &mut Stats) {
    *stats.by_type.entry(type_name(pex.script_type)).or_default() += 1;
    if pex.objects.is_empty() {
        stats.objects_with_no_object += 1;
    }
    for obj in &pex.objects {
        let func_iter = obj
            .states
            .iter()
            .flat_map(|s| s.functions.iter())
            .chain(obj.properties.iter().flat_map(|p| {
                p.read_function.iter().chain(p.write_function.iter())
            }));
        for f in func_iter {
            if f.instructions.len() > stats.max_instr_fn.1 {
                stats.max_instr_fn = (format!("{name}:{}", f.name), f.instructions.len());
            }
            for ins in &f.instructions {
                *stats.opcode_hist.entry(ins.op as u8).or_default() += 1;
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: pex_corpus_smoke <archive.bsa|archive.ba2> [more...]");
        std::process::exit(2);
    }

    let mut stats = Stats::default();
    for path in &args {
        let arch = match Archive::open(path) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("!! could not open {path}: {e}");
                continue;
            }
        };
        let pex_files: Vec<String> = arch
            .list_files()
            .into_iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(".pex"))
            .map(|s| s.to_string())
            .collect();
        eprintln!("{path}: {} .pex files", pex_files.len());

        for f in pex_files {
            stats.total += 1;
            let data = match arch.extract(&f) {
                Ok(d) => d,
                Err(e) => {
                    stats.failed += 1;
                    if stats.failures.len() < 25 {
                        stats.failures.push((f.clone(), format!("extract: {e}")));
                    }
                    continue;
                }
            };
            match parse(&data) {
                Ok(pex) => {
                    stats.ok += 1;
                    tally(&pex, &f, &mut stats);
                    // Decompile too — catch panics so one bad script can't
                    // abort the whole corpus sweep.
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        decompile_script(&pex)
                    })) {
                        Ok(Ok(_)) => stats.decompiled_ok += 1,
                        Ok(Err(e)) => {
                            stats.decompiled_err += 1;
                            if stats.decompile_failures.len() < 25 {
                                stats.decompile_failures.push((f.clone(), e.to_string()));
                            }
                        }
                        Err(_) => {
                            stats.decompiled_panic += 1;
                            if stats.decompile_failures.len() < 25 {
                                stats.decompile_failures.push((f.clone(), "PANIC".to_string()));
                            }
                        }
                    }
                }
                Err(e) => {
                    stats.failed += 1;
                    if stats.failures.len() < 25 {
                        stats.failures.push((f.clone(), e.to_string()));
                    }
                }
            }
        }
    }

    println!("\n==== .pex corpus smoke ====");
    println!("total {}  ok {}  failed {}", stats.total, stats.ok, stats.failed);
    println!("by script type: {:?}", stats.by_type);
    println!(
        "largest function: {} ({} instructions)",
        stats.max_instr_fn.0, stats.max_instr_fn.1
    );
    if stats.objects_with_no_object > 0 {
        println!("files with zero objects: {}", stats.objects_with_no_object);
    }

    println!("\nopcode coverage (count across corpus):");
    let mut unseen = Vec::new();
    for b in 0..MAX_OPCODE {
        let op = OpCode::from_u8(b).unwrap();
        let n = stats.opcode_hist.get(&b).copied().unwrap_or(0);
        if n == 0 {
            unseen.push(op.name());
        } else {
            println!("  {:<28} {}", op.name(), n);
        }
    }
    if !unseen.is_empty() {
        println!("  (never seen: {})", unseen.join(", "));
    }

    if !stats.failures.is_empty() {
        println!("\nfirst {} parse failures:", stats.failures.len());
        for (name, err) in &stats.failures {
            println!("  {name}: {err}");
        }
    }

    let dtot = stats.decompiled_ok + stats.decompiled_err + stats.decompiled_panic;
    if dtot > 0 {
        let pct = 100.0 * stats.decompiled_ok as f64 / dtot as f64;
        println!(
            "\ndecompile → AST: ok {} ({pct:.1}%)  err {}  panic {}",
            stats.decompiled_ok, stats.decompiled_err, stats.decompiled_panic
        );
        if !stats.decompile_failures.is_empty() {
            println!("first {} decompile failures:", stats.decompile_failures.len());
            for (name, err) in &stats.decompile_failures {
                println!("  {name}: {err}");
            }
        }
    }

    // Non-zero exit on any parse failure so this is usable as a gate.
    if stats.failed > 0 {
        std::process::exit(1);
    }
}
