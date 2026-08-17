# REN-DOC-01: three audit-renderer SKILL.md items describe deleted code

**Issue**: #3046
**Severity**: LOW
**Labels**: `low,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_RENDERER_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RENDERER_2026-08-16.md`.

**Location**: `.claude/commands/audit-renderer/SKILL.md` — Dimension 2
**Status note**: NEW for the Dim-2 item; the Dim-17/18 `isInteriorFill` items are a related, already-known drift.

## Description

Three `audit-renderer/SKILL.md` checklist items describe code **deleted in the last four days**. An auditor following the skill is directed to inspect symbols that no longer exist.

## Impact

The skill is the checklist a renderer audit runs against. Items pointing at deleted code cost triage time on every run and, worse, can read as "this was verified" when nothing was inspected.

Same class as #3035 (the `audit-ecs` count drift) and #2974 (the tech-debt Dim-1 recipe) — audit infrastructure drifting behind the code it measures, which this sweep found in three separate skills.

## Suggested Fix

Remove or re-point the three Dimension 2 items. Run `.claude/commands/_audit-validate.sh` afterwards — its backticked-symbol advisory exists to catch exactly this and would have flagged them.

## Related

- #3035 (ECS-2026-08-16-07), #2974 (TD4-2026-08-16-01) — same class, other skills
- The Dim-17/18 `isInteriorFill` drift (known, not re-filed)

## Completeness Checks
- [ ] **ALL-THREE**: Every stale Dim-2 item corrected
- [ ] **ADVISORY-CLEAN**: `_audit-validate.sh`'s backticked-symbol advisory no longer lists them
- [ ] **SIBLING**: The Dim-17/18 `isInteriorFill` items resolved in the same pass
- [ ] **PATH-GATE**: `_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3046 --json state` when live state is needed.*
