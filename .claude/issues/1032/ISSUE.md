# Issue #1032

**Title**: REN-D14-NEW-01: tlm.materials_unique includes seeded neutral slot — dedup ratio reads as off-by-one

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — REN-D14-NEW-01
**Severity**: LOW
**File**: `byroredux/src/main.rs:1131` ; consumer `byroredux/src/commands.rs:350`

## Issue

`main.rs:1131` writes `material_table.len()` into `tlm.materials_unique`, but `len()` includes the seeded neutral slot 0. The fix `unique_user_count()` already exists at `material.rs:658` (added precisely for this) but is unused. Result: dedup-ratio printed at `commands.rs:350` is off-by-one — Prospector baseline 87 unique reads as 88.

## Fix

Replace `material_table.len()` with `material_table.unique_user_count()` at `main.rs:1131`. One-line fix.

## Completeness Checks
- [ ] **SIBLING**: Any other consumer of `material_table.len()` should be audited for the same off-by-one?
- [ ] **TESTS**: Console-output golden assertion on Prospector baseline (87, not 88)

