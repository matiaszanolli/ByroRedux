Surfaced by the 2026-05-28 renderer audit (`docs/audits/AUDIT_RENDERER_2026-05-28.md` Dim 12). Sibling of [#1155 / TD8-024](https://github.com/matiaszanolli/ByroRedux/issues/1155) (deleted-fn doc reference pattern).

## Issue

`crates/core/src/ecs/resources.rs:645-646` says:

```rust
/// fall back to bind-pose rendering for the overflowed entity. See
/// `BONE_PALETTE_OVERFLOW_WARNED` in `byroredux::render::skinned`.
```

But that `Once`-gated warn symbol no longer exists in `byroredux/src/render/skinned.rs` — only `SKIN_DROPOUT_DUMPED` is there. The pool overflow warn now fires from `SkinSlotPool::allocate` directly via the `overflow_warned: bool` field (lines 706-714).

`grep -rn 'BONE_PALETTE_OVERFLOW_WARNED' --include='*.rs'` returns exactly one hit — the stale doc reference itself.

Introduced inadvertently by today's [#1284](https://github.com/matiaszanolli/ByroRedux/issues/1284) instrumentation patch (commit a3c2836a) that added `overflow_attempt_count` + `DebugStats::skin_pool_*` wire-through — the patch updated the warn body and the cap but didn't refresh the cross-reference comment.

## Risk

Doc rot. Future devs grep for `BONE_PALETTE_OVERFLOW_WARNED` chasing the overflow fallback contract and find a dangling reference. No correctness impact.

## Suggested fix

Update `crates/core/src/ecs/resources.rs:645-646` to:

```rust
/// fall back to bind-pose rendering for the overflowed entity. See
/// `Self::overflow_warned` (one-shot log) and `Self::overflow_attempt_count`
/// (cumulative spill telemetry surfaced via `DebugStats::skin_pool_*` and
/// the `engine::stats` `skin=L/M+S` line); see #1284 for the cap-sizing
/// feedback loop.
```

One-line change.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: grep for other `BONE_PALETTE_*` / `bone_palette_*` references that may have rotted in the same patch
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A — doc fix only
