**Severity**: LOW (informational — decode-ahead-of-consumer, intentional staging) · **Dimension**: ESM + Cell Bring-up
**Location**: decode `crates/plugin/src/esm/cell/walkers.rs:560-575`; runtime boundary `byroredux/src/components.rs:320-345` (`CellLightingRes::from_cell_lighting`)
**Source**: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D5-02)

## Description
The SF-specific XCLL tail (gravity_scale, near/far height-fog mid/range, high-density fog colours, interior_type) is decoded into `CellLighting.starfield` and pinned by a test. The shared fog fields are forwarded to `CellLightingRes`, but `from_cell_lighting` does not copy the `.starfield` sub-struct (it sets `starfield: None` in every construction — confirmed `components.rs:382,412,492`), so gravity_scale and the height-fog model stop at the plugin layer.

## Evidence
`from_cell_lighting` enumerates every field except `starfield`; no consumer of `.gravity_scale`/height-fog exists outside the parser + its test.

## Impact
None today — the engine has no interior volumetric height-fog or cell-driven gravity model, so there is nothing to forward to. Consistent with the parse-ahead pattern (NAVM, IMGS, FNV/Skyrim extended XCLL).

## Related
SF-D5-03 (same SF XCLL decode path).

## Suggested Fix
No action now. When a consumer lands, add `starfield: lit.starfield.clone()` to `from_cell_lighting`.

## Completeness Checks
- [ ] **TESTS**: When forwarding is wired, a test asserts `from_cell_lighting` propagates `.starfield` (mirroring the existing FNV/Skyrim fog-forward tests at `components.rs:417/447`)
