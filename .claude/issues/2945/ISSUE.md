# CHAR-D3-06: Skyrim/Oblivion leveling constants are sourced only to charal.md implementation prose (circular)

- **Issue**: [#2945](https://github.com/matiaszanolli/ByroRedux/issues/2945)
- **Finding ID**: `CHAR-D3-06`
- **Labels**: `low,legacy-compat,documentation`
- **Source report**: [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](../../../docs/audits/AUDIT_CHARACTER_2026-08-15.md)
- **Run**: `/audit-character` (first audit of this subsystem), 2026-08-15, HEAD `c25f61e6`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2945 --json state`.

---

- **Severity**: LOW
- **Dimension**: Leveling & Progression
- **Game**: Skyrim, Oblivion
- **Location**: `crates/core/src/character/leveling.rs:59-72` and `:110-125`; `crates/core/src/character/skyrim.rs:33-36`; documents `docs/engine/charal-skyrim-ruleset.md`, `docs/engine/charal-oblivion-ruleset.md`
- **Status**: NEW
- **Source**: `docs/engine/charal.md` §5 — "`SkillXp { xp_base, xp_mult, xp_per_skill_rank, pool_pick_gain, level_cap }` (Skyrim — `SKYRIM` = 25·L+75 XP, 1 XP/skill rank, +10 pool pick + perk/level; UESP-sourced)" — the sole documentary record of these values
- **Description**: `_audit-common.md` and this skill both designate the six `charal-*-ruleset.md` captures as "the authority for every constant". Neither `charal-skyrim-ruleset.md` (18 sections) nor `charal-oblivion-ruleset.md` (13 sections) contains a leveling or XP-curve section at all — unlike `charal-fnv-fo3-ruleset.md` and `charal-fo4-ruleset.md`, which each carry a dedicated "## XP / level curve — LOCKED" section naming the source page. The Skyrim XP constants, Oblivion's 10-major-skill-ups threshold, the `+1..5` attribute-bonus bands and `fSkillUseCurve = 1.95` are recorded only in `charal.md` §3/§5, which are *implementation summaries* — prose describing what shipped rather than a capture that preceded it. The sourcing is therefore circular: the document that verifies the code was written from the code. The three GMST names the code cites for the Skyrim curve — `fXPLevelUpBase`, `fXPLevelUpMult`, `fXPPerSkillRank` — appear in **no** document in the repository; a grep across `docs/` finds them only in `leveling.rs`'s own docstring. Two of the affected numbers do have independent per-game confirmation and are exempt: `SKYRIM_POOL_BASE = 100` and `pool_pick_gain = 10` are both confirmed by `charal-skyrim-ruleset.md` § *Magicka*.
- **Evidence**:
  ```
  $ grep -rn "fXPLevelUp\|fXPPerSkillRank" docs/ crates/ byroredux/
  crates/core/src/character/leveling.rs:64:    /// UESP *Skyrim:Leveling* (`fXPLevelUpBase`/`fXPLevelUpMult`/
  crates/core/src/character/leveling.rs:65:    /// `fXPPerSkillRank`).
  $ grep -n "^## " docs/engine/charal-skyrim-ruleset.md   # 18 sections, none on leveling
  $ grep -n "^## " docs/engine/charal-oblivion-ruleset.md # 13 sections, none on leveling
  ```
- **Impact**: These values cannot be audited. A future audit re-running this dimension will find the same closed loop and be unable to do better than "code matches the paragraph describing the code". Both models are unwired today, so nothing is currently mis-statted — this is a verification gap, not a wrong number, and it is reported at LOW precisely because no live path consumes them.
- **Related**: `charal.md` §9 "TES skill → governing-attribute maps + leveling curves — **mostly closed**" (the claim this finding qualifies); CHAR-D3-03 (the same constants, the GMST angle)
- **Suggested Fix**: Add a "## XP / level curve — LOCKED" section to `charal-skyrim-ruleset.md` and a "## Leveling" section to `charal-oblivion-ruleset.md` carrying the GMST names, their values, and the UESP page each came from — matching the shape the two Fallout captures already use. No code change.

## Completeness Checks
- [ ] **SIBLING**: The same drift class is swept across the other capture documents / docstrings, not just the row cited
- [ ] **TESTS**: A regression test pins this specific fix (`cargo test -p byroredux-core character`)

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_CHARACTER_2026-08-15.md`](docs/audits/AUDIT_CHARACTER_2026-08-15.md) — `/audit-character`, 2026-08-15, HEAD `c25f61e6`. First audit of this subsystem. Verified CONFIRMED against current code at publish time.*
