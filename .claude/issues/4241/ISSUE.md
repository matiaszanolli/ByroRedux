# FO4-D6-01: mswp.rs module doc describes XMSP consumption as future work; it already shipped

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4241

**Severity**: LOW
**Dimension**: 6 — ESM Architecture Records (SCOL/MOVS/PKIN/TXST)
**Location**: `crates/plugin/src/esm/records/mswp.rs:30-38`
**Status**: NEW

**Description**: The module doc phrases `XMSP` material-swap parsing and consumption as pending ("the cell loader will consult once … parsed"). Both the parse (`esm/cell/walkers.rs:1015-1030`) and the consumer (`byroredux/src/cell_loader/refr.rs:401-438`) already exist and are regression-tested.

**Evidence**: Confirmed in current code — `walkers.rs:1024` has a live `b"XMSP" => {` parse arm, and `refr.rs:401-438` resolves `placed.material_swap_ref` against `index.material_swaps`, populating `ov.material_swaps`/`ov.material_swaps_filter`.

**Impact**: None on behavior — stale doc only, risk of a future contributor re-implementing shipped work.

**Suggested Fix**: Update the doc's "Downstream use" paragraph to state XMSP resolution is implemented (cite `refr.rs`'s consumer instead of describing it as future work).

## Completeness Checks
- [ ] **TESTS**: N/A (documentation-only fix)
