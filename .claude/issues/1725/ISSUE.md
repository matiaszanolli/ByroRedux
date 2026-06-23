# PERF-2026-06-23-01: Player-path animation text-event Vec re-allocates each frame

**Issue**: #1725
**Severity**: LOW
**Labels**: low, performance, bug
**Source audit**: `docs/audits/AUDIT_PERFORMANCE_2026-06-23.md`
**Dimension**: CPU Hot Paths
**Location**: `byroredux/src/systems/animation.rs` (`animation_system_inner`, the AnimationPlayer text-key block, `let mut events: Vec<AnimationTextKeyEvent> = Vec::new();`, ~line 433)

## Description
The `make_animation_system` factory (#1372) captures `entities_scratch`/`playback_scratch` reused across frames; the AnimationStack path's text-event scratches were hoisted to closure scope under #828. The **AnimationPlayer** path's text-event `events` Vec is allocated fresh with `Vec::new()` *inside* `animation_system_inner` each call — `clear()`ed and reused within a frame but dropped at function end and re-grown 0→N next frame. Same regrowth pattern #828/#1372 eliminated elsewhere.

## Evidence
The stack path uses outer-scope reused `events` (#828). The player path (`animation.rs:433`) declares its own `events` Vec scoped to the emit block, not captured by the `make_animation_system` closure.

## Impact
One heap allocation + grow per frame, bounded by the count of distinct text events firing across all AnimationPlayer entities in a frame (typically 0–few). Negligible; flagged for pattern consistency. No dhat coverage for this per-frame site.

## Related
#828, #1372

## Suggested Fix
Thread a third reusable buffer (`text_events_scratch: Vec<AnimationTextKeyEvent>`) through `animation_system_inner` and capture it in `make_animation_system`; the `#[cfg(test)]` `animation_system` wrapper passes a fresh `Vec::new()`.

## Completeness Checks
- [ ] **SIBLING**: Both AnimationPlayer and AnimationStack text-event scratches share the persistence pattern
- [ ] **TESTS**: The `#[cfg(test)]` `animation_system` wrapper threads the new scratch through

## Validation
CONFIRMED against current code (HEAD 2d4c350d): animation.rs:433 fresh `Vec::new()` in AnimationPlayer text-key block; only entities_scratch/playback_scratch are closure-captured (animation.rs:756-759).
