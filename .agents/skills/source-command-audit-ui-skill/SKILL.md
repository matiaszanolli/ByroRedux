---
name: source-command-audit-ui-skill
description: Deep audit of the ByroRedux game UI tracks — Scaleform/SWF (Ruffle host bridge, AVM1/AVM2 profiles, ABC adapter injection, catalog and AVM1 scanner, resource navigator, offscreen wgpu readback, overlay upload, input routing) and the Oblivion/FO3/FNV MenuXml track (eval, layout, raster) with both HUD drivers. Use when the user requests a UI, Scaleform, HUD, MenuXml, or Ruffle audit.
---

# UI Audit: Scaleform (R4 + M48) and MenuXml (M48.4-M48.7)

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-ui/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's delta baseline, re-derived catalog counts and data-gated test caveats, per-dimension guards and first steps, and Host-Contract-Matrix report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a routed sibling audit (parsers, concurrency, safety, gameplay) only when the handoff it owns needs its deeper checklist.
- Prefer the repository's codebase-memory graph tools for host-call, guard-symbol, and route discovery; use text search for SWF/ABC bytecode, eval/layout/raster details, Vulkan upload and presentation sites, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Treat data-gated tests that silently return early without game data as unverified unless run with data, per the canonical command.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
