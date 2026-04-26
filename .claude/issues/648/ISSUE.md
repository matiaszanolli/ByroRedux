# Issue #648: RP-2: G-buffer images sampled by SVGF temporal before any color write on first 2-3 frames after resize

**File**: `crates/renderer/src/vulkan/gbuffer.rs:246-305`, `crates/renderer/src/vulkan/composite.rs:887-953`
**Dimension**: Render Pass / Layouts

After resize, `gbuffer.recreate_on_resize` allocates new images at UNDEFINED → `initialize_layouts` transitions them to SHADER_READ_ONLY_OPTIMAL with `src_access = empty` and `dst_access = SHADER_READ`. Between this and the first main render pass, SVGF temporal reads previous-frame-in-flight slot's raw_indirect / motion / mesh_id / normal — driver returns whatever the freshly-allocated memory holds (typically black). SVGF's history weight can amplify this into a black-frame bloom on the first 2-3 frames after every resize.

Not a validation error, but a correctness/quality finding.

**Fix**: Either:
- Clear each G-buffer attachment via `vkCmdClearColorImage` after `initialize_layouts` (adds GPU work every resize), OR
- Have SVGF detect "history is unusable" via a per-frame epoch counter that resize bumps, and force `alpha = 1.0` (full reset) on the first 2 frames after resize. Cleaner — no extra GPU work.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
