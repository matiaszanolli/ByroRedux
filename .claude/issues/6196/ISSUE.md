# #6196: TSD-2: height-based blending between terrain splat layers

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

