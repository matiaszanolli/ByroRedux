# Investigation — #833

**Domain**: nif (stream)

## Approach

The audit's preferred fix uses `bytemuck::cast_slice_mut` — but **bytemuck is not a workspace dependency** (`grep -rn bytemuck` across all `Cargo.toml` returns nothing despite the audit's claim). Adding it requires user approval per project rules.

The audit's documented alternative — `read_exact` into `vec.spare_capacity_mut()` then `set_len(count)` — sidesteps bytemuck. To stay within the safety contract without `MaybeUninit` choreography, I use `vec![T::default(); count]` + `slice::from_raw_parts_mut(ptr as *mut u8, count * size_of::<T>())` + `read_exact`. The Vec is zero-initialized first; `read_exact` overwrites the bytes; the Vec keeps its size. One small `unsafe` block per reader with a SAFETY comment.

## Endianness

NIF format is LE; bulk readers cast typed Vec → byte slice, so the host's endianness has to match. Every supported target (x86_64, aarch64) is LE. Add `#[cfg(target_endian = "big")] compile_error!(...)` at the top of `stream.rs` to fail fast on any future BE port (e.g. PowerPC), documenting the assumption. The per-element `from_le_bytes` paths elsewhere in the file are host-agnostic; only the bulk readers need the gate.

## Layout

`NiPoint3` is 3×f32 with `#[derive(Debug, Clone, Copy, Default)]`. No `#[repr(C)]` today. Adding `#[repr(C)]` is a no-op for this struct in practice — 3 same-size fields pack with no padding regardless — but documents the layout contract for the byte cast. The other element types are already POD: `u16`, `u32`, `f32`, `[f32; 2]`, `[f32; 4]`. No FFI exposure to break (no `cxx-bridge` reference to `NiPoint3`).

## Affected functions

`crates/nif/src/stream.rs:251-348`:
- `read_ni_point3_array` → `Vec<NiPoint3>`
- `read_ni_color4_array` → `Vec<[f32; 4]>`
- `read_uv_array` → `Vec<[f32; 2]>` (and its `read_vec2_array` alias — no change needed)
- `read_u16_array`, `read_u32_array`, `read_f32_array` → `Vec<{u16,u32,f32}>`

That's 6 unique implementations (`read_vec2_array` just delegates).

## Test strategy

Existing parse tests cover correctness via real-NIF round-trip. The behavioural change is invisible to them (output Vec content unchanged byte-for-byte on LE hosts). The allocation-count win requires dhat to verify, which the project doesn't have today (same precedent as #823, #828, #832). Add a targeted unit test that drives each rewritten reader with a known byte pattern and asserts the typed output — pins the byte-order contract so a future bytemuck migration or BE port can't silently flip endianness.

## Scope

2 files: `crates/nif/src/stream.rs` (rewrite + compile-error gate + tests) and `crates/nif/src/types.rs` (one `#[repr(C)]` annotation).
