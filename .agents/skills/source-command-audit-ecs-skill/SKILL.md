---
name: source-command-audit-ecs-skill
description: Audit ByroRedux's ECS: storage backends, queries and world access, lock ordering, resources, scheduler wiring and declared access, change tracking, hot-path invariants, component lifecycles, and the animation runtime, spanning crates/core/src/ecs and byroredux boot registration. Use when the user requests an ECS, byroredux-core, scheduler, or storage audit.
---

# ECS Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-ecs/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's blast-radius dimension order, per-dimension `First step:` scope triage, `_audit-owners.md` ownership routing, live-guard verification, pinned regression invariants, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as any requested audit focus or depth.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and keep the canonical's ownership split: gameplay-system logic routes to `/audit-gameplay`, while this audit keeps only ECS shape (storage class, lifecycle, declared access).
- Prefer the repository's codebase-memory graph tools for component/system discovery, callers, and access-pair impact analysis; use text search for `type Storage =` declarations, `scheduler.add_*` registrations, turbofish query/resource call shapes, `#[ignore]` attributes, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise run the dimensions sequentially in the canonical's blast-radius order.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
