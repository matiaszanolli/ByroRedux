# CHAR-2026-09-11-D3-04: the Skyrim skill-XP cost curve is captured in no `charal-*-ruleset.md`, and the last sweep credited it to a Skyrim section that contains no such text

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4102
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4102 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Leveling & Progression Models
- **Game**: skyrim
- **Location**: `crates/core/src/character/skyrim.rs:33-73` (`SKYRIM_SKILL_USE_CURVE`, `skyrim_skill_xp_to_next`, `skyrim_skill_xp_between`); documents `docs/engine/charal-skyrim-ruleset.md`, `docs/engine/charal-oblivion-ruleset.md:787-794`, `docs/engine/charal.md:147`
- **Source**: UNSOURCED-in-capture. The value `1.95` **is** captured, but only in the **Oblivion** document (`charal-oblivion-ruleset.md:792`, "The engine's `fSkillUseCurve` is 1.95 for the skill-use progression curve") and in `charal.md:147` (an implementation-summary table row). The cost formula `improve_mult · L^use_curve + improve_offset` and its worked anchors (Lockpicking mult 0.25 / offset 300 → 15→16 ≈ 349.13, 15→20 ≈ 1815.5) appear in **no** capture document at all.

## Description

#2945 filed "Skyrim/Oblivion leveling constants are sourced only to `charal.md` implementation prose (circular)" and was closed by adding `charal-skyrim-ruleset.md` § *XP / level curve — LOCKED* (`:711-721`) and `charal-oblivion-ruleset.md` § *Leveling — LOCKED* (`:787-794`). Those sections fully capture the *character*-XP half — rows 8, 12, 13, 14 above are now properly sourced, which is real progress. They do not capture the *skill*-XP half, which is a separate formula living in a separate function: `skyrim.rs:49-57`'s per-skill advancement cost. Its only documentary trace is `charal.md:147`'s parenthetical "(`fSkillUseCurve` 1.95)" — the same implementation-summary prose #2945 ruled circular — plus the function's own docstring. Consequently the `fSkillUseCurve` value a Skyrim reader needs is filed under *Oblivion*, and the formula shape is filed nowhere. The concrete harm has already occurred once: `AUDIT_CHARACTER_2026-08-30.md:306` records row 15's Source as "`charal-skyrim-ruleset.md`" and marks it PASS. `grep -ni "1\.95\|349\|improve\|use.curve" docs/engine/charal-skyrim-ruleset.md` returns nothing relevant — the cited authority does not contain the cited claim, so that PASS was recorded against a source that does not exist.

## Evidence

`grep -rn "fSkillUseCurve\|SkillImproveMult\|SkillImproveOffset\|349\.13\|1815" docs/` returns exactly two engine-doc hits — `charal.md:147` and `charal-oblivion-ruleset.md:792` — and zero in `charal-skyrim-ruleset.md`, whose 18 `##` sections (listed via `grep -n "^## "`) contain no skill-advancement-cost section. The code's own docstring (`skyrim.rs:44-48`) does carry the citation: "Source: UESP *Skyrim:Leveling* (Lockpicking 15→16 = `0.25·15^1.95 + 300` ≈ 349.13)", and `skill_xp_cost_matches_uesp_lockpicking` (`skyrim.rs:246-258`) pins both anchors and passes.

## Impact

No wrong number ships — `1.95` is supported and `improve_mult`/`improve_offset` are function parameters (the AVIF-authored per-skill values), not hardcoded constants, so there is nothing here for the no-guessing policy to catch. The impact is on the audit method itself: this dimension is contractually required to verify constants against the `charal-*-ruleset.md` captures, and for this formula there is nothing to verify against, which is how a fabricated source citation survived a full deep pass. It also leaves the one Skyrim-specific engine coefficient reachable only through the Oblivion document.

## Related

#2945 (CLOSED — the character-XP half of the same gap); `AUDIT_CHARACTER_2026-08-30.md:306` (the mis-citation this produced)

## Suggested Fix

Add a short "## Skill advancement cost — LOCKED" section to `charal-skyrim-ruleset.md` transcribing `skyrim.rs:42-48`'s formula, the `fSkillUseCurve = 1.95` value, the Lockpicking 0.25/300 authored pair and both worked anchors, citing UESP *Skyrim:Leveling* — and state explicitly that `improve_mult`/`improve_offset` are per-skill AVIF-authored, not engine constants, so the next sweep does not look for them as hardcoded values.

## Completeness Checks

- [ ] **SIBLING**: every other copy of this fact swept repo-wide (this class has repeatedly had one copy fixed and another missed — grep the symbol/number across `docs/`, `crates/`, `.claude/`, not just the named file)
- [ ] **TESTS**: where the doc states a pinned number or symbol, a test or `_audit-validate.sh` rule keeps it honest