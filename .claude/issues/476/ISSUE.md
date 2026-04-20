# Issue #476

FNV-3-L1: CLMT WLST chance parsed as u32 but doc says i32 — negative chance wraps positive

---

## Severity: Low

**Location**: `crates/plugin/src/esm/records/climate.rs:64-83`

## Problem

WLST entry format is `(form_id: u32, chance: i32, [global: u32])`. Parser uses `read_u32_at` and stores `chance: u32`. Negative chance values (used by mods for sentinel / subtractive weights) wrap to huge positive values and win `max_by_key` in `cell_loader.rs:539`.

Comment at `climate.rs:65` already says `i32`; code reads `u32`.

## Impact

- Silent on vanilla FNV (no negative chances ship).
- Manifests on community mods that use negative-chance sentinels.

## Fix

Change field to `pub chance: i32`; read via `i32::from_le_bytes` (or convert from `read_u32_at` cast). Filter `< 0` before `max_by_key` in `cell_loader.rs:539`.

## Completeness Checks

- [ ] **TESTS**: Synthetic CLMT with one negative-chance WLST entry — assert the max picker skips it
- [ ] **SIBLING**: Check for other `chance` / `weight` fields with the same u32/i32 mismatch
- [ ] **DOCS**: Comment already correct; just align the type

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-3-L1)
