# Issue #1034

**Title**: REN-D15-NEW-15: No-WTHR exterior fallback path doesn't write CellLightingRes — pitch-black risk

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D15-NEW-15
**Severity**: LOW (corner-case but covered by audit checklist item 10)
**File**: `byroredux/src/systems/weather.rs` (no-WTHR fallback branch)

## Issue

When no WTHR record is loaded for an exterior cell, `weather_system` returns without writing `CellLightingRes`, leaking stale interior values from the prior cell. If those values were neutral/dark this looks like pitch-black. Audit-checklist item 10 explicitly demands neutral lighting on this path.

## Fix

Write a documented neutral default `CellLightingRes` (mid-grey ambient + neutral-white sun + 6h sun direction) when WTHR is missing.

## Completeness Checks
- [ ] **SIBLING**: Same path on cell-load before any WTHR has been resolved
- [ ] **TESTS**: Synthetic exterior cell load with no WTHR → assert non-zero ambient

