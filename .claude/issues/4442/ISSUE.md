# #4442: REN-2026-09-16-D7-03: #4201 made `material_hash` build the struct, but the two call sites still route through `intern_by_hash` with a second build and comments saying the build is skipped; three doc sites still describe the retired field walk

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4442
- **Labels**: low,renderer,tech-debt,documentation,doc-rot
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW. The code is correct; the prose is stale, and a miss does
  redundant work.
- **Dimension**: Material Table
- **Location**:
  - The call sites: `collect_static_mesh_draws` (`byroredux/src/render/static_meshes.rs`)
    and `emit_particles` (`byroredux/src/render/particles.rs`), both
    `intern_by_hash(cmd.material_hash(), || cmd.to_gpu_material())`.
  - The comment at the static-mesh site: "`intern_by_hash` skips the
    `to_gpu_material()` construction on the dedup-hit path".
  - The `MaterialTable::intern_by_hash` doc in `crates/renderer/src/vulkan/material.rs`:
    "`to_gpu_material` (the dominant construction cost) is skipped on the ~97%
    dedup-hit path", and "a pure function of the same fields … in the same order".
  - The `GpuMaterial` doc: "the byte-level `Hash`/`Eq` impls below"
    (`GpuMaterial` has no `Hash` impl).
  - The `byroredux/src/cornell.rs` doc on the probe `material_alpha`:
    "material.rs writes `mat.material_alpha.to_bits()`".
- **Status**: NEW (a side effect of `479ce5266` / #4201, today)
- **Description**:
  - **What changed.** #4201 redefined `DrawCommand::material_hash` as
    `hash_gpu_material_fields(&self.to_gpu_material())`, and its own doc says so.
  - **What that leaves at the call sites.** On a dedup miss the struct is now
    built twice: once for the hash, once in the factory closure. In debug builds
    it is built twice on every hit too, because of the collision check. The
    closure-on-miss API exists only to skip a build that no longer gets skipped.
  - **Stale descriptions.** The listed comments still describe that skip, or the
    per-field `to_bits()` walk #4201 removed.
- **Evidence**: `pub fn material_hash(&self) -> u64 { super::super::material::hash_gpu_material_fields(&self.to_gpu_material()) }`
  in `crates/renderer/src/vulkan/context/types.rs`; the two call sites above.
- **Impact**:
  - The redundant build on the roughly 3% miss path is small but free to remove.
  - Readers are told a performance property that no longer exists, and are told
    to keep a field order that no longer matters.
- **Related**: #4201, #781, #3568
- **Suggested Fix**:
  1. At both call sites, call `material_table.intern(cmd.to_gpu_material())`
     (one build, one hash). Alternatively, keep `intern_by_hash` but hash a
     locally built struct and move it into the closure.
  2. Rewrite the four comments to describe the byte-hash.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
