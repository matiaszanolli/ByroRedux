# RF2-2026-11-05-03: `parse_glsl_struct_fields_typed` reads one declaration per line, so `float a; float b;` hides `b`

Labels: low,renderer,shaders,test-gap,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 2
**Status**: NEW. Part of #6680

## Location
- `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:2779`

## Why
The parser behind the GLSL field guards, including `gpu_boundary_instance_stride_matches_gpu_instance`, assumes one declaration per line. During the shared-header work, one of 13 injected faults passed until it was moved onto its own line. A real two-on-a-line declaration would be invisible to those guards.

## Change
Strip comments, then split on `;`, so every declaration on a line is returned.

## Acceptance Criteria
- A unit test shows `float a; float b;` returns both fields
- A field-order fault injected on a shared line fails the guard
- Parsed field lists for every real struct are unchanged

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6683 --json state`)._
