//! Census the Fallout 4 previs (`.uvd`) corpus across one or more BA2s
//! (#3810, EX-14/15 item C3).
//!
//! Every claim in `byroredux_bsa::uvd`'s module doc was reached by running
//! this over the complete 1 413-file corpus — base game plus the three DLC
//! archives that ship previs. Re-run it before trusting, extending or
//! contradicting any of them; a relation confirmed on a handful of samples
//! is exactly what that doc's "Rejected" list is a record of.
//!
//! Prints the exterior/interior split (the cell-grid relation), and the
//! three candidate section-directory strides that are *not* encoded in the
//! parser because they hold on only part of the corpus.
//!
//! The one claim this cannot re-check on its own is that an exterior box's
//! centre cell equals the owning `CELL`'s `XCLC` — that needs a parsed ESM,
//! which this crate has no business opening. It is pinned instead by
//! `exterior_bounds_recover_the_owning_cell_grid`.
//!
//! Usage:
//!   cargo run --release -p byroredux-bsa --example probe_uvd_corpus -- <BA2> [BA2 ...]

use byroredux_bsa::parse_uvd_header;

/// Round up to the 16-byte boundary the section directory appears to pad to.
fn align16(v: u64) -> u64 {
    (v + 15) & !15
}

fn u32_at(data: &[u8], offset: usize) -> u64 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as u64
}

fn main() {
    let (mut total, mut exterior, mut interior, mut unparsed) = (0usize, 0usize, 0usize, 0usize);
    let (mut r1, mut r2, mut r3) = (0usize, 0usize, 0usize);
    for path in std::env::args().skip(1) {
        let Ok(archive) = byroredux_bsa::Ba2Archive::open(&path) else {
            eprintln!("skip {path}");
            continue;
        };
        let names: Vec<String> = archive
            .list_files()
            .iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(".uvd"))
            .map(|f| f.to_string())
            .collect();
        for name in &names {
            let Ok(data) = archive.extract(name) else {
                continue;
            };
            total += 1;
            let Ok(header) = parse_uvd_header(&data) else {
                unparsed += 1;
                continue;
            };
            match header.exterior_cell_grid() {
                Some(_) => exterior += 1,
                None => interior += 1,
            }
            // The unencoded section-directory candidates. `0x40` reads as a
            // count shared by three sections laid out back to back.
            let count = u32_at(&data, 0x40);
            r1 += usize::from(align16(u32_at(&data, 0x44) + 24 * count) == u32_at(&data, 0x48));
            r2 += usize::from(align16(u32_at(&data, 0x48) + 32 * count) == u32_at(&data, 0x50));
            r3 += usize::from(align16(u32_at(&data, 0x50) + 4 * count) == u32_at(&data, 0xA0));
        }
        eprintln!("{path}: {} previs files", names.len());
    }
    println!("{total} .uvd files, {unparsed} rejected by parse_uvd_header");
    println!("  exterior (3x3 cell block): {exterior}");
    println!("  interior (content bounds): {interior}");
    println!("section-directory candidates, NOT encoded in the parser:");
    println!("  align16(0x44 + 24 * count) == 0x48: {r1}/{total}");
    println!("  align16(0x48 + 32 * count) == 0x50: {r2}/{total}");
    println!("  align16(0x50 +  4 * count) == 0xA0: {r3}/{total}");
}
