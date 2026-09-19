---
name: source-command-audit-parsers-skill
description: Parser-discipline audit of the non-NIF, non-ESM readers of untrusted game input — BSA/BA2/CSG archives, BGSM/BGEM, Starfield CDB, Havok HKX packfiles, FaceGen sidecars, MenuXml parsing, and Steam VDF/ACF. Use when the user requests a parser-discipline, archive-reader, or untrusted-format audit.
---

# Parser Discipline Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-parsers/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's reader contract (size bounded before allocation, contextual `Err`, no silent short decode, non-vacuous corpus gates), delta-first scoping, six-dimension sweep, and gate-matrix report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and keep the ownership split the canonical states: parsers owns the non-NIF/non-ESM readers, while NIF routes to `/audit-nif`, ESM to `/audit-esm`, `.pex`/`.psc` to `/audit-papyrus`, `.spt` to `/audit-speedtree`, HKX playback to `/audit-scripting`, and MenuXml rendering to `/audit-ui`.
- Prefer the repository's codebase-memory graph tools for callers, guard locations, and decode-to-consumer wiring; use text search for bounds checks, allocation sites, version gates, guard names, configs, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise execute the routed dimensions sequentially.
- Run `--ignored` real-data suites only with game data present under the strict lane, and never materialise the vanilla Starfield CDB through the full parse (stream it, per the canonical's Phase 1 warning).
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
