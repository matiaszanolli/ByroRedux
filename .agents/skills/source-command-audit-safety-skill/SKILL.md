---
name: source-command-audit-safety-skill
description: Audit ByroRedux for unsafe-code violations, undefined behavior, FFI lifetime bugs, memory and resource leaks, Vulkan specification errors, GPU layout drift, teardown hazards, and sandbox trust-boundary failures. Use when the user requests a safety, memory-safety, Vulkan-safety, or unsafe-code audit.
---

# Safety Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-safety/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's dimension order, regression guards, evidence rules, deduplication, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as any requested audit focus or depth.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit only when cross-referencing its contract is necessary.
- Prefer codebase-memory graph tools for symbol discovery, callers, resource lifetimes, and impact analysis. Use text search for `unsafe` blocks, safety comments, literals, configs, shader text, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Keep unverified Vulkan claims explicitly labeled as requiring validation-layer or RenderDoc evidence, as required by the canonical command.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
