# Issue #3471

**Title**: ECS-2026-08-27-02: `sample_blended_transform`'s blend pass never re-applies the weight pass's empty-key filter, so a keyless max-priority channel pollutes the blend it was excluded from

**Labels**: low, ecs, animation, bug

**Filed**: 2026-08-27 via `/audit-publish docs/audits/AUDIT_ECS_2026-08-27.md`

---

**Source**: `docs/audits/AUDIT_ECS_2026-08-27.md` — finding `ECS-2026-08-27-02` (LOW, Dimension 10: Animation Runtime — blend weights). Audited at `HEAD = 969d81c8`; re-verified against current code at publish time.

## Description

In `sample_blended_transform` (`crates/core/src/animation/stack.rs`), pass 1 (which elects `max_priority` and accumulates `total_weight`) skips any layer whose channel for this bone has all three key lists empty. Pass 3 (which does the actual blend) re-walks the layers with only a `channel.priority != max_priority` filter — the empty-key check is not repeated. A layer that pass 1 excluded from `total_weight` therefore still contributes to the blend, with `sample_translation` / `sample_rotation` / `sample_scale` all returning `None` and falling back to `Vec3::ZERO` / `Quat::IDENTITY` / `1.0`.

The two effects are: `blended_scale` gains a spurious `+1.0 * w` (a bone at scale 1 blends to ~2), and `blended_rot` is slerped toward identity by `w / (accumulated_weight + w)`. `accumulated_weight` also exceeds 1.0, since the denominator `total_weight` never counted the offending layer.

## Evidence

pass 1:

```rust
// crates/core/src/animation/stack.rs — sample_blended_transform, pass 1+2 fused
// Only inspect key presence here. Sampling is deferred to the blend
// pass below so interpolation happens once per channel (#3031).
if channel.translation_keys.is_empty()
    && channel.rotation_keys.is_empty()
    && channel.scale_keys.is_empty()
{
    continue;
}
```

pass 3 has no counterpart:

```rust
if channel.priority != max_priority {
    continue;
}

let t = sample_translation(channel, layer.local_time).unwrap_or(Vec3::ZERO);
let r = sample_rotation(channel, layer.local_time).unwrap_or(Quat::IDENTITY);
let s = sample_scale(channel, layer.local_time).unwrap_or(1.0);
```

All-empty channels are producible: `constant_transform_channel` (`crates/nif/src/anim/transform.rs`) emits empty key vectors for every axis whose pose is the `FLT_MAX` "no static pose" sentinel, and `extract_transform_channel_at` passes `convert_*_keys` output through verbatim, so a `NiTransformData` with no keys yields the same. Neither `crates/nif/src/anim/sequence.rs` nor `crates/nif/src/anim/entry.rs` filters an empty channel out before `channels.insert`.

## Impact

Wrong bone scale and rotation during a crossfade whenever one of the blended clips carries a keyless channel for that bone at the winning priority.

**Not live today**: nothing in the engine inserts an `AnimationStack` — `AnimationStack::new()` appears only in `crates/core/src/animation/controller.rs`'s test module, and `apply_pending_transition` (the sole `stack.play` caller) has no production caller. The bug is latent in a `pub` core API that `byroredux/src/boot.rs` already declares a write for, and it will go live the moment the KFM controller is wired.

## Suggested fix

Hoist the three-`is_empty` predicate into a small `fn channel_has_keys(channel: &TransformChannel) -> bool` and call it from both passes, so the two filters cannot drift again. Pin with a two-layer fixture where one layer's channel is keyless at the same priority.

## Related

#3031 (the commit that introduced the pass-1 filter, `136254e9`, and did not mirror it), #3316 (the FNV finding that made `constant_transform_channel` a high-traffic path).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other multi-pass weight/blend walk in `stack.rs`; the float/color/bool channel blends)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix (two-layer stack, one keyless channel at the winning priority, asserting scale stays 1.0)
