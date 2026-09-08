# #3305: REN-2026-08-26-01: dynamic actors (creatures) show no ground-contact shadow despite correct light/instance visibility masks

Labels: bug, renderer, medium, vulkan

## Description

Observed in a live FO4 Commonwealth screenshot (`grid-cross` boundary benchmark, radius-1 population near GNN Plaza): two `feral hound`-class creature actors show no visible ground-contact shadow, while nearby static architecture (a balustrade) casts a normal, reasonably soft-edged shadow from the same sun. Screenshot retained at
`/tmp/claude-1000/-mnt-data-src-gamebyro-redux/d0355164-7ccb-4e08-a568-3c81daed22d0/scratchpad/fo4-boundary-2/fo4/frame.png` (session-local path, not guaranteed to survive — re-run `docs/smoke-tests/m-exteriors.sh fo4 boundary` to reproduce; the dogs are ambient wildlife on this route so a repro may need `--bench-camera grid-cross` again or a `coc`/manual approach near the same plaza).

## What's ruled out

Both layers of the RT shadow-ray visibility-mask system check out correctly on paper — this is **not** an obvious mask-exclusion bug:

1. **Light-level mask** — the exterior sun's `GpuLight.params.z` is hardcoded to `VisibilityMask::FULL.bits()` (`byroredux/src/render/lights.rs:192`), the widest possible policy. `visibilityOpaqueMask()` (`shadow_common.glsl:19-21`) ANDs this against `VISIBILITY_MASK_ALL_OPAQUE`, which is unaffected by `FULL` being a superset — the sun's shadow-ray cull mask includes every opaque layer, `DYNAMIC_ACTOR` among them.
2. **Instance-level mask** — `shadow_mask_for_instance()` (`crates/renderer/src/vulkan/acceleration/predicates.rs:719-751`) maps `RenderLayer::Actor` → `VISIBILITY_LAYER_DYNAMIC_ACTOR`, which is one of the four bits composing `VISIBILITY_MASK_ALL_OPAQUE` (`crates/core/src/lighting.rs:134-135`). So `(instance.mask & ray.cullMask) != 0` should hold for a dog's TLAS instance against the sun's shadow ray, by the standard Vulkan RT masking rule.

Neither of these — the two places a "dynamic actors excluded from shadow rays" policy bug would plausibly live — explains the observation.

## Not yet checked / candidate directions

