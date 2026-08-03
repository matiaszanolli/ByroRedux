# NIFAL-D7-02: nifal.md conflates live AnimatedMorphWeights sink with genuinely-parked ambient colour channels

Source: `docs/audits/AUDIT_NIFAL_2026-08-03.md`

**Severity**: LOW
**Dimension**: Animation · **Tier Violated**: (doc)
**Location**: `docs/engine/nifal.md:244-245`
**Status**: NEW

## Description

`docs/engine/nifal.md:244-245` still lumps morph-weight channels in with
genuinely-parked per-light ambient channels ("intentionally parked... no
renderer consumer yet"). Since `a8b0cf64`, morph-weight channels reach a live
`AnimatedMorphWeights` ECS sink every frame (confirmed via
`sink_lifecycle_end_to_end_tests`) — they only lack a GPU/mesh-vertex-blend
consumer (tracked separately by `#2221`). Ambient genuinely is still dropped.
The doc conflates two different states.

## Evidence

```
Intentionally parked (captured, no renderer consumer yet, *not* leaks):
per-light ambient colour channels and morph-weight channels.
```
vs. `crates/core/src/ecs/components/animated.rs:131` (`AnimatedMorphWeights`
component), and live writers at `byroredux/src/anim_convert.rs:137,183` and
`byroredux/src/boot.rs:763` (`.writes::<byroredux_core::ecs::AnimatedMorphWeights>()`).

## Impact

Doc-only — no behavior impact, but the doc currently implies morph-weight
channels are dropped entirely (a leak-adjacent claim), when in fact they
reach a live ECS component and only lack a downstream GPU consumer (a
separate, already-tracked gap).

## Suggested Fix

Split the sentence: keep ambient colour channels as "intentionally parked,
no consumer"; describe morph-weight channels as "reaches
`AnimatedMorphWeights` every frame, GPU/mesh-vertex-blend consumer tracked by
`#2221`."

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only fix — no behavior change to pin)

## Filed as

GitHub issue #2303, labels: low, animation, documentation.
