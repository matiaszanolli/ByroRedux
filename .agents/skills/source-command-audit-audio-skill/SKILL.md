---
name: source-command-audit-audio-skill
description: Audit the ByroRedux M44 audio subsystem — the kira backend in crates/audio (spatial dispatch and the Bethesda-unit listener seam, reverb send, underwater filter, streaming music, SoundCache, ECS lifecycle) plus its engine callers, footsteps, water audio, reverb zones, and REGN ambient music. Use when the user requests an audio, kira, spatial-sound, reverb, underwater, or music-dispatch audit.
---

# Audio Subsystem Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-audio/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Follow the canonical command's delta-first scoping, regression guards, evidence rules (flag unsourced constants, never invent a rationale), deduplication, and report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit (`/audit-physics`, `/audit-ecs`, `/audit-concurrency`) only at the canonical's handoff boundaries — water submersion state, stage/lock shape — reporting audio-specific ordering only.
- Prefer the repository's codebase-memory graph tools for tracing audio-API callers and the footstep/water/reverb data flow; use text search for `play_oneshot`/`play_music`/`set_underwater` call sites, guard test names, docs, and graph gaps.
- Disclose the canonical's `#[ignore]`d audio-device lifecycle guards: run them with `-- --ignored` or state that they were skipped.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
