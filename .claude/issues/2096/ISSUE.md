# SKY-D3-NEW-04: audit-skyrim skill mischaracterizes the Dimension-3 skinning-consumer entry point

**Severity**: LOW
**Labels**: low, tech-debt, documentation
**Location**: `.claude/commands/audit-skyrim/SKILL.md:134` (Dimension 3 entry-point list)
**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-07-16.md` (SKY-D3-NEW-04)

## Description
The skill names `byroredux/src/systems/character.rs` as "skinning consumer for heads/bodies." That file is actually the player camera/movement controller; the real skinning consumer is `byroredux/src/render/skinned.rs` (the same SKILL.md file's Dimension 6 entry-point list already correctly names it).

## Impact
Low — misdirects future audit runs to the wrong file.

## Suggested Fix
Update the skill's Dimension 3 entry-point line to `byroredux/src/render/skinned.rs`.

## Completeness Checks
No rows apply — this is a documentation-only fix.
