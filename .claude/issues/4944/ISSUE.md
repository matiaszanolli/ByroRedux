# #4944: REN-D7-2026-09-27-03: Under `--upscaler taa`, water pixels are resolved as the lake bed — its stable mesh ID, normal and motion — with full α = 0.1 history, and no reactive signal reaches TAA

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4944
- **Labels**: medium,renderer,shaders,water,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D7-2026-09-27-03**._

- **Severity**: MEDIUM (opt-in path; floor = denoiser ghosting, a missing disocclusion signal)
- **Dimension**: TAA
- **Location**:
  - `crates/renderer/shaders/taa.comp`: bindings 0–8 have no mask input. The bypass gate is `offscreen || background || disocclusion || surfaceMismatch || alphaBlend`, with `alphaBlend = !meshIdHasStableHistory(currMid)`.
  - `crates/renderer/src/vulkan/water.rs` / `water.frag`: attachments 1–3 are masked off, and only 0/4/6/7 are written.
- **Status**: NEW (I searched "TAA water", "water mesh_id TAA", "TAA reactive mask" and "water ghosting").
- **Description**:
  - Alpha-blended `triangle.frag` draws set `MESH_ID_NO_HISTORY_BIT`, so TAA and SVGF bypass them.
  - Water writes no mesh ID, normal or motion by design (`water.rs` module doc), so the pixel keeps the **bed's** opaque stable ID, normal and motion.
  - TAA therefore accepts history for animated water: moving wave normals, RT reflection and refraction, and foam. It reprojects that history with the **bed's** motion vector, which is wrong by the parallax of the water depth whenever the camera moves.
  - FSR is told about this through the reactive and transparency masks water writes at full strength (#4864 tracks the value). TAA reads neither, although both attachments are still written in taa mode (#2180 / #4203).
- **Evidence**: `taa.rs` `write_descriptor_sets` binds curr HDR, motion, curr/prev mesh ID, prev history, output, params and curr/prev normal, and nothing else. `water.rs`: "attachments 1..=3 (normal, motion, mesh_id) … are masked off … so water never pollutes the G-buffer".
- **Impact**: In taa mode, water reflections and highlights smear and ghost under camera motion, and deep-water pixels reproject from the wrong place. The 3×3 γ = 1.5 clamp is loose on wavy water, so it limits the ghosting only partly. FSR mode (the default) is unaffected.
- **Related**: #4864, #2180, #4203; ground-cover blades have the analogous shape (see Needs validation).
- **Suggested Fix**: Bind the transparency (or reactive) mask attachment into `taa.comp` and fall back to the current sample, or raise α, where the mask exceeds a threshold. This mirrors what FSR does with the same data.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
