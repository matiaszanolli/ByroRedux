---
name: source-command-audit-esm-skill
description: Audit ByroRedux's ESM/ESP plugin parser: GRUP walk, sub-record byte accounting, per-record schema dispatch, FormID remap and load order, CELL/WRLD walkers, localized strings, EsmIndex-to-ECS handoff, and real-data validation across Oblivion, FO3/FNV, Skyrim, FO4/FO76, and Starfield masters. Use when the user requests an ESM, ESP, plugin parser, FormID, or load-order audit.
---

# ESM / Plugin Parser Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-esm/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's phase structure (setup, eight dimensions, merge, cleanup), per-dimension guard verification, xEdit citation requirements, the Record Coverage Matrix, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit only to route its out-of-scope territory (per-game slices, `/audit-gameplay` consumption, `/audit-exterior` translation), but always deduplicate against the latest per-game audit reports, where this crate's findings historically live.
- Prefer the repository's codebase-memory graph tools for parser call paths, dispatch sites, and consumer-handoff discovery; use text search for 4-char record codes, sub-record literals, `GameKind` arms, xEdit citations in docstrings, docs, and graph gaps.
- Interpret the canonical orchestrator (one Task agent per dimension, max 3 concurrent) as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit the dimensions sequentially.
- Keep the test surface to the canonical's allowed set (plain cargo tests, example probes); never run this crate's `--ignored` real-data tests, which have OOM-killed sessions.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
