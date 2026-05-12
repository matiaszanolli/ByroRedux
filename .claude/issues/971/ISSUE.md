# FO4-D4-NEW-08: REFR XMSP sub-record never parsed; ~2,500 MSWP material-swap tables sit unused

**Labels**: bug, medium, legacy-compat

**Audit**: `docs/audits/AUDIT_FO4_2026-05-11_DIM4.md`
**Severity**: MEDIUM
**Domain**: ESM / FO4 / REFR routing

## Premise

`parse_mswp_group` at [crates/plugin/src/esm/cell/support.rs:486-509](../../crates/plugin/src/esm/cell/support.rs#L486-L509) populates `EsmCellIndex::material_swaps` with all ~2,500 vanilla FO4 MSWP records.

The REFR walker `parse_refr_group` at [crates/plugin/src/esm/cell/walkers.rs:336-487](../../crates/plugin/src/esm/cell/walkers.rs#L336-L487) matches XATO / XTNM / XTXR / XEMI / XESP / XTEL / XPRM / XLKR / XRMR / XPOD / XRDS / XOWN / XRNK / XGLB but has **no `b\"XMSP\"` arm**.

The doc comment at [crates/plugin/src/esm/cell/mod.rs:658](../../crates/plugin/src/esm/cell/mod.rs#L658) and at `records/mswp.rs:5,31` both already promise XMSP routing (\"REFR `XMSP` sub-records ... point at MSWP entries\"). The promise is not kept.

## Gap

Every REFR's `XMSP` (4-byte MSWP FormID) falls on the `_ => {}` catch-all at `walkers.rs:486`. `PlacedRef` has no `material_swap_ref: Option<u32>` field, and `build_refr_texture_overlay` at `byroredux/src/cell_loader_refr.rs:186-235` never looks up `index.material_swaps`.

## Impact

Every vanilla Raider armour colour-variant, settlement clutter variation, station-wagon rust pattern, and Vault decay overlay renders with the **base mesh's textures** — the per-REFR substitution table is dropped. ~2,500 MSWP records sit indexed but unused.

## Suggested Fix

Three coupled edits:

1. Add `material_swap_ref: Option<u32>` field to `PlacedRef` in `crates/plugin/src/esm/cell/mod.rs`.
2. Add the arm to `parse_refr_group` (`walkers.rs:486`-area):
   ```rust
   b\"XMSP\" => material_swap_ref = read_form_id(&sub.data),
   ```
3. Extend `build_refr_texture_overlay` (`cell_loader_refr.rs:186`) to look up `index.material_swaps.get(&swap_ref)` when `material_swap_ref` is `Some`, and apply each `MaterialSwapEntry` to the overlay (analogous to the existing XATO/XTNM/XTXR fill paths).

## Completeness Checks

- [ ] **SIBLING**: Verify the existing XATO/XTNM/XTXR overlay paths still work after the MSWP application is wired — overlay precedence (REFR-level XMSP vs REFR-level explicit XTXR) matches CK convention (XTXR direct overrides win).
- [ ] **SIBLING**: Confirm `MaterialSwapEntry` enumerates both BGSM-path swaps AND raw-TXST swaps; the overlay builder needs both routing paths.
- [ ] **TESTS**: Regression test parses a Sanctuary cell, picks one Raider-armour or settlement-clutter REFR with a known XMSP, and asserts `placed_ref.material_swap_ref` is populated to the expected MSWP FormID.
- [ ] **TESTS**: Renderer integration test loads a REFR with an XMSP → MSWP that replaces one texture slot, asserts the post-overlay texture path matches the swap target.
- [ ] **DOCS**: Remove the \"FO4-DIM6-02 stage 2\" planning reference from `cell/mod.rs:658` once the arm lands.
