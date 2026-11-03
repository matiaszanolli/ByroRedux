// M39 Phase 0 census (docs/prd/texture-streaming.md, TXS-SRC-001, A4).
//
// For every DX10 record in the given texture BA2s, reads the chunk layout
// from the record table (`Ba2Archive::dx10_layout`, nothing is extracted)
// and reports:
//   - how many chunks each record ships, and how many ship one only
//     (no separate tail: a floor read would be a whole-file read);
//   - the edge of the largest level in the last chunk (the "floor" a
//     last-chunk read gives, decision Q3);
//   - the bytes that floor holds against the whole chain (inflated sizes).
// Cubemaps are counted apart: they are never streamable (§3.2).
//
// Usage:
//   cargo run --release -p byroredux-bsa --example ba2_mip_chunk_census -- \
//       "<Data>/Fallout4 - Textures1.ba2" "<Data>/Fallout4 - Textures2.ba2" ...
use byroredux_bsa::{Ba2Archive, Ba2Variant};
use std::collections::BTreeMap;

#[derive(Default)]
struct Tally {
    records: usize,
    cubemaps: usize,
    single_chunk: usize,
    chunk_counts: BTreeMap<usize, usize>,
    /// Largest-level edge of the last chunk → record count (multi-chunk only).
    tail_edge: BTreeMap<u32, usize>,
    /// Same, single-mip-per-chunk shape check: records whose every non-tail
    /// chunk holds exactly one level.
    one_level_heads: usize,
    total_bytes: u64,
    tail_bytes: u64,
    single_chunk_bytes: u64,
    /// Single-chunk 2D records by their level-0 edge → (records, bytes).
    single_chunk_edge: BTreeMap<u32, (usize, u64)>,
    /// Every single-chunk 2D record: (level-0 edge, mip count, bytes, name),
    /// classified once the dominant tail edge is known.
    single_chunk_records: Vec<(u32, u8, u64, String)>,
    truncated_chain: usize,
    single_mip: usize,
}

