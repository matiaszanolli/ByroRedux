---
name: source-command-audit-publish-skill
description: Convert a finished audit report's findings into GitHub issues with path validation, NEW-status filtering, dedup against open issues, label reconciliation, and a completeness gate. Use when the user asks to publish an audit report or convert or file its findings as GitHub issues.
---

# Audit → GitHub Issues Publisher

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-publish/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before publishing.
3. Follow the canonical command's report parsing, path-validation gate, status filter, per-finding validation, dedup, label reconciliation, issue creation, snapshot, and completeness-gate steps.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint` (the path to the audit report).
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, treating them as pointers that travel into issue bodies — publishing validates findings itself and does not run sibling audits; treat `/fix-issue` mentions as user-facing next steps only.
- Prefer the repository's codebase-memory graph tools for re-mapping moved `Location:` paths to their current submodules; use text search for the canonical `grep -rn` symbol checks against current code.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise execute the publish steps in canonical order.
- Creating the GitHub issues is this skill's deliverable: file every CONFIRMED NEW finding via `gh issue create`, while keeping the canonical status filter, open-issue dedup, label reconciliation against the live repo, and the terminal completeness gate so no finding is double-filed or silently skipped.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
