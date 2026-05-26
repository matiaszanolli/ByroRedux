# F-FO3-D3-02: NAVM records inside CELL persistent-children GRUPs silently skipped

## Severity: Medium

**Location**: `crates/plugin/src/esm/cell/walkers.rs:652-655`

## Problem

`parse_navm` exists at `crates/plugin/src/esm/records/misc/world.rs:47` and is wired into the top-level GRUP dispatcher (`records/mod.rs:433` — closed via #458). But the on-disk shape of NAVM is **not** a top-level GRUP — it lives **inside cell child-records GRUPs**:

```
GRUP CELL → CELL record → GRUP Cell Persistent Children → NAVM records
```

The cell walker's child-record loop has an explicit catch-all skip that names NAVM among the skipped types. As a result, `index.navmeshes.len() == 0` on **every game** (FO3, FNV, Skyrim SE, FO4 alike).

## Evidence

`crates/plugin/src/esm/cell/walkers.rs:652-655`:

```rust
} else {
    // Skip other record types (PGRE, PMIS, NAVM, etc.)
    reader.skip_record(&header);
}
```

Live parse on `Fallout3.esm`: `navmeshes=0`; same on `FalloutNV.esm`. The single `navi=1` is the NAVI master parsed correctly at the top level. `parse_navm` itself is functional and tested at `records/misc/world.rs:390 (parse_navm_extracts_version)` — the gap is at the cell-walker layer that never reaches it.

## Impact

The future AI / pathfinding subsystem has no navmesh data to consume. Cross-game (not FO3-specific), but visible on FO3 because Fallout 3 ships ~30,000 NAVM records across its cells (per UESP CSWiki). For now this only affects code that hasn't been written yet, hence MEDIUM not HIGH. Will become HIGH/CRITICAL the moment any AI path/navigation system tries to consume `index.navmeshes`.

## Fix

In the cell walker's child-record loop (`cell/walkers.rs` before line 652), add an arm that reads NAVM sub-records and routes the resulting `NavmRecord` onto `EsmIndex.navmeshes`. Threading the index map into the cell walker requires either:

- **(a)** extend the cell walker signature to take an `&mut HashMap<u32, NavmRecord>`, or
- **(b)** defer to a post-pass that re-walks just the cell-NAVM tier.

(a) is cleaner and matches how `landscape_textures` is already threaded through this same function.

Top-level NAVM dispatch at `records/mod.rs:433` (closed via #458) is vestigial — no Bethesda master flattens NAVMs out — but harmless. Worth a comment noting it exists for non-vanilla mods that might.

## Completeness Checks

- [ ] **TESTS**: After fix, assert `navmeshes.len() > 0` on FO3 and FNV `parse_real_esm` runs
- [ ] **SIBLING**: Same loop also catches PGRE/PMIS — confirm those remain intentionally skipped or file separate finding
- [ ] **CROSS-GAME**: Verify navmesh count is non-zero on Skyrim SE master too (cross-game gap)
- [ ] **THREADING**: If using signature option (a), confirm the existing `landscape_textures` thread-through pattern is followed cleanly

Related: #458 (closed) — top-level WATR/NAVI/NAVM/REGN/ECZN/LGTM/HDPT/EYES/HAIR dispatch stubs. That fix landed at `records/mod.rs`; this finding is the per-cell layer that #458 explicitly deferred ("NAVM/NAVI/REGN/ECZN: post-render, AI, not blocking compat" — that's no longer true once AI lands).

Audit: `docs/audits/AUDIT_FO3_2026-05-25_DIM3.md` (F-FO3-D3-02)
