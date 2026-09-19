---
name: source-command-audit-nifal-skill
description: Audit NIFAL, ByroRedux's NIF Abstraction Layer — the canonical translation tier where per-game Imported* data folds into Canonical through a single translate() boundary, audited via the single-boundary, no-fabrication, no-leak, and no-render-time-fallback invariants. Use when the user requests a NIFAL or canonical translation-layer audit.
---

# NIFAL Audit

Use the Claude Code command as the canonical workflow instead of maintaining a duplicated snapshot.

## Load the Canonical Command

1. Read `.claude/commands/audit-nifal/SKILL.md` completely.
2. Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` completely before auditing.
3. Read the live spec `docs/engine/nifal.md` (plus `docs/engine/material-abstraction.md`) as the canonical directs, then follow its four tier invariants, risk order, documented-limitation ledger, `_audit-validate.sh` gate, and Per-Category Tier Matrix report format.

## Adapt Claude Conventions to Codex

- Treat the user's text after the skill request as the command arguments described by the canonical skill's `argument-hint`.
- Resolve each `/audit-<name>` reference to that audit's `SKILL.md` under `.claude/commands/`, and load a sibling audit only at the declared scope seams — parse-side byte errors → nif, mesh-water/env translation → exterior, `GpuMaterial` layout → renderer, `Material` save shape → save, BGSM/BGEM/CDB decode → parsers.
- Prefer the repository's codebase-memory graph tools for construction-site caller counts, consumer tracing, and impact analysis; use text search for `Option` resolve-later fields, raw discriminator fields, per-game `if game` branches, shader text, docs, and graph gaps.
- Interpret Claude `Task`-agent instructions as Codex sub-agent delegation only when delegation is available and allowed. Otherwise audit one dimension at a time.
- Treat parked and documented-limitation ledger items as regression guards, not new findings, and where the canonical records known guard holes open the test and confirm a real assertion before trusting a green guard.
- Preserve the canonical output path and do not create GitHub issues; publishing remains a separate audit-publish action.

Do not copy the canonical command body into this skill. The Claude command is the single source of truth.
