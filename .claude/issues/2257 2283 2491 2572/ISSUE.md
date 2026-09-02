# Issue #2257
title:	TD1-079: material.rs crossed 2000 LOC — mostly inline test growth, no oversized production function
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, low, renderer, tech-debt, vulkan
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2257
--
**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/renderer/src/vulkan/material.rs` (2015 LOC)
**Status**: NEW

**Description**: Grew from 1931 LOC (07-25 boundary) to 2015 (+84 LOC) — a marginal crossing, ~60% of the file is `#[cfg(test)]` content. No single production function is anywhere near 200 LOC; the production code (`GpuMaterial`, `MaterialTable::intern`/`intern_by_hash`, preset constructors) is unchanged in shape from prior audits. This crossing is purely test accumulation — the tests were never split into a sibling file to begin with, unlike `texture_registry.rs`/`texture_registry_tests.rs`, the established convention elsewhere in this same directory.

**Evidence**: `git show 2cb86be5:...material.rs | wc -l` → 1931; current → 2015. Longest production function is well under 100 LOC; the file's length is dominated by ~35 `#[test]` fns plus two large GPU-layout pinning tests (~280 combined lines).

**Impact**: Maintainability only, lowest-urgency finding in this batch — no logic is hard to follow, just file length.

**Suggested Fix**: Extract the `#[cfg(test)] mod tests { ... }` block into a sibling `material_tests.rs`, mirroring the already-established `texture_registry.rs`/`texture_registry_tests.rs` split in the same directory. Purely mechanical, lowest-risk of any finding in this batch.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable


---

# Issue #2283
title:	NIF-D4-01: BsTriShapeKind::LOD triangle-count cutoffs still unreachable — regression of closed #1207
state:	CLOSED
author:	matiaszanolli (Matias Zanolli)
labels:	bug, medium, nif-parser
comments:	1
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2283
--
**Regression of #1207** (closed) — the original fix landed but never actually fires on real dispatch content; its own regression tests bypass the real block dispatcher, so the gap slipped back in unnoticed.

## Description

Two distinct wire block names dispatch to two different Rust representations. `"BSLODTriShape"` (SkyrimSE) parses as `NiLodTriShape`, a classic-`NiTriShape`-bodied struct carrying its own `lod0_size`/`lod1_size`/`lod2_size` fields that are never read past the parser struct. `"BSMeshLODTriShape"` (FO4/SkyrimSE-DLC) parses via `BsTriShape::parse_lod()` — the only producer of `BsTriShapeKind::LOD{lod0,lod1,lod2}` anywhere in the crate — but the dispatcher immediately overwrites that value with `.with_kind(BsTriShapeKind::MeshLOD)` one line later. So `BsTriShapeKind::LOD` is constructed and discarded in the same expression on every real parse and can never be the persisted `kind` the importer sees. `import/mesh/bs_tri_shape.rs`'s `extract_bs_tri_shape` matches on exactly that unreachable variant to populate `bs_lod_cutoffs`; `NiLodTriShape`-typed shapes never even reach that function (the walker unwraps to the inner classic `NiTriShape` and calls the `ni_tri_shape.rs` extractor, which hardcodes `bs_lod_cutoffs: None`).

**Location**: `crates/nif/src/blocks/mod.rs:453-456`; `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:568-591`; `crates/nif/src/import/mesh/bs_tri_shape.rs:204-207`; `crates/nif/src/import/walk/mod.rs:477-500,1113-1138`

**Game Affected**: SkyrimSE / FO4 (the two games shipping distant-LOD tri-shape wire blocks)

## Evidence

`grep -rn "BsTriShapeKind::LOD" crates/nif/src` shows the only non-doc, non-test production hit outside the enum definition is `bs_tri_shape.rs:589` inside `parse_lod`, whose sole caller (`blocks/mod.rs:454-456`) discards the value via `.with_kind(MeshLOD)`. The #1207 regression test (`lod_kind_surfaces_three_cutoffs`) hand-constructs a `BsTriShape` fixture with `kind: BsTriShapeKind::LOD{...}` directly, bypassing the real block dispatcher entirely — so it passes without ever exercising the path that would catch the gap. The #988 regression test (`bs_lod_tri_shape_imports_geometry_not_dropped`) builds a `NiLodTriShape` fixture with populated LOD sizes but only asserts the mesh renders, never that the values reach `ImportedMesh`.

