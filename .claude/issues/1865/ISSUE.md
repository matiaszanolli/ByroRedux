# #1865: SCR-D6-NEW-03: Globals (#1668) resource unconditionally rebuilt on every interior cell load but guarded on exterior

- **Severity**: MEDIUM
- **Labels**: `medium`, `bug`
- **Source**: `docs/audits/AUDIT_SCRIPTING_2026-07-03.md` (SCR-D6-NEW-03)
- **Dimension**: Scripting Runtime Systems

## Location
`byroredux/src/cell_loader/load.rs:372-378` vs. `byroredux/src/cell_loader/exterior.rs:268-279`

## Description
`exterior.rs` guards the `Globals` resource rebuild with `try_resource::<Globals>().is_none()`, preserving runtime mutations across streaming. `load.rs`'s interior path rebuilds unconditionally on every load.

## Impact
Dormant today (no production `SetGlobalValue` writer exists). Once one lands, every interior-cell transition will silently revert player-mutated GLOBs while the exterior path correctly preserves them.

## Suggested Fix
Make `load.rs`'s interior insert conditional on `try_resource::<Globals>().is_none()`, mirroring `exterior.rs` exactly.
