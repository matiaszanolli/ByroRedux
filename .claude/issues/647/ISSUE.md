# Issue #647: RP-1: mesh_id R16_UINT — comment ceiling off-by-one + no overflow guard

**File**: `crates/renderer/src/vulkan/gbuffer.rs:39`, `crates/renderer/src/vulkan/context/helpers.rs:54-56`
**Dimension**: Render Pass / G-Buffer

`MESH_ID_FORMAT = R16_UINT`; shader writes `instance_id + 1` with 0 reserved for background. Comment block says "65534-instance ceiling" but the actual usable range is `[1, 65535]` → 65535 instances, not 65534.

More importantly: triangle.vert writes `instance_index + 1` blindly with **no runtime guard**. If the caller batches more than 65535 visible instances into a single draw, the value silently wraps in the R16_UINT attachment, mapping multiple distinct meshes to the same ID and breaking SVGF disocclusion (history reads accept stale samples from an unrelated mesh → ghosting).

Skyrim/FO4 city cells routinely exceed 50K REFRs.

**Fix**:
- (a) Update comment to "65535-instance ceiling".
- (b) Add a debug-only `assert!(visible_instances < 0xFFFF)` in the per-frame instance gather.
- (c) On overflow, either bump to R32_UINT (+8 MB at 1080p, requires SVGF temporal binding update) or partition draws across multiple frames. R32 is the cleaner long-term path once instance counts justify it.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
