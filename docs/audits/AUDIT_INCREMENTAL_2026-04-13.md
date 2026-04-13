# Audit Suite Summary — quick (incremental) — 2026-04-13

Scope: Session 8 changes (35 commits), 3 parallel specialist audits.

| Audit | Findings | CRITICAL | HIGH | MEDIUM | LOW | INFO |
|-------|----------|----------|------|--------|-----|------|
| Renderer | 9 | 0 | 0 | 1 | 2 | 1 |
| ECS/Systems | 0 | 0 | 0 | 0 | 0 | 0 |
| NIF/Import | 1 | 0 | 0 | 0 | 1 | 0 |
| **Total** | **10** | **0** | **0** | **1** | **3** | **1** |

No CRITICAL or HIGH findings. Vulkan sync, memory layout, and resource lifecycle all verified correct.

---

## MEDIUM Findings

### R-02: debug_assert for draw sort order too permissive
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:334-341`
- **Description**: The `||` disjunction makes the assert pass whenever *either* pipeline key or sort depth is ordered. A sort regression where pipeline keys go backwards but depth happens to be non-decreasing will pass silently. The assert also omits `mesh_handle` from the batch key check. Should be:
  ```rust
  let k0 = (w[0].alpha_blend, w[0].is_decal, w[0].two_sided, w[0].mesh_handle);
  let k1 = (w[1].alpha_blend, w[1].is_decal, w[1].two_sided, w[1].mesh_handle);
  k0 <= k1
  ```
- **Impact**: Debug builds may not catch sort regressions that silently increase draw call counts.

---

## LOW Findings

### R-01: ui.vert GpuInstance has `_pad0` where Rust struct has `flags`
- **Location**: `crates/renderer/shaders/ui.vert:29`
- **Description**: The GLSL declares `uint _pad0` at offset 152 while the Rust struct and other shaders declare `uint flags`. Byte layout matches (both u32) but the name mismatch could cause confusion if the UI shader ever reads this field.

### R-09: Stray `/` comment syntax in triangle.frag
- **Location**: `crates/renderer/shaders/triangle.frag` (multiple lines)
- **Description**: Comment continuation lines start with `/` instead of `//`. In GLSL, single-line comments don't continue. If these are outside `/* */` blocks, recompilation would fail. Currently harmless if SPIR-V was compiled from the correct source.

### N-01: Unused `_mat` parameter in `extract_vertex_colors`
- **Location**: `crates/nif/src/import/material.rs:165`
- **Description**: `extract_vertex_colors` accepts `_mat: &MaterialInfo` but never reads it, re-deriving vertex color mode via `vertex_color_mode_for()` instead. Either use `_mat.vertex_color_mode` or remove the parameter. No correctness impact.

---

## Verified Correct (no action needed)

| Item | Status |
|------|--------|
| GpuInstance Rust/GLSL 160B layout (triangle.vert, triangle.frag) | Match |
| Shadow ray budget (MAX_SHADOW_RAYS=2) counter logic | Correct |
| BLAS eviction timing (idle threshold guards current-frame refs) | Safe |
| TLAS/BLAS destroy ordering (struct before buffer, BLAS before TLAS) | Correct |
| Instance SSBO overflow warning (log::warn at MAX_INSTANCES=8192) | Present |
| Present queue Mutex sharing (Arc::clone when same family) | Correct |
| Alpha test function extraction (bits 10-12, both NiTriShape + BSTriShape) | Correct |
| Dark texture import (all 7 NiTexturingProperty slots covered) | Correct |
| Bone refs borrow (as_slice replaces clone, fallback handles empty) | Correct |
| String read optimization (from_utf8 fast path, lossy fallback) | Correct |
| SubtreeCache invalidation (clears on Name count change) | Correct |
| Animation scratch buffers (channel_names, updates) | Correct |
| accum_root as Option<&str> (borrows from registry, outlives usage) | Correct |
| DarkMapHandle component registration | Correct |
| Material alpha_test_func + dark_map defaults | Correct |
