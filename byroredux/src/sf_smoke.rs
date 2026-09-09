//! Starfield ESM smoke-test entry point (`--sf-smoke <CELL_EDID>`).
//!
//! Walks an ESM under the existing `GameKind::Starfield` dispatch path
//! (which today routes through the FO4 record parser) and reports two
//! things for a single named interior cell:
//!
//! 1. **Base-form resolve rate** — of N REFRs in the cell, how many
//!    point at a base form the parser actually decoded into
//!    `EsmCellIndex.statics`? This is the gate question for ROADMAP
//!    Milestone B (Starfield interior cell renders): a high rate means
//!    "FO4 dispatch handles the bulk; write the gaps", a low rate
//!    means "the schema diverged; write a real Starfield parser".
//! 2. **Per-record-type breakdown** — of the resolved REFRs, what 4-CC
//!    base-form types are represented (STAT, MSTT, FURN, LIGH, …)?
//!    The shape of this distribution tells us which Starfield-new
//!    record types (PNDT / STDT / BIOM / SFBK / SUNP / GBFM / GBFT)
//!    show up in the wild and need dedicated parsers.
//!
//! See #763 / SF-D6-04. The smoke is a planning-phase deliverable; it
//! prints to stdout, makes no engine state, and is gated on `--sf-smoke`
//! at the top of `main()`.

use anyhow::{Context, Result};
use byroredux_plugin::esm::cell::CellData;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::parse_esm;
use std::collections::HashMap;
use std::path::Path;

// `parse_esm` returns an `EsmIndex` that already carries the detected
// `game: GameKind` (set from the TES4 HEDR and record-header versions via
// `GameKind::from_header`), so the smoke doesn't need to re-detect.

/// Smoke-test a single cell in an ESM under the current `GameKind`
/// dispatch. Reads `esm_path` from disk, parses it via the shared
/// `parse_esm` entry point, looks up `cell_edid` in the interior cell
/// index, and prints the resolve report.
///
/// Caller wires this from `main()` when `--sf-smoke <CELL>` is set;
/// `--esm <PATH>` provides the ESM path. The function never returns
/// `Ok(())` with a usable engine state — it's terminal: print, exit.
/// Which non-`statics` index map holds `form_id`, if any (#2637).
///
/// `sf_smoke` resolves REFR base forms against `statics`, which is correct for
/// anything that renders. Several record types are parsed and indexed but
/// deliberately live elsewhere because they carry no mesh — the Starfield-era
/// audio family in particular. A REFR pointing at one of them is fully
/// understood, not a parser gap, and reporting it as a gap is what overstated
/// the unresolved figure ~5x.
///
/// Returns the FourCC of the owning map so the report can name the bucket.
/// Ordered most-populous-first on the measured Cydonia cell; the maps are
/// disjoint by FormID so order affects only lookup cost, not the answer.
fn nonstatic_base_type(
    index: &byroredux_plugin::esm::records::EsmIndex,
    form_id: u32,
) -> Option<&'static str> {
    // #2636 indexed SECH/AOPF; ALOC/ASPC predate it. All four are
    // `MinimalEsmRecord` maps keyed by FormID, so the map that matches IS the
    // record type — no type tag needs storing alongside.
    if index.sound_echoes.contains_key(&form_id) {
        return Some("SECH");
    }
    if index.audio_occlusion_primitives.contains_key(&form_id) {
        return Some("AOPF");
    }
    if index.acoustic_spaces.contains_key(&form_id) {
        return Some("ASPC");
    }
    if index.audio_locations.contains_key(&form_id) {
        return Some("ALOC");
    }
    None
}

