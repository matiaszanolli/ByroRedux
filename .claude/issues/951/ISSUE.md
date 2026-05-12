# #951 — SAFE-26: bgem_cache + failed_paths in BgsmProvider grow unbounded across cell streaming

**Severity**: LOW
**Labels**: low, memory, safety, bug
**Source audit**: [docs/audits/AUDIT_SAFETY_2026-05-11.md](../../../docs/audits/AUDIT_SAFETY_2026-05-11.md) (Dim 3 — memory safety)
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/951

## Location

- [byroredux/src/asset_provider.rs:461](../../../byroredux/src/asset_provider.rs#L461) — `bgem_cache: HashMap<String, Arc<BgemFile>>`
- [byroredux/src/asset_provider.rs:464](../../../byroredux/src/asset_provider.rs#L464) — `failed_paths: HashSet<String>`
- [byroredux/src/asset_provider.rs:535-549](../../../byroredux/src/asset_provider.rs#L535-L549) — only insert sites

## TL;DR

`bgem_cache` and `failed_paths` are populated by every cell load that
touches a BGEM-material reference (FO4+ content) and have zero
eviction call sites. Pattern mirrors the closed #790 / #863
(AnimationClipRegistry) and the open #850 (SoundCache).

## Why it matters

Bounded by `O(distinct BGEMs in installed archives)` — vanilla FO4
~5K, mod loadouts larger. Per-entry small (parsed `BgemFile` few
hundred bytes), so absolute footprint stays in low-MB. The leak
class is long-session unbounded growth, not catastrophic memory
pressure. Severity LOW for that reason.

## Fix sketch

Hook BGEM/BGSM eviction into the M40 cell-unload path at
[cell_loader.rs:330](../../../byroredux/src/cell_loader.rs#L330)
(`freed_meshes` loop). The cell loader already tracks per-cell
texture / mesh handle sets; extending to BGEM keys makes eviction
symmetric. Bundle with #850's eventual fix.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify `bgsm_cache` (asset_provider.rs:513) has the same gap; audit and align
- [ ] **SIBLING**: `failed_paths` HashSet is a negative cache with the same growth risk
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Streaming smoke that loops cell load/unload and asserts `bgem_cache.len()` stays bounded

## Related

- #790 / E-N1 — AnimationClipRegistry grow-only fix (closed)
- #863 / FNV-D3-NEW-04 — AnimationClipRegistry LRU (closed)
- #850 / AUD-D6-NEW-09 — SoundCache no eviction (open mirror)

## Suggested next step

```
/fix-issue 951
```
