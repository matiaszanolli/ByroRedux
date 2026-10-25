# Batch: 6195 6196 6197 6198

---
## # TSD-1: per-layer terrain height maps (authored → normal-integrated → high-passed brightness), BC4 + hash-keyed cache [OPEN]

## Phase 1: per-layer height maps

Produce one height map per terrain layer texture.

Source priority:
1. Authored parallax/height map from the texture set (vanilla or mod).
2. **Integrate the normal map into a height field** (e.g. Frankot–Chellappa / Poisson integration). This gives real shape and is preferred over brightness.
3. Fallback: brightness of the diffuse with a high-pass filter (remove large color variations, keep small-scale detail), normalized per texture.

Requirements:
- Output normalized 0–1 heights, tileable (generation must wrap at edges so tiling seams don't appear).
- Store compactly (single channel, BC4 or equivalent). Cache keyed by a hash of the source texture(s) so generation runs once.
- Record per texture which source was used (authored / from normals / from brightness) so it can be inspected in the debugger.

## Acceptance Criteria
- Every terrain layer in the five test areas has a height map.
- A debug view in `byro-dbg` can display the raw height of the terrain under the camera.
- Unit tests for tiling (left/right and top/bottom edges match) and normalization.

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


---
## # TSD-2: height-based blending between terrain splat layers [OPEN]

## Phase 2: height-based blending between terrain layers

Replace the current percentage blend between terrain layers with a blend that respects height: where two layers overlap, the one whose height (plus its blend weight) is higher wins, inside a small transition band.

Requirements:
- One tunable parameter for transition sharpness, exposed in the debug UI.
- Apply the same blend result to every channel of the layer (color, normal, roughness, etc.), not just color.
- Layers without a height map behave exactly as today.
- No new texture fetches beyond the height samples themselves.

## Acceptance Criteria
- Before/after screenshots at the fixed camera positions.
- Frame-time delta reported.
- Review by Matías before Phase 3.

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


---
## # TSD-3: near-camera parallax occlusion on terrain, driven by the blended layer height [OPEN]

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


---
## # TSD-4 (optional): near-camera displaced terrain patches with per-tile BLAS rebuild — design note first [OPEN]

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


