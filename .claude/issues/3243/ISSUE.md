# 3243: Incremental: AnimatedTextureFlip cross-clip merge gap has no test coverage

**Severity**: LOW (missing test coverage) · **Report**: `docs/audits/AUDIT_INCREMENTAL_2026-08-23.md` (F2) · **Changed in**: `byroredux/src/anim_convert.rs` (commit `7fbc5baf`, Fix #2221)

## Description

`insert_missing_sinks` (pre-existing helper, reused unchanged) skips inserting a component on any entity that already carries one of that type. For scalar sinks (`AnimatedAlpha`, `AnimatedShaderFloat`, …) this is the documented, correct "a second clip must not reset an already-live value" policy, and is covered by explicit tests.

`AnimatedTextureFlip` is different: it's a `Vec<TextureFlipEntry>` keyed by `texture_slot`, explicitly designed (per its own doc comment) to hold *multiple* independent flipbooks on one entity. That merge is correctly handled **within a single `attach_animation_sinks` call**. But if a **second, later-attached clip** on the same entity introduces a texture-flip channel for a **new** `texture_slot` the entity doesn't have yet, `insert_missing_sinks` sees the entity already has *an* `AnimatedTextureFlip` (from the first clip) and drops the whole insert — the second clip's slot is never added to the Vec. `apply_texture_flip_channels`'s `.find(|e| e.texture_slot == channel.texture_slot)` then permanently no-ops for that channel.

This is an inherited limitation of the pre-existing `insert_missing_sinks` design (the same trade-off already accepted for `AnimatedMorphWeights`), not a new bug introduced by this diff — the comment describing multi-slot support only claims (and only tests) the single-clip case. No test exercises the cross-clip scenario for the new `AnimatedTextureFlip` type specifically.

## Impact

If a shape carries two separately-attached animation clips (e.g. two `AnimationStack` layers, or a second NIF-embedded clip) that each target a *different* texture slot with a flipbook, the second clip's slot silently never animates. Recorded as a missing-test item since the design limitation is pre-existing and shared with `AnimatedMorphWeights` — not asking for a design change here, just test coverage for the boundary so a future consumer relying on multi-clip flipbooks doesn't discover the gap by trial and error.

## Suggested Fix

Add a test with two separately-attached clips on the same entity, each targeting a different `texture_slot`, asserting whether the second slot is merged in or silently dropped — documenting the actual current behavior explicitly rather than leaving it unverified.

## Completeness Checks
- [ ] **TESTS**: New test added per the description above
