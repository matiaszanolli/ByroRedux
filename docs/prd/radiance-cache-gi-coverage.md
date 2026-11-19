# Radiance Cache and GI Coverage — Product Requirements

**Status**: PROPOSED (2026-11-19). Revision 3 records the owner's decisions Q2 to Q7. Revision 2 folded in review round 1: the LOD requirements move ahead of the cache key, the Lexington cause gets a branch, the acceptance data is named, the classes are merged, a ROADMAP home is asked (Q7), and the citations and restated figures are fixed. Revision 1 was the first draft. It takes the radiance cache out of the Path Tracing Reference Mode PRD's Phase 5 and out of reference-lighting's deferral of RLT-GI-012, at the owner's direction of 2026-11-19, and gives it one owner. Statements about how an original engine, a format, a paper or a vendor library behaves cite external sources; a claim that could not be verified is marked **Phase 0: verify** instead of being stated as fact.

**Scope**: Every opaque surface the player can see receives traced global illumination, at any distance from the camera and at any angle to it, on all five titles whose cells the engine loads. The promise is coverage, not a new quality term: the gate that today cuts traced GI by a surface's pixel footprint and by its distance is replaced by a world-space hash-grid radiance cache whose resolution, not its reach, varies with distance. Level of detail is therefore a property of the cache and is solved by requirement (§5.3), where today it is a cut-off. Outside this document: direct-light shadows and the direct terms (reference-lighting Phase 3; this document only measures their coverage, §9 Phase 0), the path-traced reference modes that consume the cache (M51), the neural denoiser (M54), reflection rays, and the generation of distant-LOD geometry.

**Related**: this document states the *promise*; the designs below own the *how*. Every mechanism promised here has an owner in this table, or Phase 0 writes the missing design.

| Mechanism | Owning design |
|---|---|
| The GI gate, the energy terms, gates A7 / A13, the cost ceiling (Q8) and the settled-tier rule | [Reference-Quality Lighting PRD](reference-lighting.md) (RLT-GI-003, -008, -012, RLT-PERF-001…006) |
| The interactive path-traced mode that consumes the cache | [Path Tracing Reference Mode PRD](path-tracing-reference-mode.md) (PTR-RT-002) |
| Units, emitters, the six visibility bits, the adaptive ray budget | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md) |
| Cornell L0–L5 oracle, RT debug views, the ray probe | [RT Lighting & Material Recovery](../engine/rt-lighting-material-recovery.md) |
| Buffer rows, byte math, VRAM ceilings | [Memory Budget](../engine/memory-budget.md) |
| SVGF, G-buffer and pass order | [Vulkan Renderer](../engine/renderer.md) |
| Shader bindings and the shared GPU structs | [Shader Pipeline](../engine/shader-pipeline.md) |
| Far-field sun shadows and the traced-shadow fade | [Shadow Pipeline Trade-offs](../engine/shadow-pipeline-tradeoffs.md) |
| Distant-LOD geometry the cache must cover but does not generate | [Distant LOD Completion PRD](distant-lod-completion.md) |
| Debug console commands | [Debug CLI](../engine/debug-cli.md) |
| The ROADMAP home of this work (a new M-LIGHT phase, Q7) and the table that holds its numbers | [ROADMAP](../../ROADMAP.md): the M-LIGHT row and the Lighting fidelity table |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings, as in the related designs. Requirement IDs use the `RCG-` prefix.

---

## 1. Product thesis

The owner's framing, 2026-11-19, on a Fallout 4 exterior (Lexington, Fallon's): "most large objects such as walls don't interact with illumination appropiately, they don't either bounce or refract rays according to the environments, and they don't occlude with the environment"; "all objects on screen should be affected by GI and light sources"; and "We'll need to solve LOD a little bit better than the original engine."

