# Issue #469

FNV-AN-H1: AnimationClip.weight loaded from NIF but never consumed — per-sequence weights silently dropped

---

## Severity: High

**Location**: `crates/core/src/animation/stack.rs:267,310`; `crates/core/src/animation/types.rs:140`; `byroredux/src/anim_convert.rs:214`

## Problem

`AnimationClip.weight: f32` is declared at `types.rs:140` and populated from `NiControllerSequence.weight` at `anim_convert.rs:214`. Workspace-wide grep for `clip\.weight` returns zero hits.

`stack.rs::sample_blended_transform` at lines 267 and 310 computes `ew = layer.effective_weight()` — the layer's own blend-in/out weight — and never multiplies by `clip.weight`. The single-clip `advance_time` path in `player.rs` + `systems.rs` also ignores it.

## Impact

Per-sequence default weights from KF data (FNV idle chains authored with non-unit base weight, multi-clip priority blending where some clips were authored to attenuate) are silently dropped. All clips behave as if `weight = 1.0`.

The audit verified the fix scope: 2 weight-gathering sites in `stack.rs` and 1 sampling call in the single-player path.

## Fix

In `stack.rs` at both `sample_blended_transform` weight-gathering passes:

```rust
let ew = layer.effective_weight() * clip.weight;
```

(After resolving `clip` from `registry.get(layer.clip_handle)`.) 6-line patch total.

Single-player sampling in `systems.rs` also needs a clip.weight multiply where the final transform is applied.

## Completeness Checks

- [ ] **TESTS**: Synthetic clip with `weight = 0.5`, verify sampled delta is half of `weight = 1.0` baseline
- [ ] **SIBLING**: Check `sample_float_channel` / `sample_color_channel` / `sample_bool_channel` — do they also need clip.weight scaling?
- [ ] **LOCK_ORDER**: No lock changes; no ordering concerns
- [ ] **DOCS**: Add doc comment to `AnimationClip.weight` noting it modulates layer-effective-weight

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-AN-H1)