fn full_chain(width: u16, height: u16) -> u32 {
    let m = u32::from(width.max(height).max(1));
    32 - m.leading_zeros()
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: ba2_mip_chunk_census <texture.ba2>...");
        std::process::exit(2);
    }
    let mut t = Tally::default();
    for path in &paths {
        let archive = match Ba2Archive::open(path) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("! open failed: {path}: {e}");
                continue;
            }
        };
        if archive.variant() != Ba2Variant::Dx10 {
            eprintln!("- skipped (not DX10): {path}");
            continue;
        }
        let before = t.records;
        for name in archive.list_files() {
            let Some(layout) = archive.dx10_layout(name) else {
                continue;
            };
            t.records += 1;
            let bytes: u64 = layout
                .chunks
                .iter()
                .map(|c| u64::from(c.unpacked_size))
                .sum();
            if layout.is_cubemap {
                t.cubemaps += 1;
                continue;
            }
            if u32::from(layout.num_mips) < full_chain(layout.width, layout.height) {
                t.truncated_chain += 1;
            }
            if layout.num_mips <= 1 {
                t.single_mip += 1;
            }
            t.total_bytes += bytes;
            *t.chunk_counts.entry(layout.chunks.len()).or_default() += 1;
            if layout.chunks.len() <= 1 {
                t.single_chunk += 1;
                t.single_chunk_bytes += bytes;
                let edge = u32::from(layout.width.max(layout.height)).max(1);
                t.single_chunk_records
                    .push((edge, layout.num_mips, bytes, name.to_string()));
                let slot = t.single_chunk_edge.entry(edge).or_default();
                slot.0 += 1;
                slot.1 += bytes;
                continue;
            }
            let tail = layout.chunks.last().expect("multi-chunk");
            let edge = u32::from(layout.width.max(layout.height)) >> tail.start_mip.min(31);
            *t.tail_edge.entry(edge.max(1)).or_default() += 1;
            t.tail_bytes += u64::from(tail.unpacked_size);
            let heads = &layout.chunks[..layout.chunks.len() - 1];
            if heads.iter().all(|c| c.start_mip == c.end_mip) {
                t.one_level_heads += 1;
            }
        }
        eprintln!("+ {} DX10 records: {path}", t.records - before);
    }

    let mib = |b: u64| b as f64 / (1024.0 * 1024.0);
    // The floor a last-chunk read gives is the dominant tail edge (Q3). A
    // single-chunk record is a real whole-file fallback (TXS-SRC-004) only
    // when it has a chain above that edge; single-mip records and chains
    // that fit inside it are the **whole** class (§3.2) anyway.
    let floor_edge = t
        .tail_edge
        .iter()
        .max_by_key(|(_, c)| **c)
        .map(|(e, _)| *e)
        .unwrap_or(0);
    let (mut whole_single_mip, mut whole_fits, mut fallback) =
        ((0usize, 0u64), (0usize, 0u64), (0usize, 0u64));
    for (edge, mips, bytes, name) in &t.single_chunk_records {
        let slot = if *mips <= 1 {
            &mut whole_single_mip
        } else if *edge <= floor_edge {
            &mut whole_fits
        } else {
            eprintln!("  fallback {edge}px mips={mips} {name}");
            &mut fallback
        };
        slot.0 += 1;
        slot.1 += bytes;
    }
    let streamable_pool = t.records - t.cubemaps;
    let multi = streamable_pool - t.single_chunk;
    println!("## BA2 DX10 chunk census\n");
    println!("| Measure | Value |");
    println!("|---|---|");
    println!("| Archives | {} |", paths.len());
    println!("| DX10 records | {} |", t.records);
    println!("| Cubemaps (never streamable) | {} |", t.cubemaps);
    println!("| 2D records | {} |", streamable_pool);
    println!("| Single-mip 2D records | {} |", t.single_mip);
    println!("| Truncated mip chains | {} |", t.truncated_chain);
    println!(
        "| One chunk only | {} ({:.1} %) |",
        t.single_chunk,
        100.0 * t.single_chunk as f64 / streamable_pool.max(1) as f64
    );
    println!(
        "| … single-mip (**whole**) | {} ({:.1} MiB) |",
        whole_single_mip.0,
        mib(whole_single_mip.1)
    );
    println!(
        "| … chain fits inside the {floor_edge}-px floor (**whole**) | {} ({:.1} MiB) |",
        whole_fits.0,
        mib(whole_fits.1)
    );
    println!(
        "| … chain above the floor: real whole-file fallback | {} ({:.1} MiB) |",
        fallback.0,
        mib(fallback.1)
    );
    println!("| Multi-chunk (floor-readable) | {} |", multi);
    println!(
        "| Multi-chunk whose head chunks each hold one level | {} |",
        t.one_level_heads
    );
    println!(
        "| 2D bytes, whole chains (inflated) | {:.1} MiB |",
        mib(t.total_bytes)
    );
    println!(
        "| 2D bytes in single-chunk records | {:.1} MiB |",
        mib(t.single_chunk_bytes)
    );
    println!(
        "| 2D bytes in multi-chunk tails (the floor) | {:.1} MiB |",
        mib(t.tail_bytes)
    );
    let multi_total = t.total_bytes - t.single_chunk_bytes;
    println!(
        "| Floor share of multi-chunk bytes | {:.1} % |",
        100.0 * t.tail_bytes as f64 / multi_total.max(1) as f64
    );
    println!("\n### Chunks per 2D record\n");
    println!("| Chunks | Records |");
    println!("|---|---|");
    for (n, c) in &t.chunk_counts {
        println!("| {n} | {c} |");
    }
    println!("\n### Single-chunk 2D records by level-0 edge\n");
    println!("| Edge (px) | Records | Inflated bytes |");
    println!("|---|---|---|");
    for (edge, (c, b)) in t.single_chunk_edge.iter().rev() {
        println!("| {edge} | {c} | {:.1} MiB |", mib(*b));
    }
    println!("\n### Largest-level edge of the last chunk (multi-chunk 2D records)\n");
    println!("| Edge (px) | Records | Share |");
    println!("|---|---|---|");
    for (edge, c) in t.tail_edge.iter().rev() {
        println!(
            "| {edge} | {c} | {:.1} % |",
            100.0 * *c as f64 / multi.max(1) as f64
        );
    }
}
