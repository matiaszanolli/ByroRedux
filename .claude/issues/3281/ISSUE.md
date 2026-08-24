# 3281: LC-D6-2026-08-24-01: per-game-translation-survey.md is 3 months stale and its headline example now describes a fixed bug

**Severity**: LOW · **Report**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-24.md` (LC-D6-2026-08-24-01)

## Description

`docs/engine/per-game-translation-survey.md` is named by `/audit-legacy-compat`'s own Dimension 6 as the reference for cross-game pattern findings, but hasn't been touched since 2026-05-28 (193+ commits stale). Two concrete claims are now false:

1. §2's headline "why Fallout looks broken" example describes `classify_pbr_keyword` collapsing every non-glass surface to the matte default — the pre-#1873-fix behavior. The current classifier runs an extensive, evidence-cited keyword classifier; the thesis is built on a closed bug.
2. §4.3's `RACE DATA` bullet claims "no Skyrim arm exists" for the size gate. `crates/plugin/src/esm/records/actor/mod.rs:1219` has a dedicated, byte-verified Skyrim arm (128/164-byte gated), extended 2026-08-24 to also capture Magicka/Stamina (#3219).

## Location

`docs/engine/per-game-translation-survey.md:1-52` (header + §2), `:215-218` (§4.3)

## Impact

Documentation-only. An auditor trusting §2 verbatim would misdiagnose the material boundary as broken and could waste effort re-filing #1873's closed bug.

## Related

LC-D6-03 (2026-08-20, same doc-rot class, different file). #1873 (closed, the bug §2 describes), #3219 (its code fix falsifies §4.3).

## Suggested Fix

Regenerate or hand-correct §2's example and the §4.3 RACE DATA bullet with the current per-game-arm state. Bump the `Status:` date.

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix
