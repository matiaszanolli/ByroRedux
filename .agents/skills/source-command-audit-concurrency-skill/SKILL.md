---
name: source-command-audit-concurrency-skill
description: Audit ByroRedux concurrency and synchronization — Vulkan queue and acceleration-structure sync, compute-to-AS-to-fragment barrier chains, ECS lock ordering and deadlock potential, scheduler access declarations, RwLock patterns, GPU resource teardown, and worker threads. Use when the user requests a concurrency, synchronization, deadlock, lock-ordering, or Vulkan-sync audit.
---

# Concurrency and Synchronization Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-concurrency/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's dimension order by blast radius, delta-first skipping of unchanged dimensions, deduplication, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and hand off what the canonical delegates: lock-tracker internals and the access-model guard to `/audit-ecs`, ground-cover/sky pipeline semantics to `/audit-exterior`, sandbox trust-boundary questions to `/audit-safety`, and the debug-server command surface to `/audit-tooling`.
- Apply the speculative-fix guardrail to Vulkan-sync findings: barrier bugs are invisible to `cargo test`, so a finding stays a HYPOTHESIS until a `BYRO_VALIDATION` or RenderDoc capture confirms it.
- Prefer the repository's codebase-memory graph tools for lock acquisition sites, scheduler access declarations, and worker/data-flow discovery; use text search for barrier and stage-mask literals, `Mutex`/`RwLock` patterns, `thread::spawn`/rayon sites, shader and compute files, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
