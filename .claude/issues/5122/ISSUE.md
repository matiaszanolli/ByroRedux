# SAFE-D2-2026-09-29-01: `groundcover_models.rs::push_bytes` hand-rolls a `from_raw_parts` byte view that bypasses `byte_view` / `NoUninit`

**Labels**: low,safety,renderer,memory,bug

**Source report**: `docs/audits/AUDIT_SAFETY_2026-09-29.md`

- **Severity**: LOW. The view is sound today; this is a hardening and consistency gap.
- **Dimension**: Memory Corruption / UB
- **Location**: `crates/renderer/src/vulkan/groundcover_models.rs:125-149` (`ModelPush` at :127, `push_bytes` at :140) (`ModelPush`, `push_bytes`). New in aabd99a05 (#4413).
- **Status**: NEW. #4445 and #4521 (both CLOSED) made `buffer::byte_view` the single sanctioned `T → &[u8]` path and removed hand-rolled siblings. They covered SSBO and hash views, not push constants. This is the only *new* `from_raw_parts` site in the window; the baseline-vs-HEAD grep diff is in `dim_2.md`.
- **Description**: `ModelPush` is `#[repr(C)]`: 2 × `[f32; 4]` plus 8 four-byte scalars, 64 B with no padding. Its size is pinned by `host_mirrors_match_the_shader_strides`, and its field order matches GLSL `GcModelPush` field for field. So the SAFETY text ("every byte is initialised") is true. But the proof lives in prose. `ModelPush` does not `impl NoUninit`, so a future field that introduces padding (for example a `u16` or `bool` lane) compiles silently, and the pushed range then includes uninitialised bytes. That is the exact hazard the `NoUninit` gate exists to stop at compile-review time.

  Several pre-existing push-constant casts use the same shape (in `svgf.rs`, `skin_compute.rs`, `morph_compute.rs`, `water.rs`, `presentation.rs`, `groundcover.rs` and `groundcover_bench.rs`). None is new.
- **Evidence**:
  ```rust
  fn push_bytes(push: &ModelPush) -> &[u8] {
      // SAFETY: `#[repr(C)]` over 4-byte scalars with no padding, so every byte
      // is initialised; the slice borrows `push`.
      unsafe {
          std::slice::from_raw_parts(
              (push as *const ModelPush).cast::<u8>(),
              std::mem::size_of::<ModelPush>(),
          )
      }
  }
  ```
- **Impact**: None today. The regression shape is latent UB (reading uninitialised padding into `vkCmdPushConstants`) that no test would notice.
- **Related**: #4445, #4521, #3990, Dim 6 layout pins.
- **Suggested Fix**: Add `unsafe impl NoUninit for ModelPush {}`, with the same one-line padding proof its sibling `GpuGroundCoverModelRecord` carries. Then call `buffer::byte_view(std::slice::from_ref(&push))` and delete `push_bytes`. Optionally sweep the older push-constant casts onto the same path in one follow-up.

**Validated at HEAD 9fcfdc3fc**: `crates/renderer/src/vulkan/groundcover_models.rs` `push_bytes` (:140) still builds the byte view with `std::slice::from_raw_parts`; `ModelPush` (:127) has no `unsafe impl NoUninit`; `buffer::byte_view<T: NoUninit>` exists (`buffer.rs:56`).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
