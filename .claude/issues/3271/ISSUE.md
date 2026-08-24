# 3271: CHAR-2026-08-24-D6-01: audit-character's own Scope section and dimension entry-point lists never name profile.rs

**Severity**: LOW · **Report**: `docs/audits/AUDIT_CHARACTER_2026-08-24.md` (CHAR-2026-08-24-D6-01)

## Description

`crates/core/src/character/profile.rs` defines `CharacterRulesProfile` (the single per-game policy row selected at the parser boundary), re-exported from `character::mod` and indexed in that module's own docstring. The skill file that directs `/audit-character` never lists `profile.rs` in its `## Scope` section or Dimension 1/3/5 entry-point lists — an auditor working strictly from the skill's own file lists would never read the file that owns the single construction seam Dimension 1's doctrine check and Dimension 3's coverage matrix both depend on.

## Location

`.claude/commands/audit-character/SKILL.md` — the `## Scope` section and the Entry points lines for Dimensions 1, 3, and 5

## Evidence

`grep -c "profile.rs" .claude/commands/audit-character/SKILL.md` → `0`.

## Impact

Process risk, not a runtime bug. Not skipped this cycle only because the auditor grepped the crate directly rather than following the skill's file list.

## Related

#2958 (CLOSED — the code-side equivalent of this drift, in `mod.rs`'s own docstring).

## Suggested Fix

Add a `profile.rs` bullet to the `## Scope` section and cite it explicitly in Dimension 1's and Dimension 5's entry-point lists.

## Completeness Checks
- [ ] **TESTS**: N/A — skill-doc-only fix