## Impact

No present-day rendering regression — `grep -rn "bs_lod_cutoffs" byroredux/src` returns no consumers yet; the field is documented as "for an eventual M35 LOD selector." The blast radius is entirely forward-looking: when that selector is built, it will silently receive `None` for every mesh on both games, with no authored cutoff data to work from — the exact gap #1207 was opened to close, still open in practice.

## Suggested Fix

Either thread `NiLodTriShape`'s own `lod0_size`/`lod1_size`/`lod2_size` into `bs_lod_cutoffs` from the `NiLodTriShape` walker branch (the wire type that actually carries them on real SkyrimSE content), or drop the `.with_kind(MeshLOD)` override / add a `MeshLOD{lod0,lod1,lod2}` variant so `BSMeshLODTriShape`'s parsed cutoffs survive to import. Either way, rewrite the #1207/#988 regression tests to drive the real dispatcher on a synthetic `"BSLODTriShape"`/`"BSMeshLODTriShape"` block rather than hand-built fixtures, so they can't keep passing against an unreachable code path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other tri-shape LOD variants)
- [ ] **TESTS**: Rewrite #1207/#988 regression tests to drive the real block dispatcher, not hand-built fixtures

Source: docs/audits/AUDIT_NIF_2026-08-03.md (NIF-D4-01)

---

# Issue #2491
title:	MAT-D7-2026-08-07-02: hash_material_slice docstring cites a GpuMaterial::Hash impl that does not exist, with stale line anchors
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	documentation, low, renderer, vulkan
comments:	2
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2491
--
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


---

# Issue #2572
title:	OBL-D5-02: resolve_normal_alpha_spec_roughness post-mutates canonical roughness outside translate_material, with no canonical-tier test of the combined result
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, game:oblivion, legacy-compat, low, nifal, renderer
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2572
--
**Severity**: LOW
**Dimension**: NIFAL Canonical Material Translation for Oblivion
**Location**: `byroredux/src/material_translate.rs` (called from `cell_loader/spawn.rs:1553`, `scene/nif_loader.rs:934`)
**Status**: NEW

## Description
Both load paths call `resolve_normal_alpha_spec_roughness` consistently (no divergence today), but the gate (`normal_alpha_spec_applies`) is live for a meaningful swath of non-metal Oblivion content — Oblivion stores tangent-space normals in `NiTexturingProperty`'s bump slot with the specular mask in the DDS alpha, and `env_map_scale` is 0.0 unless the SLSF1 env bit is authored — so the alpha-normal roughness formula (derived from `NiMaterialProperty.shininess`) silently overrides the classifier's default matte roughness across Oblivion architecture/clutter. Whether the resulting values are correct is unmeasured (no Oblivion BSA reachable in that audit session — a ready-made census harness exists at `crates/nif/examples/_tmp_obl_d5_nifal.rs`).

## Evidence
Confirmed directly: `resolve_normal_alpha_spec_roughness` is called from both `spawn.rs:1553` and `nif_loader.rs:934`, both post-`translate_material`.

## Impact
Two concrete test gaps: the unit tests exercise the resolver as a pure function only, and the canonical-completeness harness deliberately bypasses classifiers so it never covers Oblivion's actual roughness population. Correctness of the resulting values is unmeasured.

## Suggested Fix
Run the census harness at `crates/nif/examples/_tmp_obl_d5_nifal.rs` against real Oblivion data to measure the actual roughness population; add canonical-tier test coverage of the combined `translate_material` + post-mutation result, not just the pure-function resolver.

## Completeness Checks
- [ ] **TESTS**: Canonical-tier test covers the combined `translate_material` + `resolve_normal_alpha_spec_roughness` result for representative Oblivion content


---

