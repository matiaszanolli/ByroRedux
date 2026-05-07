# Investigation — #878 (DIM8-01)

## Domain
renderer (Vulkan SSBO upload)

## Hot path

`SceneBuffers::upload_materials`
(`crates/renderer/src/vulkan/scene_buffer.rs:999`) is called every
frame from `draw_frame`. It does
`std::ptr::copy_nonoverlapping(materials.as_ptr() as *const u8,
mapped.as_mut_ptr(), count * sizeof::<GpuMaterial>())` followed by
`flush_if_needed(device)`. For a static interior cell where
`build_render_data` produces a byte-identical materials slice each
frame, this is pure waste — ~3 MB/s steady-state PCIe traffic at 60
fps for an unchanged buffer.

## Hash primitive choice

The audit suggests xxh3. xxh3 would be a new dependency
(`xxhash-rust` or similar). Rust's std library provides
`std::collections::hash_map::DefaultHasher` (SipHash-1-3) which:
  * has documented stable state across `new()` calls within a
    process (not randomized — that's `RandomState`),
  * hashes ~1.5 GB/s, so a 200-material upload (~52 KB) takes
    ~30 µs — well under the per-frame budget at 60 fps,
  * is high-quality (SipHash is HashDoS-resistant), so collisions
    on our data sizes are statistically vanishing.

Tradeoff vs xxh3: ~10x slower hash but no new dep + no version-
contract risk. Given the hash cost is below the per-frame signal
floor either way, std SipHash is the right call. xxh3 can replace
it later if profiling shows the hash itself is hot.

## Field shape

`[Option<u64>; MAX_FRAMES_IN_FLIGHT]`. `Option` makes the "first
upload always fires" case explicit (vs `[u64; ...]` initialised to 0
where a coincidental hash of 0 would wrongly skip — vanishingly
unlikely with SipHash but `Option` removes the corner case).

## GpuMaterial Pod-ness

`GpuMaterial` is `#[repr(C)]` with explicitly-named padding fields,
verified via the existing byte-level `Hash`/`Eq` impls at
`vulkan/material.rs:280-309`. The `as_bytes(&self) -> &[u8]` helper
already unsafe-casts a single `GpuMaterial` to bytes; a slice view
follows the same safety reasoning.

## Files affected

1. `crates/renderer/src/vulkan/scene_buffer.rs` (one function +
   one struct field + struct constructor)

Single file. Test coverage: pure-Rust unit test for the hash
comparison logic if it can be factored out; otherwise rely on
existing GpuMaterial layout tests + integration test suite.

## Test approach

The `upload_materials` function itself touches `mapped_slice_mut` /
`flush_if_needed` which need a real Vulkan buffer. The hash-and-skip
DECISION is testable in isolation if we factor it into a small
helper. Plan: extract a `should_skip_material_upload(prev_hash,
new_hash) -> bool` helper or just compute the hash inline and pin
that two identical slices produce the same hash, two distinct
slices produce different hashes.
