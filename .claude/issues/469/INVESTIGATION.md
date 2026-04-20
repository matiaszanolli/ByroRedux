# #469 Investigation

## Audit trace

- `AnimationClip.weight` populated at `byroredux/src/anim_convert.rs:214` from
  `NiControllerSequence.weight`.
- Declared in `crates/core/src/animation/types.rs`.
- Workspace grep `clip\.weight` (readers): zero hits prior to fix.

## Fix sites

`crates/core/src/animation/stack.rs::sample_blended_transform` has two passes
that both read `layer.effective_weight()` without factoring in `clip.weight`.
Each pass already resolves the clip via `registry.get(layer.clip_handle)` — the
fix reorders the lookup before the weight read so the multiply is cheap.

The single-player path (`AnimationPlayer` / `advance_time`) is deliberately
**not** changed: single-clip Gamebryo playback doesn't define a blend scaler,
and the legacy source applies `NiControllerSequence.weight` only when blending
competing sequences. Documented that semantic in the doc comment on
`AnimationClip.weight` so the divergence is intentional and recorded.

## Sibling channels

`sample_float_channel` / `sample_color_channel` / `sample_bool_channel` are
single-clip samplers called from `systems.rs`. They don't participate in the
priority/weight blending loop at all — they take one `FloatChannel` and a
time, not a stack. No clip-weight multiply is meaningful there (same reasoning
as the single-player path).

## Regression test

`crates/core/src/animation/mod.rs::sample_blended_transform_applies_clip_weight`
— two clips at equal layer weight, one authored `weight=1.0` @ tx=10, the
other `weight=0.5` @ tx=20. Expected blend: 20/1.5 ≈ 13.333.
Pre-fix value would be 15.0. Passes post-fix.
