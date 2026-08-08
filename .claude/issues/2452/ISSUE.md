# EXAL-04: The 'prebaked combined-LOD games' predicate is duplicated inline in two providers instead of one named GameKind decision

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2452
**Finding ID**: EXAL-04 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 5 — EXAL
**Location**: `object_lod.rs:164-169`, `terrain_lod.rs:367` (inline `matches!`), vs `placement_lod.rs:306-308` (named `placement_lod_supported`)
**Status**: NEW

## Description
exal.md §4 requires one `GameKind`-keyed decision per quirk; the `.bto`/`.btr` "baked combined LOD" quirk is written twice as an identical inline `matches!(..., GameKind::Skyrim | GameKind::Fallout4)` literal instead of a shared named predicate, unlike the sibling `placement_lod_supported`.

## Suggested Fix
Add `baked_lod_supported(game: GameKind)` next to `placement_lod_supported`, call from both sites, with the same per-variant unit test.

## Completeness Checks
- [ ] **TESTS**: Per-variant unit test mirroring `placement_lod_supported_is_oblivion_only`
