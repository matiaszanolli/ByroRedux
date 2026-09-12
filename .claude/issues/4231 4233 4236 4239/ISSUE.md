# #4231 — FO4-D1-01: fo4-csg-format.md Implementation-status section contradicts its own Reading-an-object section on BSCRC32

**Severity**: LOW
**Dimension**: 1 — M49 Precombined Geometry
**Location**: `docs/engine/fo4-csg-format.md:110-124` vs `:213-214`
**Status**: NEW

**Description**: The doc's "Reading an object" section fully documents the `BSCRC32`/`csg_name_hash` algorithm and states the cell's owning plugin is *not* a reliable substitute for resolving the `.csg` blob. The "Implementation status" section still says the hash is "not yet reproduced" — the pre-#2369 state. `e9df743f` (#2369) landed after #1590 and hash-based resolution has shipped since.

**Suggested Fix**: Update the status bullet to state the BSCRC32 hash is implemented and resolves the `.csg` blob per-object (#2369), while the `_oc.nif` filename path separately keys off the owning plugin (#1590).

## Completeness Checks
- [x] **TESTS**: N/A (documentation-only fix)

**Disposition**: Rewrote the status bullet to state the hash is implemented (#2369), clarifying that owning-plugin resolution remains correct for the separate `_oc.nif` filename convention (#1590) — the two only diverge at the CSG blob lookup, not the filename.

---

# #4233 — FNV-D7-2026-09-11-01: RagdollTemplate attach comment's 'only skeletons do' claim is falsified by 150/220 non-skeleton FNV NIFs

**Severity**: LOW
**Dimension**: PHYSAL Ragdoll (FNV Reference Slice) — `/audit-fnv` Dimension 7
**Location**: `byroredux/src/scene/nif_loader.rs:537-556` (attach-site comment)
**Status**: NEW

**Description**: The `RagdollTemplate` attach-site comment asserts "only skeletons do" carry a Havok ragdoll articulation, falsified by a corpus sweep: 220 FNV NIFs return `Some(ImportedRagdoll)`, 150 with no `skeleton` in their path (armor gore variants, clutter, one extreme case with 110 bodies).

**Suggested Fix**: Rewrite the comment to state the real structural gate (≥1 constraint block, ≥2 bone-hosted bodies, ≥1 decoded joint) and name the FNV non-skeleton content that legitimately satisfies it.

## Completeness Checks
- [x] **SIBLING**: Checked for the same claim elsewhere — none found outside this issue's own audit report / snapshot.
- [x] **TESTS**: N/A — documentation-only fix.

**Disposition**: Rewrote the comment to describe `extract_ragdoll`'s actual gate (`crates/nif/src/import/collision/ragdoll.rs`: ≥1 authored `BhkConstraint`/`BhkBreakableConstraint` block, ≥2 bone-hosted rigid bodies, ≥1 decoded Ragdoll/LimitedHinge joint) and named the corpus population (220 NIFs, 150 non-skeleton) that satisfies it. SIBLING check found no other occurrence of the "only skeletons" claim outside the audit report and this issue's own filed text.

---

# #4236 — FO4-2026-09-11-D4-02: ImportedMesh::bs_lod_cutoffs is write-only — three producers, zero consumers

**Severity**: LOW
**Dimension**: 4 — NIF BSVER 130 + Half-Float + FO4 Collision
**Location**: `crates/nif/src/import/mesh/bs_tri_shape.rs:273-276`, `crates/nif/src/import/walk/mod.rs:574,950`, `crates/nif/src/import/types.rs:941`
**Status**: NEW

**Description**: `bs_lod_cutoffs` is populated from FO4 `BSMeshLODTriShape` and Skyrim `NiLodTriShape` but has no reader anywhere outside its own tests. Not a correctness bug (drawing all bands is correct max-detail render) but an unrealized distance-LOD optimization.

**Suggested Fix**: Either wire a distance test trimming the index range, or add a one-line doc note that the field is parsed-but-unconsumed pending that pass.

## Completeness Checks
- [x] **TESTS**: N/A — took the doc-note branch; no behavior wired.

**Disposition**: Took the doc-note option rather than wiring an actual distance-LOD selector — that would be new renderer-facing behavior (a triangle-index trim keyed on camera distance) with no `cargo test` signal for whether the visual result is correct, matching this project's established caution around speculative Vulkan/render changes. Added an explicit note on the field documenting it as write-only-by-design pending a future M35 selector, with the measured impact (12,096 FO4 shapes).

---

# #4239 — FO3-D1-2026-09-11-03: three FO3 shader-flag bits decode with no sink (External_Emittance, F2 Wireframe, F2 Premult_Alpha)

**Severity**: LOW
**Dimension**: FO3 Rendering Path (Inline Shaders) — `/audit-fo3` Dimension 1
**Location**: `crates/nif/src/shader_flags.rs:33-84` (constants), `crates/nif/src/import/material/legacy_properties.rs:386-670` (no consumer)
**Status**: NEW

**Description**: Three FO3 shader-flag bits decode with no sink: `External_Emittance` (F1 bit 29), F2 `Wireframe` (bit 16), F2 `Premult_Alpha` (bit 19). All three exist on FO3/FNV's `BSShaderFlags`/`BSShaderFlags2` per nif.xml (`prefix="F3SF1"`/`"F3SF2"`, `versions="#FO3#"`) but no FO3/FNV-specific named constant or consumer exists in `legacy_properties.rs`.

**Suggested Fix**: No action required immediately — documented for the next auditor / contributor working on emissive or alpha-blend fidelity.

## Completeness Checks
- [x] **TESTS**: N/A until a consumer is added.

**Disposition**: Verified the exact nif.xml bit positions (`BSShaderFlags` bit 29 = `External_Emittance`; `BSShaderFlags2` bit 16 = `Wireframe`, bit 19 = `Premult_Alpha`, both `#FO3#`-versioned). These bit positions/values are shared with the existing `fo4_slsf1::EXTERNAL_EMITTANCE`/`fo4_slsf2::WIREFRAME`/`fo4_slsf2::PREMULT_ALPHA` constants (same numeric value, same semantic on FO3 per nif.xml). Added doc notes on those three FO4 constants recording the shared FO3/FNV position and the missing consumer, following the issue's own "no action required immediately" disposition — this is a documentation-only fix per the issue's suggested action.