pub fn run(esm_path: &Path, cell_edid: &str) -> Result<()> {
    let bytes = std::fs::read(esm_path)
        .with_context(|| format!("failed to read ESM at {}", esm_path.display()))?;
    let size_mb = bytes.len() / (1024 * 1024);

    let index = parse_esm(&bytes).context("parse_esm failed")?;

    println!("─── ESM smoke ─────────────────────────────────────────────");
    println!("file       : {}", esm_path.display());
    println!("size       : {} MB", size_mb);
    println!("game kind  : {:?}", index.game);
    if index.game != GameKind::Starfield {
        println!(
            "note       : ESM is not Starfield. The smoke is most useful for SF \
             but works on any HEDR-detected game; resolve-rate below reflects \
             the active dispatch path, whatever that is."
        );
    }

    let target_key = cell_edid.to_ascii_lowercase();
    let Some(cell) = index.cells.cells.get(&target_key) else {
        println!(
            "cell EDID  : {} (NOT FOUND in interior cell index)",
            cell_edid
        );
        println!("─── available interiors (first 20) ──────────────────────────");
        let mut keys: Vec<&String> = index.cells.cells.keys().collect();
        keys.sort();
        for k in keys.iter().take(20) {
            println!("  {}", k);
        }
        if index.cells.cells.len() > 20 {
            println!("  ... ({} more)", index.cells.cells.len() - 20);
        }
        return Err(anyhow::anyhow!(
            "cell EDID '{}' not in {} interior cells",
            cell_edid,
            index.cells.cells.len()
        ));
    };

    print_cell_report(cell, &index.cells, &index);
    Ok(())
}

