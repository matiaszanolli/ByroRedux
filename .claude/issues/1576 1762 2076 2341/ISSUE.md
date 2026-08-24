# Issue Batch: 1576, 1762, 2076, 2341

## #1576 — SF-D4-03 (esm, byroredux-plugin)
`crates/plugin/src/esm/cell/support.rs:38-160` — `build_static_object_from_subs` only
reads a top-level `MODL` subrecord. Some Starfield STAT/BNDS/ACTI/ARMO records carry their
model reference inside a `BFCB`-wrapped component block instead, so those REFRs drop
(~140 REFRs, ~0.5% of the audited cell). Suggested fix explicitly depends on SF-D4-01's
`BFCB`/`BFCE` component walker landing first — need to check whether that walker already
exists before implementing.

## #1762 — TD8-005 (esm, byroredux-plugin)
`crates/plugin/src/manifest.rs:70-75` — `RawDependency.name` is parsed from TOML
(`#[allow(dead_code)]`) but never read into the public `Manifest` (only `.uuid` is mapped).
Suggested fix: delete the field (default) — serde silently ignores unknown TOML keys with
no `deny_unknown_fields`, so a `name = "…"` key in a `[[dependencies]]` block still parses
fine without the field.

## #2076 — TD8-102 (binary, byroredux)
`byroredux/src/cell_loader/water.rs:77-182` — `spawn_water_plane`'s `blas_specs` parameter
is discarded unconditionally (`let _ = blas_specs;`) since water meshes are excluded from
BLAS/TLAS. The interior call site in `load.rs:416,439` allocates a throwaway
`_blas_dummy: Vec<(u32, u32, u32)>` purely to satisfy the signature. Suggested fix: drop the
parameter, update both call sites (interior + exterior).

## #2341 — NAVM-01 (esm, byroredux-plugin)
`crates/plugin/src/esm/records/tests.rs:364` — a stale comment claims FNV NAVM count is 0
(pre-#1272); a live run of `parse_real_fnv_esm_record_counts` shows 4771 navmeshes, only
visible via `eprintln!`, no assertion pins it. Suggested fix: update the comment and add
`assert!(index.navmeshes.len() >= 4000, ...)` alongside the test's existing floor assertions.

## Domain classification
- #1576, #1762, #2341 → **esm** → `byroredux-plugin`
- #2076 → **binary** → `byroredux`

## Plan
All four are small, independent, single-site-ish fixes (well under the 5-file scope-check
threshold). Investigate #1576's SF-D4-01 dependency first since it may block or reshape the
fix; the other three are self-contained mechanical cleanups.
