//! Starfield ESM baseline smoke-test tool. Phase 0 of the Starfield
//! ESM roadmap (`docs/engine/starfield-esm-roadmap.md`).
//!
//! Walks an ESM file at the GRUP level (no per-record parsing) and
//! reports:
//!   * HEDR detection result (variant, hedr_version, derived `GameKind`)
//!   * Master file list
//!   * Total file size + bytes consumed by GRUPs vs orphan records
//!   * Per-top-level-GRUP-FourCC record/byte counts
//!   * Which FourCCs the existing `records/mod.rs` dispatch HANDLES vs
//!     silently SKIPS (via the catch-all `_ => skip_group` at
//!     `records/mod.rs:925`)
//!   * Any byte-level errors that would have caused a `?`-bailout in
//!     the real parser (reported, but the walker keeps going so the
//!     baseline covers the full file)
//!
//! Usage:
//!   cargo run --release -p byroredux-plugin --example sf_smoke -- <ESM_PATH> [--tsv]
//!
//! With `--tsv` writes a tab-separated baseline row to stdout (one
//! line per (esm, fourcc)) suitable for the
//! `.claude/audit-baselines/sf-esm/` checked-in baselines.

use byroredux_plugin::esm::reader::{EsmReader, EsmVariant, GameKind, GroupHeader, RecordHeader};
use std::collections::BTreeMap;

/// FourCCs the existing top-level dispatch in
/// `crates/plugin/src/esm/records/mod.rs` actually routes to a
/// per-record parser (vs the `_ => skip_group(&group)` catch-all at
/// `records/mod.rs:925`). Sourced by grepping every `b"XXXX" =>` arm
/// in the dispatch. Re-grep when the dispatch grows.
///
/// FO4-plus-gated arms (SCOL, PKIN, MOVS, MSWP) are included — the
/// dispatch handles them when game_kind is FO4/FO76/Starfield, and
/// warn-skips them otherwise. From this tool's perspective, they
/// count as "handled by the dispatch" even though they may be
/// gate-skipped.
const DISPATCH_HANDLED_FOURCCS: &[&[u8; 4]] = &[
    // Cell-only labels
    b"CELL", b"WRLD", b"LTEX", b"TXST", b"SCOL", b"PKIN", b"MOVS", b"MSWP",
    // MODL-only labels (single-arm STAT-family branch)
    b"STAT", b"MSTT", b"FURN", b"DOOR", b"LIGH", b"FLOR", b"IDLM", b"BNDS", b"ADDN", b"TACT",
    // TREE
    b"TREE",
    // Item record types
    b"WEAP", b"ARMO", b"AMMO", b"MISC", b"KEYM", b"ALCH", b"INGR",
    // ... grep `records/mod.rs` for the full list when the tool needs
    // updating; this slice is a snapshot taken 2026-05-28.
];

fn is_dispatch_handled(fourcc: &[u8; 4]) -> bool {
    DISPATCH_HANDLED_FOURCCS
        .iter()
        .any(|&handled| handled == fourcc)
}

#[derive(Default, Debug)]
struct GrupStats {
    /// Number of top-level GRUPs with this FourCC label.
    grup_count: u64,
    /// Sum of `total_size` across those GRUPs (includes the group
    /// header). Indicates how much of the file is dedicated to this
    /// record type.
    bytes_total: u64,
    /// Number of immediate child RECORD entries inside those GRUPs.
    /// Nested sub-GRUPs (CELL block/sub-block hierarchies) are NOT
    /// recursed — this is the "top-level child" count, not the
    /// "transitive record" count. CELL / WRLD will appear LOW here
    /// because their content lives in nested cell-block GRUPs; that's
    /// expected and surfaced in the report.
    immediate_records: u64,
    /// True if at least one walk hit a byte-level error (truncated
    /// record header, out-of-bounds advance). The walker keeps going
    /// past the error within this top-level GRUP by skipping to
    /// `group_content_end`, so per-FourCC totals stay coherent.
    saw_byte_error: bool,
}

