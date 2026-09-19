---
name: source-command-audit-fnv-skill
description: Audit Fallout New Vegas compatibility — the reference title covering cell load, ESM/NIF data, ragdoll, ambient AI, consumables, and HUD profile. Use when the user requests a Fallout New Vegas or FNV compatibility audit.
---

# Fallout New Vegas Compatibility Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-fnv/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Read `.claude/commands/_audit-owners.md` for the shared-mechanism routing, then follow the canonical command's `--focus` dimension selection, delta scoping, deduplication, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and file shared-mechanism defects against the owner audit named in `_audit-owners.md` instead of re-auditing the mechanism on FNV data.
- Prefer the repository's codebase-memory graph tools for guard-test locations, callers, and impact analysis; use text search for per-game literals, `GameKind` branches, baselines, configs, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit the dimensions one at a time in the canonical's risk order.
- Pull baselines from the ROADMAP row and the authoring census instead of hardcoding counts, and run the `--ignored` real-data lanes before flagging coverage gaps.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
