---
name: source-command-audit-papyrus-skill
description: Deep audit of the Papyrus frontends — the .pex reader and 5-phase decompiler (crates/pex, Champollion port) and the .psc lexer and Pratt parser (crates/papyrus) — for untrusted-input bounds, decompiler soundness, and depth caps. Use when the user requests a Papyrus, .pex, or .psc frontend audit.
---

# Papyrus Frontend Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-papyrus/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's delta-first scoping, untrusted-input severity escalations, per-dimension guard verification, and the report's untrusted-input and decompile-rate verdicts.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and respect the boundary the canonical states: papyrus owns the `.pex`/`.psc` frontends up to the AST, while `/audit-scripting` owns recognizers and the runtime (`translate_pex`, the panic net, the extender preflight) and `/audit-esm` owns VMAD decode.
- Prefer the repository's codebase-memory graph tools for callers, guard-pin locations, and consumer impact analysis; use text search for bounds checks, allocation sites, depth constants, guard names, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Keep the canonical's escalation in force: any panic, OOB, unbounded allocation or recursion reachable from raw `.pex`/`.psc` bytes is at least HIGH, and so is a wrong AST a recognizer accepts.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
