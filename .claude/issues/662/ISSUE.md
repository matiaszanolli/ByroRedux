# Issue #662: PIPE-4: skin-compute push constants _pad: u32 unused — could trim to 12 B

**File**: `crates/renderer/src/vulkan/skin_compute.rs:52`, `crates/renderer/shaders/skin_vertices.comp:66`
**Dimension**: Pipeline State

`SkinPushConstants` is `{ vertex_offset, vertex_count, bone_offset, _pad: u32 }` = 16 B. Shader-side `PushConstants` block declares the same `uint _pad` and never reads it. Three u32 fields = 12 B; Vulkan push constants don't require 16-B alignment of the whole block (only of vec4 members within), so the pad is decorative — it could be 12 B.

Not a correctness issue; there's no spec-mandated 16-B minimum for the push-constant range. The trailing pad costs an extra dword in the command stream and doesn't help std430 layout (no vec4 follows).

**Fix**: Drop `_pad` from both Rust struct + shader block, change `PUSH_CONSTANTS_SIZE` to 12, update the regression test. OR keep as-is and re-document the comment to clarify the pad is a future-proofing slot, not an alignment requirement.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
