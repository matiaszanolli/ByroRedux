# Issue #673: DEN-2: SSAO dispatch() emits UNDEFINED→GENERAL every frame, discarding initialize_ao_images clear

**File**: `crates/renderer/src/vulkan/ssao.rs:514-536`
**Dimension**: Denoiser & Composite

The pre-dispatch barrier uses `old_layout = UNDEFINED` (line 518), which the Vulkan spec defines as discarding image contents. `initialize_ao_images` (lines 404-466) carefully clears each AO image to 1.0 and transitions to SHADER_READ_ONLY_OPTIMAL — but on the very next frame, `dispatch()` executes a UNDEFINED→GENERAL barrier and the cleared 1.0 contents are formally discarded.

Since the compute shader writes every pixel anyway this is normally invisible, but if the dispatch ever fails partially (early-out bounds check, lost device) the image content is undefined.

Cross-checked: `svgf.rs:679` and `taa.rs:613` both use `old_layout = GENERAL` for steady-state ping-pong barriers (correct). SSAO is the outlier.

**Fix**: After the first dispatch, transition from `SHADER_READ_ONLY_OPTIMAL` (the layout `initialize_ao_images` left it in, and the layout the previous frame's post-dispatch barrier puts it in) → `GENERAL`. Cleanest: `old_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL` and rely on the steady state.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
