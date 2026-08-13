# REN-D2-02: RESERVOIR_LIGHT_MASK has no lockstep guard against MAX_LIGHTS

- **Severity**: LOW
- **Dimension**: 2
- **Labels**: low,renderer,bug

## Description
The 10-bit light lane has no lockstep guard, and the two constants are structurally unable to see each other (GLSL literal vs. `pub(super)`). Correct today only because 511 < 1023; raising `MAX_LIGHTS` silently selects the **wrong light**.

## Location
`crates/renderer/shaders/triangle.frag` (`RESERVOIR_LIGHT_MASK`) vs. `crates/renderer/src/vulkan/scene_buffer/constants.rs` (`MAX_LIGHTS`)

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D2-02).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2778
