# FNV-2026-08-16-D1-02: NifImportRegistry cache key diverges between sync and streaming loaders

**Issue**: #3038
**Severity**: MEDIUM
**Dimension**: 1 — Cell Loading
**Labels**: `medium,import-pipeline,performance,bug`
**Source report**: `docs/audits/AUDIT_FNV_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FNV_2026-08-16.md` (Dimension 1 — Cell Loading End-to-End).

**Location**: `byroredux/src/cell_loader/references/synth_child.rs`:362-381 vs the exterior-streaming loader

## Description

Both loaders key the shared `NifImportRegistry` on `model_path.to_ascii_lowercase()`, but they **build `model_path` differently**. The synchronous REFR path prefixes `meshes\` when the authored `StaticRecord.model_path` lacks it; the streaming path does not.

The registry therefore holds two keys for one asset, and neither loader sees the other's cache entry.

## Evidence

Live `mesh.cache failed` on the exterior 7×7 WastelandNV grid returned 17 entries containing **7 duplicate pairs** of the same asset under both key forms:

```
\wastelandshrub01.spt                              meshes\wastelandshrub01.spt
clutter\barrel02firelight.nif                      meshes\clutter\barrel02firelight.nif
clutter\supermutantcamp\supermutantbedding01.nif   meshes\clutter\…\supermutantbedding01.nif
effects\ambient\fxdustwhirlwind01.nif              meshes\effects\ambient\fxdustwhirlwind01.nif
```

Re-verified 2026-08-17.

## Impact

Every asset touched by both loaders is parsed and imported **twice**, and cached twice — wasted parse time on the streaming path and duplicated resident memory, against a project VRAM/RAM budget that targets under ~4 GB total.

It also makes cache-hit telemetry misleading: a hit rate computed over split keys understates reuse.

## Suggested Fix

Normalise `model_path` once, at a single point both loaders call, before it becomes a registry key. Prefixing `meshes\` is the natural canonical form since the archive paths carry it — but the important part is that one function owns the normalisation rather than two call sites agreeing by accident.

## Related

- #3036 (FNV-2026-08-16-D1-01 — the other cell-loading finding; both surfaced from the same `mesh.cache failed` probe)

## Completeness Checks
- [ ] **SINGLE-NORMALISER**: One function owns key normalisation; neither loader builds a key itself
- [ ] **SIBLING**: Any other `NifImportRegistry` key producer (partial loads, precombines) uses it too
- [ ] **CACHE-TELEMETRY**: Hit-rate reporting is correct after the keys merge
- [ ] **TESTS**: A regression test loads one asset through both paths and asserts a single registry entry

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3038 --json state` when live state is needed.*
