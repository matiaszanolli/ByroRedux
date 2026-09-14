# #4373 — TD6-006: `FactionRelations` is write-only, and its doc compares it to itself

**Labels**: low, scripting, combat, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4373

- **Severity**: LOW · **Dimension**: 6
- **Location**: `crates/scripting/src/combat.rs:39-83` (self-reference at `:54`) · **Status**: NEW · **Age**: `f61ea0447` (09-13) · **Effort**: trivial (doc) / medium (consumer) · **Kind**: tech-debt
- **Finding**: `SetEnemy` inserts pairs; `is_enemy` has no production caller. The scope is documented in prose but untracked.
- **Suggested Fix**: Fix the `:54` sentence and open a tracker for ambient hostility.

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
