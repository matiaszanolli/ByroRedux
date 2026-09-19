---
name: source-command-audit-speedtree-skill
description: Audit the SpeedTree (.spt) TLV walker in crates/spt, the placeholder-billboard import, and the TREE-to-billboard and wind wiring for Oblivion, FO3, and FNV. Use when the user requests a SpeedTree, .spt, tree billboard, or tree wiring audit.
---

# SpeedTree Subsystem Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-speedtree/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's settled-fact premises, Phase 1 setup (default plus recon feature lanes and the deep-only corpus harness), the six risk-ordered dimensions with the per-finding `Dimension` field, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and route Skyrim-plus BSTreeNode trees to `audit-nif`, distant-LOD and real-tree-geometry design to `audit-exterior`, and NIFAL single-boundary material findings to `audit-nifal`.
- Prefer the repository's codebase-memory graph tools for TREE wiring and import-path tracing; use text search for tag numbers, `.spt` literals, guard test names, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed; the canonical architecture is single-pass, so run all dimensions inline in its risk order.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
