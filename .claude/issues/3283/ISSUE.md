# 3283: TD3-2026-08-24-02: audit-scripting/SKILL.md cites a test name declines_on_control_flow renamed hours before this audit ran

**Severity**: LOW · **Report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-24.md` (TD3-2026-08-24-02)

## Description

`audit-scripting/SKILL.md:871`'s regression-guard list cites `declines_on_control_flow`, renamed to `declines_unmodeled_conditional_guard` by `cee35507` (2026-08-24, same day). `SKILL.md:712` already correctly references the new name — only line 871 is stale.

## Location

`.claude/commands/audit-scripting/SKILL.md:871`; renamed test at `crates/scripting/src/translate/effects.rs:2125`

## Evidence

```
$ git show cee35507 -- crates/scripting/src/translate/effects.rs | grep -n "fn declines"
208:-    fn declines_on_control_flow() {
248:+    fn declines_unmodeled_conditional_guard() {
```

## Impact

Low — a reader following the skill's regression-guard pointer gets a grep miss on one of three named tests.

## Related

#3052 / TD4-2026-08-20-01 (the general mechanism).

## Suggested Fix

One-word edit: `declines_on_control_flow` → `declines_unmodeled_conditional_guard` at `audit-scripting/SKILL.md:871`.

## Completeness Checks
- [ ] **TESTS**: N/A — skill-doc-only fix