fn print_cell_report(
    cell: &CellData,
    cells_index: &byroredux_plugin::esm::cell::EsmCellIndex,
    // #2637 — the full index, for base forms that are known but deliberately
    // outside `statics`. `EsmCellIndex` is the render-facing view and does not
    // carry the audio-family maps, so attributing those needs the parent.
    index: &byroredux_plugin::esm::records::EsmIndex,
) {
    let total = cell.references.len();
    println!(
        "─── cell {} ───────────────────────────────────────────────",
        cell.editor_id
    );
    println!(
        "display    : {:?}",
        cell.display_name.as_deref().unwrap_or("(no FULL)")
    );
    println!("interior   : {}", cell.is_interior);
    println!("references : {} REFRs", total);
    if !cell.precombined_mesh_hashes.is_empty() {
        let hashes = cell
            .precombined_mesh_hashes
            .iter()
            .map(|hash| format!("{hash:08x}"))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "precombine : {} hash(es): {}",
            cell.precombined_mesh_hashes.len(),
            hashes
        );
    }

    if total == 0 {
        println!("(cell has no REFRs — nothing to measure)");
        return;
    }

    // Tally resolved (base_form_id maps to a known StaticObject) vs not,
    // grouped by base record type when resolved. `statics` is the
    // primary base-form table; many record types route here (STAT,
    // MSTT, FURN, DOOR, LIGH, NPC_, ACTI, ALCH, AMMO, …). Unresolved
    // hits mean the base form was either (a) a record type the parser
    // doesn't dispatch yet, or (b) a Starfield-new record type with no
    // parser (PNDT / STDT / BIOM / SFBK / SUNP / GBFM / GBFT), or (c) a
    // FormID from a master plugin not loaded in this single-ESM smoke.
    let mut resolved_by_type: HashMap<String, usize> = HashMap::new();
    let mut unresolved_high_byte: HashMap<u8, usize> = HashMap::new();
    let mut unresolved_sample: Vec<u32> = Vec::new();
    // #2637 — REFRs whose base form IS indexed, just not in `statics`.
    //
    // These were being reported as "parser gap — schema diverged or record
    // type missing", which is false for every one of them: the record parsed,
    // it is in the index, and it has no mesh by design. Counting them as gaps
    // overstated the headline unresolved figure ~5x on
    // `citycydoniamainlevel`, and — the part that actually matters — buried a
    // real regression signal inside a bucket already dominated by two
    // understood causes. #2636 indexed `SECH`/`AOPF` for exactly this reason;
    // this is the consumer side it was missing.
    let mut nonstatic_by_type: HashMap<&'static str, usize> = HashMap::new();
    let mut absorbed_by_type: HashMap<String, usize> = HashMap::new();
    let mut absorbed_total = 0usize;
    let mut resolved = 0usize;
    for r in &cell.references {
        if let Some(obj) = cells_index.statics.get(&r.base_form_id) {
            *resolved_by_type
                .entry(obj.record_type.as_str().to_string())
                .or_default() += 1;
            if cell.absorbed_refs.contains(&r.form_id) {
                *absorbed_by_type
                    .entry(obj.record_type.as_str().to_string())
                    .or_default() += 1;
                absorbed_total += 1;
            }
            resolved += 1;
        } else if let Some(label) = nonstatic_base_type(index, r.base_form_id) {
            // Known base form, deliberately not a `statics` member (#2637).
            *nonstatic_by_type.entry(label).or_default() += 1;
        } else {
            // The high byte of a FormID is the master file slot (load
            // order index); for a single-ESM smoke any slot != 0 means
            // "form lives in an unloaded master" rather than "schema gap".
            let slot = (r.base_form_id >> 24) as u8;
            *unresolved_high_byte.entry(slot).or_default() += 1;
            if unresolved_sample.len() < 20 {
                unresolved_sample.push(r.base_form_id);
            }
        }
    }

    let pct = 100.0 * resolved as f32 / total as f32;
    println!("─── resolve rate ──────────────────────────────────────────");
    println!("resolved   : {} / {} ({:.1}%)", resolved, total, pct);
    println!(
        "unresolved : {} / {} ({:.1}%)",
        total - resolved,
        total,
        100.0 - pct
    );

    if !resolved_by_type.is_empty() {
        println!("─── resolved by base record type ──────────────────────────");
        let mut by_type: Vec<(String, usize)> = resolved_by_type.into_iter().collect();
        by_type.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (ty, count) in &by_type {
            let pct_t = 100.0 * *count as f32 / total as f32;
            println!("  {:>4}  {:>5}  ({:>5.1}%)", ty, count, pct_t);
        }
    }

    if absorbed_total > 0 {
        println!("─── precombined absorption by base record type ─────────────");
        println!("absorbed   : {} REFRs", absorbed_total);
        let mut by_type: Vec<(String, usize)> = absorbed_by_type.into_iter().collect();
        by_type.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (ty, count) in by_type {
            println!("  {:>4}  {:>5}", ty, count);
        }
    }

    // #2637 — print the by-design buckets BEFORE the unresolved section, so a
    // reader meets the understood causes before the residual rather than
    // after it. Previously all three landed in one undifferentiated pile
    // labelled "parser gap".
    if !nonstatic_by_type.is_empty() {
        let attributed: usize = nonstatic_by_type.values().sum();
        println!("─── known base forms outside `statics` (by design) ─────────");
        println!(
            "attributed : {} REFRs — parsed and indexed, no mesh to render",
            attributed
        );
        let mut by_type: Vec<(&'static str, usize)> = nonstatic_by_type.into_iter().collect();
        by_type.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        for (ty, count) in by_type {
            println!("  {:>4}  {:>5}", ty, count);
        }
    }

    if !index.skipped_unconsumed_groups.is_empty() {
        println!("─── GRUPs skipped by design (no consumer yet) ──────────────");
        for label in &index.skipped_unconsumed_groups {
            println!(
                "  {}  — every base form in this GRUP is unresolvable by \
                 construction, not by parser failure",
                String::from_utf8_lossy(label)
            );
        }
    }

    if !unresolved_high_byte.is_empty() {
        println!("─── unattributed by FormID master slot ─────────────────────");
        let mut by_slot: Vec<(u8, usize)> = unresolved_high_byte.into_iter().collect();
        by_slot.sort_by_key(|&(slot, _)| slot);
        for (slot, count) in by_slot {
            let hint = match slot {
                // #2637 — no longer asserts a parser gap. Everything this
                // tool can attribute has been subtracted above, but the
                // residual still mixes real gaps with skipped-GRUP members,
                // and this line cannot tell them apart. Claiming otherwise is
                // what let a real regression hide behind two understood
                // causes — the exact failure this report exists to catch.
                0x00 => {
                    "this ESM — unattributed: real parser gap, or a member \
                         of a skipped GRUP listed above"
                }
                0xFD => "Medium Master (ESH) slot — load order index 0xFD",
                0xFE => "Light Master (ESL) slot",
                0xFF => "runtime / dynamic FormID",
                _ => "load-order slot (master not loaded in single-ESM smoke)",
            };
            println!("  slot 0x{:02X}  {:>5}  — {}", slot, count, hint);
        }
        println!(
            "─── unresolved FormID sample (first {}) ────────────────────",
            unresolved_sample.len()
        );
        for id in &unresolved_sample {
            println!("  {:08X}", id);
        }
    }

    println!("─── verdict ────────────────────────────────────────────────");
    if pct >= 80.0 {
        println!("high resolve rate — FO4 dispatch covers the bulk. Milestone B sized as");
        println!("\"fill the gaps\" rather than \"full rewrite\". Inspect the by-type table");
        println!("above for which records show up most; that's where SF-specific parsers");
        println!("(PNDT / STDT / BIOM / SFBK / SUNP / GBFM / GBFT and evolved STAT/CELL/");
        println!("REFR/LIGH/DOOR/MSTT/LGTM) deliver the most coverage.");
    } else if pct >= 50.0 {
        println!("partial resolve — FO4 dispatch picks up half. Milestone B requires a");
        println!("dedicated SF parser pass; the by-slot 0x00 unresolved count is the");
        println!("number of records inside Starfield.esm itself that the FO4 path");
        println!("doesn't decode.");
    } else {
        println!("low resolve rate — Starfield's schema has diverged enough from FO4");
        println!("that the dispatch route mostly drops base forms. Milestone B will");
        println!("need a `crates/plugin/src/legacy/starfield.rs` from-scratch parser,");
        println!("not a delta on FO4.");
    }
}

