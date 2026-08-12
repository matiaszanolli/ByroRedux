# SAFE-D8-01: AnimationClipRegistry slot vector grows monotonically

**Issue**: #2689
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`

- **Severity**: LOW
- **Dimension**: 8 — NPC / Animation spawn safety
- **Location**: [registry.rs](crates/core/src/animation/registry.rs) — `AnimationClipRegistry::add`, `::release`, `::get_or_insert_by_path`
- **Status**: NEW
- **Description**: The #790 path-memo dedup is intact and case-insensitive (see
  PASS list), so a *repeated* load of the same `.kf` costs nothing. But
  `release()` (called from the `NifImportRegistry` LRU eviction path, #863)
  clears a slot's contents while keeping the slot occupied, **and** removes the
  reverse-map entry so a later `get_or_insert_by_path` for that key rebuilds via
  `add()` — which unconditionally `self.clips.push(...)` at a fresh index. There
  is no free list. An evict→reload cycle therefore leaks one empty
  `AnimationClip` header per cycle, permanently.
- **Evidence**:
  ```rust
  pub fn add(&mut self, clip: AnimationClip) -> u32 {
      let handle = self.clips.len() as u32;
      self.clips.push(clip);
      handle
  }
  ```
  and in `release`: `self.clip_handles_by_path.retain(|_, h| *h != handle);`
  with the slot left in place (documented as the no-stale-handle invariant).
- **Impact**: Slow, unbounded CPU-side growth under exterior streaming churn —
  a few hundred bytes per evict→reload cycle, so single-digit MB over a long
  session, not a per-frame leak. The deliberate no-stale-handle design is sound;
  what is missing is any bound or telemetry on the stub count.
- **Related**: #790, #863, #2524 (LRU eviction dropping freed handles).
- **Suggested Fix**: Either push released handles onto a free list and have
  `add` prefer it (safe: the slot is empty and its path binding is already gone,
  so reuse cannot alias a live consumer to *different* content — only to the
  same empty stub semantics it already reads), or expose a `stub_slot_count()`
  for the debug stats so the growth is at least observable.

---


---
*Filed from [`docs/audits/AUDIT_SAFETY_2026-08-12.md`](docs/audits/AUDIT_SAFETY_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `SAFE-D8-01`.*

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or a bench delta vs the checked-in baseline) pins this fix
