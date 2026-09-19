---
name: source-command-audit-scripting-skill
description: Deep audit of the M30/M47 scripting runtime — the AST-to-ECS recognizer chain (decline-on-unmodeled), fragment and quest-stage dispatch, ECS event/timer/condition/trigger systems, cell-loader script attach, scene/package/cinematic playback, legacy ObScript quest execution, and the SKSE/JContainers provider-compat layer. Use when the user requests a scripting-runtime, recognizer, fragment-dispatch, event-runtime, ObScript, or provider/extender-compat audit.
---

# Scripting Runtime Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-scripting/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's setup and test gates, the decline-invariant focus of its seven dimensions, its Untrusted-Input finding fields and severity escalations, and the merge/cleanup report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load `audit-papyrus` for the compiler frontends (the `.pex` decompiler and `.psc` parser) — this audit keeps only the runtime side per the canonical's split.
- Prefer the repository's codebase-memory graph tools for recognizer, lock-order, and caller tracing; use text search for effect primitives, marker drains, guard-test source scans, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise execute the routed dimensions sequentially.
- Treat documented declines, known-open issues, and phase-2 residuals named in the canonical as context to cite and re-verify, not new findings.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
