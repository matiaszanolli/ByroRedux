---
name: source-command-audit-renderer-skill
description: Deep audit of the ByroRedux Vulkan renderer — pipeline, synchronization, GPU memory, shaders, ray tracing, GPU-struct layout, denoiser/resolve chain, and per-feature passes. Use when the user requests a renderer, Vulkan, shader, ray-tracing, or denoiser audit.
---

# Renderer Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-renderer/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's delta-scoped dimensions, guard-first verification, symbol-anchored evidence rules, per-dimension outputs with merge and dedup, and report format; audit against `docs/engine/shader-pipeline.md` and `docs/engine/memory-budget.md` rather than restating them.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit only at its declared scope boundary (exterior pass content, the NIFAL material boundary, performance pool policy) instead of duplicating its checklist.
- Prefer the repository's codebase-memory graph tools for symbol discovery, callers, resource lifetimes, and impact analysis; use text search for shader text, `#[repr(C)]` structs, barrier and flag literals, configs, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit the dimensions sequentially in risk order.
- Keep Vulkan claims that `cargo test` cannot see explicitly labeled needs-RenderDoc (or a `BYRO_VALIDATION=1` sync-validation run), as the canonical command requires.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