- **Skinned-BLAS refit timing.** `blas_skinned.rs` has an explicit "first-sight frame" concept (`refit_skinned_blas`, doc at `:390-412`) for a same-frame build+refit ordering. A freshly-streamed-in actor (radius-1 population load, as in the reproducing screenshot) is exactly the shape of case that path exists for — worth checking whether a dog's BLAS is fully built/refit *before* the frame its shadow is expected to render, or whether there's a one-frame gap where the TLAS instance references stale/degenerate geometry.
- Whether the specific creature race/record in question resolves to `RenderLayer::Actor` at all (vs. some other classification that's technically still inside `ALL_OPAQUE` but wired differently upstream).
- Whether this is animal/creature-specific (as opposed to humanoid NPCs) — creatures may go through a different skin/BLAS registration path than the humanoid ragdoll/skeleton pipeline.

This needs actual capture/inspection tooling (RenderDoc — the TLAS instance list + the shadow ray's actual hit/miss result for the exact pixel) to pin down further, not more source-reading — per this project's own convention (`feedback_speculative_vulkan_fixes`): don't ship render-pass/pipeline changes whose failure modes are invisible to `cargo test`; RenderDoc or revert, not speculation.

## Suggested Fix

Capture the reproducing scene in RenderDoc, inspect the TLAS instance list for a shadow-less dog's entity, and check whether (a) its instance exists at all at the frame in question, (b) its instance mask is what `shadow_mask_for_instance` computed, and (c) the shadow ray from a fragment on the ground beneath it actually intersects that instance's geometry (vs. missing due to a BLAS bounds/refit issue).

## Related
Closed sibling investigations in the same visibility-mask area: #2227 (SHADOW_MASK_OPAQUE silently excluded glass), #2224 (fire-refraction proxies wrongly stayed opaque occluders), #2238 (MultiLayerParallax missed the glass mask).


---

# #3306: FO4-2026-08-26-01: live terrain.seams gate caught a real 8-16 unit LAND height crack at Commonwealth cells (3,0)/(4,0)

Labels: bug, medium, game:fo4, terrain-exterior, esm-plugin

## Description

The newly-live-wired `terrain.seams` gate (EX-10/11 item 7, #2371) caught a genuine, confirmed height discontinuity in vanilla FO4 Commonwealth's LAND data — the exact class of defect this checker exists to find, on its first real cross-cell run.

## Evidence

`docs/smoke-tests/m-exteriors.sh fo4 boundary` against real FO4 Commonwealth data:
```
terrain-seams: sampled=1 pairs_checked=17 pairs_dirty=1 height_mismatch_vertices=3 normal_mismatch_pairs=15 verdict=FAIL
```
A one-off diagnostic (magnitude probe, not retained) pinned the exact mismatch:

| Cell pair | Direction | Edge index | height_a | height_b | delta |
|---|---|---|---:|---:|---:|
| (3,0) / (4,0) | EastWest | 5 | 720 | 712 | **8** |
| (3,0) / (4,0) | EastWest | 6 | 728 | 712 | **16** |
| (3,0) / (4,0) | EastWest | 7 | 712 | 696 | **16** |

`(3,0)`'s east edge (column 32) and `(4,0)`'s west edge (column 0) are supposed to be the *same physical row of vertices*, authored to share edge values by construction (`check_seam`'s own doc, `byroredux/src/cell_loader/terrain_seam.rs:5-7`). Three consecutive edge vertices (rows 5, 6, 7 of the shared column) disagree by 8–16 world units each — not floating-point noise (these are exact-looking, coarse-grained integer deltas, consistent with the two cells' VHGT delta chains genuinely encoding different heights at this row range, not a rounding artifact), and large enough that a rendered mesh would show a visible step/crack at this exact seam.

## Location

FO4 Commonwealth worldspace, cell grid coordinates `(3, 0)` and `(4, 0)`, shared east/west edge, row indices 5–7 (of the LAND record's 33×33 grid, `crates/plugin/src/esm/cell/mod.rs`'s VHGT layout).

## Not yet determined

- **Is this present in vanilla `Fallout4.esm`, or introduced by a mod/plugin override this session's data directory has installed?** The reproducing run used `BYROREDUX_FO4_DATA` pointed at whatever's installed at `/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data` — not confirmed to be a clean vanilla install. If a mod overrides one of these two cells' LAND record without also updating its neighbor's, that's exactly the "botched DLC/mod override" scenario `check_seam`'s own module doc names as the target defect class — a real finding either way, but the fix (or lack thereof) differs: a vanilla-content defect is something to accept/document as a known engine-visible-only artifact (Bethesda's own renderer may hide it via some mechanism ByroRedux doesn't yet replicate, or vanilla may just have this crack too), whereas a mod-introduced defect means the gate is correctly doing its job against modded content and no engine change is needed at all.
- **Does this manifest as an actual visible crack in a screenshot at this location?** Not yet captured — the `boundary` mode's retained screenshot is from wherever the traversal happened to be when the bench window closed, not necessarily overlooking cell (3,0)/(4,0)'s shared edge.
- Whether this is an isolated one-off or the tip of something broader (only 1 of 17 checked pairs in one traversal, but the traversal only samples cells along `grid-cross`'s specific 3-cell path — a fuller sweep might find more or confirm this is rare).

## Suggested next step

1. Confirm whether the installed FO4 `Data` directory has any Commonwealth-touching plugins beyond the base game + official DLCs (`plugin.list` / load order inspection) — if so, identify which plugin touches cell (3,0) or (4,0) and re-test with it disabled.
2. If confirmed vanilla-only: capture a screenshot centered on this exact seam (`cam.where`/manual fly-to, or a dedicated bench-camera waypoint) to confirm visual impact before deciding whether this needs an engine-side mitigation (e.g., snapping/averaging shared-edge heights at spawn time) or is purely a documentation/known-issue item.
3. If mod-introduced: no engine change needed — this becomes a positive regression-test fixture instead (a real crack-triggering case for `check_seam`'s live wiring, which previously only had synthetic fixtures).

## Related
#2371 (EX-10/11, parent — item 7's "live validation against real cross-plugin/DLC LAND override content" is the open item this closes the loop on, whichever way it resolves). `byroredux/src/cell_loader/terrain_seam.rs` (the checker), `byroredux/src/streaming_helpers.rs::update_terrain_seam_stats` (the live wiring).


---

# #3307: EX-10/11 item 8: active VWD full-model culling (decouple full-REFR spawn radius from radius_unload)

Labels: enhancement, renderer, legacy-compat, terrain-exterior

## Description

Split out of #2371 (EX-10/11 item 8) per that issue's own stated intent ("scope as its own follow-up issue rather than folding into EX-10/11 closure") — filed now since it was never actually spun off.

## Background

The VWD (VisibleWhenDistant) flag is fully parsed (#1731) and materialized per placement as the `VisibleWhenDistant` marker (#1889), with **no render-time consumer today, by design**. Full REFRs only ever spawn inside `radius_unload`; both terrain and object LOD rings load strictly outside it — so a full model and its `.bto`/LOD proxy structurally never coexist under the current streaming radii, and there's nothing to cull yet.

The **detection half** already landed (commit `2a84ab97`): `LodCoverageStats::vwd_full_model_overlaps` audits, live, that a resident `VisibleWhenDistant`-flagged REFR's cell never falls inside a resident object-LOD quad's footprint — reads 0 on every real session today, proving the ring-separation argument holds, and becomes the regression gate for whoever builds the active cull.

## What's actually needed

Building the *active* VWD cull requires giving VWD-flagged REFRs their own streaming radius **beyond** `radius_unload` — today's streaming is whole-cell granularity only; nothing spawns an individual REFR independent of its cell. `docs/engine/exal.md` §5.2 already states this decoupling "needs real-game visual validation before it is enabled" — building it and shipping disabled would be real effort with no way to prove correctness beyond synthetic tests; enabling it blind is exactly the kind of visually-consequential, `cargo-test`-invisible change this project's standing policy says not to ship speculatively (`feedback_speculative_vulkan_fixes`, generalized to any render-visible streaming-radius change).

## Suggested approach

1. Design the per-REFR VWD streaming radius as an addition to (not a replacement of) `radius_unload` — needs to reintroduce the #1866 overlap risk carefully, since that's exactly what the current ring-separation argument prevents by construction.
2. Wire the actual cull consumer (skip drawing/spawning the full REFR once it enters `radius_unload` proper, relying on the LOD proxy instead — or the inverse, depending on the chosen direction).
3. Validate live against a location with real VWD-flagged content and confirm no visual pop/seam at the transition boundary.
4. `LodCoverageStats::vwd_full_model_overlaps` already gates the regression case (a VWD REFR and its LOD quad proxy must never both render) — keep it green throughout.

## Related
#2371 (EX-10/11, parent). #1866/#1889/#1731 (VWD parse/marker history). `docs/engine/exal.md` §5.2.


---

# #3308: EX-10/11 item 9: reversed-Z depth buffer (current 250,000 BU LOD ring has ~37,253 BU/step resolution at the far edge)

Labels: enhancement, renderer, vulkan, terrain-exterior

## Description

Split out of #2371 (EX-10/11 item 9) per that issue's own stated intent ("file it separately given the blast radius") — filed now since it was never actually spun off.

## Background

`Camera::depth_resolution_at` (`crates/core/src/ecs/components/camera.rs:34-64`) measures — not guesses — the world-space span of one depth-buffer step at distance, for the current conventional (non-reversed) `D32_SFLOAT` depth pipeline with `near=0.1`:

| Distance | Depth resolution |
|---|---:|
| 1,000 BU | 0.6 |
| 50,000 BU | 1,490 |
| 196,608 BU (old LOD ring) | 23,040 |
| 250,000 BU (current LOD ring) | **37,253** |

At the current 250,000 BU LOD ring, there is effectively **no depth discrimination** between distant terrain and the object LOD standing on it — a strong candidate explanation for distant-LOD z-fighting. This is long-standing (already ~23,000 units at the *old* ring), not introduced by the EX-11 band-ladder work; that work only made it more visible by extending the ring further.

Two remedies were identified and both deliberately deferred:

- **Raising the near plane** (what every shipped Gamebryo game does — `fNearDistance = 5` in `Fallout_default.ini`, `10` in `Oblivion_default.ini` — buys ~50×) cannot be a constant edit: one camera contract serves both BU-scale worldspaces and the unit-scale demo/loose-NIF scenes (`scene.rs` parks the no-content camera 4 units from the origin — a 5-unit near plane would swallow it entirely). Needs a scale-aware `near`, itself a real design change.
- **Reversed-Z** is the actual fix and pairs well with `D32_SFLOAT`, but touches the projection, depth clear, pipeline compare state, and every depth consumer: SSAO, SVGF reprojection, TAA, composite, water, FSR3 linearization. None of those failure modes are visible to `cargo test` — needs a GPU capture gate (RenderDoc) per this project's standing no-speculative-Vulkan-fixes policy.

## Suggested approach

1. Scope the reversed-Z projection/depth-clear/compare-state change precisely against each of the six-plus named consumers before touching any of them.
2. Build a GPU capture/comparison gate (RenderDoc or a live before/after depth-buffer dump) since none of this is `cargo-test`-visible.
3. Consider the scale-aware near-plane change as a cheaper, narrower first step (~50× improvement) if reversed-Z's full blast radius proves too large for one session.

## Related
#2371 (EX-10/11, parent). `crates/core/src/ecs/components/camera.rs:34-64` (the measurement + `DEFAULT_RENDER_DISTANCE` documentation).


---

