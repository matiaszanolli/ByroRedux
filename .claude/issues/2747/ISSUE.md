# REN-D10-02: getHitTriWorldPositions returns relative positions on the rigid branch, absolute on the skinned branch

- **Severity**: MEDIUM
- **Dimension**: 10 — Camera-Relative Precision. See Cluster D.
- **Location**: `crates/renderer/shaders/include/ray_hit.glsl` (`getHitTriWorldPositions`, consumed by `getHitTriNormal` and `getRayHitTangentFrame`)
- **Description**: The two branches emit positions in two different conventions. Skinned (`hi.boneOffset != 0 && hi.skinnedVertexAddress != 0ul`) reads the `skin_vertices.comp` output, which bakes the **absolute** bone palette — the same convention `tlas_instance_transform` relies on when it emits `IDENTITY_VK_TRANSFORM` for skinned instances. Rigid multiplies bind-pose vertices by `hi.model`, and `GpuInstance.model` has been **render-origin-relative** since the markarth cascade — `rebase_model_matrix` subtracts `render_origin` from the translation column of every draw, unconditionally. The header comment reads "World-space positions of a ray-query hit triangle's three vertices" and the #2219 block states the skinned positions are "already absolute-world", which reads as a whole-function guarantee it does not hold.
- **Evidence**: `crates/renderer/src/vulkan/context/draw.rs` — `let current_model = rebase_model_matrix(m, render_origin);` runs for every `draw_cmd` before the `GpuInstance` is pushed, while `tlas_instance_transform(draw_cmd)` consumes the un-rebased `draw_cmd.model_matrix`. `ray_hit.glsl` then does `w0 = (hi.model * vec4(v0, 1.0)).xyz;` in one branch and a raw `SkinnedVertexRef` read in the other.
- **Impact**: No wrong pixels today — every consumer uses only differences, and a uniform origin offset cancels in both. The exposure is that a public-looking helper whose name, header and doc all promise absolute world space hands the next caller a silently branch-dependent frame: any re-projection, distance-to-camera, world-space hash, or second-bounce origin gets a `render_origin`-sized displacement on rigid geometry only. Same failure class #1488 shipped for the caustic writers.
- **Related**: #2219 (added the skinned branch), #1487 (skinned TLAS identity), #1488.
- **Suggested Fix**: Make the contract explicit rather than changing behaviour — either rebase the skinned branch to relative (`-= renderOrigin.xyz`) so both branches return relative and rename to `getHitTriPositionsRel`, or lift the rigid branch to absolute and keep the current name.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in related files (other RT-consumers of world-space hit positions)
- [ ] TESTS: A regression test pins this specific fix

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2747
