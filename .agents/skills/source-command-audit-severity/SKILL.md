---
name: source-command-audit-severity
description: Unified severity scale for ByroRedux audit findings — CRITICAL/HIGH/MEDIUM/LOW definitions, special rules, and the decision tree. Use when assigning or reviewing the severity of an audit finding.
---

# Unified Severity Definitions — ByroRedux

Use the Claude Code reference file as the canonical source instead of maintaining a duplicated snapshot.

## Load the Canonical File

1. Read `.claude/commands/_audit-severity.md` completely.
2. Apply the severity definitions, special-rule table and decision tree exactly as written there.

## Adapt Claude Conventions to Codex

- This is a shared reference file, not a runnable command (leading `_`): load it from an audit skill, never run it on its own.
- Paths inside it are repository paths under `.claude/commands/`; do not rewrite them to another tool's directory.

Do not copy the canonical file body into this skill. The Claude file is the single source of truth.
