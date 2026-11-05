# RF2-2026-11-05-01: `MorphDeltaRef` and `MorphWeightRef` are hand-copied across three shaders with no parity guard

Labels: low,renderer,shaders,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 2
**Status**: NEW. Part of #6680

## Location
- `crates/renderer/shaders/include/bindings.glsl:28-31`
- `crates/renderer/shaders/triangle.vert:55-58`
- `crates/renderer/shaders/skin_vertices.comp:87-90`
- Host layout: `crates/renderer/src/vulkan/morph_compute.rs:76`

## Why
`b8233b4b2` gave six shared GPU structs one struct-only header each, pinned by `shared_gpu_structs_are_declared_once`. These two `buffer_reference` blocks stayed as copies. `skin_vertices.comp:84` says they are "not covered by an automated parity test", so a change to one copy can ship without the others. Of the duplicates left after that commit, these have the least protection.

## Change
Move both blocks into one header under `crates/renderer/shaders/include/` and include it from the three shaders. Extend the declared-once check to recognise the `buffer X` form.

## Constraints
`shared_gpu_structs_are_declared_once` (`crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:323`) fails on any `layout` in these headers, and these blocks need `layout(buffer_reference, ...)`, so it needs a narrow exception. The includer supplies the `GL_EXT_buffer_reference` extension directive. Recompile with plain `-V -I.`.

## Acceptance Criteria
- Each block is declared once, in a header included by `bindings.glsl`, `triangle.vert` and `skin_vertices.comp`
- A second declaration in any shader fails a test, shown by injecting that fault once
- `scripts/check-shader-artifacts.sh` passes, and recompiled modules are byte-identical apart from `OpSourceExtension` strings from a shader's first include
- `cargo test -p byroredux-renderer shader_contract` passes

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6681 --json state`)._