At HEAD the first half is a deliberate, documented trade, not an accident. The main pass traces a bounded path only where a pixel's world footprint is under about 0.67 world units, measured with the larger screen derivative, and fades that path to nothing between 4000 and 6000 world units from the camera. Everything outside gets the sky's spherical-harmonics ambient times screen-space AO: no bounce, no ray occlusion. The owner deferred widening the gate on 2026-11-02 because it cost +3 to +11 ms on content and dropped settled tiers, to tier 0 (where GI is off entirely) on two scenes (the table in reference-lighting's RLT-GI-012). A cheaper way to widen it was always the goal; this PRD is that way.

The corollary that shapes every requirement: **coverage must not be bought by distance or angle**. A cache keyed on world position and resolved by distance makes the cost of a pixel's GI independent of where the pixel is: one bounce ray and one cache lookup, whether the wall is two metres away or two hundred. Level of detail moves from "does this surface get GI" to "how large is the cell that answers for it". The footprint gate and the distance fade then have no job and are removed, not retuned.

The feature is for the player standing in an exterior street, where the walls are far and seen at a grazing angle, and for the M-LIGHT milestone, which cannot close RLT-GI-012 or the GI energy deficit without it. After it the engine will not cut traced GI by distance or by angle, and it will not use the cache for direct light.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Traced share** | The part of `indirectLight` that came from rays: a bounded path today, a bounce ray ending in a cache lookup after this PRD. Contrast the *untraced share*. |
| **Untraced share** | The sky spherical-harmonics ambient (or the authored ambient or DALC cube) times screen-space AO. Not occluded by geometry along any ray. |
| **Footprint gate** | The condition `rtLOD < RT_LOD_GI` in `triangle.frag`, where `rtLOD` is derived from the larger screen derivative of the fragment's world position. Reference-lighting RLT-GI-012 names it. |
| **Distance fade** | `giFade`, the smoothstep between 4000 and 6000 world units that scales the traced share to zero with camera distance. |
| **Eligible pixel** | While GI rays are enabled (`giRayEnabled`: ray tracing on, no compile-time or debug switch off), a pixel whose primary surface is not a window, not glass, not emissive (`giEmissiveLum < 0.01`) and carries diffuse weight (`pathLuminance(primaryDiffuseWeight) > 1e-5`). These are the material conditions of `giTransportEligible`; its ray-budget terms (`maxPathSegments > 0`, `maxShadedHits > 0`) and its `giFade` term are not part of eligibility, because RCG-COV-004 and RCG-COV-002 change them. |
| **Radiance cache** | A world-space hash grid of outgoing radiance keyed by a hashed quantized position (Binder, Fricke and Keller 2019, [arXiv:1902.05942](https://arxiv.org/abs/1902.05942)). Defined in [path-tracing-reference-mode.md](path-tracing-reference-mode.md); this PRD owns its realtime form. |
| **Cache level** | One of a logarithmic series of voxel sizes selected by distance from the camera. |
| **Cache entry** | One hashed voxel: its accumulated radiance, its sample count and its age. |
| **Update path** | A path traced from a sparse subset of visible surface points each frame whose hits feed the cache. |
| **Resolve** | The pass that folds this frame's accumulation into the entries and evicts stale ones. |
| **Terminator** | A lookup that ends a path at a hit by reading the cache instead of tracing further segments. |
| **Cache miss** | A lookup that finds no resolved entry; that pixel falls back to the untraced share for that frame. |
| **Coverage** | Eligible pixels whose traced share is non-zero ÷ eligible pixels, per frame and per scene. |
| **Visibility bits** | The six TLAS and emitter classes: architecture, static props, dynamic actors, foliage, glass, effects ([backbone](../engine/physical-lighting-backbone.md)). |
| **Settled tier** | The ray quality tier a scene's adaptive budget settles at, as defined in reference-lighting (RLT-PERF-005). |

---

## 3. Core model

Each eligible pixel receives its indirect light from exactly one of two sources, chosen per frame and never by distance or angle:

| Class | Source | When |
|---|---|---|
| **T — traced to a cache terminator** | One bounce ray from the primary surface; the path continues for as many segments as the ray quality tier allows (from one up to `MAX_PATH_SEGMENTS`), then ends in a cache lookup at the last hit | Every eligible pixel, at every tier |
| **F — fallback** | The untraced share | Only on a cache miss, and only for that frame |

Two classes, disjoint: a pixel is T, or it is F for that frame. How many segments a T pixel spends is the tier's business (`GpuRayBudget`), not a class. Reporting: the miss count and the cache level and hit view report F (RCG-DBG-001, RCG-DBG-002); `gi.cache` reports coverage per visibility class (RCG-DBG-003); the segment allowance is the existing `GpuRayBudget` tier word.

The cache is updated from a sparse subset of visible surface points, resolved every frame, and indexed by hashed (level, quantized position) of the surface point. Its level is a function of the point's distance from the camera alone. The lookup happens at the first bounce hit, not at the primary surface, so the primary surface's own radiance is never read from the cache. That is the difference from the published SHaRC usage, which typically excludes the primary hit ([SHaRC Integration guide](https://github.com/NVIDIA-RTX/SHARC/blob/main/docs/Integration.md), "SHaRC Render").

The inventory below is the state at HEAD on 2026-11-19.

| # | Item | Class | Today |
|---|---|---|---|
| 1 | Footprint gate | gate to remove | `RT_LOD_SCALE = 6.0` (`triangle.frag:1075`), `RT_LOD_GI = 2.0` (`:1088`), `rtLOD = clamp(log2(rtFootprint * rtLodScale), 0, 3)` (`:1093`), gate `if (giTransportEligible && rtLOD < RT_LOD_GI)` (`:4721`). Pinned by `phase2_gi_energy_contracts` (`scene_buffer/shader_contract_tests.rs:4123`), which asserts the gate text and `!frag.contains("rtLodGi")` |
| 2 | Distance fade | gate to remove | `giFade = 1.0 - smoothstep(4000.0, 6000.0, giDist)` (`triangle.frag:4715`); `giTransportEligible` also requires `giFade > 0.01` (`:4716`) |
| 3 | Untraced share | fallback (class F) | on exteriors `exteriorSkyDiffuseOr(N, …) * primaryDiffuseWeight`, weighted by `1 - tracedDiffuseWeight` and by AO; with an authored DALC cube the cube along the normal times AO; otherwise `ambient * max(ao, 0.3)`; `ao = max(SSAO, 0.20)` (`triangle.frag:4983`, `:5041`–`:5066`) |
| 4 | Path allowance | tier-controlled | `MAX_PATH_SEGMENTS 6`, `MAX_SHADED_HITS 2` (`shader_constants.glsl:385`–`:386`), `MAX_DIFFUSE_BOUNCES = 2` (`triangle.frag:4803`); the cold-start budget has both at 0 (`cold_start_uses_the_watchdog_safe_quality_floor`, `scene_buffer/ray_budget.rs`) |
| 5 | Telemetry and views | existing | counters `gi_traced`, `gi_lod_culled` in `GpuRayBudget` (17 × `u32`, 68 B, [shader-pipeline.md](../engine/shader-pipeline.md)); printed on the `rt-lod-telemetry:` line (`scripts/rt-lod-sweep.sh`); views `RENDER_DEBUG_RT_LOD` (7), `RENDER_DEBUG_INDIRECT_ONLY` (4), `RENDER_DEBUG_TRACED_SHARE` (17) |
| 6 | Direct-light shadow reach | context, out of scope | traced to `SHADOW_FADE_START` 8000 and fading by `SHADOW_FADE_END` 12000 world units (`shader_constants.glsl:370`–`:371`), then the far-sun pass `far_sun_shadow.comp` |
| 7 | Hash-grid radiance cache | absent | no include, buffer, pass or memory-budget row exists; `git grep` finds only the comments naming the deferral (`triangle.frag:1083`, `shader_contract_tests.rs:4119`, `:4145`) |
| 8 | TLAS membership of distant-LOD geometry, ground cover and FO4 precombines | unknown | **Phase 0: verify** (V3); `draw_command_eligible_for_tlas` (`acceleration/predicates.rs:718`) is the filter |

---

## 4. Goals and non-goals

### 4.1 Goals

- **RCG-G-001.** On every Wave 1 acceptance scene (§7.2), coverage MUST be at least the §8 threshold (Q6) at every ray quality tier, measured by RCG-VAL-002.
- **RCG-G-002.** The GI cost of one eligible pixel MUST NOT depend on its distance from the camera or its angle to it (RCG-PERF-004).
- **RCG-G-003.** Level changes of the cache MUST NOT be visible as a step or a pop on a receding surface (RCG-LOD-002).
- **RCG-G-004.** The GI energy deficit gate A7 MUST be evaluated with the cache as terminator and meet its Q6 budget, which closes RLT-GI-003 (RCG-CACHE-005).
- **RCG-G-005.** RLT-GI-012 MUST be met on the box rung and on content (RCG-VAL-001, RCG-VAL-002), which closes it.

### 4.2 Non-goals

- **RCG-NG-001.** Replacing or gating SVGF. The cache feeds the existing SVGF and TAA chain; M54 owns any change of the spatial filter (reference-lighting RLT-NG-004).
- **RCG-NG-002.** Direct-light and shadow coverage. The direct terms and their reach belong to reference-lighting Phase 3 and the far-sun pass; Phase 0 only reports their coverage (V2) so the owner can decide whether a separate PRD is owed.
- **RCG-NG-003.** The path-traced reference and interactive PT modes. M51 consumes this cache (PTR-RT-002) and does not carry a second implementation.
- **RCG-NG-004.** Reflection and refraction coverage (`RT_LOD_REFLECT`). The reflection gate stays as it is; revisit it under its own issue if the owner wants it widened.
- **RCG-NG-005.** Generating distant-LOD geometry, and far-field shadows from it. This PRD only requires that what is drawn is lit (RCG-LOD-006).
- **RCG-NG-006.** Froxel in-scatter querying the cache. A possible later consumer ([Volumetric Lighting PRD](volumetric-lighting.md)); not promised here.
- **RCG-NG-007.** A neural radiance cache (PTR-NG-003).

---

## 5. Requirements by area

### 5.1 Coverage (`RCG-COV`)

- **RCG-COV-001.** Every eligible pixel MUST receive a traced share (class T) regardless of its pixel world footprint, its angle to the camera and its distance up to the far plane. Today: eligible pixels are cut by the footprint gate and the distance fade (§3 rows 1 and 2, at HEAD on 2026-11-19).
- **RCG-COV-002.** The shader MUST contain no condition on `rtLOD`, on screen derivatives of world position or on camera distance that sets `tracedDiffuseWeight` to zero or skips the traced share. `phase2_gi_energy_contracts` MUST be rewritten to pin the replacement, and MUST assert that the footprint gate and `giFade` are gone.
- **RCG-COV-003.** On a cache miss the pixel MUST fall back to the untraced share for that frame only, and the shader MUST count the miss. The steady-state miss fraction at a parked camera after 16 frames MUST be published on the bench line (RCG-DBG-002); its threshold is Q6's.
- **RCG-COV-004.** The adaptive ray budget MUST NOT shed coverage. A lower tier MAY reduce path segments, shaded hits, the update fraction and the resolve rate, and MUST NOT remove the cache lookup. At ray quality tier 0, coverage MUST still meet the §8 threshold. Today: tier 0 turns GI off (reference-lighting Phase 2 measurement; `cold_start_uses_the_watchdog_safe_quality_floor`).
- **RCG-COV-005.** The material conditions of eligibility (§2) MUST be unchanged: glass, windows and emissive surfaces stay out of the traced share as they do today, so coverage is measured against the same denominator on both sides of the change.

### 5.2 The cache (`RCG-CACHE`)

- **RCG-CACHE-001.** A radiance cache MUST exist as a GLSL include whose entry points (initialise a sample, update on hit, update on miss, resolve an entry, query) are shared by the main pass and the update pass, keyed by the hashed (level, quantized position) of a surface point. Today: absent (§3 row 7).
- **RCG-CACHE-002.** The cache MUST store outgoing radiance, so a path may end at any hit, and MUST apply the same material-aware shading the main pass applies at a hit (`pathHitRadiance` and `ray_hit.glsl`), including emission. A contract test MUST pin that the update shader calls `pathHitRadiance` with the arguments the main pass's path walk passes.
- **RCG-CACHE-003.** The include MUST be written from Binder et al. (2019) and the published SHaRC integration description, and MUST carry no RTX SDK source (Q2). Its header MUST cite those two references and no other.
- **RCG-CACHE-004.** A CPU reference of the hash, the level function and the quantization MUST exist as a unit test, and MUST agree with the GLSL on a fixed corpus of positions. Observable: the test, run by `cargo test -p byroredux-renderer`.
- **RCG-CACHE-005.** With the cache as the bounded path's terminator, A7 MUST be reported at tiers 1 to 3 on the Lambertian box rung and MUST meet the budget the owner writes at Phase 2 entry (reference-lighting Q6).
- **RCG-CACHE-006.** The cache MUST be bound through the descriptor layout and the shared GPU-struct headers like every other scene buffer: one header per shared struct, a contract test pinning the layout, and a new row in [shader-pipeline.md](../engine/shader-pipeline.md).

### 5.3 Level of detail (`RCG-LOD`)

- **RCG-LOD-001.** The cache level MUST be selected from the surface point's distance to the camera on a logarithmic series, so the screen-space size of one voxel stays within the band Phase 1 publishes (Q3 chooses the scheme, not the band) across the whole range from the near clamp to the far plane. Observable: a unit test over distances that asserts the band.
- **RCG-LOD-002.** A receding flat Lambertian receiver, lit by a fixed sky and a fixed sun, MUST show no luminance step across a level boundary larger than the threshold Phase 1 publishes, measured as the largest adjacent-pixel difference at the boundary over the same difference one pixel away. Observable: a capture and a script.
- **RCG-LOD-003.** Level selection MUST NOT take a screen derivative or a surface angle as input; its signature MUST be (position, camera position, level parameters). A grazing surface and a face-on surface at the same distance MUST map to the same level. Today: the gate selects by the larger screen derivative, which is why a grazing wall loses traced GI at a distance where a face-on one keeps it (reference-lighting RLT-GI-012, at HEAD on 2026-11-02).
- **RCG-LOD-004.** The minimum voxel size near the camera MUST be stated and bounded, so contact-scale GI stays sharp, and the level scheme's three parameters (scene scale, level ratio, near bias) MUST be named constants in `shader_constants.glsl` with their generated-header test. SHaRC exposes the same three as `sceneScale`, `logarithmBase` and `levelBias` ([Integration guide](https://github.com/NVIDIA-RTX/SHARC/blob/main/docs/Integration.md), "Hash Grid Visualization"); how it maps distance to size is not stated there (**Phase 0: verify**, V4).
- **RCG-LOD-005.** The level scheme MUST be independent of render resolution and of the FSR upscaler's quality mode. Observable: the same world point, captured at two render scales, reports the same level in the RCG-DBG-001 view.
- **RCG-LOD-006.** Phase 0 MUST publish which draw classes sit outside the TLAS (distant-LOD terrain, object LOD and trees, ground cover, FO4 precombines), and coverage MUST be reported for them as separate rows. A surface outside the TLAS MUST be looked up in the cache at its own position and MUST NOT write entries. Today: unknown (§3 row 8).

### 5.4 Update and resolve (`RCG-UPD`)

- **RCG-UPD-001.** The update pass MUST trace paths from a sparse subset of visible surface points each frame; the subset fraction MUST be a `GpuRayBudget` field. SHaRC's example is one random pixel per 5×5 block, about 4% of pixels, varied between frames ([Integration guide](https://github.com/NVIDIA-RTX/SHARC/blob/main/docs/Integration.md), "SHaRC Update"); this engine's value is set in Phase 1 against the A7 gate.
- **RCG-UPD-002.** Resolve MUST accumulate with a capped history and MUST evict an entry unused for the stale-frame limit, both named constants. After a step change of the sun direction, the cache MUST reach within the Q6 budget of the converged value in no more than the frame count Phase 1 publishes. SHaRC states both knobs (`staleFrameNumMax`, the accumulated-frame cap) without defaults for the second ([Integration guide](https://github.com/NVIDIA-RTX/SHARC/blob/main/docs/Integration.md), "SHaRC Resolve").
- **RCG-UPD-003.** A moving blocker (an actor, a door) MUST NOT leave stale radiance on the surfaces it un-occludes beyond the frame count RCG-UPD-002 publishes. Observable: the door-opening scene, capture and script.
- **RCG-UPD-004.** An update path MUST see the same scene as the main pass: the same TLAS masks, the same alpha coverage test and the same glass handling. A contract test MUST pin that the update shader includes the same hit and coverage code as `triangle.frag`'s path walk.
- **RCG-UPD-005.** With the camera parked and a fixed seed, two runs of the same scene SHOULD produce the same cache contents after the same frame count, so the A/B harness and the state hash stay usable. Today: the seed advances every frame when parked (`triangle.frag`, `giSeed`).

### 5.5 Cost (`RCG-PERF`)

- **RCG-PERF-001.** The update and resolve passes MUST each carry a GPU timer bracket on the `bench:` line (reference-lighting RLT-PERF-001).
- **RCG-PERF-002.** Every new per-pixel ray, update ray and cache query MUST be bounded by a `GpuRayBudget` field so the adaptive tiers can shed it (RLT-PERF-002, RLT-GI-008). The struct and its 68 B size claim in [shader-pipeline.md](../engine/shader-pipeline.md) MUST be updated together, with the contract tests that pin them.
- **RCG-PERF-003.** The phase that removes the footprint gate MUST report its frame-time delta on the four game benchmark scenes with the tier pinned (A13), against the Q4 ceiling. Today: the cost of widening the old gate is recorded in the table under RLT-GI-012 in [reference-lighting.md](reference-lighting.md), not restated here.
- **RCG-PERF-004.** The traced-share cost per eligible pixel MUST be flat across distance and angle. Observable: `scripts/rt-lod-sweep.sh` extended to bucket the main-pass time by distance, with the spread between the cheapest and dearest bucket below the figure Phase 1 publishes.
- **RCG-PERF-005.** No scene's settled tier MAY drop against its Phase 1b control (reference-lighting RLT-PERF-005). A tier drop that coverage forces is a Q4 decision, not a regression to absorb.

### 5.6 Memory (`RCG-MEM`)

- **RCG-MEM-001.** The cache buffers MUST have a row in [memory-budget.md](../engine/memory-budget.md) with byte math and the allocation rule (RLT-GI-008), and the total MUST stay inside the ceiling that page states for the RT-minimum target. For scale, SHaRC's default is 40 bytes per entry (8 + 16 + 16) and 2^22 entries, 160 MiB ([Integration guide](https://github.com/NVIDIA-RTX/SHARC/blob/main/docs/Integration.md), "Memory Usage"); this engine's size is set in Phase 1.
- **RCG-MEM-002.** The cache MUST report occupancy, evictions and overflowed inserts, and a full table MUST degrade to cache misses, not to wrong radiance or a device fault. Observable: a test that overfills a small table.
- **RCG-MEM-003.** Occupancy MUST stay under the Q6 limit on the largest exterior the engine loads (radius 12, the clamp in [CLAUDE.md](../../CLAUDE.md#running-the-engine)) on FNV and FO4. The cache size MUST NOT scale with the grid radius.

### 5.7 Debug and telemetry (`RCG-DBG`)

- **RCG-DBG-001.** A render debug view MUST show each pixel's cache level and whether the lookup hit, in the `RenderDebugMode` catalog; `RENDER_DEBUG_MODE_MAX`, `USER_MODES` and their contract tests move together.
- **RCG-DBG-002.** The `rt-lod-telemetry:` line MUST report `gi_traced`, a new resolved-from-cache count and the miss count, and after Phase 2 `gi_lod_culled` MUST read 0 on every scene.
- **RCG-DBG-003.** A console command `gi.cache` MUST print occupancy, evictions, the voxel edge of each level, the update fraction, the miss fraction and coverage per visibility class, and MUST be documented in [debug-cli.md](../engine/debug-cli.md) with the command count (`every_registered_command_is_named_in_debug_cli_docs`).

### 5.8 Validation (`RCG-VAL`)

- **RCG-VAL-001.** RLT-GI-012's own measurement MUST pass with the cache path: on the box rung at tiers 2 and 3 the traced-share view MUST be non-zero on at least 99% of every L4 region's pixels.
- **RCG-VAL-002.** Coverage MUST be measured on FO4 Lexington (Fallon's), FNV Prospector Saloon, Skyrim WhiterunBanneredMare, FO4 MedTekResearch01 and Dugout Inn at camera poses pinned in a manifest, at every tier, and MUST meet the §8 threshold. The measurement MUST use `RENDER_DEBUG_TRACED_SHARE` and the eligible-pixel mask, not a hand count.
- **RCG-VAL-003.** A temporal-stability gate MUST hold: with the camera parked for 255 frames, the frame-to-frame luminance variance of cache-resolved pixels MUST stay within the budget Phase 1 publishes, so the cache does not boil.
- **RCG-VAL-004.** The coverage and flatness gates MUST be runnable from `scripts/m-light-gates.sh` and MUST report SKIP, not PASS, when the data is absent.
- **RCG-VAL-005.** A receding-plane lab MUST exist for RCG-LOD-002 and RCG-PERF-004: a flat Lambertian ground plane running from the near clamp to the far plane under a fixed sky and sun, built in the manner of the shaft lab (`byroredux/tests/golden_frames.rs`), game-data-free and capturable headless. Today: absent (at HEAD on 2026-11-19).

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Which lighting terms the original applies to distant objects, and how its lighting steps with distance: **Phase 0: verify** (V5), from the CK / GECK wikis and UESP | Every surface keeps traced GI at any distance; level of detail lives in the cache (RCG-COV-001, RCG-LOD-001) | The footprint gate, the distance fade, the `rtLOD` GI classification | Visible voxel steps on receding surfaces → RCG-LOD-002, RCG-VAL-003 |
| 2 | SHaRC's documented use typically excludes the primary hit ([Integration guide](https://github.com/NVIDIA-RTX/SHARC/blob/main/docs/Integration.md), "SHaRC Render"); this engine looks up at the first bounce hit | The primary surface's own radiance is never read from the cache (§3) | Nothing | Blocky GI where one voxel spans many pixels at far levels → RCG-LOD-001 bounds the voxel's screen size |

Risks with no original-engine counterpart:

| # | Risk | Handled by |
|---|---|---|
| 3 | History-based accumulation lags and ghosts after a light or blocker moves | RCG-UPD-002, RCG-UPD-003 |
| 4 | A fixed-size hash table collides and overflows on large exteriors | RCG-MEM-002, RCG-MEM-003 |
| 5 | A SHaRC-style layout needs native half-float shader types and, unless a lock buffer is used, 64-bit atomics ([Integration guide](https://github.com/NVIDIA-RTX/SHARC/blob/main/docs/Integration.md), "Shader Compiler Requirements" and "Resource Binding", `HASH_GRID_ENABLE_64_BIT_ATOMICS`); a GPU without them is excluded | V6 and Q2 |
| 6 | The cost of widening the old gate returns in another form | RCG-PERF-003, RCG-PERF-004, Q4 |
| 7 | Surfaces outside the TLAS are looked up in a cache nothing wrote to | V3, RCG-LOD-006 |
| 8 | The Lexington walls are missing from the TLAS rather than culled by the gate, so the cache works and the scene is still wrong | V1 and the Phase 0 branch (§9) |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

The waves are those of [reference-lighting §7.1](reference-lighting.md), linked rather than restated. Every requirement is downstream of the translation boundary. The cache is world-space and game-agnostic; no shader branches on the source game ([CLAUDE.md invariants](../../CLAUDE.md#architecture-invariants)), and a per-game difference such as the Starfield metric-unit normalisation stays at the import boundary the [backbone](../engine/physical-lighting-backbone.md) names.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Interior and exterior load; exercised through the shared path |
| Fallout 3 | yes | Shares FNV's ESM path and inputs |
| Fallout: New Vegas | yes | Prospector Saloon, the reference title and benchmark scene |
| Skyrim (LE / SE / AE) | SE / AE yes; LE no | SE: WhiterunBanneredMare. LE has no cell path (the ROADMAP compatibility matrix) and inherits this when it lands |
| Fallout 4 | yes | Lexington (Fallon's) is the scene that motivated this PRD; MedTekResearch01 and Dugout Inn are benchmark scenes. Whether FO4 precombined geometry is in the TLAS is V3 |
| Fallout 76 | no | No cell loader; nothing is lit |
| Starfield | yes | Walkable Cydonia interior |

### 7.2 Acceptance

A scenario passes when:

1. On FO4 Lexington at the Fallon's pose and launch command that Phase 0 records in a manifest (captured with `cam.where` on a held `--bench-hold` instance), `RENDER_DEBUG_TRACED_SHARE` is non-zero on at least the §8 coverage threshold of eligible pixels at tiers 0 to 3, and the `rt-lod-telemetry:` line reports `gi_lod_culled=0`. If V1 finds the walls absent from the TLAS, this scenario is gated on the separate defect that V1 files (§9 Phase 0).
2. On the Lambertian box rung at tiers 2 and 3, the traced-share view is non-zero on at least 99% of every L4 region (RLT-GI-012's clause) and A7 reports its deficit within the budget (RCG-VAL-001, RCG-CACHE-005).
3. On the receding-plane lab (RCG-VAL-005), no level boundary shows a luminance step above the published threshold (RCG-LOD-002).
4. Parked for 255 frames on each acceptance scene, the cache-resolved pixels hold the flicker budget (RCG-VAL-003).
5. On each of the four benchmark scenes with the tier pinned, the A13 frame-time delta (`scripts/m-light-baselines.sh --exit-cost` against a control build) is within the Q4 ceiling and no settled tier drops (RCG-PERF-003, RCG-PERF-005).
6. With the data absent the gates exit SKIP (77), never PASS (RCG-VAL-004).

---

## 8. Metrics

| Metric | Definition | Granularity |
|---|---|---|
| Coverage | eligible pixels with a non-zero traced share ÷ eligible pixels. Threshold: 99% (Q6) | per scene, per tier |
| Cache miss fraction | lookups that miss ÷ lookups, after 16 parked frames | per scene |
| Cache occupancy | occupied entries ÷ table entries, and evictions per frame | per scene, per grid radius |
| Cost flatness | (dearest − cheapest) ÷ cheapest main-pass time across the distance buckets of RCG-PERF-004 | per scene |
| Level-step | largest adjacent-pixel luminance difference at a level boundary ÷ the same one pixel away | per level boundary |
| Reconvergence | frames until the cache is within the Q6 budget of its converged value after a sun step or a blocker move | per scenario |
| A7 deficit | the existing GI-deficit gate, with the cache as terminator | per tier |
| Frame-time delta | the A13 exit-cost figure against the pinned-tier control | per benchmark scene |
| Out-of-TLAS coverage | coverage restricted to each draw class RCG-LOD-006 lists | per scene, per class |

The numbers live in ROADMAP's Lighting fidelity (M-LIGHT) table, which reference-lighting's RLT-REF-004 generates, and in the checked-in gate artefacts, not here.

---

## 9. Delivery sequence

**Phase 0 — Measure and verify (no engine code).** Settle which of three causes explains the Lexington walls, using the existing telemetry and debug views on a pinned pose: the footprint gate (the `gi_traced` against `gi_lod_culled` counters and the `RENDER_DEBUG_RT_LOD` view), absence from the TLAS (`mesh.info` on a Fallon's wall reference), or weak AO (the AO-off flag) (V1). Record the Fallon's pose, the launch command and the data files in a manifest, captured with `cam.where` on a held `--bench-hold` instance. Census which of direct sun, local lights, sky ambient and traced GI reach each visibility class on each acceptance scene, using the existing views (V2). Publish the TLAS membership of distant-LOD geometry, ground cover and FO4 precombines (V3). Close the SHaRC details this document marks and the cascaded-cache question of Q3 (V4), the original engine's lighting LOD (V5) and the device features a half-float, 64-bit-atomic cache needs on the RT-minimum GPU (V6). Q2 to Q7 are decided; close V2 of the path-tracing PRD (the licence clauses) before Phase 1. **Branch on V1.** If the footprint gate is the cause, Phases 1 to 3 proceed as written. If the walls are absent from the TLAS, that defect is filed on its own (FO4 precombines are M49's, [Distant LOD Completion PRD](distant-lod-completion.md) lists them out of its scope), acceptance scenario 1 is gated on it, and Phases 1 to 3 still proceed, because RLT-GI-012 requires coverage independently of the Lexington scene. If AO is the cause, the AO floor is filed as a tuning issue and the same holds. Phase 0 closes **Phase 0: verify** items V1 to V6 in `docs/audits/RCG_phase0_<date>.md`. Exit: that artefact is checked in with V1 to V6 answered and the pose manifest recorded, and every finding that is a defect is filed as an issue.

**Phase 1 — The cache on the Cornell harness.** RCG-CACHE-001 to -004 and -006, RCG-LOD-001, -003 and -004 (the level function is part of the cache key), RCG-UPD-001, -002 and -004, RCG-MEM-001 and -002, RCG-PERF-001 and -002, RCG-DBG-001 and RCG-VAL-005. The cache terminates the bounded path on the box rung; the footprint gate is untouched, so it still removes traced GI from the grazing floor and ceiling (RLT-GI-012). Exit: A7 is published at tiers 1 to 3 on the box rung split by region, the memory-budget row and both bench brackets exist, and the numbers Phase 1 owes (the RCG-LOD-001 band, the RCG-LOD-002 threshold, the RCG-UPD-002 frame count, the RCG-VAL-003 budget and the RCG-PERF-004 figure) are published.

**Phase 2 — Coverage.** RCG-COV-001 to -005, RCG-CACHE-005, RCG-VAL-001 and RCG-DBG-002. The footprint gate and the distance fade are removed and `phase2_gi_energy_contracts` is rewritten. Exit: RLT-GI-012's box-rung measurement passes, A7 meets its budget (RCG-CACHE-005), `gi_lod_culled` reads 0 on the bench line of every scene, and the A13 delta is published.

**Phase 3 — LOD and content.** RCG-LOD-002, -005 and -006, RCG-UPD-003 and -005, RCG-MEM-003, RCG-PERF-003 to -005, RCG-DBG-003 and RCG-VAL-002 to -004. Exit: acceptance scenarios 1 to 6 pass on every Wave 1 scene (scenario 1 as gated by the V1 branch) and the A13 delta is within the Q4 ceiling.

**Phase 4 — Close-out.** ROADMAP's M-LIGHT row (Q7) is updated, with memory-budget, shader-pipeline and debug-cli; reference-lighting records RLT-GI-012 and RLT-GI-003 closed; the path-tracing PRD's PTR-RT-002 links the shipped include. This work depends on M-LIGHT (the cost ceiling and the gates it owns) and is a dependency of M51's interactive mode. Exit: the gates of Phase 3 run from `scripts/m-light-gates.sh` and the PRD moves to SHIPPED with the last phase's tracking issue closed.

---

## 10. Decisions required

1. **Q1 — Who owns the cache.** **Decided** (2026-11-19): this PRD owns the realtime cache, its allocation rule and the replacement of the footprint gate; reference-lighting's RLT-GI-008 stays as the allocation contract this PRD satisfies, and the path-tracing PRD consumes the cache. The owner chose a separate PRD over folding into the volumetric or reference-lighting PRDs, and the cache over widening the gate.
2. **Q2 — Cache provenance.** Recommended: a clean-room GLSL cache written from Binder et al. (2019) and the published SHaRC integration description, so no RTX SDK source enters the MIT tree. This is the decision the path-tracing PRD's Q2 held; it moves here with the cache, and V2 of that PRD (the licence clauses) must close before it is decided. Alternative A: vendor NVIDIA SHaRC; consequence: its licence clause conflicts with the MIT licence and ties the application to NVIDIA hardware interoperability, per the path-tracing PRD. Alternative B: no cache, deeper paths with Russian roulette; consequence: the cost the owner already rejected, and RCG-COV-004 cannot hold at tier 0. **Decided** (2026-11-19): the recommendation, clean-room GLSL; V2 of the path-tracing PRD (the licence clauses) still closes before Phase 1 writes the include.
3. **Q3 — The level scheme.** Recommended: logarithmic distance levels selected by camera distance alone (RCG-LOD-003), with scene scale, level ratio and near bias as the three constants. Alternative A: camera-centred cascades; consequence: cheap lookups and no hashing of level, but a fixed range, and whether a visible ring shows where a cascade ends is **Phase 0: verify** (V4), from the published cascaded-cache literature, which is the cut-off this PRD removes if it does. Alternative B: select by screen-space footprint; consequence: it is the current bug, and grazing surfaces would lose coverage again. **Decided** (2026-11-19): the recommendation, logarithmic distance levels selected by camera distance alone.
4. **Q4 — The cost ceiling.** Recommended: reference-lighting Q8's rule (the larger of 5% and 0.25 ms per phase, 12% for the milestone, no settled-tier drop), measured at the pinned tier, with coverage non-negotiable: when coverage does not fit, the owner chooses between raising the ceiling and cutting the update fraction, and nothing is cut by distance. Alternative: waive Q8 for this PRD and report cost only; consequence: coverage is guaranteed but frame time is unbounded. **Decided** (2026-11-19): the recommendation, Q8's rule with coverage fixed.
5. **Q5 — Tier 0.** Recommended: no tier removes the cache lookup (RCG-COV-004); tier 0 keeps one bounce ray and the lookup and sheds only depth and update rate. Alternative: tier 0 keeps turning GI off; consequence: the safety valve is preserved and "every object on screen" is false on the slowest tier. **Decided** (2026-11-19): the recommendation, no tier removes the cache lookup.
6. **Q6 — Thresholds.** Recommended: coverage 99% (RLT-GI-012's own clause), the miss fraction, flicker, level-step, reconvergence and occupancy budgets written by the owner at the entry of the phase that first measures each, as reference-lighting Q6 does. Alternative: coverage 99.9% now; consequence: a stricter bar before the cache's miss behaviour has been measured. **Decided** (2026-11-19): the recommendation, coverage 99% now and each other budget written at its phase entry.

7. **Q7 — The ROADMAP home.** Recommended: a new phase of M-LIGHT, because RLT-GI-012 and RLT-GI-003 live there and M51 only consumes the cache; the M-LIGHT row and the Lighting fidelity table take the numbers. Alternative A: a new M-number under Tier 8; consequence: a second owner of the GI terms. Alternative B: a phase of M51; consequence: the cache keeps waiting on a milestone whose Phase 5 is unscheduled, which is what the owner rejected. **Decided** (2026-11-19): the recommendation, a new phase of M-LIGHT; the M-LIGHT row and the Lighting fidelity table take the numbers.

---

## 11. Document map

| Need | Go to |
|---|---|
| The GI gate, RLT-GI-003, -008, -012, gates A7 and A13, the cost ceiling | [Reference-Quality Lighting PRD](reference-lighting.md) |
| The interactive PT mode and the cache's definition | [Path Tracing Reference Mode PRD](path-tracing-reference-mode.md) |
| A possible later consumer, froxel in-scatter | [Volumetric Lighting PRD](volumetric-lighting.md) |
| Distant-LOD geometry | [Distant LOD Completion PRD](distant-lod-completion.md) |
| Units, visibility bits, the adaptive ray budget | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md) |
| RT debug views, the Cornell oracle | [RT Lighting & Material Recovery](../engine/rt-lighting-material-recovery.md) |
| Buffer rows and VRAM ceilings | [Memory Budget](../engine/memory-budget.md) |
| Shader bindings and shared GPU structs | [Shader Pipeline](../engine/shader-pipeline.md) |
| Traced-shadow fade and the far-sun pass | [Shadow Pipeline Trade-offs](../engine/shadow-pipeline-tradeoffs.md) |
| SVGF, G-buffer, pass order | [Vulkan Renderer](../engine/renderer.md) |
| Console commands | [Debug CLI](../engine/debug-cli.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-19 | First draft. The cache and traced-GI coverage leave M51's Phase 5 and reference-lighting's RLT-GI-012 deferral; Q1 is the owner's decision of this date, Q2 moves here from the path-tracing PRD, and the volumetric PRD is cross-linked | — |
| 2 | 2026-11-19 | Review round 1 folded in: LOD requirements moved ahead of the cache key; the V1 branch and the pose manifest; the receding-plane lab (RCG-VAL-005) and the A13 command named; classes T and N merged and their reporting stated; the ROADMAP home asked (Q7) and the table named; SHaRC citations and the cascade claim fixed; restated figures and waves linked; deviations split from plain risks; thesis audience and non-goal added; Q6 dependence marked | claude: revise |
| 3 | 2026-11-19 | Owner decisions Q2 to Q7 recorded, each as recommended: clean-room GLSL, logarithmic distance levels, Q8's rule with coverage fixed, no tier removes the lookup, coverage 99% with other budgets at phase entry, a new M-LIGHT phase. Every §10 decision is now decided; the status stays PROPOSED until the owner's approval | — |
