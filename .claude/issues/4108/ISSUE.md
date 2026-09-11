# CHAR-2026-09-11-D6-01: `charal.md` still documents the deleted `SkillSet::FALLOUT_FO3_FNV` and its merged 15-skill roster, at two sites

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4108
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4108 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: fo3, fnv
- **Location**: `docs/engine/charal.md:146` and `docs/engine/charal.md:307`
- **Source**: `crates/core/src/character/skill.rs:166-205` — the two shipped rosters, `SkillSet::FALLOUT3` (13 skills, `AVBigGuns` present) and `SkillSet::FALLOUT_NV` (13 skills, `AVBigGuns` deliberately absent, `SmallGuns`/`Throwing` keyed by EditorID not display name). Split from the merged set by #3169 (`.claude/issues/3169/ISSUE.md:39`: *"split `SkillSet::FALLOUT_FO3_FNV` into `FALLOUT3` / `FALLOUT_NV` and corrected FNV's `Guns` → …"*).

## Description

`charal.md` is the layer spec and the first document `/audit-character` Phase 1 loads. Two of its rows still name a constant that no longer exists and state a roster size that was never right after the split. `:305-308` reads *"Shipped rosters: `SkillSet::OBLIVION` (21 governed), `SkillSet::SKYRIM` (18 ungoverned), `SkillSet::FALLOUT_FO3_FNV` (15 = FO3 ∪ FNV, SPECIAL-governed) and `SkillSet::NONE` (FO4/FO76)"* — three of the four entries are correct and verified (21 / 18 / `NONE`), which is what makes the fourth read as authoritative. `:146` repeats the symbol in the AUTHORED-vs-ENGINE-SUPPLIED table's "Skill → governing attribute map" row (*"canonical `SkillSet` rosters shipped (OBLIVION / SKYRIM / FALLOUT_FO3_FNV)"*). The union framing is also substantively wrong now: the whole point of #3169 was that FO3 and FNV do **not** share a roster — FNV drops `BigGuns` and adds `Throwing`, so `|FO3 ∪ FNV| = 14`, not 15, and neither shipped roster is the union.

## Impact

Same class as CHAR-2026-09-11-D3-02 (`charal-oblivion-ruleset.md`'s `AttributeSet::OBLIVION`) and CHAR-2026-09-11-D2-03 (`charal-skyrim-ruleset.md`'s 32 B pin) — and it lands in the **parent** spec rather than a child capture, so a contributor who greps the symbol finds nothing and a contributor who reads the prose learns a merged roster that the code deliberately abolished. Prior sweeps have twice had to re-falsify an FO3↔FNV-collapse premise from stale documentation (last sweep's D6-03 was the SKILL.md copy); this is the third surviving copy of the same retired fact and the only one still live. Documentation-only: no code reads either line.

## Related

#3169 (the split); CHAR-2026-09-11-D2-03, CHAR-2026-09-11-D3-02 (the same incomplete symbol/number sweep, filed this sweep by Dimensions 2 and 3); CHAR-2026-08-30-D6-03 (the SKILL.md copy, since fixed)

## Suggested Fix

At `:307` replace with *"`SkillSet::FALLOUT3` (13, SPECIAL-governed) and `SkillSet::FALLOUT_NV` (13 — `BigGuns` dropped, `Throwing` added; keyed by `AVIF` EditorID, not display name)"*, and at `:146` replace `FALLOUT_FO3_FNV` with `FALLOUT3 / FALLOUT_NV`. Both are one-line edits.

## Completeness Checks

- [ ] **SIBLING**: every other copy of this fact swept repo-wide (this class has repeatedly had one copy fixed and another missed — grep the symbol/number across `docs/`, `crates/`, `.claude/`, not just the named file)
- [ ] **TESTS**: where the doc states a pinned number or symbol, a test or `_audit-validate.sh` rule keeps it honest