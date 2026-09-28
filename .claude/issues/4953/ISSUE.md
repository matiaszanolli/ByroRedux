# #4953: REN-D3-2026-09-27-02: `0e0d35b96` made `GpuCamera`'s `NoUninit` SAFETY comment false — it deleted `[u32; 4]` while `render_debug: [u32; 4]` is a live field

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4953
- **Labels**: low,renderer,safety,documentation,doc-rot

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D3-2026-09-27-02**._

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:648-651` (`unsafe impl NoUninit for GpuCamera`); the field is at `gpu_types.rs:573` (`pub render_debug: [u32; 4]`)
- **Status**: NEW (collateral of the #4870 sweep; the correct text was already in place before `0e0d35b96`)
- **Description**: #4870 item 3 asked for `CompositeParams`' SAFETY text to name its `[u32; 4]` lane, and `0e0d35b96` did fix that. The same commit also rewrote `GpuCamera`'s SAFETY in the opposite direction. The old text was "every field is `[f32; 4]`, `[u32; 4]`, or `[[f32; 4]; 4]`"; the new text is "every field is `[f32; 4]` or `[[f32; 4]; 4]`".
  - `GpuCamera` has carried `render_debug: [u32; 4]` since `8e7582ed4` (2026-08-16).
  - The no-padding conclusion still holds, because a `[u32; 4]` is also a 16-byte lane.
  - The stated premise of an `unsafe impl` is now false, which is what #3761/#3990 exist to prevent.
- **Evidence**: `git show 0e0d35b96 -- crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` shows the `-// SAFETY: every field is [f32; 4], [u32; 4], or …` / `+// SAFETY: every field is [f32; 4] or …` hunk.
- **Impact**: No runtime effect. A future reviewer who checks the premise will find it false, and the comment no longer describes the struct it vouches for.
- **Related**: #4870 (open; items 1, 3 and 6 are now fixed and item 4 remains), #3761, #3990.
- **Suggested Fix**: Restore the `[u32; 4]` mention, naming `render_debug`. Fold this into #4870's remaining work.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds or touches `unsafe`/`NoUninit`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