#[derive(Default, Debug)]
struct WalkReport {
    /// Bytes consumed by the TES4 header (subtracted from file size
    /// to derive "GRUP payload bytes" below).
    #[allow(dead_code)] // exposed via the console report only — kept on the struct for diffing.
    tes4_bytes: u64,
    /// Total bytes accounted for by top-level GRUPs (sum of
    /// `grup.total_size`).
    grup_bytes_total: u64,
    /// Stray top-level records (not inside any GRUP).
    orphan_record_count: u64,
    /// Per-FourCC stats keyed on the GRUP label (which equals the
    /// record type for type-0 top-level GRUPs).
    by_fourcc: BTreeMap<[u8; 4], GrupStats>,
    /// Byte-level errors caught during the walk. Each entry is
    /// `(byte_offset, fourcc_context, message)`.
    errors: Vec<(usize, [u8; 4], String)>,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let emit_tsv = args.iter().any(|a| a == "--tsv");
    args.retain(|a| a != "--tsv");
    let esm_path = args
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("usage: sf_smoke ESM_PATH [--tsv]"))?;

    let bytes = std::fs::read(&esm_path)?;
    let file_len = bytes.len() as u64;

    // Phase 0: header detection. EsmVariant::detect reads up to 24
    // bytes of header; GameKind::from_header derives the game from
    // (variant, hedr_version).
    let variant = EsmVariant::detect(&bytes);
    let mut reader = EsmReader::with_variant(&bytes, variant);
    let file_header = reader.read_file_header()?;
    let game_kind = GameKind::from_header(variant, file_header.hedr_version);
    let tes4_bytes = reader.position() as u64;

    eprintln!("[sf_smoke] {}", esm_path);
    eprintln!(
        "  variant       : {:?}",
        variant
    );
    eprintln!(
        "  hedr_version  : {:.4}",
        file_header.hedr_version
    );
    eprintln!("  game_kind     : {:?}", game_kind);
    eprintln!("  localized     : {}", file_header.localized);
    eprintln!(
        "  master_files  : {} ({:?})",
        file_header.master_files.len(),
        &file_header.master_files
    );
    eprintln!("  total_records : {} (declared in HEDR)", file_header.record_count);
    eprintln!("  file_size     : {} bytes", file_len);
    eprintln!("  tes4_bytes    : {} bytes", tes4_bytes);

    // Walk the top-level GRUP tree. We DON'T recurse into nested
    // CELL/WRLD block sub-GRUPs — Phase 0's goal is the top-level
    // dispatch map, not the leaf-record count. That's Phase 1.
    let mut report = WalkReport {
        tes4_bytes,
        ..Default::default()
    };

    while reader.remaining() > 0 {
        if !reader.is_group() {
            // Stray top-level record (shouldn't happen on vanilla ESMs
            // post-TES4, but counted for completeness).
            match reader.read_record_header() {
                Ok(header) => {
                    report.orphan_record_count += 1;
                    reader.skip_record(&header);
                }
                Err(e) => {
                    report.errors.push((
                        reader.position(),
                        *b"____",
                        format!("orphan record header: {e}"),
                    ));
                    // Can't recover — break.
                    break;
                }
            }
            continue;
        }
        let grup = match reader.read_group_header() {
            Ok(g) => g,
            Err(e) => {
                report.errors.push((
                    reader.position(),
                    *b"GRUP",
                    format!("group header: {e}"),
                ));
                break;
            }
        };
        let stats = report.by_fourcc.entry(grup.label).or_default();
        stats.grup_count += 1;
        stats.bytes_total += grup.total_size as u64;
        report.grup_bytes_total += grup.total_size as u64;

        // Count immediate children. We don't recurse — that's Phase 1.
        let content_end = reader.group_content_end(&grup);
        if let Err(e) = count_immediate_children(&mut reader, content_end, stats, &grup) {
            stats.saw_byte_error = true;
            report.errors.push((
                reader.position(),
                grup.label,
                format!("counting children: {e}"),
            ));
        }
        // Whatever happened, jump past the group so the outer loop
        // stays coherent. group_content_end is the absolute byte
        // offset right after this group's last byte.
        if reader.position() < content_end {
            let skip_to = content_end.saturating_sub(reader.position());
            reader.skip(skip_to);
        }
    }

    // ── Console report ──────────────────────────────────────────────
    eprintln!();
    eprintln!("  grup_bytes    : {} bytes ({:.1}% of file)",
        report.grup_bytes_total,
        100.0 * report.grup_bytes_total as f64 / file_len as f64);
    eprintln!("  orphans       : {} stray top-level records",
        report.orphan_record_count);
    eprintln!("  errors        : {} byte-level walk errors", report.errors.len());
    if !report.errors.is_empty() {
        for (off, fourcc, msg) in report.errors.iter().take(10) {
            eprintln!(
                "    @ 0x{:08x} [{}] {}",
                off,
                String::from_utf8_lossy(fourcc),
                msg,
            );
        }
        if report.errors.len() > 10 {
            eprintln!("    ... +{} more", report.errors.len() - 10);
        }
    }

    eprintln!();
    eprintln!("  Per-FourCC top-level GRUPs (sorted by byte size):");
    eprintln!("    {:>4} {:>8} {:>12} {:>12} {:>9} {}",
        "type", "grups", "bytes", "imm-rec", "handled?", "fourcc");
    let mut by_size: Vec<(&[u8; 4], &GrupStats)> = report.by_fourcc.iter().collect();
    by_size.sort_by(|a, b| b.1.bytes_total.cmp(&a.1.bytes_total));
    let mut handled_bytes: u64 = 0;
    let mut unhandled_bytes: u64 = 0;
    let mut handled_grups: u64 = 0;
    let mut unhandled_grups: u64 = 0;
    for (fourcc, stats) in &by_size {
        let handled = is_dispatch_handled(fourcc);
        if handled {
            handled_bytes += stats.bytes_total;
            handled_grups += stats.grup_count;
        } else {
            unhandled_bytes += stats.bytes_total;
            unhandled_grups += stats.grup_count;
        }
        eprintln!("    {:>4} {:>8} {:>12} {:>12} {:>9} {}",
            String::from_utf8_lossy(fourcc.as_slice()),
            stats.grup_count,
            stats.bytes_total,
            stats.immediate_records,
            if handled { "YES" } else { "skip" },
            if stats.saw_byte_error { "(byte-error)" } else { "" });
    }

    eprintln!();
    eprintln!("  ── Summary ────────────────────────────────");
    eprintln!("  Distinct top-level GRUP FourCCs: {}",
        report.by_fourcc.len());
    eprintln!("  Handled by dispatch  : {} grups / {} bytes ({:.1}% of GRUP bytes)",
        handled_grups,
        handled_bytes,
        100.0 * handled_bytes as f64 / report.grup_bytes_total.max(1) as f64);
    eprintln!("  Silently skipped     : {} grups / {} bytes ({:.1}% of GRUP bytes)",
        unhandled_grups,
        unhandled_bytes,
        100.0 * unhandled_bytes as f64 / report.grup_bytes_total.max(1) as f64);

    // ── Optional TSV output for baseline check-in ───────────────────
    if emit_tsv {
        // Stable header for the baseline file.
        println!(
            "# sf_smoke baseline — {} (variant={:?} game={:?} hedr={:.4})",
            esm_path, variant, game_kind, file_header.hedr_version
        );
        println!("# columns: fourcc\tgrup_count\tbytes_total\timmediate_records\thandled\tbyte_error");
        for (fourcc, stats) in &by_size {
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}",
                String::from_utf8_lossy(fourcc.as_slice()),
                stats.grup_count,
                stats.bytes_total,
                stats.immediate_records,
                if is_dispatch_handled(fourcc) { 1 } else { 0 },
                if stats.saw_byte_error { 1 } else { 0 },
            );
        }
    }

    Ok(())
}

fn count_immediate_children(
    reader: &mut EsmReader<'_>,
    content_end: usize,
    stats: &mut GrupStats,
    parent: &GroupHeader,
) -> anyhow::Result<()> {
    while reader.position() < content_end {
        if !reader.is_group() {
            let header: RecordHeader = reader.read_record_header()?;
            stats.immediate_records += 1;
            reader.skip_record(&header);
        } else {
            let sub = reader.read_group_header()?;
            // Don't recurse — Phase 0 is top-level counting only.
            // Skip the entire sub-GRUP.
            let _ = parent; // silence unused if we never need it
            reader.skip_group(&sub);
        }
    }
    Ok(())
}
