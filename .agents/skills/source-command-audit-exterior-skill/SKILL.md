---
name: source-command-audit-exterior-skill
description: Audit ByroRedux's exterior layers (EXAL/SKYAL/WATAL and ground cover) for translation-boundary, terrain and splatting, ground-cover pipeline, sky bake with clouds and SH, weather and sun, water translation, distant LOD, and tree regressions against the single-boundary, no-fabrication, no-leak, and no-render-time-fallback invariants. Use when the user requests an exterior, EXAL, SKYAL, WATAL, terrain, ground-cover, sky, weather, water, or LOD audit.
---

# Exterior Audit — EXAL / SKYAL / WATAL / Ground Cover

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-exterior/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's four tier invariants, per-dimension `Paths:` scope triage, live-guard rule, extra per-finding fields (Dimension, Tier Violated, Game Affected), Known-Open Register, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and cross only at the canonical's declared seams: byte-level WTHR/WATR/LTEX/GRAS/LAND decode to `/audit-esm`, water shading and sky consumption to `/audit-renderer`, buoyancy to `/audit-physics`.
- Prefer the repository's codebase-memory graph tools for translation-boundary callers, producer/consumer chains, and impact analysis; use text search for `translate_*` call sites, `GameKind` tokens in Rust and GLSL, sentinel literals, shader text, docs, and graph gaps.
- Interpret the canonical orchestrator (one Task agent per dimension, max 3 concurrent) as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit the dimensions sequentially.
- Keep evidence to cargo tests and the canonical's captured-frame smoke harness; no windowed engine launch (*feedback_no_parallel_engine_launch*).
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
