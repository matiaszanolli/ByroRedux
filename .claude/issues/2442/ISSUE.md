# NIFAL-D2-04: A raw-tier AnimationClip shares its name with the canonical one while nifal.md asserts 'no parallel struct'

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2442
**Finding ID**: NIFAL-D2-04 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 2 — NIFAL mapping shape
**Location**: `crates/nif/src/anim/types.rs:183` (raw) vs `crates/core/src/animation/types.rs:186` (canonical); claim at `docs/engine/nifal.md:242-244`
**Status**: NEW

## Description
nifal.md states "no parallel struct" for `AnimationClip`; `byroredux_nif::anim::AnimationClip` (raw tier, tier-model-permitted) does exist and is correctly type-qualified at all call sites — the defect is only that the doc's phrasing denies it, making a grep-based single-producer check ambiguous.

## Evidence
Confirmed directly: both `crates/nif/src/anim/types.rs:183` and `crates/core/src/animation/types.rs:186` declare `pub struct AnimationClip { pub name: String, pub duration: f32, pub cycle_type: CycleType, ... }`.

## Impact
None at runtime; costs audit precision.

## Suggested Fix
Reword nifal.md, or rename the raw type to `ImportedAnimationClip` matching the rest of the `Imported*` convention.

## Completeness Checks
- [ ] **TESTS**: N/A unless renamed — if renamed, all call sites across `crates/nif`/`byroredux` still compile
