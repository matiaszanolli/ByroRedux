# LOW-1: audit-skyrim skill's Dimension-3 checklist still misstates the upperbody.nif pre-scan as applying to Skyrim+

**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-08-03.md` (Dimension 3)
**GitHub issue**: #2324

**Severity**: LOW
**Location**: `.claude/commands/audit-skyrim/SKILL.md:137`

## Description

`humanoid_body_paths` returns `&[]` for `GameKind::Skyrim | Fallout4 |
Fallout76 | Starfield` — the `upperbody.nif` pre-scan mechanism the
Dimension-3 checklist describes is real, but exclusively for the kf-era
`Oblivion | Fallout3NV` arm. Skyrim's actual body-coverage mechanism is the
race-default-skin fallback + post-loop occupancy filter (#2093/#2094).

## Evidence

Confirmed at HEAD (1ae86f62): `humanoid_body_paths` match arm for
`Skyrim | Fallout4 | Fallout76 | Starfield` returns `&[]`; the checklist
line at `.claude/commands/audit-skyrim/SKILL.md:137` still implies the
pre-scan applies to Skyrim+.

## Impact

Documentation drift only — costs each future Dimension-3 audit pass time
re-deriving the same clarification.

## Suggested Fix

Reword the checklist line to point at the actual Skyrim+ mechanism
(race-default-skin fallback + post-loop occupancy filter, #2093/#2094).

## Completeness Checks
- [ ] **SIBLING**: Scan other per-game audit-skill checklist files for the same kf-era-mechanism-attributed-to-Skyrim+ pattern
