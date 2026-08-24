# 3285: FNV-2026-08-24-D4-01: LVLI multi-pick semantics fix (#3217) has no FNV-specific test coverage despite a 229-record blast radius

**Severity**: LOW · **Report**: `docs/audits/AUDIT_FNV_2026-08-24.md` (FNV-2026-08-24-D4-01)

## Description

`#3217` changed `multi_pick` from `flags & (0x02 | 0x04) != 0` to `flags & 0x04 != 0` — only bit 2 ("Use All") now triggers "select every eligible entry". The fix and both tests are entirely Skyrim-sourced. `expand_leveled_inner` is shared, game-agnostic code. A byte-level census of `FalloutNV.esm` (2738 `LVLI` records) found **229 records** (bit `0x02`-only or `0x01|0x02`) whose selection output changed from "every eligible tier" to "single highest tier" — including `LeveledLegionArmorDecanus`, `LeveledNVRangerCivilianOutfitArmor`, nine `VendorAmmo*` ladders, and more.

## Location

`crates/plugin/src/equip.rs:343-420` (`expand_leveled_inner`), fix commit `1d0c5d4b`, tracked as `#3217`

## Evidence

FNV `LVLF` byte histogram: `{4: 528, 0: 445, 3: 1128, 2: 540, 1: 97}`. Sample: `LeveledLegionArmorRecruitPrime` (flags `0x3`) has entries at level 1 and level 9 — old code selected both, new code selects only level 9.

## Impact

Loot/vendor ammo ladders and Legion/NCR outfit bundles change shape on FNV NPCs and containers, unverified against any FNV fixture — pure test-gap risk, not a confirmed-wrong behavior.

## Suggested Fix

Add an FNV-sourced regression fixture pinning current output for a bit-`0x02`-only record, or research/confirm the correct GECK-era "calculate for each item" semantics.

## Completeness Checks
- [ ] **TESTS**: An FNV-sourced regression fixture pinning `expand_leveled_inner`'s current output for at least one bit-`0x02`-only record
