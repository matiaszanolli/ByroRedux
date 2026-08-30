# #3727 — ESM-2026-08-30-D4-01: GameKind::from_header's "latent" note is stale

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: LOW (doc-rot) · **Dimension**: Record Schema Dispatch
**Record / Sub-record**: `ARMO`/`WEAP`/`BOOK` `DATA`
**Location**: `crates/plugin/src/esm/reader.rs` (`GameKind::from_header`'s band-rationale comment, ~:183-188)
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D4-01)

## Description

The comment states, present tense: *"Latent because WEAP/ARMO/AMMO DATA arms in items.rs bucket Fallout4 with Fallout3NV/Oblivion."*

That bucketing is **gone**, verified at HEAD:

- `items.rs:517` gives `ARMO DATA` a dedicated `GameKind::Fallout4` arm that **swaps** the last two fields (`value, weight, health` vs FO3NV's `value, health, weight`) at the same 12-byte length
- `items.rs:294` makes `WEAP DATA` an empty FO4 arm (confirmed: 0 WEAP `DATA` in `Fallout4.esm`)
- `items.rs:839` gives `BOOK` its own 8-byte FO4 arm

## Impact

No live mis-band exists — every band was re-verified end-to-end against sampled values this run. But the comment is the reasoning a future band edit is checked against, and it now **understates** the cost: a FO3↔FO4 mis-band would silently swap armor weight and health and zero every weapon's value/weight/damage.

## Related

#439 / audit FO3-3-01 (the original inversion this comment documents).

## Suggested Fix

Rewrite the "latent" clause to name the three live FO4-specific arms and the concrete corruption a mis-band now produces.

## Completeness Checks
- [ ] **SIBLING**: The `/audit-esm` SKILL's Dim-4 checklist checked for the same stale "latent" premise
- [ ] **TESTS**: n/a (documentation)
