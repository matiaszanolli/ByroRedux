# Batch fix: #1857, #1897, #1914, #1915

## #1857 — TD1-001: context/draw.rs is 4265 LOC with a 1844-LOC draw_frame
**Labels**: bug, renderer, low, tech-debt
**Location**: `crates/renderer/src/vulkan/context/draw.rs:1992-3836` (`draw_frame`), `:784-1404` (`record_geometry_pass`), `:1404-1992` (`record_skinned_blas_refit`)

`context/draw.rs` is the largest file in the tree at 4265 LOC. `#1748` (closed) fixed
the original 3325-LOC `draw_frame` (now ~1844 LOC), but the file grew back around it:
`record_geometry_pass` (~620 LOC) and `record_skinned_blas_refit` (~588 LOC) add ~1200
more LOC of per-frame command-recording code. Suggested fix: extract cohesive recording
blocks into named `&self` helpers (not reordering barriers/passes) so `draw_frame` becomes
an orchestrator. Effort: large.

## #1897 — NIF-D2-05: ShaderFlags typed view + has_shader_property_fo3_fields are transitively dead in production
**Labels**: bug, nif-parser, low, nif
**Location**: `crates/nif/src/shader_flags.rs`, `crates/nif/src/version.rs` (`has_shader_property_fo3_fields`), `crates/nif/src/stream.rs` (`variant()`)

`version.rs` justifies keeping `has_shader_property_fo3_fields` on the grounds it "still
has a live consumer (shader_flags.rs)" — but `ShaderFlags::classify`/`is_decal`/`is_two_sided`
have zero production callers (test-only). `NifStream::variant()` has no production call site
either. The production material importer reads raw `flags1`/`flags2`/`sf1_crcs`/`sf2_crcs`
directly. Fix: either wire `ShaderFlags::classify` into the import/material path as a genuine
consumer, or delete `ShaderFlags` + `has_shader_property_fo3_fields` + `variant()`. Either way
remove the incorrect "live consumer" comment.

## #1914 — REN-D2-01: RL-03 per-light ambient fill is missing its stated point/spot gate
**Labels**: bug, renderer, medium
**Location**: `crates/renderer/shaders/triangle.frag:2191-2197`

The RL-03 fill's contract comment says "point/spot only... a true directional sun has no
'ambient' component," but the code has no `lightType` gate. For the exterior sun (type 2.0),
`isInteriorFill = radius < 0.0` is false, so execution falls into the ambient fill line,
adding an unshadowed `sunColor × albedo × 0.15` term to every exterior fragment — bypassing
RT shadows and ignoring N·L. Fix: add the gate the comment promises (skip fill for
`lightType >= 1.5`, or hoist inside point/spot arms only). One-line shader change + SPIR-V
recompile.

## #1915 — REN-D2-03: shader-pipeline.md descriptor + instance-flag tables lag the live Set-1 layout
**Labels**: documentation, renderer, low
**Location**: `docs/engine/shader-pipeline.md` vs `crates/renderer/shaders/include/bindings.glsl`

Four doc-rot divergences (code verified correct): (1) Set-1 table missing bindings 15/16/17
(`depthHistoryTex`, `ReservoirCurrBuffer`, `ReservoirPrevBuffer`, Session-49 ReSTIR); (2)
GpuInstance flags table omits bit 8 `INSTANCE_FLAG_DIFFUSE_ALPHA`; (3) binding-11 consumer
list wrongly includes "volumetrics" (it uses its own set-0 layout); (4) GpuLight section
says "prefixed by `u32 lightCount`" but the real prefix is a 16-byte header (`u32 count` +
3×`u32` pad). Fix: update the doc rows to match.
