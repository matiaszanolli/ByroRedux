---
name: source-command-audit-owners
description: The ByroRedux audit ownership map — path prefix to owning audit(s) and risk floor, used for incremental routing, suite coverage, and validator gap detection. Use when routing a changed file to its audit, checking audit coverage of a path, or editing ownership rows.
---

# Audit Ownership Map — ByroRedux

Use the Claude Code reference file as the canonical source instead of maintaining a duplicated snapshot.

## Load the Canonical File

1. Read `.claude/commands/_audit-owners.md` completely.
2. Apply the path-prefix → owner audit → risk-floor table (first matching row wins) exactly as written there.

## Adapt Claude Conventions to Codex

- This is a shared reference file, not a runnable command (leading `_`): load it from an audit skill, never run it on its own.
- Paths inside it are repository paths under `.claude/commands/`; do not rewrite them to another tool's directory.

Do not copy the canonical file body into this skill. The Claude file is the single source of truth.
