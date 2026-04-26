# Issue #677: DEN-9: SVGF / TAA recreate_on_resize doesn't re-issue UNDEFINED→GENERAL one-time barrier

**File**: `crates/renderer/src/vulkan/svgf.rs:759-812`, `crates/renderer/src/vulkan/taa.rs:677-721`
**Dimension**: Denoiser & Composite

`initialize_layouts` is called once after `new()` (per docstring at svgf.rs:577-578). After `recreate_on_resize` the new images are again UNDEFINED, but `recreate_on_resize` does NOT call `initialize_layouts` — it relies on `frames_since_creation = 0` (line 781) so the first dispatch takes the `first_frame=1.0` branch and clears them.

But: the first dispatch's pre-barrier (svgf.rs:679) declares `old_layout = GENERAL`. If the image is actually still UNDEFINED post-resize, validation layer fires.

**Fix**: `recreate_on_resize` should re-issue the UNDEFINED→GENERAL one-time barrier that `initialize_layouts` does — either factor out a private helper or call `initialize_layouts` from inside `recreate_on_resize`. Same pattern needed for `taa.rs:677-721`.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
