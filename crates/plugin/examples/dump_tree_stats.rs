//! TREE-record diagnostic dump: for each game's master ESM, prints
//! how many TREE bases ship MODB / OBND / BNAM and a sample of MODB
//! values. Used during the #1001 fix to verify Oblivion ships MODB
//! on 100 % of records while FO3/FNV ship OBND on 100 % and MODB on
//! none — the corpus signal that drives `compute_billboard_size`'s
//! OBND → MODB → default precedence in `crates/spt/src/import/mod.rs`.
//!
//! Run with:
//! ```bash
//! cargo run -p byroredux-plugin --release --example dump_tree_stats
//! ```

use byroredux_plugin::esm;
use byroredux_plugin::esm::records::tree::TreeRecord;

fn dump_one(path: &str, label: &str) {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => { eprintln!("{label}: skip ({e})"); return; }
    };
    let parsed = match esm::parse_esm(&bytes) {
        Ok(p) => p,
        Err(e) => { eprintln!("{label}: parse error: {e}"); return; }
    };
    let trees: Vec<&TreeRecord> = parsed.trees.values().collect();
    let n = trees.len();
    let with_modb = trees.iter().filter(|t| t.bound_radius > 0.0).count();
    let with_obnd = trees.iter().filter(|t| t.bounds.is_some()).count();
    let with_bnam = trees.iter().filter(|t| t.billboard_size.is_some()).count();
    let modb_vals: Vec<f32> = trees.iter().filter(|t| t.bound_radius > 0.0).map(|t| t.bound_radius).collect();
    let min = modb_vals.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = modb_vals.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let mean = if modb_vals.is_empty() { 0.0 } else { modb_vals.iter().sum::<f32>() / modb_vals.len() as f32 };
    eprintln!("{label}: {n} TREEs | MODB present: {with_modb} ({:.0}%) | OBND: {with_obnd} ({:.0}%) | BNAM: {with_bnam}",
        100.0 * with_modb as f32 / n.max(1) as f32,
        100.0 * with_obnd as f32 / n.max(1) as f32,
    );
    if !modb_vals.is_empty() {
        eprintln!("  MODB stats: min={min:.2} max={max:.2} mean={mean:.2}");
        eprintln!("  Sample (first 5 with MODB):");
        for t in trees.iter().filter(|t| t.bound_radius > 0.0).take(5) {
            eprintln!("    {} | MODB={:.2} | OBND={:?}", t.editor_id, t.bound_radius, t.bounds);
        }
    }
}
fn main() {
    dump_one("/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/Oblivion.esm", "Oblivion");
    dump_one("/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout3.esm", "FO3");
    dump_one("/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/FalloutNV.esm", "FNV");
}
