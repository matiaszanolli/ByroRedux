---
name: source-command-audit-fo3-skill
description: Audit Fallout 3 compatibility — NIF v20.2.0.7, BSA v104, and the FNV-shared ESM parser, hunting FO3 divergences from the FNV path. Use when the user requests a Fallout 3 or FO3 compatibility audit.
---

# Fallout 3 Compatibility Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-fo3/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Read `.claude/commands/_audit-owners.md` for the shared-mechanism routing, then follow the canonical command's `--focus` dimension selection, real-data-lane-first order, FNV-shared versus FO3-divergence framing, deduplication, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, file shared-mechanism defects against their owner audit per `_audit-owners.md` while tagging the FO3 reach, and load `/audit-fnv` dimensions when re-verifying a classic-era guard on FO3 data.
- Prefer the repository's codebase-memory graph tools for guard-test locations, callers, and impact analysis; use text search for profile-scoped seams, `GameKind` branches, record literals, baselines, configs, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit the dimensions one at a time in the canonical's order.
- Treat the canonical known-open issue list and measured authoring census as do-not-re-file premises, re-measuring before asserting the opposite.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
