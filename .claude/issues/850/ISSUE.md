## Source Audit
`docs/audits/AUDIT_AUDIO_2026-05-05.md`
M44 audio subsystem

## Severity / Dimension
LOW / ECS Lifecycle (Cell Streaming)

## Location
`crates/audio/src/lib.rs:951-1041`

## Description
`SoundCache.map: HashMap<String, Arc<StaticSoundData>>` has no eviction policy. The crate docstring at lib.rs:962-966 acknowledges this ("Eviction strategy: **none today**") and argues the vanilla SFX set is small enough to fit. But mod-loaded SFX (LARGE mods like Project Nevada, Tale of Two Wastelands) each add hundreds of unique sounds, and the cache key is the full BSA path (no path-aliasing collapse for "same sound, different filename"). A 24-hour session with frequent mod swaps could grow the cache unboundedly.

## Evidence
```rust
// crates/audio/src/lib.rs:951-1041
pub struct SoundCache {
    map: HashMap<String, Arc<StaticSoundData>>,
}
// No clear(), no LRU, no max_entries.
```

## Impact
Memory growth on long sessions with heavy mod use. The vanilla case is bounded (~6,000 unique SFX × ~30 KB decoded average = ~180 MB; well within budget). Mod-stack with FCO + TTW + Mojave Express + Project Nevada can push past 1 GB.

## Suggested Fix
Either (a) document the upper bound and accept it, (b) add an LRU eviction with a `max_entries` cap once a future scenario surfaces, or (c) add a `clear()` method that the cell-unload path can call when a region exits scope. The crate docstring already acknowledges this — no urgent action, but pin a `cache_bytes_estimate()` helper that telemetry can poll so a future regression surfaces in `stats` output.

## Related
AnimationClipRegistry (#790, similar process-lifetime cache pattern).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
