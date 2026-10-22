# #6198: TSD-4 (optional): near-camera displaced terrain patches with per-tile BLAS rebuild — design note first

## Phase 4 (optional): real displacement near the camera

Only if phases 2–3 are not enough.

Important constraint: the engine is ray-traced only. Hardware tessellation shaders do not feed the acceleration structures, so displaced geometry would not match shadows and reflections. The approach therefore has to be:

- A compute pass that generates subdivided, displaced terrain patches for the tiles nearest the player.
- Rebuild or refit only those tiles' acceleration structures.
- Small amplitude (a few centimeters), fading to zero at the patch edges so it joins the undisplaced terrain without cracks (reuse the existing `terrain.seams` checker).
- Physics and collision stay on the original terrain heights. Gameplay must not change.

## Acceptance Criteria
- A design note with cost estimates is written and reviewed by Matías before any code.

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

