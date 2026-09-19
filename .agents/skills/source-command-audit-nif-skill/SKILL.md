---
name: source-command-audit-nif-skill
description: Audit ByroRedux's NIF parser (crates/nif) for byte-accurate correctness across the Oblivion → Starfield span — stream-position drift, version gating, block dispatch coverage, geometry/import handoff, collision + shader block parsing, allocation hygiene. Use when the user requests a NIF parser or NIF byte-format audit.
---

# NIF Parser Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-nif/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's stream-drift-first dimension order, `docs/engine/nif-parser.md` / nif.xml reference policy, regression pins (verify still fixed, never re-report), opt-in corpus-gate rules, per-finding Dimension + Game Affected fields, and Block Type Coverage Matrix report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit only at the canonical's declared seams — archive readers and previs → parsers, per-game material translation → nifal, SpeedTree `.spt` → speedtree, Havok behaviour → physics (only the constraint CInfo decode is a NIF seam).
- Prefer the repository's codebase-memory graph tools for symbol discovery, callers, and impact analysis; use text search for raw version literals, `bsver` bands, nif.xml `since=`/`until=` gates, `block_sizes` reads, and dispatch/shader arms.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Corpus gates are opt-in `#[ignore]` tests needing installed game data — check the latest per-title gate result before trusting a green default `cargo test -p byroredux-nif` for parse rates.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
