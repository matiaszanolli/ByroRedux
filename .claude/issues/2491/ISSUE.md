# MAT-D7-2026-08-07-02: hash_material_slice docstring cites a GpuMaterial::Hash impl that does not exist, with stale line anchors

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2491
**Finding ID**: MAT-D7-2026-08-07-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 7 — Material Table
**Location**: `crates/renderer/src/vulkan/scene_buffer/descriptors.rs::hash_material_slice`
**Status**: NEW

## Description
The doc comment says the slice hash is "routed through `GpuMaterial::as_bytes`-equivalent slice cast so the same byte view used by `GpuMaterial`'s `Hash`/`Eq` impls (`vulkan/material.rs:280-309`) drives the slice hash too". `GpuMaterial` has no `Hash` impl — dedup is keyed on the field-walking `hash_gpu_material_fields` (#781 moved the index key off the struct itself); only `PartialEq`/`Eq` use `as_bytes`. The cited line range `280-309` now lands in the supplemental-texture-role field block, not the `as_bytes`/`PartialEq` block (which sits around `material.rs:588-611`).

## Evidence
`material.rs` declares only `impl PartialEq for GpuMaterial { fn eq(&self, other: &Self) -> bool { self.as_bytes() == other.as_bytes() } }` and `impl Eq for GpuMaterial {}`. No `impl Hash`. `MaterialTable::index` is `FxHashMap<u64, u32>` keyed on `hash_gpu_material_fields`.

## Impact
Documentation only. A reader chasing "which hash does dedup use" is pointed at a non-existent impl and at unrelated line numbers, which is exactly the failure mode the two-walk lockstep contract (#781) depends on people understanding.

## Related
#781 / PERF-N4, #878 / DIM8-01, #1368, #2273.

## Suggested Fix
Reword to "the same raw-byte view `GpuMaterial::as_bytes` gives the `PartialEq`/`Eq` impls" and drop the hard-coded line numbers in favour of the symbol name.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
