---
name: source-command-audit-fo4-skill
description: Audit Fallout 4 compatibility — BA2 archives, half-float vertices, BGSM/BGEM materials, and M49 CSG precombines. Use when the user requests a Fallout 4 or FO4 compatibility audit.
---

# Fallout 4 Compatibility Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-fo4/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's `--focus` dimension selection, guard-liveness checks, forward-scope rules, deduplication, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and defer BA2/CSG/BGSM reader discipline, NIF block parsing, the ESM walker, NIFAL invariants, the Scaleform HUD, and runtime telemetry to the owner audit the canonical names, keeping this audit on FO4's data through the shared mechanisms.
- Prefer the repository's codebase-memory graph tools for guard-test locations, callers, and impact analysis; use text search for BGSM field names, BSVER and shader-flag literals, `#[ignore]`d guards, configs, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit the dimensions one at a time in the canonical's risk order.
- Treat everything the canonical marks as shipped as regression guards rather than pending proposals, and keep forward-scope items out of blockers.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
