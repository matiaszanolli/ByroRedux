---
name: source-command-audit-gameplay-skill
description: Audit ByroRedux's gameplay layer in byroredux/src — inventory/equipment, interaction/locks/loot persistence, consumables, combat + death, NPC spawn → AI packages → locomotion, player feedback, gameplay-state save coverage and stage order. Use when the user requests a gameplay, combat, inventory, or AI-package audit.
---

# Gameplay Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-gameplay/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's delta-first dimension selection, per-dimension paths and guards, ground-truth doc and smoke-gate re-verification, known-open register, deduplication, and invariant-matrix report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit only when a finding lands outside the gameplay layer (CHARAL formulas → character, save schema → save, lock order → ecs + concurrency, HUD rendering → ui, KCC/ragdoll → physics, ALCH/PACK decoders → esm, Papyrus → scripting); this skill owns what gameplay state should be restored.
- Prefer the repository's codebase-memory graph tools for caller tracing, per-actor runtime-component inventories, and written-never-read checks; use text search for stage-order comments, `save_io` registry rows, guard test names, and determinism greps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Treat the canonical known-open register as cite-don't-refile context, and never launch a second engine beside the user's.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
