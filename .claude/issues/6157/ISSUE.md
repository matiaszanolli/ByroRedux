# TD2-2026-10-19-12: Skyrim class skill and pool allocation share a 20-line leftover-distribution loop, copied with its tie-break

Labels: low,character,esm-plugin,game:skyrim,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:416-448` (`skyrim_class_skill_points`, N=18) vs `:462-500` (`skyrim_class_pool_points`, N=3)
- **Age**: 7c740eca0 (2026-10-14), tie-break reversed in 91d5e8d5c (2026-10-17), pool copy b9e60846e (#5914, 2026-10-18)
- **Effort**: trivial
- **Description**: `sets = total / sum`, per-slot `sets * weight`, then `order` sorted by (weight desc, index desc) and a `taken[]` round-robin until the leftover is gone. The pool doc says "distributed exactly like the skills" and carries its own re-statement of the index-descending tie rule that 91d5e8d5c had to change on the skill copy.
- **Proposed fix**: `fn distribute_by_weight<const N: usize>(weights: &[u8; N], total: i32) -> [i32; N]` in the same file; the skill fn passes `8 * (level-1)`, the pool fn `10 * (level-1)` and adds the Health term after.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
