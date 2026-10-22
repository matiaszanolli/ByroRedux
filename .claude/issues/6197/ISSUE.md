# #6197: TSD-3: near-camera parallax occlusion on terrain, driven by the blended layer height

## Phase 3: parallax on terrain, near the camera only

Reuse the existing POM for terrain layers, driven by the blended height from Phase 2.

Requirements:
- Only active within a short distance of the camera, with a smooth fade to plain shading (start around 0–30 m; make the range tunable).
- Step count scales with viewing angle (more steps at grazing angles, fewer looking straight down).
- Optional: self-shadowing toward the sun, behind a toggle.
- The ray-traced paths (shadows, reflections, GI hits on terrain) keep sampling the flat surface. Document this as a known, accepted mismatch.
- Check behavior with TAA, SVGF and FSR: parallax shifts texture coordinates but not geometry, so motion vectors stay valid; confirm no new shimmering. **Ask Matías to judge.**

## Acceptance Criteria
- Screenshots at the fixed positions, including at least one grazing-angle shot.
- Frame-time delta reported for both phases combined.
- Review by Matías. Decide together whether Phase 4 is needed.

Part of the **terrain surface detail: height-aware ground (RDR2-style)** plan. Goal: make the ground read as having physical relief that follows its own textures: stones that poke out of dirt, dirt that fills the cracks between rocks, and visible depth up close. Each phase ends with a review by Matías before the next one starts.

### Ground rules (apply to every phase)
- **Per-game knowledge stays out of the renderer.** Height generation and texture interpretation live in the preprocessing/abstraction layer. The renderer only consumes "a height map per terrain layer".
- **One source of height.** The per-layer height maps produced in Phase 1 are the same input the planned lightweight grass system will use. Do not create a second, parallel height pipeline.
- **Mod/user assets win.** If a terrain texture set ships an authored parallax/height map, use it. Generated heights are the fallback only.
- **No regressions without measurement.** `triangle.frag` is the hot path; every phase reports the frame-time delta on the fixed camera positions. Shader source/binary drift must pass `check-shader-artifacts.sh`.
- **Visual judgment belongs to Matías.** Capture screenshots, but do not decide alone whether noise, shimmering or aliasing is acceptable.
- **Units.** Any height amplitude is expressed in meters and converted through `BETHESDA_UNITS_PER_METER`, nowhere else.

### Test positions
Fixed camera positions, driven through `byro-dbg`, captured for every phase:
1. Skyrim: tundra near Whiterun (rock + grass + dirt).
2. Skyrim: a riverbank near Riverwood (mud/gravel transitions).
3. FNV: Mojave near Goodsprings (sand + rock).
4. FO4: a Commonwealth dirt road.
5. Oblivion: a forest floor near the Imperial City.

Each position: one shot looking down, one at eye level, one at a grazing angle. Same time of day and weather across runs.

