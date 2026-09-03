# #3221 — SKY-2026-08-20-D4-02 (remainder): fXPPerSkillRank is authored by no shipped Skyrim master

**Severity**: LOW · **Dimension**: Character leveling (CHARAL)
**Location**: `crates/core/src/character/leveling.rs`, `docs/engine/charal.md`

## Investigation — the code half was already fixed

Verified the premise against current code first, per standing practice.
`LevelingModel::with_gmst` (`crates/core/src/character/leveling.rs`)
requests exactly two GMSTs — `fXPLevelUpBase` and `fXPLevelUpMult` —
and carries `xp_per_skill_rank` through untouched:

```rust
Self::SkillXp {
    xp_base: gmst("fXPLevelUpBase").unwrap_or(xp_base),
    xp_mult: gmst("fXPLevelUpMult").unwrap_or(xp_mult),
    xp_per_skill_rank,
    ...
}
```

The `fXPPerSkillRank` read this issue describes as "a permanent silent
no-op" does not exist in the current tree at all — it was withdrawn
2026-08-24 as a settled design decision (per
`docs/audits/AUDIT_CHARACTER_2026-08-30.md`'s own
CHAR-2026-08-30-D6-02 finding, which traced the same discrepancy this
issue's remaining half is about). An existing regression test already
pins the exact fix this issue's own TESTS checklist asks for —
`skyrim_gmst_overlay_reads_only_authored_curve_settings` asserts
`requested.into_inner() == ["fXPLevelUpBase", "fXPLevelUpMult"]`, an
exact-match pin stronger than the "subset of a checked-in name list"
the checklist requested.

`docs/engine/charal-skyrim-ruleset.md`'s capture (the doc this issue's
own suggested fix named for correction) was also already fixed — line
717 already reads "that coefficient is an engine rule, not a
`fXPPerSkillRank` GMST."

## What was actually still stale

`docs/engine/charal.md` §8 item 6 — a *different* document from the two
above — still claimed *"Skyrim's XP curve now overlays the authored
`fXPLevelUpBase`, `fXPLevelUpMult`, and `fXPPerSkillRank` values with
sourced fallbacks"*, contradicting both the code and its own sibling
capture document. This exact contradiction is independently documented
as `AUDIT_CHARACTER_2026-08-30.md`'s CHAR-2026-08-30-D6-02 finding
(same underlying issue, filed against the one artifact that hadn't
caught up yet).

## Fix

Corrected `charal.md` §8 item 6: dropped `fXPPerSkillRank` from the
overlay sentence, and added the same half-clause the (already-correct)
capture document uses — "only the level curve is GMST-authored; the
skill-rank coefficient is engine-owned" — plus a citation back to this
issue and the 2026-08-24 withdrawal date, so a future reader lands on
the explanation instead of re-discovering the same contradiction a
third time.

## SIBLING (issue's own checklist item — "every other `gmst(\"…\")`
literal in `crates/core/src/character/` checked, see #3173")

The issue explicitly scopes this to the already-separately-filed #3173
(the Fallout `fAVDSkill*` derived-skill GMST class). Confirmed #3173 is
already **CLOSED** — the sibling class is fully covered there, nothing
further needed here.

## TESTS (issue's own checklist item)

Already covered by the pre-existing
`skyrim_gmst_overlay_reads_only_authored_curve_settings` test (see
above) — no new test needed for a documentation-only fix with an
existing exact-match code pin already in place.

## Verification

- `cargo test -q -p byroredux-core --lib character::leveling::`: 8
  passing, 0 failing (unchanged — confirms the existing pin already
  covers this).
- `cargo test -q --no-fail-fast` (full workspace): **7181 passing, 0
  failing** (doc-only change, no new tests).