#[cfg(test)]
mod unresolved_attribution_tests {
    use super::nonstatic_base_type;
    use byroredux_plugin::esm::records::EsmIndex;

    fn minimal(form_id: u32) -> byroredux_plugin::esm::records::MinimalEsmRecord {
        byroredux_plugin::esm::records::MinimalEsmRecord {
            form_id,
            ..Default::default()
        }
    }

    /// #2637 (SF-D4-06) — a REFR whose base form is indexed outside `statics`
    /// must be attributed to its own record type, not counted as a parser gap.
    ///
    /// The measured decomposition on `citycydoniamainlevel`: of 2,461
    /// "unresolved" REFRs, ~140 (0.5%) were a real #1576 gap, 1,846 (6.6%)
    /// intentionally-unconsumed PDCL, and ~369 (1.3%) audio markers that parse
    /// fine and simply have no mesh. Reporting all three as "parser gap —
    /// schema diverged or record type missing" overstated the real figure ~5x
    /// and, worse, gave a genuine regression somewhere to hide.
    #[test]
    fn indexed_non_static_base_forms_are_attributed_to_their_record_type() {
        let mut index = EsmIndex::default();
        index.sound_echoes.insert(0x0000_1001, minimal(0x0000_1001));
        index
            .audio_occlusion_primitives
            .insert(0x0000_1002, minimal(0x0000_1002));
        index
            .acoustic_spaces
            .insert(0x0000_1003, minimal(0x0000_1003));
        index
            .audio_locations
            .insert(0x0000_1004, minimal(0x0000_1004));

        for (form_id, expected) in [
            (0x0000_1001u32, "SECH"),
            (0x0000_1002, "AOPF"),
            (0x0000_1003, "ASPC"),
            (0x0000_1004, "ALOC"),
        ] {
            assert_eq!(
                nonstatic_base_type(&index, form_id),
                Some(expected),
                "form {form_id:#010X} is indexed and must be attributed to {expected}"
            );
        }
    }

    /// The residual must stay residual: a form in no map at all is still
    /// unattributed, which is the bucket a real parser gap has to land in for
    /// the report to remain useful.
    #[test]
    fn a_genuinely_unknown_base_form_stays_unattributed() {
        let mut index = EsmIndex::default();
        index.sound_echoes.insert(0x0000_2001, minimal(0x0000_2001));

        assert_eq!(
            nonstatic_base_type(&index, 0x0000_DEAD),
            None,
            "an unindexed form must not be attributed to any by-design bucket \
             — that would hide the real gaps this report exists to surface"
        );
    }

    /// The report must no longer assert a parser gap for the whole slot-0x00
    /// bucket, and must print the by-design sections that explain most of it.
    #[test]
    fn the_report_separates_by_design_buckets_from_the_residual() {
        const SRC: &str = include_str!("sf_smoke.rs");
        let production = &SRC[..SRC
            .find("mod unresolved_attribution_tests")
            .expect("this test module must keep its name")];

        assert!(
            !production.contains("parser gap — schema diverged or record type missing"),
            "the slot-0x00 hint still asserts a parser gap for every \
             unattributed form, which is false for the majority of them (#2637)"
        );
        for section in [
            "known base forms outside `statics` (by design)",
            "GRUPs skipped by design (no consumer yet)",
        ] {
            assert!(
                production.contains(section),
                "the report no longer prints the `{section}` section, so its \
                 contents fall back into the undifferentiated bucket (#2637)"
            );
        }
    }
}
