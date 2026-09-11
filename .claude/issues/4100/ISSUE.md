# CHAR-2026-09-11-D3-02: `charal-oblivion-ruleset.md`'s header cites `AttributeSet::OBLIVION`, a symbol that does not exist

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4100
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4100 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Leveling & Progression Models
- **Game**: oblivion
- **Location**: `docs/engine/charal-oblivion-ruleset.md:4-6` vs `crates/core/src/character/tes.rs:103` and `crates/core/src/character/attribute.rs:85-116`
- **Source**: n/a — this is a symbol-name claim, not a numeric one, so no capture value is required. The correct name is `AttributeSet::TES_CLASSIC` (`attribute.rs:98`).

## Description

The Oblivion capture document — the authority Dimension 3 is required to read *before* the Rust — opens by naming the four shipped pieces of the Oblivion core ruleset: "`AttributeSet::OBLIVION`, `SkillSet::OBLIVION`, `LevelingModel::OBLIVION`, `oblivion_attribute_bonus`, `oblivion_health_formula`". Three of the five exist. `AttributeSet::OBLIVION` does not: `attribute.rs` declares exactly four attribute-set consts — `FALLOUT`, `TES_CLASSIC`, `SKYRIM`, `STARFIELD` — and `oblivion_ruleset` builds with `AttributeSet::TES_CLASSIC` (`tes.rs:103`). The name is deliberate and load-bearing: `TES_CLASSIC` signals that Morrowind and Oblivion share one 8-attribute roster, which is exactly the fact a per-game name would hide. `charal.md:336` and `tes.rs:92` both use the right name; this document is the sole outlier.

## Evidence

`grep -rn "AttributeSet::OBLIVION" . --include='*.rs' --include='*.md'` returns **one** hit in the entire repository — `docs/engine/charal-oblivion-ruleset.md:4` — and zero in any `.rs` file. The paragraph containing it was edited in this delta (`git diff 64f64480..HEAD -- docs/engine/charal-oblivion-ruleset.md` rewrites the same sentence's tail to add the "unwired" clause), so the stale symbol was read past during that edit.

## Impact

Low but real for this audit's own method: an auditor instructed to read the capture document first, then verify the code against it, is handed a symbol that does not compile and must reverse-engineer which set was meant. It also weakly implies a per-game Oblivion roster exists, inviting exactly the duplicate-per-game-const pattern `TES_CLASSIC` was named to prevent.

## Related

#3848 (the same sentence's freshly-corrected "unwired" clause); CHAR-2026-09-11-D3-01, -D3-03

## Suggested Fix

Replace `AttributeSet::OBLIVION` with `AttributeSet::TES_CLASSIC` in `charal-oblivion-ruleset.md:4` and note in half a clause that the roster is shared with Morrowind, so the next reader does not "fix" it back.

## Completeness Checks

- [ ] **SIBLING**: every other copy of this fact swept repo-wide (this class has repeatedly had one copy fixed and another missed — grep the symbol/number across `docs/`, `crates/`, `.claude/`, not just the named file)
- [ ] **TESTS**: where the doc states a pinned number or symbol, a test or `_audit-validate.sh` rule keeps it honest