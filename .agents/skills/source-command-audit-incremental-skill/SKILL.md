---
name: source-command-audit-incremental-skill
description: Audit only recently changed ByroRedux code for new bugs, regressions, contract breaks, unsafe deltas, and missing tests. Use when the user requests an incremental, delta, working-tree, commit-range, or since-date audit.
---

# Incremental Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-incremental/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's scope selection, routing, checks, deduplication, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and read only the routed owner skills needed for the changed files.
- Prefer the repository's codebase-memory graph tools for code discovery and call tracing; use text search for literals, configs, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise execute the routed dimensions sequentially.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
