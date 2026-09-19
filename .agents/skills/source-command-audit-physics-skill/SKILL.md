---
name: source-command-audit-physics-skill
description: Deep audit of PHYSAL, the Havok→Rapier physics layer — collider shape translation, fixed-step determinism and explosion recovery, the phase-ordered ECS sync tick, ragdoll articulation, the character/NPC kinematic controller, and the WATAL buoyancy sink. Use when the user requests a physics, PHYSAL, ragdoll, character-controller, or buoyancy audit.
---

# Physics / PHYSAL Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-physics/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's dimension order, ground-truth doc reading, guard verification, known-open register checks, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit only when a finding crosses the canonical handoff boundaries (gameplay behaviour, WATR translation, water shading, `bhk*` parsing, CollisionShape extraction, lock order).
- Prefer the repository's codebase-memory graph tools for callers, phase-order traces, and lock-order analysis; use text search for guard symbols, constants, `GameKind`/`bsver` seams, configs, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Treat the canonical known-open register and closed-fix guards as regression baselines: verify they still hold, never re-file them as new.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
