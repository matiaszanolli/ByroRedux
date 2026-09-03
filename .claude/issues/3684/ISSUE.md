# #3684 — PERF-D4-2026-08-30-04: CameraUBO is the only hand-duplicated GPU struct with no field name/order/type lockstep test

**Severity**: LOW · **Dimension**: SSBO Sizing & Upload
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera`), the five GLSL `uniform CameraUBO` declarations

## Fix

Added `camera_ubo_glsl_copies_stay_in_lockstep`, following the two-leg
pattern `GpuLight`/`GpuInstance` already established: leg 1 (mirror-vs-
mirror across all five GLSL copies via `strip_struct_body`, matching
`GpuLight`'s simpler approach since CameraUBO has no multi-name
declarations), leg 2 (typed name/order/type comparison against the Rust
`#[repr(C)] struct GpuCamera`, matching `GpuMaterial`'s typed leg).

Extended `rust_glsl_scalar_type_matches` (shared by `GpuMaterial`'s own
type-check leg) to recognize `GpuCamera`'s fixed-size-array field shapes
— `[f32; 4]` ↔ `vec4`, `[u32; 4]` ↔ `uvec4`, `[[f32; 4]; 4]` ↔ `mat4` —
since no prior struct in this file needed array/matrix types (`GpuMaterial`
is bare scalars only, per that function's own doc comment anticipating
exactly this extension).

Repointed the four stale `scene_buffer.rs` shader comments (`triangle.vert`,
`caustic_splat.comp`, `cluster_cull.comp` ×2) at `scene_buffer/gpu_types.rs`
— the file that struct actually lives in since the Session 34 split — per
the issue's own "adjacent, same root" note.

## A genuine, pre-existing finding surfaced while writing the typed leg

Two `GpuCamera` fields are named differently between the Rust struct and
every GLSL mirror: Rust `position` is GLSL `cameraPos`, and Rust `flags`
is GLSL `sceneFlags`. Both differences are **consistent across all five
GLSL copies** (leg 1 proves that), so this is a deliberate naming
convention (GLSL side names things by shader-author-facing meaning, Rust
side by the CPU struct field), not a drift bug — but it would have made a
strict name-equality check permanently red. Recorded as an explicit,
narrow two-entry alias table (`KNOWN_NAME_ALIASES`) rather than silently
loosening the check, so any *other*, genuinely accidental name mismatch
still fails loud.

## SIBLING (issue's own checklist item)

The four stale `scene_buffer.rs` references were the issue's own named
sibling finding — fixed above, in the same change.

Deliberately did **not** call `assert_mirror_list_is_complete` /
`shader_sources_declaring` for the five-source completeness check those
other tests get: that shared helper requires its `decl` argument to be
the exact START of a trimmed source line, which matches a plain `struct
X {` declaration but not `layout(set = N, binding = M) uniform CameraUBO
{` — the `layout(...)` qualifier always precedes it. Loosening that
already-tested, shared helper (used by three other structs' lockstep
tests) to a bare substring match would reopen the exact false-positive
its own doc comment names as the reason it isn't one already
(`skin_vertices.comp`'s comment *mentioning* `struct GpuInstance` while
declaring none). A sixth shader adding `CameraUBO` without joining this
test's `SOURCES` list is a real but narrower gap than the field-lockstep
defect this fix closes, and wasn't part of the issue's own suggested fix.

## TESTS (issue's own checklist item)

Verified the guard actually catches the regression it exists to prevent
(this session's established quality bar) with three separate probes,
each reverted after confirming:

- **Within-size reorder**: swapped `skyTint`/`sunDirection` in one of the
  five GLSL mirrors (`bindings.glsl`) — leg 1 caught it (layout mismatch
  between mirrors).
- **Type flip in one mirror**: changed `uvec4 renderDebug` to `vec4
  renderDebug` in a single file — leg 1 caught it (mirror disagreement).
- **Type flip in all five mirrors** (the class leg 1 alone *cannot* see,
  since all five would then agree with each other): changed `uvec4
  renderDebug` to `vec4` in every mirror — leg 2's type check caught it,
  correctly reporting the Rust/GLSL type mismatch specifically.

## Verification

- `cargo check --workspace --tests`: clean (one pre-existing, unrelated
  `unused_mut` warning in `esm/records/grup_walker.rs` predates this fix).
- `cargo test -q -p byroredux-renderer`: 817 tests passing, 0 failing
  (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7097 passing, 0
  failing**.
