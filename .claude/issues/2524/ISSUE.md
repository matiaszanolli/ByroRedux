# PERF-D3-NEW-01: NifImportRegistry LRU eviction drops freed AnimationClipRegistry handles in the precombined-mesh insert path

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2524
**Finding ID**: PERF-D3-NEW-01

**Severity**: MEDIUM
**Dimension**: GPU Memory Pressure & Eviction Thrash
**Location**: `byroredux/src/cell_loader/precombined.rs:313-316`
**Status**: NEW (fresh reintroduction of the #863 bug class at a call site added 2026-08-04, `9e5540899` — not a regression of the original fix, whose three original call sites remain correct)

## Description
`NifImportRegistry::insert` returns `Vec<u32>` — the `AnimationClipRegistry` handles of any entries the 2048-cap LRU sweep evicted as a side effect of this insert — and is marked `#[must_use = "evicted clip handles must be released into AnimationClipRegistry to free their keyframe arrays — see #863"]`. Four of the five production call sites forward the returned handles to `AnimationClipRegistry::release`. The precombined-mesh commit path does not:
```rust
// byroredux/src/cell_loader/precombined.rs:313-316
{
    let mut reg = world.resource_mut::<NifImportRegistry>();
    let _freed = reg.insert(path.clone(), parsed.clone());
}
```
Binding the `#[must_use]` return to a named variable (`_freed`, not the bare `_` discard) satisfies both the `must_use` and `unused_variables` lints, so the compiler gives no warning — the exact silent-drop shape #863's original bug had before the `Vec<u32>` contract was added.

## Evidence
Confirmed directly at `precombined.rs:313-316`. `AnimationClipRegistry::release` is what actually clears a slot's channel collections — skipping it leaves those collections (and their backing allocations) resident indefinitely. The precombine path's own inserted entry never itself owns a clip handle, but the LRU sweep triggered by *this* insert can evict any other cache entry once the registry is at cap (2048 default, or `BYRO_NIF_CACHE_MAX`), including animated NIFs registered via the three correctly-forwarding call sites — whichever victim the sweep picks, if it owned a clip handle, that handle is silently dropped here instead of released.

## Impact
A slow CPU-RAM leak (not VRAM) — bounded by `AnimationClipRegistry`'s slot count growing without corresponding frees, gated on (a) FO4 precombined-mesh content being loaded (M49), (b) the `NifImportRegistry` LRU cache being at its cap, and (c) the evicted victim happening to be an animated NIF with a registered clip handle. In a long FO4 session that revisits precombine-heavy cells repeatedly, this compounds the same way #863 originally did, just through a narrower door.

## Related
#863 (original fix, three-of-four-then-correct call sites), #544 (clip_handles map cleanup on eviction).

## Suggested Fix
Mirror `partial.rs:69`'s pattern — capture the returned `Vec<u32>` as `freed` (not `_freed`), and after the block, if non-empty, forward each handle to `world.resource_mut::<AnimationClipRegistry>().release(h)`.

## Completeness Checks
- [ ] **TESTS**: A regression test forces an LRU eviction during a precombine-path insert and confirms the evicted entry's clip handle is released
- [ ] **SIBLING**: All five `NifImportRegistry::insert` call sites forward the returned handles consistently
