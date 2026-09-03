# #3761 — SAFE-2026-08-30-D4-01: GpuBuffer::write_mapped's SAFETY comment asserts a false invariant

**Severity**: MEDIUM · **Location**: `crates/renderer/src/vulkan/buffer.rs` — `GpuBuffer::write_mapped`, `create_device_local_buffer`, `create_vertex_buffer`
**Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (SAFE-D4-01)

`write_mapped<T: Copy>`'s SAFETY comment claimed `T: Copy guarantees no
padding/drop concerns` — false: `Copy` says nothing about padding, only
about the absence of `Drop` glue. A `Copy` `#[repr(C)]` type whose fields
don't tile its size has uninitialised padding bytes, and viewing it as
`&[u8]` (as `write_mapped` does, then reads via `copy_from_slice`) is UB by
Rust's uninit-byte rules — this is exactly the invariant `bytemuck::NoUninit`
exists to encode. Latent, not live: every current call site happened to
pass a padding-free type. `create_device_local_buffer` (the function
`write_mapped`'s sibling `create_vertex_buffer` bottoms out in) carried the
identical false claim, unaddressed by the same #2683 that fixed this
function's three other false SAFETY assertions.

**Premise correction**: the issue's own evidence claims "the crate is
already a workspace dependency (`crates/nif` uses `AnyBitPattern`)" —
**false**. `bytemuck` is not a dependency anywhere in this workspace;
`crates/nif/src/stream.rs`'s `AnyBitPattern` is a **local, hand-rolled**
unsafe trait, whose own doc comment explicitly says "kept local to avoid a
new dependency." Verified via a workspace-wide grep before implementing.
This matters: the issue's primary suggested fix (`T: bytemuck::NoUninit`,
`bytemuck::cast_slice`) would require adding a new external crate, which
this project's fix-issue convention requires user approval for. Given the
premise was false and the project already has a working local-trait
precedent for exactly this problem shape, the fix mirrors that precedent
instead of adding the dependency.

## Fix implemented

A local `pub unsafe trait NoUninit: Copy {}` in `buffer.rs`, matching
`crates/nif`'s `AnyBitPattern` pattern exactly (same rationale, same
"kept local to avoid a new dependency" framing) but encoding the opposite
direction of the same invariant — `AnyBitPattern` is for reading arbitrary
bytes *into* a `T`; `NoUninit` is for treating an already-valid `T`'s bytes
as exportable `u8` data, which is what `write_mapped` does, so it does not
need `AnyBitPattern`'s `Default` bound. `write_mapped`, `create_device_local_buffer`,
and `create_vertex_buffer` all changed from `T: Copy` to `T: NoUninit`
(with `mesh::MeshRegistry::upload<V>` following the same chain), and the
SAFETY comments now state the real, compiler-enforced invariant instead of
the false one.

Also added `unsafe impl<T: NoUninit, const N: usize> NoUninit for [T; N] {}`
— a fixed-size array of a `NoUninit` element has no padding either (an
array's layout is exactly `N` copies of the element's layout with no gaps),
and several call sites pass an owned array directly as `T`.

Every type that instantiates any of these three functions across the crate
now carries an explicit, individually-reasoned `unsafe impl NoUninit for X {}`
— the compiler surfaced every real call site by refusing to build until
each was covered, which found **more** types than the issue's own "19 call
sites" enumeration: `GpuFogClusterEntry`, `GpuFogVolumeUpload`,
`IntegrationParams`, `SvgfTemporalParams` (the `write_mapped` family),
plus `Vertex` and `UiVertex` (the sibling `create_vertex_buffer` family,
found via the SIBLING check below). Each impl's safety comment states the
specific field-layout reasoning (homogeneous scalar/vector arrays under
`#[repr(C)]`, which — unlike GLSL std430 — gives an array only its
element's natural alignment, not an inflated 16-byte vec3/vec4 alignment;
`Vertex`'s comment cross-checks against the struct's own documented tight
104 B layout).

**SIBLING** (issue's own checklist item — "other `from_raw_parts`-over-`T:
Copy` byte views in the renderer crate"): found and fixed two more, both in
`buffer.rs`: `create_device_local_buffer<T: Copy>` (the exact same false
"T: Copy guarantees no padding" comment as `write_mapped`) and
`create_vertex_buffer<T: Copy>` (its caller, which delegates to the former
with no unsafe of its own — needed the same bound tightening to keep the
chain sound end-to-end). `mesh::MeshRegistry::upload<V: Copy>` — the actual
public entry point most callers use — was the same chain one level further
out and needed the same fix. The four `from_raw_parts` sites in
`scene_buffer/descriptors.rs` are typed directly to already-`NoUninit`-covered
structs (`GpuMaterial`/`GpuInstance`/`DrawCommand`/`GpuLight`), not a bare
generic `T`, so no separate fix was needed there.

**UNSAFE** (issue's own checklist item — verify no new unsafe is
introduced beyond the fix's own): the fix *removes* the false-premise
unsafe reasoning and replaces it with a true one; every new `unsafe impl
NoUninit for X {}` carries its own safety comment stating exactly which
field-layout property makes it sound.

**TESTS** (issue's own checklist item — "the `NoUninit` bound is the test;
confirm all 19 call sites still compile"): confirmed directly. The bound
tightening is inherently compiler-enforced — `cargo check -p
byroredux-renderer` and `cargo check -p byroredux` both compile clean with
zero new warnings, and the full renderer suite (816 tests) plus the full
workspace suite pass unchanged.

Full workspace: `cargo test --no-fail-fast` 7069 passing, 0 failing
(unchanged — this fix adds no new runtime tests, only compile-time
guarantees).
