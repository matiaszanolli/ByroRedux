# #1875 — TD3-2026-07-05-01: Path gate RED — 7 stale resources.rs refs after the #1869 split

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1875
**Labels**: documentation, medium, tech-debt
**Filed via**: /audit-publish docs/audits/AUDIT_TECH_DEBT_2026-07-05.md

---

- **Severity**: MEDIUM
- **Dimension**: 3 (Stale Documentation) / 4 (Audit-Finding Rot)
- **Location**: `.claude/commands/audit-performance/SKILL.md` (lines 78, 84, 113, 116, 140), `.claude/commands/audit-starfield/SKILL.md` (line 199), `.claude/commands/audit-tech-debt/SKILL.md` (line 113)
- **Status**: NEW
- **Age**: introduced by `2d823f11` (Fix #1869, split `SkinSlotPool` out of `resources.rs`) — landed after the 2026-07-03 audit, which recorded a GREEN gate at 1006/1006 refs.

## Description

`crates/core/src/ecs/resources.rs` no longer exists; it is now the directory
`crates/core/src/ecs/resources/` (`mod.rs` + `skin_slot_pool.rs`). Seven
backticked refs across three audit-skill files still point at the vanished
file, so `.claude/commands/_audit-validate.sh` now exits 1. This is the
recurring TD7-* stale-path class the gate was created to catch (#1114); it
also means the gate blocks `/audit-publish`'s own path-validation step until
fixed.

## Evidence

`.claude/commands/_audit-validate.sh` output:
```
STALE: .claude/commands/audit-performance/SKILL.md:78 — `crates/core/src/ecs/resources.rs`
STALE: .claude/commands/audit-performance/SKILL.md:84 — `crates/core/src/ecs/resources.rs`
STALE: .claude/commands/audit-performance/SKILL.md:113 — `crates/core/src/ecs/resources.rs`
STALE: .claude/commands/audit-performance/SKILL.md:116 — `crates/core/src/ecs/resources.rs`
STALE: .claude/commands/audit-performance/SKILL.md:140 — `crates/core/src/ecs/resources.rs`
STALE: .claude/commands/audit-starfield/SKILL.md:199 — `crates/core/src/ecs/resources.rs`
STALE: .claude/commands/audit-tech-debt/SKILL.md:113 — `crates/core/src/ecs/resources.rs`
FAIL: 7 stale path reference(s).
```

## Impact

Any agent running `/audit-performance` or `/audit-starfield` is directed to a
nonexistent file; `/audit-publish`'s path gate fails on every report until
fixed.

## Related

Same class as the original #1114 / TD7-050 that motivated the gate. Distinct
from #1761 (dead_code, unrelated). While in these files, also fold in the
stale skill-text note: `audit-tech-debt/SKILL.md` Dim 3 still tells auditors
`docs/feature-matrix.md`'s M45 save/load and M47.2 transpiler rows read
"unstarted" — both were corrected in the doc on 2026-06-21.

## Suggested Fix

Re-point each ref. Most target the resource *domain* generally →
`crates/core/src/ecs/resources/mod.rs`; if a ref names `SkinSlotPool`
specifically, point it at `crates/core/src/ecs/resources/skin_slot_pool.rs`.
Re-run `_audit-validate.sh` to confirm GREEN.

## Completeness Checks
- [ ] **SIBLING**: All 7 refs re-pointed (grep `resources.rs` across `.claude/commands/` finds none)
- [ ] **TESTS**: `.claude/commands/_audit-validate.sh` exits 0 (GREEN) after the fix
