---
name: source-command-audit-runtime-skill
description: Audit ByroRedux runtime telemetry by driving a headless engine on per-game cells, diffing stats against checked-in baselines, and reporting the state of the smoke / golden-frame gate matrix. Use when the user requests a runtime telemetry, capture, or gate-matrix audit.
---

# Runtime Telemetry Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-runtime/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's gate matrix, `bench_mode`-first diff, severity and advisory rules, harness/baseline-integrity checks, and report format — this audit is live, not static: it drives the engine headless on on-disk game data via `.claude/commands/audit-runtime/capture.sh` and diffs against the checked-in baselines under `.claude/audit-baselines/runtime/`.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit only when a finding's resolution chain routes there (e.g. `/audit-nifal` after a `tex_missing_base_color` bump).
- Prefer the repository's codebase-memory graph tools for symbol discovery and metric-source location; use text search for metric keys, log-line formats, capture scripts, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise run captures and gates serially, exactly as the canonical command prescribes.
- Advisory metrics are never findings, and a SKIP is never folded into a pass; preserve the canonical output path and do not create GitHub issues — publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
