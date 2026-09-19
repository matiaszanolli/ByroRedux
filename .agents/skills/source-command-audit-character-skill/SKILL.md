---
name: source-command-audit-character-skill
description: Audit CHARAL in crates/core/src/character — the per-game character ruleset translation into canonical ActorValues, Level, and Perks, covering derived-stat formulas, leveling models, skill/attribute rosters, regen, affliction, and reputation systems, plus the parse-to-actor population boundary. Use when the user requests a character, CHARAL, ActorValue, derived-stat, leveling, or population-boundary audit.
---

# Character / CHARAL Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-character/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's capture-first ground truth, delta-first scoping, known-open register (verify, never re-file), deduplication, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and route per the canonical's cross-audit routing: component shape to `/audit-ecs`, AVIF/CLAS/NPC_ parsing to `/audit-esm`, CTDA evaluation to `/audit-scripting`, scheduler access to `/audit-concurrency`, gameplay writers to `/audit-gameplay`, and ActorValues save schema to `/audit-save`.
- Read the `charal-*` capture documents before any Rust constant; a numeric finding without a capture-line Source is not reportable.
- Prefer the repository's codebase-memory graph tools for ruleset construction sites, derived-stat consumers, and population-boundary call chains; use text search for `GameKind` branches, roster FormIDs, capture-document terms, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
