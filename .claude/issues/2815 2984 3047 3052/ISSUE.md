# Batch fix: #2815, #2984, #3047, #3052

Domain: **renderer** → `byroredux-renderer`.

## #2815 — REN-D19-04: perturbNormal Path 1 NaN when tangent ∥ normal
`material_sampling.glsl`'s `perturbNormal` Path 1 normalizes the
tangent-minus-projection vector without guarding for near-zero length
(T ∥ N case), unlike sibling TBN builders (`parallaxDisplaceUV`,
`getRayHitTangentFrame`) which do guard. Real bug: NaN propagates into
shaded normal, G-buffer octEncode write, and RT ray origins.

## #2984 — TD9-2026-08-16-02: shader-include allow-list missing presentation.frag
`shader_constants.rs` `affected_shaders_include_constants_header` allow-list
(lines ~324-394) has 16 of 17 live `#include "include/shader_constants.glsl"`
consumers; `presentation.frag` (added 5f970bae, 2026-08-15) is missing.

## #3047 — REN-DOC-02: _audit-common.md shader-include roster stale
`.claude/commands/_audit-common.md`'s "Shader Includes:" row lists 9 of 12
live headers in `crates/renderer/shaders/include/`.

## #3052 — SAFE-2026-08-16-05: audit-safety SKILL.md cites nonexistent symbol
`.claude/commands/audit-safety/SKILL.md`:257 names `REFRACT_PASSTHRU_BUDGET = 2`
— doesn't exist anywhere in the tree; `shader_constants.rs` actively asserts
its ABSENCE. Real mechanism (triangle.frag ~1688-1695) is a passthru
allowance growing 2/4/6/8 by quality tier, not a fixed 2.
