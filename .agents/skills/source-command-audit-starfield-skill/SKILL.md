---
name: source-command-audit-starfield-skill
description: Audit Starfield per-game compatibility — BA2 v2/v3 with LZ4 block compression, CDB materialsbeta.cdb material lookup, BSGeometry .mesh resolution, and the walkable Cydonia interior. Use when the user requests a Starfield compatibility, per-game Starfield, or Starfield data-flow audit.
---

# Starfield Compatibility Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-starfield/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's owner-audit scope split (this audit owns Starfield's data through shared mechanisms), its measured status authority, the six risk-ordered dimensions with per-dimension scratch outputs, and the Phase 3 merge (CRC32 flag table, remaining-work chain, deduplication) and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and route shared-mechanism defects to their owners: `audit-parsers` (BA2/CDB readers), `audit-nif` (block parsing), `audit-esm` (ESM walker), `audit-nifal` (canonical material invariants), `audit-ui` (HUD), `audit-character` (NPC stat model).
- Prefer the repository's codebase-memory graph tools for symbol discovery, callers, and impact analysis; use text search for guard test names, FourCCs, corpus gate commands, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed; the canonical runs each dimension as an agent (max 3 concurrent), otherwise audit the dimensions one at a time in risk order.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
