# SF-D3-01: sf_cdb_cache is an uncapped, never-evicted process-lifetime hold of up to 233 MB

**Issue**: #3054
**Severity**: MEDIUM
**Labels**: `medium,memory,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_STARFIELD_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_STARFIELD_2026-08-16.md` (Dimension 3 — CDB material provider).

**Location**: `byroredux/src/asset_provider/material.rs`:135-210
**Status note**: NEW — a consequence of #2705's fix (CLOSED), which introduced this cache to stop re-decompressing the CDB.

## Description

`sf_cdb_cache` is an **uncapped, never-evicted, process-lifetime** hold of up to 233 MB.

```rust
// material.rs:148
pub(super) fn sf_cdb_cache() -> &'static Mutex<HashMap<String, Arc<[u8]>>> {
```

It is a module-scope `static` keyed by path, populated at :194 and read at :173. Nothing caps its entry count, evicts by LRU, or clears it on game/cell switch.

## Impact

#2705 correctly stopped the 105 MB `materialsbeta.cdb` being decompressed and discarded on every `build_material_provider` call. The fix traded that for an unbounded resident hold: up to 233 MB of decompressed CDB stays in RAM for the process lifetime, and a session that touches multiple CDBs accumulates all of them.

Against the project's stated ~4 GB total budget (`docs/engine/memory-budget.md`), a quarter-gigabyte never-freed allocation is worth an explicit decision rather than an accident.

## Suggested Fix

Give the cache the treatment every other cache in the engine has — an entry cap or LRU eviction, sized against `memory-budget.md`, and a clear on game switch. `TextureRegistry` and the BGSM cache are the models.

If a permanent hold is the right call (there is realistically one CDB per game), say so in a comment and record the 233 MB in `memory-budget.md` so it is budgeted rather than invisible.

## Related

- #2705 (CLOSED — the fix that introduced this cache)
- #2706 (the `sf_cdbs` doc-comment drift in the same module)
- `docs/engine/memory-budget.md` (the budget this is absent from)

## Completeness Checks
- [ ] **BUDGETED**: The hold is either bounded or documented in `memory-budget.md`
- [ ] **CLEAR-ON-SWITCH**: The cache resets on game/cell switch if it stays unbounded
- [ ] **SIBLING**: The BGSM/BGEM caches in the same module checked for the same shape
- [ ] **NO-REGRESSION**: #2705's re-decompression fix is preserved
- [ ] **TESTS**: A regression test asserts bounded growth across repeated provider builds

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3054 --json state` when live state is needed.*
