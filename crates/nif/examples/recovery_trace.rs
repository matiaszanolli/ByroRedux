//! recovery_trace — capture per-block recovery events for a single NIF.
//!
//! parse_nif logs info! on every runtime-size-cache / oblivion_skip_sizes
//! recovery and warn! on truncation. This runs parse_nif with
//! env_logger at info level and nothing else, so every NiUnknown
//! substitution surfaces on stderr in order. Used by #554 Phase 1.

use byroredux_bsa::BsaArchive;
use byroredux_nif::{blocks::NiUnknown, parse_nif};

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("byroredux_nif=info"),
    )
    .init();

    let args: Vec<String> = std::env::args().collect();
    let data = match args.len() {
        2 => std::fs::read(&args[1]).expect("read file"),
        3 => {
            let archive = BsaArchive::open(&args[1]).expect("open BSA");
            archive.extract(&args[2]).expect("extract NIF")
        }
        _ => {
            eprintln!("usage: recovery_trace <path> | <bsa> <path-in-bsa>");
            std::process::exit(1);
        }
    };

    let scene = parse_nif(&data).expect("parse NIF");
    let mut unk_by_type: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    let mut unk_indices: Vec<(usize, String)> = Vec::new();
    for (i, b) in scene.blocks.iter().enumerate() {
        if let Some(u) = b.as_any().downcast_ref::<NiUnknown>() {
            *unk_by_type.entry(u.type_name.to_string()).or_insert(0) += 1;
            unk_indices.push((i, u.type_name.to_string()));
        }
    }
    println!("─── scene.blocks.len() = {} ───", scene.blocks.len());
    println!("─── NiUnknown substitutions: {} ───", unk_indices.len());
    for (i, name) in &unk_indices {
        println!("  [{}] {}", i, name);
    }
    println!("─── by type ───");
    for (name, cnt) in &unk_by_type {
        println!("  {} × {}", cnt, name);
    }
}
