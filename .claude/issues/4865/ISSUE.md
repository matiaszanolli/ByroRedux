# #4865: REN-D11-2026-09-24-06: the Rust and C FFI structs are hand-mirrored with no cross-language layout pin

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D11-2026-09-24-06**._

- **Severity**: LOW as filed (layouts match today; a future one-sided edit would be an FFI layout violation, hence worth a pin; `/audit-safety` Dim 1 may also own it).
- **Dimension**: FSR/Presentation
- **Location**: `crates/fsr3-sys/src/lib.rs` `RawDispatchDesc`, `RawCreateDesc`, `RawImage`, `RawVersion`; `crates/fsr3-sys/native/byro_fsr3.h` `ByroFsr3DispatchDesc`, `ByroFsr3CreateDesc`, `ByroFsr3Image`, `ByroFsr3Version`.
- **Status**: NEW
- **Description / Evidence**: `dispatch_abi_structs_are_plain_and_pointer_width_stable` checks `size_of::<RawImage>() == 24`, `offset_of!(RawDispatchDesc, color)` and a `>=` bound; there is no `static_assert(sizeof/offsetof)` on the C side. Compiled side by side (gcc + rustc) the layouts are identical today (`ByroFsr3DispatchDesc` 248 B with `reset` @216, `depth_inverted` @244; `ByroFsr3CreateDesc` 48 B; `ByroFsr3Version` 24 B).
- **Suggested Fix**: `static_assert`s on the C side and a Rust test comparing `size_of` / `offset_of` to constants exported by the shim (already linked in `cargo test`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
- [ ] **FFI**: Rust and C struct layouts stay pinned across the boundary; pointer lifetimes across the FFI are sound

