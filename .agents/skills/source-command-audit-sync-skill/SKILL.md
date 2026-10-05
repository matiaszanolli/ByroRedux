---
name: source-command-audit-sync-skill
description: Re-sync every ByroRedux audit skill (.claude/commands/audit-*/SKILL.md and the shared _audit-* files) against the code landed since the last sync — closed-issue sweep, delta read, premise re-measure, ownership-map fold-in. Use when the user asks to update, refresh, or re-sync the audit skills.
---

# Audit-Skill Sync

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-sync/SKILL.md` completely.
2. Run its `prep.sh` exactly as described; it renders `brief.tmpl` into the scratch directory and validates `groups.txt`.
3. Follow its five phases: prep, fan-out by `groups.txt`, collect proposals, verify (validator + skill-reading Rust tests), close (update `BASELINE`, report gaps and fixed-but-open issues).

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Interpret Claude background agent fan-out as Codex sub-agent delegation, one per `groups.txt` line (max 3 concurrent); without delegation, work through the groups sequentially using the rendered brief.
- Do not commit unless the user asks.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
