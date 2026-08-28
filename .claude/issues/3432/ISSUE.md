# #3432 — SAFE-2026-08-27b-01: NiControllerSequence `duration` and `weight` are unsanitised past #3258 — both latch a NaN into the pose

- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-27b.md`
- **Severity**: MEDIUM
- **Labels**: `medium,safety,animation,nif-parser,nif,bug`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3432

---

From `docs/audits/AUDIT_SAFETY_2026-08-27b.md` (Dimension 8 — NPC/animation spawn safety + Dimension 9 — NaN/Inf on the GPU).

- **Severity**: MEDIUM
- **Location**: producer `crates/nif/src/anim/sequence.rs:20` (`duration`) and `:23` (`weight`); boundary `byroredux/src/anim_convert.rs:506` + `:520`; consumers `crates/core/src/animation/player.rs:61-84` + `:134-142`, `crates/core/src/animation/stack.rs:165-181`, `:332-334`, `:378-380`
- **Status**: NEW. Sibling of #3258 (CLOSED, fixed in `bbfd742f`); nothing in the issue list or `docs/audits/` covers `duration`/`weight`.

## Description

#3258 established the rule: `NiControllerSequence` scalars are raw file data, and a non-finite one that reaches the animation clock latches the entity's pose to NaN for the rest of its life. It fixed exactly one scalar, `frequency`, at the translate boundary (`sanitized_clip_frequency`), plus a defense-in-depth `finite_time_delta` on the `dt * speed * frequency` product.

The **two adjacent fields of the same struct, read by the same parser function, and passed through the same three lines of `convert_nif_clip`, were not touched** — and each has its own latch:

1. **`duration`** — `CycleType::Reverse` routes through `fold_reverse_time` (`player.rs:61-84`), whose only guard is `if duration <= 0.0`. `NaN <= 0.0` is **false**, so a NaN duration falls through: `period = 2.0 * NaN = NaN`, `m = (phase + delta).rem_euclid(NaN) = NaN`, and the `m > duration` branch is `NaN > NaN` = false, so it returns `(NaN, false)`. `local_time` is NaN from that tick onward and never recovers. `advance_stack` (`stack.rs:172-181`) carries the byte-identical arm.
2. **`weight`** — `sample_blended_transform`'s per-layer skip is `let ew = layer.effective_weight() * clip.weight; if ew < 0.001 { continue; }` (`stack.rs:332-334`, repeated at `:378-380`). `NaN < 0.001` is **false**, so a NaN-weighted layer is *not* skipped; `total_weight` becomes NaN, the `total_weight < 0.001` early return at `:363` is likewise false, and the blended position / rotation / scale come out NaN.

`find_key_pair` (`crates/core/src/animation/interpolation.rs`) does not rescue either: it handles ±inf correctly (endpoint clamps) but a NaN `time` fails **both** comparisons, falls into the binary search, and emits `t = (NaN - t_lo) / dt` = NaN. There is no `is_finite` check anywhere between there and the GPU.

The affected import path is the one that matters: `import_sequence` is what `import_kf` calls for **both** standalone `.kf` files and embedded `NiControllerManager` sequences (`crates/nif/src/anim/entry.rs`). The other path, `import_embedded_animations`, is already immune — it derives duration from key times behind a `> 0.0` guard.

## Evidence

Producer — no finiteness gate on either field:
```rust
// crates/nif/src/anim/sequence.rs:20-23
let duration = seq.stop_time - seq.start_time;
let cycle_type = CycleType::from_u32(seq.cycle_type);
let frequency = seq.frequency;
let weight = seq.weight;
```

Boundary — the gap is visible in three consecutive lines:
```rust
// byroredux/src/anim_convert.rs:504-520
AnimationClip {
    name: nif.name.clone(),
    duration: nif.duration,                          // ← unvalidated
    cycle_type,
    // #3258 — `NiControllerSequence.frequency` is raw file data …
    frequency: sanitized_clip_frequency(nif.frequency),
    weight: nif.weight,                              // ← unvalidated
```

Float semantics verified by execution rather than by reading:
```
f32::MIN - f32::MAX = -inf   finite=false
NaN <= 0.0                   = false     // fold_reverse_time's only guard
(0.35f32).rem_euclid(2.0*NaN)= NaN,  NaN > NaN = false
NaN < 0.001                  = false     // sample_blended_transform's skip
```

## Impact

A `.kf` or embedded sequence carrying a non-finite `stop_time`/`start_time` pair (or a literal NaN `weight`) poisons the affected entity's bone transforms permanently — `Transform` → `GlobalTransform` → the `GpuInstance` model matrix and the bone palette. NaN on the GPU is UB by this project's own severity rules. Corrupt or hostile archive content is the realistic source, which is exactly the reachability #3258 was accepted on. Rated MEDIUM to match #3258's own label rather than escalated.

## Related

#3258 (the fix that stopped one field short), #3194 (the same NaN-transparency class on the SpeedTree wind field), #3373 (the same "a later field was added past the sanitiser" shape in `Material`).

## Suggested Fix

Sanitise both at the same boundary `frequency` already uses. `duration`: reject non-finite (and negative) to `0.0`, which every cycle arm already treats as "no wrap / no fold". `weight`: reject non-finite to `1.0`, nif.xml's own default. Then make the gates NaN-safe rather than NaN-transparent — `if !(ew >= 0.001) { continue; }` and `if !(duration > 0.0) { return (0.0, false); }` — so a future producer cannot reopen it.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the `advance_time` / `advance_stack` twin arms, the second `ew < 0.001` site at `stack.rs:378`)
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→canonical boundary — the sanitiser belongs in `anim_convert.rs`, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
