---
name: source-command-audit-tooling-skill
description: Audit ByroRedux tooling and host-facing contracts — the SDK (Studio snapshots, typed commands, StorageUtil compat), debug server, debug protocol and byro-dbg, debug-ui panels, launcher boot/settings handoff, install detection, byro-detect, and texture-upscale. Use when the user requests a tooling, SDK, debug-server, launcher, or host-contract audit.
---

# Tooling and Host-Contract Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-tooling/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's area routing and delta-first scoping, tooling-crate test setup, per-dimension trust-boundary checklists with the Exposure field, and Exposure-Matrix report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a routed sibling audit (safety, concurrency, renderer, parsers, save) only when the boundary it owns needs its deeper checklist.
- Prefer the repository's codebase-memory graph tools for command-dispatch and host-call tracing; use text search for bind, `fs::write`, wire, capability, and version-literal checks.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
