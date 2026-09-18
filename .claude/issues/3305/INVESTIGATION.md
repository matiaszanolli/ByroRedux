# #3305 — creature actors cast no ground-contact shadow (investigation state)

Date: 2026-09-18 (light & shadow correctness campaign, Wave 1)
Status: root cause narrowed to a fix-sized hypothesis; fix deferred to the
next batch (device repro required to confirm which failure mode fires).

## Confirmed sound (re-verified this session, do not re-audit)

1. Sun shadow-ray cull mask is `VisibilityMask::FULL` → `ALL_OPAQUE`
   (`render/lights.rs` directional push; `shadow_common.glsl::
   visibilityOpaqueMask`).
2. Instance masks: `shadow_mask_for_instance` maps `RenderLayer::Actor` →
   `VISIBILITY_LAYER_DYNAMIC_ACTOR`, and blended actor submeshes
   deliberately keep the actor bucket (predicates.rs, the hair/lash arm).
3. `tlas_exclusion` has no actor gate (LOD terrain / decal / fire-refraction
   only). Actor draws opt into the TLAS.

## The mechanism that produces the symptom

`build_tlas` (`acceleration/tlas.rs:524-535`): a skinned draw whose
per-entity BLAS is absent emits **no TLAS instance** and counts
`missing_skinned_blas` — "the entity will be invisible to RT this frame,
but raster's inline-skinning path still renders it correctly". That is the
exact reported picture: dog visible, no shadow; balustrade (rigid,
per-mesh BLAS) shadows normally.

The #3305 `ShadowMaskSnapshot` census added in that same pass is the
measurement instrument for the next step.

## Why it may PERSIST (the fix-sized hypothesis)

The refit path skips entities with no BLAS and says "the first-sight path
owns the retry" (`context/skinned_blas_refit.rs:782-797`). If a first-sight
BUILD fails once or is suppressed (BLAS budget gate #4196, scratch
pressure, slot-pool exhaustion under a radius-1 streaming pop-in — the
repro's own scenario) and the creature's `pose_dirty` never re-fires
 afterwards (idle animal), nothing re-arms a build: the actor is
permanently BLAS-less, so permanently shadowless.

## Next steps (device + this hypothesis)

1. Reproduce `docs/smoke-tests/m-exteriors.sh fo4 boundary`, attach
   `byro-dbg`, read the `missing_skinned_blas` rate + the census: if the
   dogs' entity ids persist in the missing samples across many frames, the
   no-retry hypothesis is confirmed.
2. Desk-fix: re-arm a first-sight build for any skinned draw that appears
   in the missing set for >N frames (bounded retries), OR build-on-miss in
   `build_tlas`'s caller (draw-frame ordering already processes first-sight
   before the TLAS build — the gap is failure re-armament, not ordering).
3. Check the first-sight failure reasons while there (#4196 admission +
   slot pool) so the fix removes the cause, not just the staleness.
