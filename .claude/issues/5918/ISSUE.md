# CHAR-2026-10-16-M-05: `grant_experience`'s `to_next as u32` truncation spins forever when an authored curve gives `0 < xp_to_next < 1`

Labels: medium,character,bug,game:skyrim
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: MEDIUM (hardening; impact is a hang, trigger is an odd mod GMST)
- **Dimension**: Progression & Pools
- **Game**: skyrim (overlaid curve); latent for Fallout once M-02 lands
- **Location**: `byroredux/src/combat.rs:637-651`
- **Status**: NEW
- **Source**: no constant claimed; reachable through `LevelingModel::with_gmst`, which overlays mod-authored `fXPLevelUpBase`/`fXPLevelUpMult` (`charal-skyrim-ruleset.md:715-722`).
- **Description**: The loop breaks on `to_next <= 0.0`, then does `progress.xp -= to_next as u32`. For `0.0 < to_next < 1.0` the cast is 0, `xp` never decreases, `level` saturating-adds and `reached` grows without bound; `capped` is false when `level_cap() == 0`, so the loop never ends. A fractional `to_next` (e.g. 100.5) also drops 0.5 per level.
- **Impact**: A main-thread hang in the Late stage on the first scripted skill advance with a GMST pair such as base 0.5 / mult 0. Vanilla Skyrim.esm (75 / 25) is unaffected.
- **Suggested Fix**: Guard `to_next < 1.0` as well as `<= 0.0` (or use `to_next.ceil() as u32`), and bound the loop (e.g. at `u16::MAX` iterations).

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
