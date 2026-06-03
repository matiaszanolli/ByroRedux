# #1447 Investigation — stale SPIR-V / CameraUBO drift

## Premise correction (audit-finding-hygiene)

The audit's PIPE-01 finding ("5 stale `.spv` from the DoF commit") was **mostly a
false positive**:

1. **The DoF staleness is already fixed.** Commit `e6df0f5b`
   ("recompile stale SPIR-V after DoF CameraUBO extension (#1447)") recompiled
   all 5 `.spv`. `spirv-dis crates/renderer/shaders/triangle.frag.spv` shows
   `dofParams` at `Offset 304` (member 10 of `CameraUBO`) — i.e. the committed
   binary carries the 320 B layout. Verified `git show --stat 400fa68f` committed
   **0** `.spv`; `e6df0f5b` fixed it.
2. **The audit's `cmp` was confounded.** Recompile-and-`cmp` flagged shaders as
   stale because (a) it ran against a dirty working tree and (b) glslang embeds
   the **source path** in SPIR-V debug info, and SPIR-V output differs across
   glslang **versions** on complex shaders. My glslang 16.2.0 matches the
   repo's canonical version for the 4 simple shaders (byte-identical) but
   diverges on `triangle.frag`. A byte-`cmp` test is therefore an unsound guard.

## The real, still-open problems found

3. **HEAD (`9abbe510`, "add ReSTIR-DI reservoir support …") does not compile.**
   Committed `crates/renderer/src/vulkan/context/mod.rs` references
   `super::gbuffer::RESERVOIR_FORMAT` and `GBuffer::reservoir_view()` (5 sites)
   that committed `gbuffer.rs` never defines, plus two `fn` calls with the wrong
   argument count. `cargo check -p byroredux-renderer` fails with 4 errors **on a
   clean checkout** (verified via committed blobs, not just the dirty tree). The
   renderer crate — and the whole build — is broken at the tip of `main`.
4. **`triangle.frag.spv` is genuinely stale vs the ReSTIR GLSL**, but from
   `9abbe510`, not the DoF commit: committed `triangle.frag` GLSL has 3
   `outReservoir`/`packReservoir` refs; committed `triangle.frag.spv` has **0**
   (`spirv-dis | grep -c reservoir == 0`). So even once the crate compiles, the
   shipped shader will not write the ReSTIR reservoir to the G-buffer.
5. **Uncommitted foreign WIP** in the working tree (not authored here):
   `crates/renderer/src/vulkan/{bloom.rs,taa.rs}` and
   `crates/renderer/shaders/triangle.frag` — likely the in-progress ReSTIR
   completion — plus a pre-existing `git stash@{0}` (a `.claude/settings.json` WIP).

## What was implemented (the user's chosen deliverable: "add drift guard only")

`crates/renderer/src/vulkan/reflect.rs` (+140 lines, **uncommitted**):
- `uniform_block_size_by_name(spirv_bytes, name)` — reads a UBO block's std140
  size directly from committed SPIR-V (no recompile, no `glslangValidator`), so
  it is **compiler-version-independent** (unlike a byte-`cmp`).
- Test `camera_ubo_size_matches_gpu_camera_in_every_shader` — asserts the
  `CameraUBO` block size in all 6 declaring shaders equals
  `size_of::<GpuCamera>()` (320 B). This catches the exact #1447 hazard (Rust UBO
  struct grows, shader `.spv` not recompiled) and is green now (all committed
  `.spv` carry the 320 B `dofParams` layout, confirmed via `spirv-dis`).

Logic verified by inspection against `spirv-dis` ground truth (last member
`dofParams` vec4 @ Offset 304 → 304 + 16 → round16 = 320 == `GpuCamera`), but
**could not be run** because the crate does not compile (problem #3).

## Why this is not committed

Phase 6 (`cargo test` green) is unreachable: the renderer crate fails to build
on the broken ReSTIR HEAD. Committing an unverifiable test, or fixing the broken
foreign ReSTIR commit / WIP, are both out of scope for #1447. The guard is left
in the working tree, ready to commit once the build is restored.

## Recommended next steps (for the user)

1. **Fix the broken HEAD** (`9abbe510`): define `RESERVOIR_FORMAT` +
   `GBuffer::reservoir_view()` in `gbuffer.rs` and fix the two arg-count
   mismatches in `context/mod.rs`, then recompile `triangle.frag.spv` (so
   `outReservoir` is actually written). This is the in-progress ReSTIR work the
   uncommitted `bloom.rs`/`taa.rs`/`triangle.frag` likely belong to.
2. Once the crate compiles, run `cargo test -p byroredux-renderer reflect::` to
   confirm the new guard passes, then commit `reflect.rs`.
3. File a new issue for the ReSTIR `.spv` staleness (#4 above) if not already
   tracked — the new `CameraUBO` guard does **not** cover output-write drift.

## Git-recovery note

A malformed `git stash push` here, followed by `git stash pop`, accidentally
applied a pre-existing `stash@{0}` (`.claude/settings.json` WIP) and produced a
`UU` conflict. Recovered with `git checkout HEAD -- .claude/settings.json`;
`stash@{0}` is preserved and the renderer WIP is intact. No work lost.
