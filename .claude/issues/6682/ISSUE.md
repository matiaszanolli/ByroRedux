# RF2-2026-11-05-02: `CameraUBO` is declared five times because each shader binds it at its own set and binding

Labels: low,renderer,shaders,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 2
**Status**: NEW. Part of #6680

## Location
- `crates/renderer/shaders/include/bindings.glsl:238` (set 1, binding 1)
- `crates/renderer/shaders/triangle.vert:77` (set 1, binding 1)
- `crates/renderer/shaders/water.vert:59` (set 1, binding 1)
- `crates/renderer/shaders/cluster_cull.comp:53` (set 0, binding 1)
- `crates/renderer/shaders/caustic_splat.comp:66` (set 0, binding 4)

## Why
A struct-only header does not fit this block, because each copy carries its own `layout(set, binding)`. `camera_ubo_glsl_copies_stay_in_lockstep` (`crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:3659`) catches drift, but adding a field still means editing five shaders.

## Change
Put the member list in a body-only header and include it inside each block's braces, so each shader keeps its own set and binding. First confirm `glslangValidator` accepts an `#include` inside a block. Then replace the lockstep test with a field-order check against `GpuCamera` plus a declared-once check.

## Constraints
`GcCameraUBO` (`crates/renderer/shaders/groundcover_blade.vert:239`) is out of scope: it is its own block with `gc`-prefixed members, not a sixth copy.

## Acceptance Criteria
- The member list exists in one header, and all five blocks include it
- A field-order test compares that header with the Rust `GpuCamera`
- `scripts/check-shader-artifacts.sh` passes, and recompiled modules are byte-identical apart from `OpSourceExtension` strings from a shader's first include

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6682 --json state`)._
