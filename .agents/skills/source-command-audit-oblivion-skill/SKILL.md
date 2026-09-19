---
name: source-command-audit-oblivion-skill
description: Audit Oblivion (TES4) per-game compatibility — NIF v20.0.0.4 plus the v10.x NetImmerse tail, BSA v103, the live ESM path, ObScript quests, and the MenuXml HUD against real game data. Use when the user requests an Oblivion or TES4 per-game compatibility audit.
---

# Oblivion Compatibility Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-oblivion/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's ownership routing through `_audit-owners.md`, corpus-lane-first gating, delta scoping, dimension agents, deduplication, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and route shared-mechanism defects to their owner audit (`/audit-nif`, `/audit-esm`, `/audit-renderer`, `/audit-exterior`, `/audit-scripting`, `/audit-ui`), keeping only Oblivion-specific data slices here.
- Prefer the repository's codebase-memory graph tools for callers, pin/test locations, and impact analysis; use text search for version bands, header quirks, census figures, configs, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise execute the routed dimensions sequentially.
- Run the release-build NIF corpus lane before reading source and treat a red gate as the headline finding; keep heavy `--ignored` ESM suites to the named cheap tests, never the whole plugin crate.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
