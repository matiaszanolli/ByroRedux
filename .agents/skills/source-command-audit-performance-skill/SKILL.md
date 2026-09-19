---
name: source-command-audit-performance-skill
description: Audit ByroRedux GPU/CPU performance — CPU hot paths, per-frame allocation churn, draw batching and instancing, GPU memory pressure and eviction thrash, SSBO sizing and upload cost, pass cost, streaming stalls, and telemetry. Use when the user requests a performance, frame-time, hot-path, GPU-cost, or bench audit.
---

# Performance Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-performance/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's parameter parsing and bench discipline, dimension order with their regression guards, pass-inventory verification, merge phase, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit only when a dimension hands the contract to it (renderer layout/scratch, NIFAL material boundary).
- Prefer the repository's codebase-memory graph tools for hot-path call tracing and guard-symbol discovery; use text search for allocations, bench scripts, timer brackets, literals, configs, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise execute the dimensions sequentially.
- Keep every cost claim measurement-backed per the canonical bench discipline: cite a timer or an observed-vs-ROADMAP delta inside the noise floor, state confidence on speculative Vulkan findings, and never assert harness byte-stability from memory.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
