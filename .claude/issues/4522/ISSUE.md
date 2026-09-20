# REN-3-2026-09-20-03: hash_gpu_material_fields doc still says '428 bytes' after #4422 grew GpuMaterial to 432

- **ID**: REN-3-2026-09-20-03
- **Labels**: low,renderer,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: GPU-Struct Layout
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-3-2026-09-20-03)

**Location**: `crates/renderer/src/vulkan/material.rs` — `hash_gpu_material_fields` doc

**Description**
The gpu_material_size_claims scanner misses this line because it names no Gpu* type on the same line; the size-pin test guards the struct, not the prose.

**Evidence**
Audit D3, 2026-09-20.

**Impact**
Doc rot on the dedup-hash hot path; the scanner's blind spot is the systemic issue.

**Suggested Fix**
Fix the number; optionally teach gpu_material_size_claims to catch bare 'bytes' claims near the symbol.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
