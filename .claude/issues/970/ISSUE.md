# OBL-D3-NEW-06: Oblivion CELL RCLR (regional-color override) never parsed

**Labels**: bug, low, legacy-compat

**Audit**: `docs/audits/AUDIT_OBLIVION_2026-05-11_DIM3.md`
**Severity**: LOW
**Domain**: ESM / TES4 cell records

## Premise

The cell walker matches a long list of sub-records (`EDID`, `FULL`, `DATA`, `XCLW`, `XCIM`, `XCWT`, `LTMP`, `XCAS`, `XCMO`, `XCMT`, `XCCM`, `XLCN`, `XCLR`, `XOWN`, `XRNK`, `XGLB`, `XCLL`, …) but has no arm for `b\"RCLR\"`.

[crates/plugin/src/esm/cell/walkers.rs:85-242](../../crates/plugin/src/esm/cell/walkers.rs#L85-L242)

## Gap

Oblivion exterior CELL records can carry `RCLR` (3-byte RGB regional fog/sky color override). FO3+ moved this concept to the LGTM/CLMT chain.

## Impact

Editor-authored cell-level color tint is dropped. Rare in vanilla, occasional in modded cells. No effect on renderer defaults.

## Suggested Fix

Add to `CellData`:

```rust
pub regional_color_override: Option<[u8; 3]>,
```

Add a 3-byte read arm in the cell walker, gated on Oblivion (or simply read it cross-game — the field is harmless when None elsewhere):

```rust
b\"RCLR\" if sub.data.len() >= 3 => {
    regional_color_override = Some([sub.data[0], sub.data[1], sub.data[2]]);
}
```

## Completeness Checks

- [ ] **TESTS**: Regression test parses an Oblivion exterior cell with a known RCLR (modded data) and asserts the override is populated.
- [ ] **RENDERER**: Wire `regional_color_override` into the LGTM fallback chain if the engine has no LGTM for the cell — defer; not required to close this issue.
