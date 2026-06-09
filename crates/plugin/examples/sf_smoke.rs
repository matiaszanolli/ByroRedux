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
/// `records/mod.rs:925`). Refresh via:
///
/// ```bash
/// grep -oE 'b"[A-Z_][A-Z_0-9]{3}"' crates/plugin/src/esm/records/mod.rs \
///   | sort -u
/// ```
///
/// FO4-plus-gated arms (SCOL, PKIN, MOVS, MSWP) are included — the
/// dispatch handles them when game_kind is FO4/FO76/Starfield, and
/// warn-skips them otherwise. From this tool's perspective, they
/// count as "handled by the dispatch" even though they may be
/// gate-skipped. Many other arms (CLOT, CREA, HAIR, IMOD, etc.) are
/// for pre-FO4 games — they'll never appear in SF ESMs but the
/// dispatch still recognizes them. Snapshot 2026-05-28 (post-Phase 0).
const DISPATCH_HANDLED_FOURCCS: &[&[u8; 4]] = &[
    b"ACTI", b"ADDN", b"ALCH", b"ALOC", b"AMEF", b"AMMO", b"ANIO", b"APPA", b"ARMA", b"ARMO",
    b"ASPC", b"AVIF", b"BNDS", b"BOOK", b"BPTD", b"BSGN", b"CAMS", b"CCRD", b"CDCK", b"CELL",
    b"CHAL", b"CHIP", b"CLAS", b"CLMT", b"CLOT", b"CMNY", b"COBJ", b"CONT", b"CPTH", b"CREA",
    b"CSNO", b"CSTY", b"DEBR", b"DEHY", b"DIAL", b"DOBJ", b"DOOR", b"ECZN", b"EFSH", b"ENCH",
    b"EXPL", b"EYES", b"FACT", b"FLOR", b"FLST", b"FURN", b"GLOB", b"GMST", b"GRAS", b"HAIR",
    b"HDPT", b"HUNG", b"IDLE", b"IDLM", b"IMAD", b"IMGS", b"IMOD", b"INGR", b"IPCT", b"IPDS",
    b"KEYM", b"LGTM", b"LIGH", b"LSCR", b"LSCT", b"LTEX", b"LVLC", b"LVLI", b"LVLN", b"MESG",
    b"MGEF", b"MICN", b"MISC", b"MOVS", b"MSET", b"MSTT", b"MSWP", b"MUSC", b"NAVI", b"NAVM",
    b"NOTE", b"NPC_", b"OTFT", b"PACK", b"PERK", b"PKIN", b"PROJ", b"PWAT", b"QUST", b"RACE",
    b"RADS", b"RCCT", b"RCPE", b"REGN", b"REPU", b"RGDL", b"SCOL", b"SCPT", b"SGST", b"SLGM",
    b"SLPD", b"SOUN", b"SPEL", b"STAT", b"TACT", b"TERM", b"TREE", b"TXST", b"VTYP", b"WATR",
    b"WEAP", b"WRLD", b"WTHR",
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

/// Recursive leaf-record counts inside the nested CELL / WRLD GRUP
/// hierarchy. Populated only when `--recurse` is passed. The CELL
/// dispatch path produces nested block-and-sub-block GRUPs whose
/// leaves are the actual CELL records + their REFR / PGRE / PMIS /
/// PHZD / NAVM children. WRLD adds the worldspace + exterior cell
/// blocks on top. Without this recursion, top-level WRLD reports
/// "433 immediate records" but the file holds thousands of CELL +
/// hundreds of thousands of REFR descendants — the dominant
/// per-record-type signal lives in the recursive count.
#[derive(Default, Debug)]
struct RecursiveLeafCounts {
    /// Per-FourCC leaf record count (e.g. CELL=8542, REFR=480123, …).
    by_fourcc: BTreeMap<[u8; 4], u64>,
    /// Total bytes consumed by leaf records (excluding the GRUP
    /// header bytes — those are already counted in top-level stats).
    bytes_total: u64,
    /// Any byte-level error during recursive walk.
    errors: Vec<(usize, [u8; 4], String)>,
}

#[derive(Default, Debug)]
struct WalkReport {
    /// Bytes consumed by the TES4 header (subtracted from file size
    /// to derive "GRUP payload bytes" below).
    #[allow(dead_code)]
    // exposed via the console report only — kept on the struct for diffing.
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
    /// Recursive leaf-record counts inside CELL / WRLD nested
    /// GRUPs. `None` when `--recurse` wasn't passed.
    recursive: Option<RecursiveLeafCounts>,
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let emit_tsv = args.iter().any(|a| a == "--tsv");
    let recurse = args.iter().any(|a| a == "--recurse");
    args.retain(|a| a != "--tsv" && a != "--recurse");
    let esm_path = args
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("usage: sf_smoke ESM_PATH [--tsv] [--recurse]"))?;

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
    eprintln!("  variant       : {:?}", variant);
    eprintln!("  hedr_version  : {:.4}", file_header.hedr_version);
    eprintln!("  game_kind     : {:?}", game_kind);
    eprintln!("  localized     : {}", file_header.localized);
    eprintln!(
        "  master_files  : {} ({:?})",
        file_header.master_files.len(),
        &file_header.master_files
    );
    eprintln!(
        "  total_records : {} (declared in HEDR)",
        file_header.record_count
    );
    eprintln!("  file_size     : {} bytes", file_len);
    eprintln!("  tes4_bytes    : {} bytes", tes4_bytes);

    // Walk the top-level GRUP tree. With `--recurse`, also dive into
    // nested CELL/WRLD block sub-GRUPs to count leaf records by
    // FourCC — answers the Phase 1 question "does the existing
    // CELL/WRLD handler decode SF records or silently drop them?"
    let mut report = WalkReport {
        tes4_bytes,
        recursive: if recurse {
            Some(RecursiveLeafCounts::default())
        } else {
            None
        },
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
                report
                    .errors
                    .push((reader.position(), *b"GRUP", format!("group header: {e}")));
                break;
            }
        };
        let stats = report.by_fourcc.entry(grup.label).or_default();
        stats.grup_count += 1;
        stats.bytes_total += grup.total_size as u64;
        report.grup_bytes_total += grup.total_size as u64;

        // Count immediate children. With `--recurse`, additionally
        // dive into nested sub-GRUPs (CELL block/sub-block hierarchy,
        // WRLD worldspace tree) and tally leaf records by FourCC.
        let content_end = reader.group_content_end(&grup);
        let walk_result = if let Some(ref mut leaves) = report.recursive {
            recursive_walk(&mut reader, content_end, stats, leaves)
        } else {
            count_immediate_children(&mut reader, content_end, stats, &grup)
        };
        if let Err(e) = walk_result {
            stats.saw_byte_error = true;
            report.errors.push((
                reader.position(),
                grup.label,
                format!("walking children: {e}"),
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
    eprintln!(
        "  grup_bytes    : {} bytes ({:.1}% of file)",
        report.grup_bytes_total,
        100.0 * report.grup_bytes_total as f64 / file_len as f64
    );
    eprintln!(
        "  orphans       : {} stray top-level records",
        report.orphan_record_count
    );
    eprintln!(
        "  errors        : {} byte-level walk errors",
        report.errors.len()
    );
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
    eprintln!(
        "    {:>4} {:>8} {:>12} {:>12} {:>9} {}",
        "type", "grups", "bytes", "imm-rec", "handled?", "fourcc"
    );
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
        eprintln!(
            "    {:>4} {:>8} {:>12} {:>12} {:>9} {}",
            String::from_utf8_lossy(fourcc.as_slice()),
            stats.grup_count,
            stats.bytes_total,
            stats.immediate_records,
            if handled { "YES" } else { "skip" },
            if stats.saw_byte_error {
                "(byte-error)"
            } else {
                ""
            }
        );
    }

    eprintln!();
    eprintln!("  ── Summary ────────────────────────────────");
    eprintln!(
        "  Distinct top-level GRUP FourCCs: {}",
        report.by_fourcc.len()
    );
    eprintln!(
        "  Handled by dispatch  : {} grups / {} bytes ({:.1}% of GRUP bytes)",
        handled_grups,
        handled_bytes,
        100.0 * handled_bytes as f64 / report.grup_bytes_total.max(1) as f64
    );
    eprintln!(
        "  Silently skipped     : {} grups / {} bytes ({:.1}% of GRUP bytes)",
        unhandled_grups,
        unhandled_bytes,
        100.0 * unhandled_bytes as f64 / report.grup_bytes_total.max(1) as f64
    );

    // Recursive leaf-record report (--recurse only).
    if let Some(ref leaves) = report.recursive {
        eprintln!();
        eprintln!("  ── Recursive leaf records (inside CELL/WRLD/etc. sub-GRUPs) ──");
        eprintln!(
            "  Total leaf bytes  : {} ({:.1}% of GRUP bytes)",
            leaves.bytes_total,
            100.0 * leaves.bytes_total as f64 / report.grup_bytes_total.max(1) as f64
        );
        eprintln!("  Distinct leaf FourCCs: {}", leaves.by_fourcc.len());
        eprintln!("  Recursive walk errors: {}", leaves.errors.len());
        if !leaves.errors.is_empty() {
            for (off, fourcc, msg) in leaves.errors.iter().take(5) {
                eprintln!(
                    "    @ 0x{:08x} [{}] {}",
                    off,
                    String::from_utf8_lossy(fourcc),
                    msg,
                );
            }
            if leaves.errors.len() > 5 {
                eprintln!("    ... +{} more", leaves.errors.len() - 5);
            }
        }
        eprintln!();
        eprintln!("  Top 20 leaf FourCCs by record count:");
        let mut leaf_by_count: Vec<(&[u8; 4], &u64)> = leaves.by_fourcc.iter().collect();
        leaf_by_count.sort_by(|a, b| b.1.cmp(a.1));
        for (fourcc, count) in leaf_by_count.iter().take(20) {
            eprintln!(
                "    {:>4} {:>12}",
                String::from_utf8_lossy(fourcc.as_slice()),
                count
            );
        }
    }

    // ── Optional TSV output for baseline check-in ───────────────────
    if emit_tsv {
        // Stable header for the baseline file.
        println!(
            "# sf_smoke baseline — {} (variant={:?} game={:?} hedr={:.4})",
            esm_path, variant, game_kind, file_header.hedr_version
        );
        println!("# columns: scope\tfourcc\tgrup_count\tbytes_total\timmediate_records\thandled\tbyte_error");
        for (fourcc, stats) in &by_size {
            println!(
                "top\t{}\t{}\t{}\t{}\t{}\t{}",
                String::from_utf8_lossy(fourcc.as_slice()),
                stats.grup_count,
                stats.bytes_total,
                stats.immediate_records,
                if is_dispatch_handled(fourcc) { 1 } else { 0 },
                if stats.saw_byte_error { 1 } else { 0 },
            );
        }
        // Recursive leaf rows: scope=leaf, grup_count column carries
        // the leaf-record count, bytes column is 0 (per-record bytes
        // aren't tallied because the recursive walk skips by header
        // size). `handled` column reflects whether the dispatch
        // recognizes the leaf FourCC as a top-level record — useful
        // for catching things like REFR that LIVE inside CELL block
        // sub-GRUPs but aren't independently dispatched.
        if let Some(ref leaves) = report.recursive {
            for (fourcc, count) in &leaves.by_fourcc {
                println!(
                    "leaf\t{}\t{}\t0\t0\t{}\t0",
                    String::from_utf8_lossy(fourcc.as_slice()),
                    count,
                    if is_dispatch_handled(fourcc) { 1 } else { 0 },
                );
            }
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

/// Recursive walk that descends into every nested sub-GRUP under a
/// top-level GRUP. Counts:
///   * immediate records on the top-level GRUP into `stats.immediate_records`
///     (same as `count_immediate_children` — kept consistent so the
///     top-level table doesn't change between `--recurse` and not).
///   * every leaf record at any nesting depth into `leaves.by_fourcc`.
///
/// CELL / WRLD GRUPs are the dominant use case: their content lives
/// in nested cell-block → sub-block → CELL-record → REFR-children
/// hierarchies. Without recursion we see "CELL: 0 immediate records"
/// even though the file holds 8 000+ CELL records inside.
///
/// Doesn't decode any sub-records — just skips by header. Cheap.
fn recursive_walk(
    reader: &mut EsmReader<'_>,
    content_end: usize,
    stats: &mut GrupStats,
    leaves: &mut RecursiveLeafCounts,
) -> anyhow::Result<()> {
    let mut stack: Vec<usize> = vec![content_end];
    let mut top_level = true;
    while let Some(&top) = stack.last() {
        if reader.position() >= top {
            stack.pop();
            top_level = false;
            continue;
        }
        if !reader.is_group() {
            let header: RecordHeader = match reader.read_record_header() {
                Ok(h) => h,
                Err(e) => {
                    leaves
                        .errors
                        .push((reader.position(), *b"____", format!("leaf header: {e}")));
                    // Can't recover at this nesting level — skip to
                    // the enclosing GRUP boundary.
                    let skip = top.saturating_sub(reader.position());
                    reader.skip(skip);
                    continue;
                }
            };
            if top_level {
                stats.immediate_records += 1;
            }
            *leaves.by_fourcc.entry(header.record_type).or_insert(0) += 1;
            // 24-byte header + payload.
            leaves.bytes_total += 24 + header.data_size as u64;
            reader.skip_record(&header);
        } else {
            let sub = match reader.read_group_header() {
                Ok(g) => g,
                Err(e) => {
                    leaves.errors.push((
                        reader.position(),
                        *b"GRUP",
                        format!("sub-grup header: {e}"),
                    ));
                    let skip = top.saturating_sub(reader.position());
                    reader.skip(skip);
                    continue;
                }
            };
            let sub_end = reader.group_content_end(&sub);
            stack.push(sub_end);
            top_level = false;
        }
    }
    Ok(())
}
