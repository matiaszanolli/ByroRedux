# #4445: REN-2026-09-16-D7-05: `GpuMaterial` has three independent `unsafe` byte views, and #4201 made the one with a prose-only safety argument the dedup identity

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4445
- **Labels**: low,renderer,safety,tech-debt,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW (hardening / duplication; every view is currently sound)
- **Dimension**: Material Table / GPU-Struct Layout
- **Location**:
  - `GpuMaterial::as_bytes` (`crates/renderer/src/vulkan/material.rs`): used by
    `PartialEq` and, since #4201, by `hash_gpu_material_fields`.
  - `hash_material_slice` (`crates/renderer/src/vulkan/scene_buffer/descriptors.rs`).
  - `byte_view<T: NoUninit>` (`crates/renderer/src/vulkan/buffer.rs`): private,
    and used by `write_mapped`.
- **Status**: NEW
- **Description**: #3990 introduced `unsafe trait NoUninit` so that the
  "no uninitialised bytes" argument lives at the type level, and `GpuMaterial`
  implements it. The two material-specific views still carry their own `unsafe`
  blocks with prose justifications instead of reusing the bounded helper.
  - `as_bytes`' prose says "all padding bytes are named fields the producer always
    initialises".
  - `hash_material_slice` says "explicit padding fields". `GpuMaterial` has had no
    pad fields since #3909.

  Both still hold, because every field is a 4-byte scalar. They are the kind of
  argument #3990 and #3761 set out to retire, and `as_bytes` is now the key that
  decides material identity.
- **Evidence**:
  - `unsafe { std::slice::from_raw_parts(self as *const Self as *const u8, …) }`
    in `as_bytes`.
  - The same pattern over a slice in `hash_material_slice`.
  - `fn byte_view<T: NoUninit>(data: &[T]) -> &[u8]` (not `pub(crate)`).
- **Impact**: None today. A future `GpuMaterial` field that introduces padding
  would be caught by `NoUninit`'s audit point only at `write_mapped`, not at the
  dedup hash or equality.
- **Related**: #3990, #3761, #4201
- **Suggested Fix**: Make `byte_view` `pub(crate)` and implement `as_bytes` and
  `hash_material_slice` on top of it. This removes two `unsafe` blocks, and the
  per-user rule favours improving the shared helper over keeping copies.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **TESTS**: A regression test pins this specific fix
