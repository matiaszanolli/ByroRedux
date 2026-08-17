# FO3-D2-01: /audit-fo3 names BSSegmentedTriShape, but vanilla FO3 authors zero

**Issue**: #3101
**Severity**: LOW
**Labels**: `low,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_FO3_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FO3_2026-08-16.md` (Dimension 2 — divergence surface).

**Location**: `.claude/commands/audit-fo3/SKILL.md`:44 (divergence map), :87 (Dim 2 checklist), :114-117 (Dim 4 checklist)

## Description

`/audit-fo3` names `BSSegmentedTriShape` as an FO3 divergence surface, but **vanilla FO3 authors zero of them**.

## Impact

Three checklist items direct an auditor to inspect a block type that does not occur in the game's shipped content. The auditor either reports it clean (having examined nothing) or spends time establishing the absence — which is what this finding did.

This is the **sixth** audit-skill drift found in this sweep (#2974, #3035, #3046, #3052, #3079, this). The pattern is consistent enough to be worth treating as a class: skill checklists accrete claims that no one re-measures.

## Suggested Fix

Remove or re-scope the three items. If `BSSegmentedTriShape` matters for a *different* game, move the claim there rather than deleting it.

Worth considering the class fix alongside: a periodic pass that re-measures each skill's named divergence surfaces against shipped data, since six instances in one sweep suggests hand-maintained checklists drift faster than they are read.

## Related

- #2974, #3035, #3046, #3052, #3079 — the other five audit-skill drifts this sweep

## Completeness Checks
- [ ] **ALL-THREE**: Items at :44, :87 and :114-117 all corrected
- [ ] **RE-SCOPED**: If the block matters for another game, the claim moves rather than vanishing
- [ ] **MEASURED**: The replacement claim (if any) is measured against shipped FO3 content
- [ ] **PATH-GATE**: `_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3101 --json state` when live state is needed.*
