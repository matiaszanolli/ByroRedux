---
name: source-command-audit-suite-skill
description: Orchestrate a preset suite of ByroRedux audits in parallel, or derive one from a code area or git diff via the ownership map (renderer, nif, esm, per-game, safety, and more). Use when the user requests an audit suite, a preset sweep, or audit coverage of a whole code area or change set.
---

# Audit Suite Orchestrator

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-suite/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's three modes (`--preset`, `--area`, `--changed` routed through `_audit-owners.md` and `_audit-route.sh`), its launch rules including the orchestration hazard (synchronous dimension analysis or `/tmp/audit/<name>/dim_N.md` scratch files, verified against each written report), report verification, and the summary-table merge format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and dispatch each routed audit by reading its owner `SKILL.md`, adding the owner's neighbor audits exactly as the canonical tables define them.
- Derive audit sets with `git ls-files` / `git diff --name-only` piped through `_audit-route.sh`; use codebase-memory or text search only to resolve an unclear owner or path.
- Interpret Claude background `Task` fan-out as Codex sub-agent delegation — the suite's normal mode, max 3 concurrent; without delegation, run the routed audits sequentially and merge their reports as usual.
- Preserve each routed audit's canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action and never belongs in a suite.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
