## FNV-D3-NEW-05: finish_partial_import re-runs convert_nif_clip on already-cached NIFs

## Source Audit
`docs/audits/AUDIT_FNV_2026-05-05.md` — Dim 3 LOW

## Severity / Dimension
LOW / Animation — wasted work + minor leak per cell crossing

## Location
`byroredux/src/cell_loader.rs:1591-1599` (`finish_partial_import`)

## Description
When a streaming-worker payload arrives for a model whose `NifImportRegistry` entry already carries a `clip_handle`, `finish_partial_import` re-runs `convert_nif_clip` on the worker-supplied `embedded_clip` and calls `clip_reg.add(clip)` — appending another clip to `AnimationClipRegistry` and overwriting `NifImportRegistry.clip_handles[key]` with the new handle.

```rust
// cell_loader.rs:1591-1599
let clip_handle = embedded_clip.as_ref().map(|nif_clip| {
    let clip = {
        let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
        crate::anim_convert::convert_nif_clip(nif_clip, &mut pool)
    };
    let mut clip_reg = world
        .resource_mut::<byroredux_core::animation::AnimationClipRegistry>();
    clip_reg.add(clip)
});
// …
if let Some(handle) = clip_handle {
    reg.set_clip_handle(cache_key, handle);
}
```

`finish_partial_import` does not check whether `reg.clip_handle_for(&cache_key).is_some()` before re-converting.

## Impact
Compounds with `FNV-D3-NEW-03` (streaming worker bypasses cache). Each cell crossing that the worker re-parses produces one new keyframe-vector clone per embedded controller. The previous handle is leaked into `AnimationClipRegistry` (see `FNV-D3-NEW-04`).

Severity: LOW because the steady-state cell rotation is bounded by 49 cells (radius 3 grid) and the keyframes are usually small. The LOW finding is mostly here so the dependency between this and FNV-D3-NEW-03/04 is documented.

## Suggested Fix
Early-out at the top of `finish_partial_import` when `reg.get(&cache_key).is_some()`. The worker's parse work is wasted but the clip-registry pollution is avoided. Best fix is the upstream MEDIUM (`FNV-D3-NEW-03`) — worker doesn't re-parse cached models in the first place.

## Related
- `FNV-D3-NEW-03` (parent perf issue — fixing that obviates this one)
- `FNV-D3-NEW-04` (leak this compounds)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: N/A
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Streaming integration test asserting `AnimationClipRegistry.len()` is stable across a load → unload → reload cycle of the same cell
