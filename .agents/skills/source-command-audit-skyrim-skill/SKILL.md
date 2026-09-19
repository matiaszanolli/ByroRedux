---
name: source-command-audit-skyrim-skill
description: Per-game audit of Skyrim Special Edition and Skyrim LE compatibility — BSTriShape packed geometry and SSE skinned reconstruction, BSLightingShaderProperty shader-type dispatch, NPC equip and FaceGen, multi-master load order, BSA v104/v105 archives, and corpus gates. Use when the user requests a Skyrim compatibility, SE/LE per-game, or Skyrim regression audit.
---

# Skyrim Compatibility Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-skyrim/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's guard ledger and `#[ignore]`d real-data tests, its five risk-ordered dimensions, the control-bench regression framing, and the merge/publish report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and defer mechanism defects to their owner audits (`audit-nif`, `audit-nifal`, `audit-esm`, `audit-parsers`, `audit-gameplay`) — this audit owns only Skyrim's data through the shared mechanisms.
- Prefer the repository's codebase-memory graph tools for block-dispatch and import-path tracing; use text search for block names, shader-type numbers, guard tests, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Take live parse-rate and bench numbers from a fresh harness run or ROADMAP's compat matrix, never from the canonical text.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
