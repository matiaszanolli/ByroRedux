# #4029 — REN-2026-09-06-D3-05: `GpuInstance`'s per-frame PCIe accounting still quotes 128 B per instance, two sizes behind

**Labels**: low, renderer, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D3-05), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/descriptors.rs` (`hash_instance_slice` doc comment), `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`SceneBuffers::upload_instances` dirty-gate comment)
- **Status**: NEW
- **Description**: Both comments size the instance dirty-gate's benefit at *"7359 draws at 128 B per `GpuInstance` ≈ 920 KB/frame … ~54 MB/s sustained PCIe at 60 fps"*, and both carry a parenthetical recording the previous correction (`"#2692 — was stated as 112 B / 805 KB / 48 MB/s, pre-#2219"`). #3231 then grew the struct 128 → 160 B and neither figure moved. The correct numbers are ≈ 1.12 MiB/frame and ≈ 67 MB/s.
- **Evidence**: `size_of::<GpuInstance>()` is pinned at 160 by `gpu_instance_is_160_bytes_std430_compatible`; 7359 × 160 = 1 177 440 B, × 60 = ~70.6 MB/s.
- **Impact**: Documentation only — the code reads `std::mem::size_of::<GpuInstance>()`, so no behaviour depends on the figure. It understates the dirty-gate's value by ~25 % in the two comments that justify its existence, and it sits directly adjacent to the `unsafe` safety argument in D3-02, where a reader checking one number and finding it stale has cause to distrust the other.
- **Related**: #2692 (the previous correction of the same figures); #3231 (the growth that stranded them). Same class as #3846.
- **Suggested Fix**: Restate as 160 B / ~1.12 MiB per frame / ~67 MB/s, and consider deriving the byte figure in the comment from `size_of::<GpuInstance>()` prose-side (i.e. state draws × `size_of`) so the next growth cannot strand it a third time.

---

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix
