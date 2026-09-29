# TD4-2026-09-29-03: Two audit skills backtick a nonexistent `triangle_early.frag`

**Labels**: low,tech-debt,documentation,doc-rot

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 4 · **Status**: NEW · **Effort**: trivial · **Kind**: doc-rot
- **Location**: `.claude/commands/audit-performance/SKILL.md:55,128,129`;
  `.claude/commands/audit-renderer/SKILL.md:38,110` (introduced by `9fcfdc3fc`, today)
- **Evidence**:
  - The early-test variant is `triangle.frag` compiled to `triangle_early.frag.spv`
    (`scripts/check-shader-artifacts.sh:59`).
  - `_audit-validate.sh` lists all five as deleted-file basename advisories.
- **Suggested Fix**: write `triangle_early.frag.spv`, or "the early-test variant of `triangle.frag`".

**Validated at HEAD 9fcfdc3fc**: `crates/renderer/shaders/triangle_early.frag` does not exist (only `triangle_early.frag.spv`, built from `triangle.frag` by `scripts/check-shader-artifacts.sh:59`); the bare backticked `triangle_early.frag` is at `.claude/commands/audit-performance/SKILL.md:55,128` and `.claude/commands/audit-renderer/SKILL.md:110` (perf :129 and renderer :38 already say `.spv`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
