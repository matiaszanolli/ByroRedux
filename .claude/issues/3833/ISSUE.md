# #3833: REN-2026-09-05-D1-02: TlasIntegritySnapshot remains a dead accessor with no consumer anywhere in the workspace

Filed from `docs/audits/AUDIT_RENDERER_2026-09-05.md` (REN-2026-09-05-D1-02) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3833 --json state`.

---

**Severity**: LOW
**Dimension**: AS Correctness (observability)
**Source**: `docs/audits/AUDIT_RENDERER_2026-09-05.md` (`REN-2026-09-05-D1-02`)

**Filing note**: first reported as `REN-2026-08-30-D1-01` in `AUDIT_RENDERER_2026-08-30.md` and never converted to an issue. This is its **second** sweep without a tracking number — the audit explicitly recommended it as the one finding from that dimension that should actually be filed.

## Location

- `crates/renderer/src/vulkan/acceleration/mod.rs` — `TlasIntegritySnapshot`, the `tlas_integrity` field
- `crates/renderer/src/vulkan/acceleration/tlas.rs` — `integrity_snapshot` accessor; the write in `build_tlas_instances`

## Description

`TlasIntegritySnapshot` is computed and stored every frame, and exposed through a `pub` accessor that nothing calls. The data that would turn a steady-state RT-membership regression into a positive per-frame assertion is produced and then thrown away.

## Evidence

`grep -rn "tlas_integrity\|TlasIntegritySnapshot\|integrity_snapshot"` returns 7 hits (definition, field decl, `Default` init, one write, one accessor, docs) and `grep -rn "integrity_snapshot()"` returns **0** call sites outside its own definition. Re-verified at publish time across `crates/`, `byroredux/` and `tools/`.

## Impact

A steady-state RT-membership regression (stuck LRU eviction, failing skinned first-sight build) is observable only via a once-per-second rate-limited `log::warn!`; the snapshot that would make it a positive per-frame assertion is computed and discarded. This is the observability half of the gap tracked as #1228.

## Related

#1228 (the underlying telemetry gap), `AUDIT_RENDERER_2026-08-30.md` `REN-2026-08-30-D1-01`.

## Suggested Fix

Wire a warmup-guarded `debug_assert_eq!` at the end of `build_tlas`, or surface the fields through an existing debug command family — and stop leaving a `pub` accessor with no reader.

## Completeness Checks

- [ ] **SIBLING**: Other computed-then-discarded telemetry structs in `acceleration/` checked
- [ ] **TESTS**: If wired as an assertion, a regression test pins the invariant it asserts
