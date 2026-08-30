# #3705 — ECS-2026-08-30-D10-05: clip release is driven by NIF-cache LRU pressure with no liveness check, so live animated REFRs freeze permanently

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: Animation Runtime / Component Lifecycles
**Location**: `byroredux/src/cell_loader/nif_import_registry.rs` (`NifImportRegistry::insert` eviction loop, ~:488-530); `crates/core/src/animation/registry.rs` (`AnimationClipRegistry::release`, ~:156-191); callers at `byroredux/src/streaming_helpers.rs:544`, `byroredux/src/cell_loader/references/mod.rs:136`, `byroredux/src/cell_loader/partial.rs:151`
**Source**: `docs/audits/AUDIT_ECS_2026-08-30.md` (ECS-D10-05)

## Description

`NifImportRegistry::insert` evicts by pure LRU on a default 2048-entry cap, returns the evicted entry's memoised clip handle, and every caller forwards it to `AnimationClipRegistry::release`. **Nothing consults whether an `AnimationPlayer` still holds that handle.** `AnimationPlayer` stores only the numeric handle — no path back to the source path — so nothing ever re-resolves. Post-release the slot reads as an empty clip (every channel map cleared, `duration` zeroed), so the still-loaded REFR's UV scroll / emissive pulse / visibility flicker stops for the rest of the session.

## Evidence

```rust
// byroredux/src/cell_loader/nif_import_registry.rs — eviction is LRU-only; no liveness test
while self.core.len() > self.max_entries {
    let victim = self.access_tick.iter().min_by_key(|(_, &tick)| tick).map(|(k, _)| k.clone());
    // ...
    if let Some(handle) = self.clip_handles.remove(&victim_key) {
        freed_clip_handles.push(handle);
    }
```

The registry's own doc calls this "live handles stay resolvable and just stop animating" — safe, but the eviction *trigger* is cache pressure, not entity lifetime.

## Impact

In a long exterior traversal (>2048 distinct model paths streamed while an earlier cell stays resident) animated scenery in older cells silently and permanently stops, indistinguishable from "the clip never loaded" — no log, no telemetry.

## Related

#863 (CLOSED — added the release path this finding says is unguarded), #2689 (OPEN — the *slot vector* growing monotonically, a different defect), #1854, #2524.

## Suggested Fix

Refcount clip handles against live `AnimationPlayer` / `AnimationLayer` holders so eviction can only release clips nothing is playing; alternatively store the source path on the player so a released clip can be lazily rebuilt via `get_or_insert_by_path`.

## Completeness Checks
- [ ] **SIBLING**: All three `release`-forwarding call sites get the same liveness gate
- [ ] **TESTS**: A regression test evicts a cache entry whose clip a live `AnimationPlayer` holds and asserts the clip survives
