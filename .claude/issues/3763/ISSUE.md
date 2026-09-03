# #3763 — SAFE-2026-08-30-D6-02: GpuLight has no Rust↔GLSL lockstep leg and no size pin

**Severity**: MEDIUM · **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`, `gpu_types.rs`
**Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (SAFE-D6-02)

`GpuMaterial` and `GpuInstance` each have two legs of protection: mirror-vs-
mirror across their GLSL copies, and mirror-vs-Rust field order/type
against the `#[repr(C)]` struct. `GpuLight` — the *most* mirrored struct
after `GpuInstance` (four GLSL copies) — only had the first leg;
`gpu_light_glsl_copies_stay_in_lockstep` never looked at `gpu_types.rs` at
all. There was also no `size_of::<GpuLight>()` assertion anywhere in the
crate, unlike `GpuInstance` (160 B) and `GpuCamera` (368 B). A `GpuLight`
field appended or reordered on the Rust side alone would pass every test in
the workspace and silently upload a stride the shaders don't decode.
`gpu_instance_glsl_copies_stay_in_lockstep`'s own doc comment additionally
claimed `GpuLight` already had this second-leg coverage — false at the
time, now true.

## Fix implemented

Both parts of the issue's own suggested fix, reusing the exact machinery
`gpu_instance_glsl_copies_stay_in_lockstep` already established:

1. **Second leg**: `gpu_light_glsl_copies_stay_in_lockstep` now also parses
   `gpu_types.rs`'s `pub struct GpuLight` via `parse_rust_struct_fields`,
   normalizes both sides with `normalize_ident`, and asserts field count
   and order match the shared GLSL field list (any one of the four already-
   proven-identical mirrors stands in for all four). Leg 1 (the existing
   four-way mirror comparison) is untouched.
2. **Size pin**: `gpu_light_is_64_bytes` in `gpu_instance_layout_tests.rs`,
   next to the `GpuCamera`/`GpuInstance` siblings.

The `gpu_instance_glsl_copies_stay_in_lockstep` doc comment's "the same
two-leg coverage `GpuMaterial` and `GpuLight` already have" claim needed no
correction — this fix makes it true rather than false, so it was left as-is.

**Verified the new guard actually catches the drift class it exists for**:
temporarily swapped `GpuLight`'s `color_type`/`position_radius` field
declaration order in `gpu_types.rs` (names preserved, so only order
changed, isolating the exact defect class from the issue) — the new second
leg failed immediately with the expected `field #0 ORDER mismatch:
Rust color_type vs GLSL position_radius` message; reverted, reran, both
tests pass again.

**SIBLING** (issue's own checklist item): audited every `Gpu*` struct in
`gpu_types.rs` for which legs it has:

- `GpuInstance`, `GpuMaterial` — already have both legs (pre-existing).
- `GpuLight` — now has both legs (this fix).
- `GpuCamera` — a `CameraUBO` re-declared in six shaders, but pinned
  against the shipped `.spv` via `reflect.rs`'s SPIR-V reflection
  (`uniform_block_size_by_name`) rather than source-text comparison — a
  stronger check than the source-parsing pattern this issue's fix uses,
  already in place.
- `GpuDalcCube` — one GLSL declaration site (`DalcCubeUBO` in
  `bindings.glsl`; `triangle.frag` only *uses* the type via `#include`, it
  doesn't redeclare it), pinned by `reflect.rs`'s SPIR-V reflection size
  check (#2464). No mirror-vs-mirror class of bug applies — nothing to
  mirror against.
- `GpuTerrainTile` — also one GLSL declaration site (same
  `bindings.glsl`-only shape as `GpuDalcCube`), has a size pin
  (`gpu_terrain_tile_is_96_bytes`) but no explicit Rust-vs-GLSL field-order
  check. Lower risk than `GpuLight`'s pre-fix state (only one declaration,
  no mirror-vs-mirror drift possible), but a same-size same-type field swap
  wouldn't be caught by the size pin alone. Noted here as a minor,
  unconfirmed residual gap rather than filed separately — no evidence of
  live drift, and out of this issue's own stated `GpuLight` scope.

**TESTS** (issue's own checklist item — "the fix *is* the test"): the fix
consists entirely of new/extended tests, verified directly against a
synthetic drift as described above.

Full workspace: `cargo test --no-fail-fast` 7069 passing, 0 failing (+1 new
test — the second leg extends an existing test function rather than adding
a new one, `gpu_light_is_64_bytes` is the +1).
