---
name: source-command-audit-legacy-compat-skill
description: Audit ByroRedux compatibility gaps against Gamebryo 2.3 and Creation-era behavior across NIFAL, EXAL, PHYSAL, coordinate conversion, per-game translation, and uncovered legacy subsystems. Use when the user requests a legacy compatibility or cross-engine mapping audit.
---

# Legacy Compatibility Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-legacy-compat/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Read the live layer specifications named by the selected dimensions, then follow the canonical command's process and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as any requested audit focus or scope.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit only when the selected dimension requires its deeper checklist.
- Use codebase-memory graph tools first for symbol discovery, callers, translation boundaries, and impact analysis. Use text search for legacy headers, literals, configs, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Treat documented limitations and closed leak inventories as regression guards, not new findings.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
