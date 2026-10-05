---
name: source-command-audit-common
description: Shared audit protocol loaded by every ByroRedux audit — layout traps, reference docs, game data locations, delta-first scoping, path/symbol validation, deduplication, finding format, and report finalization. Use when running any audit skill or when the user asks for the shared audit protocol.
---

# Shared Audit Protocol — ByroRedux

Use the Claude Code reference file as the canonical source instead of maintaining a duplicated snapshot.

## Load the Canonical File

1. Read `.claude/commands/_audit-common.md` completely.
2. Apply the shared protocol (layout traps, delta-first scoping, dedup, finding format, report finalization) exactly as written there.

## Adapt Claude Conventions to Codex

- This is a shared reference file, not a runnable command (leading `_`): load it from an audit skill, never run it on its own.
- Paths inside it are repository paths under `.claude/commands/`; do not rewrite them to another tool's directory.

Do not copy the canonical file body into this skill. The Claude file is the single source of truth.
