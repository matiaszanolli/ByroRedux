# #3761 — SAFE-2026-08-30-D4-01: `GpuBuffer::write_mapped`'s SAFETY comment asserts "`T: Copy` guarantees no padding" — false for the generic bound

**Labels**: bug, renderer, medium, safety

---

- **Severity**: MEDIUM
- **Dimension**: 4 — Unsafe-Block Discipline
- **Location**: `crates/renderer/src/vulkan/buffer.rs` — `GpuBuffer::write_mapped`
- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (`SAFE-D4-01`), HEAD `64f64480`

## Description

```rust
pub fn write_mapped<T: Copy>(&mut self, device: &ash::Device, data: &[T]) -> Result<()> {
    // SAFETY: T: Copy guarantees no padding/drop concerns. The pointer is
    // valid and aligned (from a live slice), and size_of_val gives the
    // exact byte length.
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data))
    };
```

**`T: Copy` guarantees no `Drop` glue, but it says nothing about padding.** A `Copy`
`#[repr(C)]` struct whose fields do not tile its size (or one carrying
`#[repr(C, align(N))]` with a field sum that is not a multiple of `N`) has uninitialised
padding bytes, and materialising a `&[u8]` over them — then reading them in the
`copy_from_slice` below — is **UB by Rust's uninit-byte rules**.

This is precisely the invariant `bytemuck::Pod` exists to encode, and the crate is already
a workspace dependency (`crates/nif` uses `AnyBitPattern`).

This is a *distinct* false claim from the three that **#2683** corrected in the same
function: that issue fixed the `aligned_flush_range` "contained in the allocation"
assertions and left the `T: Copy` line untouched. This is the exact class the safety brief
prioritises — a commented `unsafe` block whose **stated invariant is false** — rather than
a missing comment.

## Evidence

- The bound is `T: Copy`, not `T: Pod` / `T: AnyBitPattern` / `T: NoUninit` (re-verified at
  HEAD).
- All 19 live call sites were enumerated and each `T` inspected; **every one is a
  `#[repr(C)]` GPU-contract struct that happens to be padding-free**: `GpuCamera`,
  `GpuDalcCube`, `GpuSelectedRayProbe`, `SsaoParams`, `DownsampleParams`, `UpsampleParams`,
  `CausticParams`, `CompositeParams`, `TaaParams`, `GpuWaterParams`,
  `vk::AccelerationStructureInstanceKHR`, and `&[u8]`.
- The two `#[repr(C, align(16))]` types — the ones where trailing padding *would* be
  introduced — are `VolumetricsParams` (all `mat4`/`vec4` fields) and `GpuFogVolume`
  (6 × `vec4` = 96 B, pinned by an `assert_eq!(size_of::<GpuFogVolume>(), 96)`). Both are
  clean.
- So this is a **latent** soundness gap, not a live one: nothing in the signature stops the
  next caller from passing a padded type, and **no test would catch it**.

## Impact

No current miscompile or corruption. The exposure is that a future `write_mapped` caller
with a padded `#[repr(C)]` param struct — **the natural thing to write when adding a new
compute pass** — silently introduces UB (reading uninit bytes) and simultaneously uploads
indeterminate padding to the GPU, **with the SAFETY comment actively vouching for it**. It
also nudges anyone reading the file toward believing `Copy ⇒ no padding`, which is wrong
and reusable in the wrong direction.

## Suggested Fix

Tighten the bound rather than the prose — the invariant then becomes compiler-enforced:

```rust
pub fn write_mapped<T: bytemuck::NoUninit>(&mut self, device: &ash::Device, data: &[T]) -> Result<()> {
    // SAFETY: `T: NoUninit` guarantees every byte of `T` is initialised (no
    // implicit padding), so the byte view contains no uninit bytes. The
    // pointer is valid and aligned (from a live slice) and `size_of_val`
    // gives the exact borrowed length.
    let bytes: &[u8] = bytemuck::cast_slice(data);
```

(`bytemuck::cast_slice` removes the `unsafe` entirely.) Each of the ~13 param structs then
needs `#[derive(bytemuck::NoUninit)]` (or `Pod`), **which is itself the drift guard being
asked for**. If the derive churn is unwanted, the minimum fix is to correct the comment to
state the *real* invariant — "every current call site passes a `#[repr(C)]` type whose
fields tile its size; a padded `T` would make this unsound" — so the next caller is warned
instead of reassured.

## Related

#2683 (CLOSED — corrected the other three false SAFETY assertions in this same function,
left this one); #84 (CLOSED — `write_mapped` silent truncation, the sibling robustness
issue); `crates/nif/src/stream.rs` (the correct pattern: `T: AnyBitPattern`).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant — here the fix *removes* an `unsafe`; verify no new one is introduced
- [ ] **SIBLING**: Same pattern checked in related files — other `from_raw_parts`-over-`T: Copy` byte views in the renderer crate
- [ ] **TESTS**: A regression test pins this specific fix — the `NoUninit` bound is the test; confirm all 19 call sites still compile
