//! Dimension-8 parse-cost probe (throwaway audit scratch).
//!
//! Times the production `parse_esm` entry point on a real master and
//! prints the resulting `EsmIndex` category totals.
//!
//! Usage: cargo run --release -p byroredux-plugin --example esm_dim8_bench -- <file.esm>

use byroredux_plugin::esm::records::parse_esm;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: esm_dim8_bench <file.esm>"))?;

    let t_read = std::time::Instant::now();
    let bytes = std::fs::read(&path)?;
    let read_ms = t_read.elapsed().as_secs_f64() * 1000.0;

    let t = std::time::Instant::now();
    let index = parse_esm(&bytes)?;
    let parse_ms = t.elapsed().as_secs_f64() * 1000.0;

    let interior = index.cells.cells.len();
    let exterior: usize = index.cells.exterior_cells.values().map(|m| m.len()).sum();
    println!(
        "BENCH\t{}\t{}\t{:.1}\t{:.1}\t{:?}\t{}\t{}\t{}\t{}\t{}",
        path,
        bytes.len(),
        read_ms,
        parse_ms,
        index.game,
        index.total(),
        interior,
        exterior,
        index.cells.statics.len(),
        index.navmeshes.len(),
    );
    println!("{}", index.category_breakdown());
    // Keep the index alive across the RSS high-water mark.
    std::hint::black_box(&index);
    Ok(())
}
