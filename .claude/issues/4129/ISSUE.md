### REG-01: `_audit-common.md` GpuMaterial size reference is stale (432 B vs live 428 B)

- **Severity**: LOW
- **Dimension**: Regression / doc-rot
- **Location**: `.claude/commands/_audit-common.md:101`
- **Status**: NEW
- **Description**: The shared audit doc states GpuMaterial's "current size is pinned by `gpu_material_size_is_432_bytes`, not `_348_`" — but the live, passing test is `gpu_material_size_is_428_bytes` (`crates/renderer/src/vulkan/material.rs`). The shrink is deliberate and already fully reconciled in code: `a65dbffe` ("Fix #3909: remove GpuMaterial.texture_index, the unsampled lane in the dedup key") dropped the struct from 432 → 428 B and updated the test name, the offset pin, the GLSL field-name needle list, `bindings.glsl`, and the struct's own doc comments all in the same commit. Only this one shared-skill-doc line was left unsynced.
- **Evidence**: `cargo test -p byroredux-renderer gpu_` → `gpu_material_size_is_428_bytes ... ok` (53 passed total). Confirmed live: `crates/renderer/src/vulkan/material.rs` carries `gpu_material_size_is_428_bytes` at three call-sites (lines 74, 90, 399) plus one comment explicitly noting the 432→428 shrink (line 1380); no `gpu_material_size_is_432_bytes` test exists anywhere in the crate. `_audit-common.md:101` still reads "current size is pinned by `gpu_material_size_is_432_bytes`."
- **Impact**: None on shipped code — this is a documentation pointer used by future audits, not a code contract. A future auditor trusting this line would look for a nonexistent `_432_` test and could misreport a false regression.
- **Related**: #3909 (the actual GpuMaterial shrink fix).
- **Suggested Fix**: Update `_audit-common.md:101` to cite `gpu_material_size_is_428_bytes` and 428 B.

## Completeness Checks
- [ ] **TESTS**: n/a — documentation-only fix; no code test applies
