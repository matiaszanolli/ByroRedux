# Issue #4939

**Title:** COORD-04: crates/bsa/src/uvd.rs declares a second EXTERIOR_CELL_UNITS, not pinned to the coord SoT
**State:** OPEN
**Labels:** bug, import-pipeline, low, legacy-compat, tech-debt

- **ID**: COORD-04
- **Labels**: low,bug,tech-debt,legacy-compat,import-pipeline
- **Filed from**: docs/audits/AUDIT_LEGACY_COMPAT_2026-09-27.md

**Severity**: LOW · **Dimension**: 1 — Coordinate + placement fidelity (exterior grid single source)
**Status**: NEW (landed `00b75ed00`, 2026-09-15; not flagged by the 2026-09-22 run)

**Location**: `crates/bsa/src/uvd.rs` — `EXTERIOR_CELL_UNITS` constant, used by `UvdHeader::exterior_cell_grid`

## Description
`EXTERIOR_CELL_UNITS` in `crates/core/src/math/coord.rs` is the sole source for cell-grid math (#1112 collapsed six divergent literals, one of which had a sign bug). FO4 previs grid recovery, `UvdHeader::exterior_cell_grid`, declares its own `pub const EXTERIOR_CELL_UNITS: f32 = 4096.0` with the same name and does grid math with it (`(min / EXTERIOR_CELL_UNITS).floor() as i32 + PREVIS_BLOCK_CELLS / 2`).

The `byroredux-bsa` crate has no `byroredux-core` dependency, so it can't import the SoT. That is a reasonable isolation choice, but nothing pins the two constants equal. The module is private (`mod uvd;` in `crates/bsa/src/lib.rs`), so the constant isn't re-exported, and a pin can't be written from outside without a re-export.

## Evidence
```rust
// crates/bsa/src/uvd.rs
/// Side of one Fallout 4 exterior cell, in game units. An exterior `.uvd`'s
/// bounds are a whole number of these on X and Y.
pub const EXTERIOR_CELL_UNITS: f32 = 4096.0;
```

## Impact
None today. There's no production consumer: the only caller of `exterior_cell_grid` is `crates/bsa/examples/probe_uvd_corpus.rs`, and `cell_loader/precombined.rs` only cites it in docs. The value is a format fact that won't change. The risk is structural: a second same-named source for the exact quantity #1112 consolidated. It becomes load-bearing once `precombined.rs` wires previs-grid recovery into placement.

## Related
#1112. Incidental: `crates/core/src/ecs/components/camera.rs`'s test `far_plane_clears_the_widest_lod_ring_corner` declares a local `const CELL_UNITS: f32 = 4096.0` inside the crate that owns the SoT. It is test-only, but its doc says it exists "so the number cannot drift", and the local literal defeats that. Fold it into the same fix.

## Suggested Fix
Re-export the constant (`pub use uvd::EXTERIOR_CELL_UNITS as UVD_CELL_UNITS`) and add `const _: () = assert!(byroredux_bsa::UVD_CELL_UNITS == byroredux_core::math::coord::EXTERIOR_CELL_UNITS);` in `byroredux`, which depends on both crates. Alternatively, rename it so a grep for the SoT name has one hit. Switch the camera test to `coord::EXTERIOR_CELL_UNITS`.

## Completeness Checks
- [ ] **SIBLING**: No other crate re-declares a 4096 cell-size constant (`grep -rn '4096\.0' crates --include='*.rs'`)
- [ ] **TESTS**: A compile-time or unit pin asserts the uvd constant equals `coord::EXTERIOR_CELL_UNITS`

