# SAFE-2026-08-16-05: REFRACT_PASSTHRU_BUDGET is a backticked symbol that exists nowhere

**Issue**: #3052
**Severity**: LOW
**Labels**: `low,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_SAFETY_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAFETY_2026-08-16.md`.

**Location**: `.claude/commands/audit-safety/SKILL.md`:257 (Dimension 7, first bullet). Ground truth: `crates/renderer/shaders/triangle.frag`.
**Status note**: NEW — distinct from the OPEN #2686 (which is about `GLASS_RAY_BUDGET` being a dead constant) and from the 2026-08-12 skill-correction table, which did not cover this symbol.

## Description

`/audit-safety` Dimension 7 names `` `REFRACT_PASSTHRU_BUDGET = 2` `` — a **backticked symbol that exists nowhere**, at a value 4× off the live cap.

## Evidence

Re-verified 2026-08-17:

```
$ grep -c "REFRACT_PASSTHRU_BUDGET" crates/renderer/shaders/triangle.frag
0
```

The only occurrences anywhere are the SKILL.md bullet and a guard in `crates/renderer/src/shader_constants.rs`:1166 that asserts the string is **absent**:
```rust
!src.contains("REFRACT_PASSTHRU_BUDGET = 2"),
```

So the codebase actively guards against the exact string the skill instructs auditors to look for. `triangle.frag`'s own comments (:1688-1695) describe a passthru allowance that grows 2/4/6/8 interfaces — not a fixed 2.

## Impact

An auditor following Dimension 7 searches for a symbol that cannot be found, and either reports a false finding or silently skips the check. Per the project's path-reference convention, a backticked symbol asserts it exists right now — this one does not.

`_audit-validate.sh`'s backticked-symbol advisory exists precisely to catch this class.

## Suggested Fix

Replace the bullet with the live mechanism and its real cap, or italicise it as a historical reference. Then clear it from the validate script's advisory list rather than scrolling past it.

## Related

- #2686 (OPEN — `GLASS_RAY_BUDGET` dead constant; adjacent but distinct)
- #3045 (REN-DOC-01), #3035 (ECS-07), #2974 (TD4-01) — audit-skill drift found in four separate skills this sweep

## Completeness Checks
- [ ] **SYMBOL-EXISTS**: The replacement names a symbol that is actually in the tree, or is italicised
- [ ] **VALUE-CORRECT**: The cited cap matches `triangle.frag`'s live behaviour
- [ ] **ADVISORY-CLEAN**: `_audit-validate.sh`'s advisory no longer lists it
- [ ] **PATH-GATE**: `_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3052 --json state` when live state is needed.*
