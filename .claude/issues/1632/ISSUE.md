# TD8-001: Nine stale allow(dead_code) on CellLightingRes fields — all now read

_Filed as #1632 from `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md`. Immutable snapshot as-filed; GitHub is authoritative for live state._

**Severity**: LOW · **Dimension**: Dead Code · **Effort**: trivial
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-06-14.md` (TD8-001)
**Status**: NEW

## Description
`CellLightingRes` (`byroredux/src/components.rs`) documents a staged-rollout policy: each field's `#[allow(dead_code)]` is "removed in lockstep with the matching shader-side consumer landing." A consumer landed — `cell.lighting` (`commands.rs:1697+`) reads every field. The nine allows (lines 269, 281, 284, 287, 290, 296, 299, 303, 306) now defeat their own purpose: a genuinely-dead field added later would be masked.

## Evidence
Nine `#[allow(dead_code)]` on `CellLightingRes` fields in the 269-306 range; `commands.rs:1697` `match world.try_resource::<crate::components::CellLightingRes>()` dumps each field via the `cell.lighting` console command. Stripping all 9 allows + `cargo check -p byroredux` → zero dead-code warnings.

## Impact
The allows mask future genuinely-dead fields on this struct — the staged-rollout policy explicitly says they should be removed once consumed, and they have been consumed.

## Suggested Fix
Delete the 9 allow lines; keep the policy comment, noting the fields are now consumed by `cell.lighting`. (Leave the unrelated `#1199` worldspace-transition allow at line 813 intact.)

## Completeness Checks
- [ ] **SIBLING**: Only the nine consumed `CellLightingRes` field allows removed; the line-813 `#1199` future-hook allow left in place
- [ ] **TESTS**: `cargo check -p byroredux` is warning-clean after removal (no field was actually dead)
