# Issue #3827: ESM-D5-01: no walker-level regression test for the XCLW no-water sentinel

**Labels**: low,esm-plugin,water,bug,test-gap
**Filed**: 2026-09-04, via /audit-publish from the water-deep audit suite

---

**Severity**: LOW
**Dimension**: CELL / WRLD Walkers
**Record/Sub-record**: `CELL.XCLW`
**Location**: `crates/plugin/src/esm/cell/tests/cell.rs`, `crates/plugin/src/esm/cell/tests/wrld.rs`
**Source report**: `docs/audits/AUDIT_ESM_2026-09-04.md` (water-deep suite, Dim 5)

## Description
`xclw_water_height` (`crates/plugin/src/esm/cell/helpers.rs:53-68`) is unit-tested for all four cases (normal height, `#INT_MIN#`, Skyrim `f32::MAX`, short/non-finite). The walker/integration level (`parse_cell_group` in `tests/cell.rs`) covers "XCLW present with a normal height" and "XCLW absent," but no test in `tests/cell.rs` or `tests/wrld.rs` drives a full walker call with an authored sentinel payload and asserts the resulting `CellData` carries `water_height_is_explicit == true && water_height == None` together — the third tri-state value is exercised only at the helper's own unit-test boundary, not at the record walker that actually assembles `CellData`.

## Evidence
`grep -n -i "sentinel\|INT_MIN\|FLT_MAX" crates/plugin/src/esm/cell/tests/{cell,wrld}.rs` returns nothing. The composing code (`walkers.rs:262-274`, `wrld.rs:411-419`) is today provably correct by inspection — `water_height_is_explicit = true; water_height = xclw_water_height(&sub.data);` — but a future refactor that wraps the arm in a length guard matching the surrounding sub-record arms' style (e.g. `b"XCLW" if sub.data.len() >= 4 => { ... }`) would silently collapse "sentinel" back to "absent" for a too-short XCLW, and nothing at the walker level would catch it.

## Impact
None today. If it regresses: a cell whose author explicitly suppressed water (dry basement, drained reservoir) would grow a water plane back at whatever `WorldspaceRecord::default_water_height` resolves to for its worldspace — a silent, visual-only, per-cell regression.

## Related
#1305 / OBL-D6-NEW-02 (original tri-state fix); #3548 (a separate, already-fixed and well-tested consumer-side edge case — the Skyrim+/FO4 interior "authored `0.0` is the Creation Kit's inert default, not real water" heuristic in `byroredux/src/cell_loader/water.rs::interior_water_height` — deliberately out of the parser's tri-state).

## Suggested Fix
Add one test per walker: encode an `XCLW` payload of `f32::MAX.to_le_bytes()` (or `#INT_MIN#`) through the full `parse_cell_group` / `parse_wrld_children` call and assert `water_height_is_explicit == true` and `water_height == None` on the resulting `CellData`.

## Completeness Checks
- [ ] **TESTS**: This finding IS the test-gap — the suggested fix adds the missing coverage
