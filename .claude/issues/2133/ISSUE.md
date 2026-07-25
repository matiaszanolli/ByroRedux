**Severity**: LOW · **Dimension**: Audit-infrastructure (doc-rot)
**Source**: `docs/audits/AUDIT_SAFETY_2026-07-25.md` (SAFE-2026-07-25-03)
**Status**: NEW
**Location**: `.claude/commands/audit-safety/SKILL.md` Dimension 7; actual value at `crates/renderer/src/shader_constants_data.rs:122` and its GLSL mirror `crates/renderer/shaders/include/shader_constants.glsl:40` (both `2_097_152` / `2097152u` — in lockstep with each other, just not with the SKILL's cited figure)

## Description
The SKILL text reads "`GLASS_RAY_BUDGET = 1048576`... raised from 8192 in `6efe1706`." Current code has it at `2_097_152` — a further doubling landed at some point after the SKILL prose was last synced, with the Rust constant and its generated GLSL `#define` still correctly matched to each other (this is NOT a lockstep-drift bug, just a stale citation in the audit skill's own prose).

## Evidence
```
$ grep -n "GLASS_RAY_BUDGET" crates/renderer/src/shader_constants_data.rs crates/renderer/shaders/include/shader_constants.glsl
crates/renderer/shaders/include/shader_constants.glsl:40:#define GLASS_RAY_BUDGET 2097152u
crates/renderer/src/shader_constants_data.rs:122:pub const GLASS_RAY_BUDGET: u32 = 2_097_152;

$ grep -n "GLASS_RAY_BUDGET\|1048576" .claude/commands/audit-safety/SKILL.md
214:- **Glass ray budget** `GLASS_RAY_BUDGET = 1048576`
```

## Impact
None to code correctness — the Rust↔GLSL lockstep between the two live files holds. A future auditor citing "1048576" as the live budget from the SKILL doc would be quoting a number at least one revision out of date, and might incorrectly flag the Rust/GLSL pair as drifted from the SKILL's expectation when they are actually consistent with each other and simply ahead of the doc.

## Suggested Fix
Update the SKILL's cited value to `2_097_152`, or better, drop the literal number from the prose and point at the constant by name only (the lockstep guard between the two files is what actually matters, not the magnitude).

## Related
`docs/audits/AUDIT_SAFETY_2026-07-25.md` Dimension 7 PASS list — confirms the Rust/GLSL lockstep and the glass-passthrough loop guard (#789) are both intact; only the SKILL's cited number is stale.

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix, no code path affected
