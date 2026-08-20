# #3221 — SKY-2026-08-20-D4-02: fXPPerSkillRank is authored by no shipped Skyrim master — the third GMST of the #2942 fix is a permanent silent no-op behind a "GMST-sourced" claim (remainder half; reachability half is #3170)

**Issue**: #3221 — https://github.com/matiaszanolli/ByroRedux/issues/3221
**Finding ID**: `SKY-2026-08-20-D4-02`
**Severity**: LOW
**Dimension**: 4 — per-title data semantics
**Audit**: `/audit-skyrim` — `docs/audits/AUDIT_SKYRIM_2026-08-20.md` (HEAD `bb0b92f2`, 2026-08-20 comprehensive suite)
**Labels**: low, legacy-compat, gameplay, bug
**Filed**: 2026-08-20 · `/audit-publish`

---

**Audit**: `/audit-skyrim` — `docs/audits/AUDIT_SKYRIM_2026-08-20.md` (Dim 4), HEAD `bb0b92f2`
**Finding ID**: `SKY-2026-08-20-D4-02` (**remainder half** — see Scope)

- **Severity**: LOW
- **Status**: NEW

## Scope — what this issue is and is not

`SKY-2026-08-20-D4-02` reported **two independent reasons** the Skyrim leveling GMST overlay never runs:

1. **Unreachable** — `CharacterRulesProfile::SKYRIM` carries `ruleset: RulesetBuilder::None`, so `build_ruleset` returns `None` before the `with_gmst` line. → **already filed as #3170, not re-filed here.**
2. **One of the three GMST names does not exist in the game.** → **this issue.**

## Location

- `crates/core/src/character/leveling.rs:92` — `xp_per_skill_rank: gmst("fXPPerSkillRank").unwrap_or(xp_per_skill_rank)`
- Docstring citing it: `crates/core/src/character/leveling.rs:66`
- The capture document that asserts it is authored: `docs/engine/charal-skyrim-ruleset.md:711-720` ("XP / level curve — **LOCKED**")

## Description

`1c9b8d7a` ("Source Skyrim leveling values from GMST", *Fix #2942*) reads three GMSTs by name. Two exist. **`fXPPerSkillRank` is authored by no shipped Skyrim plugin**, so even once #3170's reachability is fixed, that lookup is a permanent silent no-op that quietly retains the hard-coded `1.0` while the code and the capture document both claim it is GMST-sourced.

This is the `feedback_no_guessing` failure mode: an unverified constant name shipped behind a "GMST-sourced" claim, in a commit that closed an issue about exactly that.

## Evidence

Independent byte-scan for the literal EDID strings across every installed Skyrim master (`Skyrim.esm`, `Update.esm`, `Dawnguard.esm`, `HearthFires.esm`, `Dragonborn.esm`):

```
Skyrim.esm       fXPPerSkillRank=0   fXPLevelUpBase=1   fXPLevelUpMult=1
Update.esm       fXPPerSkillRank=0   fXPLevelUpBase=0   fXPLevelUpMult=0
Dawnguard.esm    fXPPerSkillRank=0   fXPLevelUpBase=0   fXPLevelUpMult=0
HearthFires.esm  fXPPerSkillRank=0   fXPLevelUpBase=0   fXPLevelUpMult=0
Dragonborn.esm   fXPPerSkillRank=0   fXPLevelUpBase=0   fXPLevelUpMult=0

every distinct "fXP*" string in Skyrim.esm:  fXPLevelUpBase, fXPLevelUpMult
```

A GMST EDID sweep over the same masters agrees: `Skyrim.esm` holds 1 584 `GMST` records, and the complete `fXP*` set install-wide is `{fXPLevelUpBase, fXPLevelUpMult}`.

The two names that **do** exist match `LevelingModel::SKYRIM`'s hard-coded values exactly (`fXPLevelUpBase = 75.0`, `fXPLevelUpMult = 25.0`, vs `xp_base: 75.0, xp_mult: 25.0`) — the sourced constants are right; only the third name is not a GMST.

`docs/engine/charal-skyrim-ruleset.md:712-714` states: *"Skyrim's character XP curve is authored by the GMST settings `fXPLevelUpBase`, `fXPLevelUpMult`, and `fXPPerSkillRank`"*, sourced to UESP. Whatever UESP documents, the shipped data does not carry that record, so the parser can never supply it.

## Impact

**None at runtime today** — the code is unreachable on Skyrim (#3170), which is why this is LOW.

It matters because the next person to wire `RulesetBuilder::Skyrim` inherits a silent no-op behind a documented "GMST-sourced, mods may override" claim, and because `#2942` and `#2945` are both closed on the strength of that claim.

## Related

- **#3170** — the reachability half of the same finding (already filed; **do not duplicate**)
- **#2942** (CLOSED) — closed by `1c9b8d7a`, the commit that introduced the name
- **#2945** (CLOSED) — closed by adding the `charal-skyrim-ruleset.md` "XP / level curve — LOCKED" section that asserts the name
- **#3173** — the same class one layer down: GMST names cited for the Fallout skill auto-calc coefficients that no shipped master authors

## Suggested Fix

Either drop the `fXPPerSkillRank` lookup and mark the `1.0` as an engine constant with its real provenance, **or** replace it with a name verified present in `Skyrim.esm`. Correct `charal-skyrim-ruleset.md:711-720` in the same change so the capture stops asserting an unauthored GMST.

## Completeness Checks
- [ ] **SIBLING**: every other `gmst("…")` literal in `crates/core/src/character/` is checked against the shipped masters for the same class of error (see #3173)
- [ ] **TESTS**: a test asserts the GMST names the code reads are a subset of a checked-in, data-derived name list — so a fabricated name fails at test time rather than at runtime
- [ ] **DOCS**: `charal-skyrim-ruleset.md` no longer claims an unauthored GMST
