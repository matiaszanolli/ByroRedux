# SKY-D4-2026-10-17-01: esm/cell split (d7c47d741) and #5463 left stale source/doc pointers and a misplaced doc comment

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/5965 (snapshot as filed 2026-10-17; GitHub is authoritative for current state)

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-17.md` (report ID SKY-4-01) · **Severity**: LOW (doc rot) · **Dimension**: 4 · **Game**: Skyrim SE

## Description
The `esm/cell` split (`d7c47d741`) and #5463 (`3b28dca71`) left stale pointers and a misplaced doc comment:
1. `crates/plugin/src/esm/cell/helpers.rs:328-346`: `3b28dca71` inserted `xccm_targets_region` between `read_form_id` and its doc block, so the `read_form_id` doc ("Read a 4-byte FormID from a sub-record payload...", #3314) now runs straight into the XCCM doc and attaches to the wrong item; `read_form_id` has no doc.
2. `byroredux/src/cell_loader/terrain_seam.rs:7` cites `crates/plugin/src/esm/cell/mod.rs:165-167` for the LAND row/col orientation; that text now lives at `crates/plugin/src/esm/cell/landscape.rs:38`.
3. `docs/engine/esm-records.md:277` points to `cell/mod.rs` for the authoritative `CellData` struct, which is now `cell/cell_data.rs`.
4. `CLAUDE.md:193` lists the cell submodules as "helpers / support / walkers / wrld"; the split added `base_objects`, `cell_data`, `index`, `landscape`, `lighting`, `placed_ref`, `worldspace`.

## Impact
Documentation only. The `include_str!` source-scan guards were verified live (not vacuous): `mod.rs:77` scans `index.rs`, `mod.rs:156` scans `cell_data.rs`, `tests/txst.rs:616` scans `base_objects.rs`.

## Suggested fix
Move `xccm_targets_region` above the `read_form_id` doc block; retarget the `terrain_seam.rs` citation to `cell/landscape.rs` with a symbol anchor; retarget `esm-records.md:277` to `cell/cell_data.rs`; refresh `CLAUDE.md:193` (edit the real file, `AGENTS.md` is a symlink).

Related: closed #5313, #1054, #1059 (same class of rot).

## Completeness Checks
- [ ] **SIBLING**: Other docs that name `cell/mod.rs` for items the split moved (grep `esm/cell/mod.rs`)
