title:	LOW-1: audit-skyrim skill's Dimension-3 checklist still misstates the upperbody.nif pre-scan as applying to Skyrim+
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	documentation, legacy-compat, low
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2324
--
**Severity**: LOW
**Location**: `.claude/commands/audit-skyrim/SKILL.md:137`

## Description

`humanoid_body_paths` returns `&[]` for `GameKind::Skyrim | Fallout4 |
Fallout76 | Starfield` — the `upperbody.nif` pre-scan mechanism the
Dimension-3 checklist describes is real, but exclusively for the kf-era
`Oblivion | Fallout3NV` arm. Skyrim's actual body-coverage mechanism is the
race-default-skin fallback + post-loop occupancy filter (#2093/#2094). The
underlying fact was noted narratively in the 2026-07-16 audit report, but
the skill file's checklist line was never corrected, so every subsequent
Dimension-3 pass re-derives the same clarification from scratch.

## Evidence

```rust
// byroredux/src/asset_provider (humanoid_body_paths)
GameKind::Oblivion | GameKind::Fallout3NV => &[
    r"meshes\characters\_male\upperbody.nif",
    r"meshes\characters\_male\lefthand.nif",
    r"meshes\characters\_male\righthand.nif",
],
GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => &[],
```

`.claude/commands/audit-skyrim/SKILL.md:137` still reads: "Skyrim+
`resolve_armor_mesh` walks ARMO → ARMA → worn-mesh; body-slot armor pre-scan
skips `upperbody.nif` to kill z-fight + double bone-palette overhead." —
implying the pre-scan applies to Skyrim+, which it does not.

## Impact

Documentation drift only — costs each future Dimension-3 audit pass time
re-deriving the same clarification.

## Suggested Fix

Reword the checklist line to point at the actual Skyrim+ mechanism
(race-default-skin fallback + post-loop occupancy filter, #2093/#2094)
instead of the kf-era `upperbody.nif` pre-scan.

## Completeness Checks
- [ ] **SIBLING**: Scan other per-game audit-skill checklist files for the same kf-era-mechanism-attributed-to-Skyrim+ pattern

