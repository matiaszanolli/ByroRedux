---
name: source-command-audit-save-skill
description: Deep audit of the M45 save/load subsystem — full-ECS-snapshot capture, type-erased SaveRegistry, FORMAT_MAJOR schema discipline, atomic disk writes and the quicksave ring, save-side refusal and validation gates, and the M45.1 live load-apply (cell reload, FormId-keyed deltas, player-pose restore). Use when the user requests a save/load, snapshot, save-corruption, quicksave-ring, or load-apply audit.
---

# Save / Load Subsystem Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-save/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's guard ledger, delta-first setup, five dimensions, Data-Loss Class finding fields, and merge/cleanup report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and route gameplay persistence semantics to `audit-gameplay` and the SDK extension API to `audit-tooling` per the canonical's ownership split.
- Prefer the repository's codebase-memory graph tools for registry, driver, and caller tracing; use text search for `register_` sites, serde derives, guard-test source scans, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise execute the routed dimensions sequentially.
- Treat the guard ledger's allowlist reasons and the prior report's fixed findings as claims to re-verify, not settled truth.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
