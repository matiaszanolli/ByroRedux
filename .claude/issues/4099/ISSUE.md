# CHAR-2026-09-11-D3-01: `charal.md` §4.2 declares `CharacterLevel.xp` as `f32`; the shipped component has always been `u32`

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4099
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4099 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Leveling & Progression Models
- **Game**: all
- **Location**: `docs/engine/charal.md:167-175` (§4.2) vs `crates/core/src/character/components.rs:14-22`
- **Source**: `docs/engine/charal.md:170` — "`pub struct CharacterLevel { level: u16, xp: f32 /* progress toward next */ }`". This is the *canonical component spec*; there is no per-game capture value for it, so the mismatch is doc-vs-code, not a wrong game constant.

## Description

`charal.md` §4.2 is the design authority for the canonical `CharacterLevel` component and every downstream audit has quoted it verbatim as a Source line (e.g. #2947). It declares `xp: f32`. The component has carried `pub xp: u32` since its first commit (`be3e69d7`, confirmed with `git log -S"pub xp: u32"` — the field was never `f32`, so this is an original transcription error in the doc, not later drift). The code even documents the reasoning the doc lacks: "`u32` is ample — the per-level threshold never approaches `u32::MAX` even at FO4 extremes, and storing per-level progress (not cumulative) keeps it bounded." The mismatch is not cosmetic in one specific way: the value `xp` is compared against is `LevelingModel::xp_to_next`, which returns `f32` (`leveling.rs:166`), so the first leveling runtime to land must introduce a cast the spec says is unnecessary.

## Evidence

`grep -rn "xp: f32" docs/ crates/ byroredux/` returns `docs/engine/charal.md:170` and nothing in any `.rs` file; `components.rs:22` is `pub xp: u32,`. The only production readers are `crates/save/src/validate.rs:552` (`if level.xp != 0`) and `byroredux/src/npc_spawn.rs:196` (`xp: 0`), both integer-typed.

## Impact

Documentation only today — no leveling runtime exists, so nothing mis-computes. The blast radius is the moment one is written: `charal.md` §4.2 is what a contributor reads to learn the canonical shape, and it disagrees with the struct on the one field a progression system writes every frame. It is also an active source of bad audit citations — §4.2 has already been quoted as authoritative in a filed issue.

## Related

#2947 (quotes this exact §4.2 line as its Source); CHAR-2026-09-11-D3-02 and -D3-03 (same class — CHARAL prose disagreeing with byte-unchanged code)

## Suggested Fix

Change `charal.md:170` to `xp: u32` and carry over the one-line justification already in `components.rs:19-21`. If `f32` was the intended design, the change belongs in the code with a stated reason — but nothing today wants it.

## Completeness Checks

- [ ] **SIBLING**: every other copy of this fact swept repo-wide (this class has repeatedly had one copy fixed and another missed — grep the symbol/number across `docs/`, `crates/`, `.claude/`, not just the named file)
- [ ] **TESTS**: where the doc states a pinned number or symbol, a test or `_audit-validate.sh` rule keeps it honest