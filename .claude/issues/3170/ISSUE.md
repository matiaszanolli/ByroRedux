# CHAR-2026-08-20-D3-01: #2942's GMST-sourcing seam has zero production reach — with_gmst handles only the one LevelingModel variant that is never constructed

**Issue**: #3170 — https://github.com/matiaszanolli/ByroRedux/issues/3170
**Finding ID**: `CHAR-2026-08-20-D3-01`
**Severity**: MEDIUM
**Dimension**: 3 — Leveling & Progression
**Audit**: `/audit-character` — 2026-08-20 comprehensive suite, HEAD `bb0b92f2`
**Labels**: medium, legacy-compat, gameplay, bug

---

**Audit**: `/audit-character` — `docs/audits/AUDIT_CHARACTER_2026-08-20.md` (HEAD `bb0b92f2`)
**Finding ID**: `CHAR-2026-08-20-D3-01`
**Severity**: MEDIUM
**Dimension**: 3 — Leveling & Progression
**Game**: all wired games (FO3 / FNV / FO4)

## Location

- `crates/core/src/character/leveling.rs:81-98` — `with_gmst`
- `crates/core/src/character/profile.rs:146-159` — `build_ruleset`, the only caller
- `byroredux/src/npc_spawn.rs:223-229` — `build_character_ruleset`, which builds the `gmst` closure

## Description

**#2942** ("every leveling constant is hardcoded, shadowing ~2039 parsed-but-unreadable GMSTs")
was closed by `1c9b8d7a`, which added:

```rust
pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
    match self {
        Self::SkillXp { .. } => Self::SkillXp {
            xp_base: gmst("fXPLevelUpBase").unwrap_or(xp_base),
            xp_mult: gmst("fXPLevelUpMult").unwrap_or(xp_mult),
            xp_per_skill_rank: gmst("fXPPerSkillRank").unwrap_or(xp_per_skill_rank),
            ..
        },
        other => other,          // ← XpCurve (FO3/FNV/FO4) and SkillUse (Oblivion)
    }
}
```

`SkillXp` is **Skyrim's variant and Skyrim's alone**. `CharacterRulesProfile::SKYRIM` sets
`ruleset: RulesetBuilder::None`, and `build_ruleset` returns `None` **before** reaching the
`with_gmst` line for that arm. The three arms that *do* reach it — `Fallout3`,
`FalloutNewVegas`, `Fallout4` — all carry `XpCurve`, which falls straight through `other => other`.

**Net effect at HEAD: `index.game_setting_float` is invoked zero times by CHARAL on every game
that actually loads a ruleset.** The fix executes only inside `leveling.rs`'s own unit test
(`:322-331`).

## Evidence

`grep -rn "with_gmst" crates byroredux` returns exactly three sites: the definition
(`leveling.rs:81`), the single call (`profile.rs:157`), and the unit test (`leveling.rs:322`).

`RulesetBuilder` (`profile.rs:40-46`) has four variants — `None`, `Fallout3`,
`FalloutNewVegas`, `Fallout4` — **no `Skyrim`**. `profile.rs:151-156` returns `None` for
`RulesetBuilder::None` before `ruleset.leveling.with_gmst(gmst)` is reached.

The shadowing is not hypothetical. Read straight out of the shipped masters this session:

| GMST | FO3 | FNV | FO4 | CHARAL hardcodes |
|---|---|---|---|---|
| `fAVDActionPointsBase` | 65.0 | 65.0 | 60.0 | `65.0` / `65.0` / `60.0` |
| `fAVDActionPointsMult` | 2.0 | 3.0 | 10.0 | `2.0` / `3.0` / `10.0` |
| `fAVDCarryWeightsBase` | 150.0 | 150.0 | 200.0 (`fAVDCarryWeightBase`) | `150.0` / `150.0` / `200.0` |
| `fAVDCarryWeightMult` | 10.0 | 10.0 | 10.0 | `10.0` |
| `fAVDHealthEnduranceMult` | 20.0 | 20.0 | — | `20.0` |
| `fAVDHealthLevelMult` | 10.0 | 5.0 | — | `10.0` / `5.0` |

Every hardcoded value is **correct for vanilla** — which is the good news, and also why nothing
fails. But all six are authored, parsed, and readable today.

Doctrine source: `docs/engine/charal-oblivion-ruleset.md:361-372` — *"All future formula rows
built for Oblivion should read these by name once GMST parsing lands (CHARAL §8 item 6), not
re-hardcode the numeric constants captured here."* GMST parsing **has** landed
(`EsmIndex::game_setting_float`).

## Impact

Two things:

**(a) The closed issue's stated remedy does not apply to any shipped game.** A future reader will
believe leveling/derived constants are GMST-sourced when they are not. #2934's doctrine note names
#2942 as the paired precondition for moving `skill_calc` onto `CharacterRuleset`; that
precondition now *reads as satisfied* when in practice it is not.

**(b) Every retune mod for FO3/FNV/FO4 derived stats — a common category — has no effect.** No
crash, no log line, no failing test: precisely the silent-wrong-constant class this audit exists
for, and it lands on stats that reach real actors.

## Related

- **#2942** — CLOSED. The fix is present but unreachable, so this is **not a regression, it is an
  incomplete close**.
- **#2934** — CLOSED; its doctrine note's precondition now reads as satisfied when it is not.
- `CHAR-2026-08-20-D2-02` — the same GMST-reach problem one layer up, in the skill auto-calc comments.
- `CHAR-2026-08-20-D2-01` — the cheapest fix here (a `Skyrim` arm) is also what makes that finding go live.

## Suggested Fix

Extend `with_gmst` to the `XpCurve` and `SkillUse` arms, and — more valuable — apply the same
treatment to the **derived** table, where the sourced GMST names already sit in the capture
documents and the code comments.

The cheapest honest interim step is to give `RulesetBuilder` a `Skyrim` arm: `skyrim_ruleset` is
already written, sourced, and its four keys all resolve on `Skyrim.esm` (`AVDamageResist 0x5CE`,
`AVLightArmor 0x452`, `AVCarryWeight 0x3F0`, `AVStamina 0x3EA`), which at minimum makes the
existing `with_gmst` code reachable at all.

⚠️ **Note**: wiring a `Skyrim` arm makes `SkillSet::SKYRIM` production-reachable, which promotes
`CHAR-2026-08-20-D2-01` (the `Illusion` / `AVMysticism` key) from latent to live. Fix that first,
or in the same commit.

## Completeness Checks
- [ ] **SIBLING**: `SkillUse` (Oblivion) and `XpCurve` (FO3/FNV/FO4) arms both get GMST sourcing, not just one
- [ ] **CANONICAL-BOUNDARY**: GMST reads stay at the CHARAL ruleset-construction seam (`build_character_ruleset` / `build_ruleset`), never re-derived per consumer
- [ ] **TESTS**: a test asserts `gmst` is actually *called* on a wired game (the current unit test only exercises the `SkillXp` arm in isolation)
- [ ] **TESTS**: if a `Skyrim` arm lands, `CHAR-2026-08-20-D2-01`'s roster key is fixed and pinned first
