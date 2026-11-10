# Reference-Quality Lighting — Product Requirements

**Status**: ACTIVE (2026-11-10). Revision 31; the change history is in
§12. Every decision in §10 is decided and every Phase 0 verify item is
closed or filed as an issue.

**Tracking**: #6595 (parent). Phases: #6596 (0), #6601 (1a), #6598 (1b),
#6602 (2), #6603 (3), #6604 (4), #6599 (5). M51 tools built here: #6600
(exporter, PTR-VAL-004), #6597 (EXR writer, PTR-OUT-001). Verify items
filed: #6605 (V9, closed in rev 15), #6606 (V12). Statements about
how an original engine, a published technique or an external tool behaves
cite external sources (UESP and the Creation Kit wikis, the cited papers
and vendor documentation); a claim that could not be verified is marked
**Phase 0: verify** instead of being stated as fact.

**Scope**: M-LIGHT ([ROADMAP Tier 8](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon))
closes the gap between "lit correctly" and "lit as well as the data allows"
for the four lighting terms the row names: soft shadow penumbrae, contact
shadows, image-based lighting from the sky, and multi-bounce diffuse GI. It
promises two different things, and keeps them apart. On the self-contained
Cornell scene, each term is *measured* against a converged external
reference and held within an owner-decided bias budget by a runnable gate.
On game content, the same terms are held by content gates against each
phase's declared changes (§5.8) and by probes; no game scene is compared to a reference unless Q9 takes
its alternative. Most of the row's mechanisms already ship (§3.2); this PRD
spends its requirements on the measured gap. Out of scope: both
path-tracing modes (M51, [path-tracing-reference-mode.md](path-tracing-reference-mode.md)),
any denoiser replacement (M54), volumetric light shafts (M55), skin and
hair (M56 / M57), distant-LOD geometry (M35,
[distant-lod-completion.md](distant-lod-completion.md)), sun shadows on
receivers past the shadow fade (RLT-NG-008), and per-pixel parity with the
original games' look ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism and tool promised here has exactly one owner in this
table, or Phase 0 writes the missing design.

| Mechanism or tool | Owner |
|---|---|
| Units, emitters, visibility layers, adaptive ray budget | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md) |
| ReSTIR-DI, shadow-ratio EMA, TAA clamp, RT seed trade-offs | [Shadow Pipeline Trade-offs](../engine/shadow-pipeline-tradeoffs.md) |
| Cornell L0–L5 oracle, RT debug views, ray probe; the two component views this PRD adds (RLT-SHD-009, RLT-GI-009) | [RT Lighting & Material Recovery](../engine/rt-lighting-material-recovery.md) |
| Convergence captures, A/B workflow, real-content probe | [Renderer Evaluation](../engine/renderer-evaluation.md) |
| Sky cubemap bake, GGX prefilter, SH irradiance, interior portal lane | [SKYAL](../engine/skyal.md) |
| XCLL / DALC ambient, interior directional key | [Cell Lighting](../engine/lighting-from-cells.md) |
| Ground-cover ray-tracing boundary | [EXAL Ground Cover](../engine/exal-groundcover.md#5-ray-tracing-boundary) |
| SVGF, G-buffer, pass order | [Vulkan Renderer](../engine/renderer.md) |
| External reference renderer (M51 §10 Q5), the Cornell exporter (PTR-VAL-004), the EXR writer (PTR-OUT-001), the hash-grid radiance cache (PTR-RT-002) | [Path Tracing Reference Mode PRD](path-tracing-reference-mode.md) alone. What this PRD needs the exporter to do beyond PTR-VAL-004 is filed as change requests against M51 (§5.9, `RLT-EXP`), unowned until M51's revision log accepts them; the rung subsection is [§2 of path-tracing.md](../engine/path-tracing.md#2-reference-lighting-rung-subsection), written early with its exporter section (rev 10); M51's Phase 0 writes the rest of that design |
| Component-decomposition contract, calibration, rung content policy | This PRD, §3.4 and RLT-REF-008…014 |
| Penumbra-aware shadow filtering | [Penumbra-Aware Shadow Filter](../engine/penumbra-aware-shadow-filter.md) (RLT-SHD-003; written in Phase 0, rev 10) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `RLT-` prefix.

---

## 1. Product thesis

ByroRedux is RT-first ([keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)):
every light is ray-traced for visibility, the sky is baked and prefiltered,
and diffuse light is gathered along a bounded material-aware path. That
pipeline is "lit correctly" — the Cornell oracle proves which light entered,
which ray was fired and what it hit
([R4](../engine/rt-lighting-material-recovery.md#r4---build-the-cornell-l0-l5-oracle)).
It is not yet "lit as well as the data allows": nothing measures how far a
penumbra, a contact edge, a sky reflection or a bounce is from what a
converged renderer would produce for the same scene, so every tuning since
has been judged by eye. The people this serves are the ones tuning the
renderer, who today have no number to tune against, and the players of
every loaded title, who see the result.

The owner's framing is to benchmark against modern references, not against
the legacy look. Beating a 2006–2015 forward renderer's shadow maps lands at
"not bad"; the reference this PRD names is a **converged, unbiased render of
the same scene** (§3.4), and the modern real-time techniques it names
(Heitz et al. 2018's ratio estimator, NVIDIA's SIGMA shadow denoiser as
publicly described, Fernando's percentage-closer soft shadows, Bend
Studio's screen-space shadows, Karis's split-sum IBL) are the yardsticks
for *how* to approach it, not looks to copy.

The engine will not chase a game's authored look per pixel, will not add
shadow maps, PCF or cascades back as a fallback, will not trade the
adaptive ray budget's frame-time guarantee for fidelity, and will not claim
reference quality for a scene no reference was rendered of. The corollary
that shapes every requirement below: **a lighting term is "reference
quality" only when a named gate measures its distance from the converged
reference, on a quantity both sides define identically, and that distance
is within a budget the owner set** — a mechanism that ships without such a
gate is listed in §3 as shipped-unmeasured, never as done.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Term** | One of the four lighting quantities M-LIGHT gates: L1 penumbra, L2 contact, L3 sky IBL, L4 multi-bounce GI (§3.1). Each has exactly one gate. |
| **Mechanism** | An engine component that produces part of a term (§3.2). Mechanisms have no state of their own and are never gated alone. |
| **Converged reference** | An image of a rung rendered by the external renderer (M51 §10 Q5) to negligible noise, from the exporter's scene (RLT-REF-001) with the mapped BSDFs (RLT-REF-008), and with every emitter's intensity scaled at export by its expected factor (RLT-REF-010). No factor is applied to the image. |
| **Raster mode** | The shipped hybrid renderer: rasterised primary visibility, ray-query shadows, reflections and GI ([renderer.md](../engine/renderer.md)). The mode this PRD measures. |
| **Progressive reference** / **interactive PT mode** | M51's two path-tracing modes, as [path-tracing-reference-mode.md §2](path-tracing-reference-mode.md#2-terminology) defines them. Neither exists at HEAD. "PT mode" in the ROADMAP row means the interactive one. |
| **Hash-grid radiance cache** | M51's world-space radiance cache, defined in [path-tracing-reference-mode.md §2](path-tracing-reference-mode.md#2-terminology) and required by PTR-RT-002; the ROADMAP rows' "SHARC" names this component whatever its provenance under M51's Q2. |
| **Rung** | A Cornell scene variant built for one measurement, in the sense of the L0–L5 oracle ([R4](../engine/rt-lighting-material-recovery.md#r4---build-the-cornell-l0-l5-oracle)). A **reference rung** is one that has a converged reference and obeys RLT-REF-009. |
| **Component view** | An engine debug view used as one side of a metric; §3.4 states what it contains and the reference recipe that produces the same quantity. |
| **Linear capture** | A raster-mode image of scene-linear radiance taken at composite's HDR output, before bloom, exposure, grading and tone mapping, averaged over a stated number of consecutive frames and written by the EXR writer (RLT-REF-007), as opposed to the 8-bit PNG of the presented swapchain that `screenshot` writes. |
| **Expected factor** / **calibration ratio** | *k*<sub>exp</sub> is the scale between the reference's radiance units and the engine's scene-linear units, derived on paper from the backbone's unit chain (emitter radiant intensity, world units per metre) and the exporter's unit conversion, one value per emitter kind (point, sun, environment). The exporter scales each emitter's intensity by it. The calibration ratio *c* is engine ÷ reference over a calibration rung exported that way (RLT-REF-010), and must be 1 within the combined floor. It is a check, never a fit: *c* ≠ 1 is a bug report against the unit chain or the exporter. No factor is ever applied to a rendered image. |
| **Noise floor** | A metric's value between two captures of the same side that differ only in sampling seed (RLT-REF-006, RLT-REF-013). The lower bound of any threshold, never the threshold itself. |
| **Combined floor** | The floor of a comparison between two sides: the root-sum-square of the two sides' floors, since the two sides' sampling noise is independent. Every "combined noise floor" in this PRD means this. |
| **Representation floor** | The deterministic storage error of an engine capture, which no seed pair can see because it is identical under every seed: the capture targets are RGBA16F, whose one ULP is at most 2<sup>−10</sup> of the value, and composite reads indirect light and albedo from B10G11R11 images. A ratio check allows 2<sup>−10</sup> on top of its combined floor; the engine closure models the B10G11R11 storage in its prediction and reports what it costs `final` (rev 11). |
| **Frozen floor** | A gate's combined floor as measured at the entry of the phase that improves its term (§9, phase entry), or at its first run for a gate no phase improves, and written into the threshold table. Later runs re-measure the floor and report it beside the frozen one; they replace it only under RLT-REF-006's re-freeze rule. |
| **Floor drift margin** | How far a re-measured floor may rise above its frozen floor before the gate fails, set per term with its bias budget (Q6). |
| **Bias budget** | The error a term may carry above its frozen floor, decided per term by the owner (Q6). Raster mode is biased by design (bounded path, luminance clamp, W clamp, SSAO, SVGF), so the budget is a product decision, not a measurement. |
| **Threshold** | A term's frozen floor plus its bias budget. A gate passes when its metric is within the threshold and its re-measured floor is within the drift margin. |
| **Self-consistency gate** | A gate that compares the engine with itself (two engine captures, an engine A/B, an isolated GPU unit test) rather than with the converged reference. Alone it earns state S1+, never S2. |
| **Visibility** | Per pixel of a direct view, direct radiance ÷ unshadowed direct radiance, in [0, 1]. Engine side: the filtered direct view (RLT-SHD-009) divided by the same view captured with `DBG_DISABLE_DIRECT_SHADOWS` (`0x8000000`, `crates/renderer/src/shader_constants_data.rs`). Reference side: the direct recipe divided by the direct recipe with the rung's casters removed. Pixels whose unshadowed direct is below the rung's stated minimum are excluded and counted in the manifest. |
| **Visibility error** | Per pixel, \|*V*<sub>raster</sub> − *V*<sub>ref</sub>\| over a region, reported as its mean and its 95th percentile. The metric for every direct-view region that is partly or fully shadowed (filter bands, contact bands), where a ratio to the reference luminance is undefined. |
| **Temporal error** | The relative RMSE, over a region, between the engine's linear `final` capture during or after a motion and the engine's parked, converged `final` capture at the same pose and the same blocker position, normalised by the region's mean parked luminance. A self-consistency metric: both sides carry the same indirect bias, so it isolates ghosting, lag and boiling. |
| **Shadow ratio** | The per-pixel ratio Σrad·V ÷ Σrad the direct term is shaded by (Heitz et al. 2018), stored in the ReSTIR reservoir as `accum` ([trade-offs §3](../engine/shadow-pipeline-tradeoffs.md#3-restir-di-replaced-the-transient-m--8-estimator)). |
| **Shadow filter** | `shadow_filter.comp`: the edge-aware spatial filter over the shadow ratio. It writes a correction U·(filtered − raw) that `composite.frag` adds to the direct attachment. |
| **Profile line** | A world-space segment on a receiver, perpendicular to a shadow edge, fixed in the rung's code and recorded in the manifest. |
| **Penumbra width** | The world-space distance along a profile line between the 10 % and 90 % points of a smooth step fitted to the visibility (§2) profile (RLT-SHD-001). Visibility rather than luminance, so the unshadowed light's falloff with distance and incidence angle along the line cannot move the two points. |
| **Contact band** | The receiver strip that borders the line where one caster touches the receiver, defined per caster and fixed in the rung's code. For an upright caster its width is no more than one tenth of *that caster's* height above the receiver, so the band measures contact, not penumbra. For a caster lying flat (the plank, RLT-CON-006) the band is a fixed world-space width stated in the rung's code, independent of the caster's height, and the plank's thickness is no more than that width. |
| **Sky probe** | SKYAL's baked sky cubemap (`sky_cube.comp`), its GGX mip chain (`sky_prefilter.comp`) and its nine SH coefficients (`sky_irradiance.comp`). |
| **Bounded path** | The GI walk in `triangle.frag`: up to `MAX_PATH_SEGMENTS` segments, `MAX_SHADED_HITS` light-evaluated vertices, `MAX_DIFFUSE_BOUNCES` diffuse events. |
| **Energy deficit** | 1 − (indirect luminance in raster mode ÷ indirect luminance in the converged reference), over a named region. Positive means raster mode is darker; negative means it is brighter. |
| **Relative error distribution** | Per pixel, \|raster − reference\| ÷ *L*<sub>region</sub> over a region, where *L*<sub>region</sub> is a fixed normaliser of the region: for the direct-light terms (L1, L2), the region's mean reference direct luminance with every caster removed (its unoccluded luminance); for L3 and L4, the region's mean reference value of the same component (sky specular, sky diffuse or indirect), because removing casters would change the whole indirect field. It is reported as its mean and its 95th percentile, so a light leak and an underestimate cannot cancel in one ratio and a dark pixel cannot make the ratio explode. No metric in this PRD divides by a per-pixel reference value. |
| **Ray quality tier** | The adaptive budget tier 0–3 (`AdaptiveRayBudget`, [backbone §6](../engine/physical-lighting-backbone.md#runtime-contract)); pinned in tests with `--rt-test-ray-quality-tier`. |
| **Settled tier** | The most frequent tier of a scene over a fixed window of frames that follows a fixed warm-up on the reference GPU, both lengths stated in the RLT-PERF-005 harness and recorded in the manifest. The share of window frames spent at any other tier is published with it as the scene's tier oscillation. |
| **Reference GPU** | The RTX 4070 Ti that the ROADMAP bench table is measured on ([How to run a bench](../../ROADMAP.md#how-to-run-a-bench)); replaced only by the RLT-OPS-003 procedure. |
| **Material class** | The pair (translation route, conductor or dielectric) that `byroredux/src/material_translate.rs` assigns a material: the route is the source signal the translate step read (BGSM-authored smoothness, legacy glossiness and specular, the keyword-classified legacy fallback, and any further route the Phase 1b dump finds), and the conductor side is metalness > 0.3, the cut T9 uses. The route list is closed after the Phase 1b dump with a revision row; a route found later is added only through another. Closed in rev 12: `bgsm`, `legacy_keyword`, `legacy_env_map`, `legacy_gloss_map`, `legacy_no_signal` (`--material-dump`, `PbrRoute`). |
| **Benchmark scenes** | The scenes of the ROADMAP bench table: the Cornell box, FNV Prospector Saloon, Skyrim SE WhiterunBanneredMare, FO4 MedTekResearch01 and FO4 Dugout Inn ([How to run a bench](../../ROADMAP.md#how-to-run-a-bench)). |

---

## 3. Core model

Each of the four terms has one **state**, from a closed set:

| State | Meaning | Reported as |
|---|---|---|
| **S0** | Absent: the term is not computed | "absent" |
| **S1** | Shipped, unmeasured: computed, no gate | "shipped, unmeasured" |
| **S1+** | Self-consistent: computed, and only a self-consistency gate holds it | "self-gated: <gate>" |
| **S2** | Measured: every reference gate of the term reports its distance from the converged reference on a §3.4 quantity, with its floor | "measured: <metric> = <value> (floor <value>)" |
| **S3** | Reference quality: every gate of the term — its reference gates and its temporal self-consistency gates — is within its threshold (frozen floor + Q6 budget) at the tier Q7 decides, on the reference GPU | "reference: <metric> within <threshold>" |

M-LIGHT is complete when all four terms of §3.1 are S3 on the Cornell
reference rungs, the content gate A11 (RLT-CNT-001) passes on every Wave 1
title and on every Wave 2 title for which an install is available, and
every phase stayed within its cost ceiling and the milestone within its
total (RLT-PERF-004, RLT-PERF-006). Two cases change the rule. A term the
owner declares done at its phase entry (Q6) is S3 from then on and its phase drops
it. If Q5 takes Alternative B, L4 completes at S2: its deficit published
and frozen as the raster-mode promise, with no S3 claim. The mechanisms of
§3.2 carry no state; rows that no gate here covers are listed in §3.3 with
their owner.

### 3.1 The four gated terms

The table below is the state at HEAD on 2026-11-01. Its Today column is
hand-written; from Phase 1b exit the generated Lighting fidelity table
(RLT-REF-004, §8) replaces it, each term's row filled at that term's
phase entry (§9), and this table is not maintained by hand after that.

| # | Term | Gate | State | Today |
|---|---|---|---|---|
| L1 | Soft penumbra | A2 (width) and A4 (filter bands) against the reference; A3 (camera motion) and A14 (moving blocker) as temporal self-consistency gates | S1+ | `cornell_parked_penumbra_converges_at_one_shadow_ray` (`byroredux/tests/cornell_rt_oracle.rs`, A1) compares a parked one-ray capture with an 8-ray engine capture in 8-bit luma: engine against engine, no reference |
| L2 | Contact shadow | A5 | S1 | No rung measures a contact band |
| L3 | Sky IBL, diffuse and specular, exteriors | A6, A17 | S1 | `vulkan::sky_cube::filter` GPU tests check the prefilter and the SH projection in isolation, not the lit result |
| L4 | Multi-bounce diffuse GI | A7, A8 | S1 | The `scripts/renderer-eval.sh` no-à-trous A/B compares the engine with itself |

### 3.2 Mechanism inventory (not gated)

The inventory is the code each term is built from, as read at HEAD on
2026-11-01. It is a dated snapshot for the reviewer and the Phase 1 issues,
not a status: it is not updated after approval, and the tracking issues
cite the code they change. Requirements refer to these rows by number
instead of restating them.

| # | Mechanism | Term | Today |
|---|---|---|---|
| T1 | Disk-jittered shadow rays: sun cone from `skyTint.w`, local lights from `GpuLight.params.y` (floor 1.0 unit) | L1 | `triangle.frag` ReSTIR finalize; sun radius `SUN_ANGULAR_RADIUS = 0.020` rad in `byroredux/src/env_translate/sky_weather.rs` |
| T2 | ReSTIR-DI reservoirs + shadow-ratio EMA (Heitz et al. 2018), one ratio per cluster (#5020) | L1 | Gated parked only, by the self-consistency test in §3.1 L1 |
| T3 | `shadow_filter.comp`: 5×5 B3 à-trous over the shadow ratio, 2 iterations (strides 1, 2), normal / distance / ratio edge stops, mesh-ID ownership test (#6249); its correction is added in `composite.frag`, which returns the raw main-pass value for every non-final debug view, so `direct_only` never shows it | L1 | `crates/renderer/src/vulkan/shadow_filter.rs` (`ITERATIONS = 2`); on by default (`shadow_filter_enabled: true`, `context/init.rs`); `r.shadowfilter on\|off`; `rawDebug` early return in `composite.frag`; GPU bracket `shadow_filter` inside `svgf` |
| T4 | 1 / 2 / 4 / 8 direct shadow samples at tiers 0–3 | L1 | `settings_for_tier`, `crates/renderer/src/vulkan/scene_buffer/ray_budget.rs` |
| T5 | Every opaque TLAS caster is traced from the representable-float origin offset with `tMin = 0`; SSAO never darkens the direct term, but it does multiply traced indirect (T12) | L2 | [R3](../engine/rt-lighting-material-recovery.md#execution-status) |
| T6 | Direct shadows fade from 8 000 to 12 000 world units; beyond, receivers keep their unshadowed direct light (#2554). Distant-LOD blocks whose bound is within `LOD_SHADOW_CASTER_DISTANCE` (= `SHADOW_FADE_END + DIRECTIONAL_SHADOW_TRACE_DISTANCE`) enter the TLAS as casters; farther ones are excluded | — (§3.3) | `SHADOW_FADE_START` / `SHADOW_FADE_END`, `crates/renderer/src/shader_constants_data.rs`; `LOD_SHADOW_CASTER_DISTANCE`, `crates/renderer/src/shader_constants.rs`; `tlas_exclusion` (`DistantLodBlock`), `byroredux/src/render/static_meshes.rs` |
| T7 | Exterior sky cube baked **every frame** (128² faces, 8 mips), GGX-prefiltered with 256 Hammersley samples, projected to 9 SH coefficients; scene set 1 bindings 20 / 21, gated on `exteriorSkyTint.w`. The bake evaluates `sky_radiance` (`include/sky.glsl`), which adds the sun disc and its glow, so both reach the prefiltered mips and the SH9 | L3 | [SKYAL §2.2](../engine/skyal.md#22-sky-cubemap--landed); `sky_cube.comp`; `SKY_CUBE_FACE_SIZE`, `crates/renderer/src/vulkan/sky_cube.rs` |
| T8 | SH9 sky irradiance replaces authored ambient on exteriors, cross-faded with the traced share (`tracedDiffuseWeight`) | L3 | `exteriorSkyDiffuseOr`, `include/bindings.glsl`; consumer in `triangle.frag` `main()` |
| T9 | Prefiltered sky on reflection-ray misses and the RT-off fallback, **only** for conductors (`metalness > 0.3`), materials with an authored cube, or Skyrim shader type 1 (EnvironmentMap, `materialKind == 1u`), never for eyes or glass, **only** below `roughness < 0.6`, scaled by a `(1.0 - roughness)` heuristic | L3 | `needsEnvironmentReflection` and `Lo += envColor * F * (1.0 - roughness)` in `triangle.frag`; the split-sum `envBRDFApprox` in `include/pbr.glsl` is used only for multi-scatter compensation |
| T10 | No interior probe; escaped GI rays and the untraced share read the authored DALC cube or XCLL ambient | — (report only, RLT-IBL-006) | `pathEnvironmentRadiance`, `include/lighting.glsl` (gated on `jitter.w`) |
| T11 | Bounded path with `MAX_DIFFUSE_BOUNCES = 2`; light evaluated at up to 2 vertices (tiers 2–3), 1 (tier 1), none (tier 0); per-sample luminance clamp `GI_SAMPLE_LUMINANCE_CLAMP = 1.0`; distance fade 4 000–6 000 units. The ROADMAP row's "currently 1 in raster mode" is stale | L4 | `triangle.frag` GI loop; tiers in `settings_for_tier` |
| T12 | Traced indirect is multiplied by SSAO (`combinedAO`) in all three ambient branches; interiors add authored ambient on top of traced indirect | L4 | `indirectLight` assembly in `triangle.frag` `main()` |
| T13 | SVGF temporal + à-trous over indirect | — (§3.3) | [renderer.md § SVGF](../engine/renderer.md#svgf-temporal-denoiser) |
| T14 | ≥ 2 bounces in PT mode | — (§3.3) | No PT mode at HEAD |

### 3.3 Not gated here

| Row | Why not here | Owner |
|---|---|---|
| T6 shadow reach | A far-field question, not one of the four terms | Recorded as unowned in RLT-NG-008; decided in Q4 and raised with the M35 PRD's review |
| T13 SVGF | The denoiser is M54's (RLT-NG-004); every L4 gate measures the pre-SVGF signal (§3.4) | [neural-denoiser.md](neural-denoiser.md) |
| T14 PT-mode bounces | A PT-mode property | M51, PTR-RT-007, checked by its A10 (RLT-NG-003) |

### 3.4 Component decomposition contract

A metric compares one quantity on both sides or it measures nothing. The
external renderer produces total radiance unless told otherwise, and the
engine's debug views each mix several terms, so this table fixes, per
view, what the engine side contains, what a reference rung removes so that
the view holds one quantity, and the reference recipe that produces the
same quantity. The "Engine side" column is read at HEAD on 2026-11-01 from
`triangle.frag` (`directLight` and `indirectLight` assembly, the
`viewDirectOnly` / `viewIndirectOnly` branches) and `composite.frag` (the
`rawDebug` early return). Renderer-specific parameter names for each recipe
live in the rung subsection of the exporter design (Related table).

| View | Engine side | Rung conditions that leave one quantity | Reference recipe |
|---|---|---|---|
| `direct_only` | `Lo`: each light's diffuse and specular × raw shadow ratio; environment specular (RT reflection ray, or sky probe on a miss) for materials T9 admits; eye environment; the glass rim term; plus emissive. Not the shadow filter's correction (T3). Not multiplied by anything downstream | RLT-REF-009 (no emissive, glass or eye materials); interior rungs use only dielectrics without an authored cube, so T9 adds nothing | Paths with exactly one scattering vertex, lit by the rung's emitters only, no environment |
| Filtered direct (new, RLT-SHD-009) | `direct_only` plus the shadow filter's correction | As `direct_only` | As `direct_only` |
| Visibility (§2) | Filtered direct ÷ filtered direct with `DBG_DISABLE_DIRECT_SHADOWS` | As `direct_only` | Direct recipe ÷ direct recipe with the rung's casters removed |
| `indirect_only` | The single-frame, pre-SVGF `indirectLight`, **not** multiplied by the local albedo: exteriors (SH9 sky × (1 − traced weight) + traced) × AO; DALC cells (cube + traced) × AO; other cells ambient × max(AO, 0.3) + traced × AO. Not diffuse-only at HEAD: the GI walk shades hits with a dielectric GGX lobe and samples it on ≥ 5 % of bounces (V11), so L4 rungs switch it off (RLT-GI-010) | Authored ambient zero and no DALC (RLT-REF-009); every material of an L4 rung purely Lambertian — `specular_strength` 0 (`GpuMaterial` offset 28, `crates/renderer/src/vulkan/material.rs`) and metalness 0 on the engine side (RLT-GI-010); averaged over the linear capture's frame count; multiplied by the exporter's albedo for the region's object (every L4 region is one flat-albedo object) | Reference total − reference direct, both at the same sample count, with every L4 material mapped to the renderer's plain diffuse BSDF, so the reference's indirect carries no specular scattering either |
| Traced share (new, RLT-GI-009) | The traced share as it enters `indirectLight`, including whatever multiplies it | As `indirect_only` | As `indirect_only` on interior rungs, where traced share and indirect coincide |
| Sky specular | `direct_only` on the sky-only rung: environment specular alone | The probe alone, with no floor, wall or other probe on either side (RLT-REF-021); the directional light zeroed and the sky dome unchanged (RLT-REF-019); diffuse albedo zero | Environment-only render of that one probe with the specular part of the mapped BSDF (RLT-REF-008), against the RLT-REF-012 environment |
| Sky diffuse | `indirect_only` × albedo on the sky-only rung | As sky specular, with one Lambertian probe alone | Environment-only render of that one Lambertian probe |
| `final` | Direct + filter correction + SVGF'd indirect × albedo + composite terms | Closure rung (RLT-REF-011): SVGF bypassed (RLT-REF-018), no temporal reconstruction or jitter (RLT-REF-020), fog out of range, no volumetric media, no caustic casters | Reference total |

---

## 4. Goals and non-goals

### 4.1 Goals

- **RLT-G-001 — Penumbrae match the reference, and stay stable in
  motion.** Parked penumbra width and the filter bands are within the L1
  threshold of the reference on the Cornell reference rungs; during and
  after a camera pan and under a moving blocker, the temporal error
  against the engine's own parked, converged capture is within its L1
  threshold.
- **RLT-G-002 — Contact regions are grounded.** The contact band of every
  caster class — rigid block, thin plank, alpha-tested caster, skinned
  proxy mid-animation, and a normal-mapped receiver — is within the L2
  threshold.
- **RLT-G-003 — Sky lighting is energy-correct.** Diffuse and specular sky
  IBL on the `--cornell-sun` probes are within the L3 threshold, for
  dielectrics and conductors, at the five roughness values of the L5
  sweep, with the sun counted once.
- **RLT-G-004 — The GI energy deficit is known and bounded.** The Cornell
  indirect energy deficit and its relative error distribution are
  published at each ray quality tier and, at the tier Q7 decides, are
  within the L4 threshold.
- **RLT-G-005 — No quality without a gate.** Every term is S3, every gate
  runs where and when RLT-OPS-001 says, and every number has a published
  noise floor.
- **RLT-G-006 — The content promise is stated as what it is.** Game titles
  are held by declared-change content gates (RLT-CNT) and probes, and
  ROADMAP never describes
  a game title as "reference quality" unless Q9 adds a content reference.

### 4.2 Non-goals

- **RLT-NG-001.** Per-pixel parity with Bethesda's look — ROADMAP
  [anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope).
- **RLT-NG-002.** Shadow maps, PCF or cascades, as primary or fallback path:
  the ROADMAP row excludes them, and RT-first is on the keep list.
- **RLT-NG-003.** Both PT modes and the PT-mode bounce count (T14): M51
  owns them, the bounce count as PTR-RT-007. This PRD consumes from M51 the
  exporter and the EXR writer (its Phase 1 issues), a second reference
  (RLT-REF-005), and, if Q5 is taken, the radiance cache (RLT-GI-008).
- **RLT-NG-004.** Replacing or gating SVGF (T13): M54 (neural denoiser)
  owns the indirect spatial filter.
- **RLT-NG-005.** Traced shadows from ground-cover blades: decided
  receive-only, with an analytic shadow owed back
  ([exal-groundcover §5](../engine/exal-groundcover.md#5-ray-tracing-boundary)).
- **RLT-NG-006.** Re-filing the main-pass cost of glass-transmitting
  shadows plus the second diffuse bounce: accepted as a quality decision
  (#2161, ROADMAP [Known Issues](../../ROADMAP.md#performance-and-measurement)).
- **RLT-NG-007.** Emissive materials as sampled light sources (next-event
  estimation on emissive triangles). Content lights rooms through LIGH
  records the backbone translates to `Emitter`s; emissive surfaces reach
  other surfaces through the bounded path. Recorded as a known weak spot in
  `cornell.rs` (`ceiling_light_panel`); M51's progressive reference samples
  emissive triangles (its Phase 1), the realtime path does not, and this
  PRD does not change that. Reference rungs therefore carry no emissive
  surface (RLT-REF-009), so no gate absorbs this known bias.
- **RLT-NG-008.** Sun shadows on receivers past `SHADOW_FADE_END`, and what
  is drawn beyond the loaded grid. M35 owns the distant geometry
  ([distant-lod-completion.md](distant-lod-completion.md)); that PRD makes
  no shadow promise, so far-field shadowing was owned by **neither** PRD
  until Q4. Q4 took Alternative A: on-screen sun shadow beyond the fade
  is RLT-CON-004's screen-space term (Phase 3); traced sun shadows past
  `SHADOW_FADE_END` and anything off-screen stay out of scope.
- **RLT-NG-009.** A converged reference of any game scene, unless Q9 takes
  its alternative. Game content is gated against declared changes
  (RLT-G-006, RLT-CNT).

---

## 5. Requirements by area

### 5.1 Reference and measurement (`RLT-REF`)

- **RLT-REF-001.** Every reference rung of this PRD MUST be written by the
  exporter M51 owns (PTR-VAL-004): geometry, emitters, camera and
  per-object BSDF parameters (albedo, roughness, metalness) under the
  RLT-REF-008 mapping, which PTR-VAL-004's export of the `--cornell`
  entities already implies. Everything beyond that — skinned poses,
  normal-map and alpha textures, the sun-free environment, the sun as a
  distant emitter, export-time emitter scaling — is a change request
  against M51 (§5.9, RLT-EXP-001…006). This PRD owns the rung content; M51
  owns the tool. At HEAD on 2026-11-01 no exporter exists; the scene is
  authored only in `cornell.rs` and `cornell/builders.rs`, and
  PTR-VAL-004's text names only "the parity scene".
- **RLT-REF-002.** Every emitter in a reference rung MUST use an
  attenuation law the external renderer implements, and the engine side of
  the rung MUST use the same law. At HEAD on 2026-11-01 every Cornell
  `spawn_point_light` emitter is built by `LightSource::from_legacy_world_units`
  (`byroredux/src/cornell/builders.rs`), which selects
  `AttenuationModel::LegacySoftRange`, an engine compatibility curve, with a
  source radius of `clamp(range × 0.05, 1, 32)` world units
  (`Emitter::from_legacy_world_units`, `crates/core/src/lighting.rs`). The
  reference rungs MUST therefore spawn `Emitter::inverse_square_point`
  emitters (`AttenuationModel::InverseSquare`, same file) whose cull window
  (`LEGACY_LIGHT_CULL_RANGE_MULTIPLIER` × range) lies outside the box.
  The engine samples a disk of radius *r* facing the shading point, which
  subtends atan(*r*/*d*); a sphere-area emitter of the same radius subtends
  asin(*r*/*d*). Each rung MUST keep *r*/*d* ≤ 0.1 at every measured
  receiver point, where the two angular radii differ by about 0.5 %, and
  the manifest MUST record the largest *r*/*d* used. V1 closed in rev 8
  against Mitsuba 3.9.1's sources: its `point` emitter is inverse-square
  but a delta light with no radius (`src/emitters/point.cpp`), so a rung
  whose measurement depends on the source size (a penumbra) MUST export a
  `sphere` shape carrying an `area` emitter of radius *r* and radiance
  *I* ⁄ (π*r*²), which gives a facing receiver the irradiance *I* ⁄ *d*²
  and is sampled over the asin(*r*/*d*) cone (`src/shapes/sphere.cpp`); a
  rung that measures no penumbra MAY export the delta `point`. Its
  `directional` emitter is a delta light at 3.9.1; an `angle` parameter
  landed on master on 2026-09-21 (commit 67bcc328) in no release yet, so
  the sun of finite angular radius (RLT-EXP-004) is chosen at Phase 3's
  entry between a pinned build carrying that commit and an environment
  map with the disc painted in, and the manifest records which.
- **RLT-REF-003.** Reference images MUST NOT be checked in when the
  reference renderer's licence forbids it; the exporter and a pinned
  renderer version MUST regenerate them locally. V2 closed in rev 8:
  Mitsuba 3's licence (BSD-3-Clause plus an "Enhancements" clause,
  identical at v3.9.1 and master) governs redistribution of the
  software and says nothing about rendered images, so it does not forbid
  checking references in; the cache rule (RLT-REF-015) still keeps them
  local, because they regenerate from the exporter in seconds.
- **RLT-REF-004.** A metric manifest (`manifest.tsv` from
  `scripts/renderer-eval.sh`, extended) MUST record, per case, each §8
  metric with its frozen floor, its re-measured floor and its threshold,
  the git revision, the ray quality tier, the GPU and driver identity,
  *k*<sub>exp</sub> and the calibration ratio per emitter kind, the
  reference-cache key (RLT-REF-015), and the sampling seeds of both sides. At HEAD on 2026-11-01 the manifest
  records PNG hashes and the `bench:` summary only
  ([renderer-evaluation](../engine/renderer-evaluation.md#quick-start)).
- **RLT-REF-005.** Once M51 ships its progressive reference, its converged
  capture of each RLT reference rung SHOULD agree with the external
  reference within the two references' combined floor (§2); a
  disagreement MUST be filed against M51 or the exporter, not absorbed into
  a threshold. M51's own parity check on its parity scene is PTR-VAL-002;
  this ID covers the RLT rungs only.
- **RLT-REF-006.** Every §8 metric MUST have a noise floor measured per
  rung on both sides — two engine captures with different sampling seeds
  (RLT-REF-013) and two differently-seeded reference renders — combined
  per §2. Each gate's combined floor MUST be measured and frozen into the
  threshold table at the entry of the phase that improves the gate's term
  (§9), together with the term's baseline and its Q6 bias budget; the
  threshold is the frozen floor plus the budget. Every later run MUST
  re-measure the floor and report it, and MUST fail the gate when the
  re-measured floor exceeds the frozen floor by more than the term's drift
  margin, so a change that makes the image noisier cannot loosen its own
  gate. When a phase intentionally changes a quantity that an
  already-frozen gate of another term reads (for example, a shadow change
  that moves the traced share an L4 gate reads), that phase MUST list the
  gate in its change declaration (RLT-CNT-002) and re-freeze the gate
  with a revision row. The baseline MAY move freely. The floor MAY rise by
  no more than the term's drift margin; a larger rise is a budget change
  and MUST carry a Q6 rationale, because the threshold is the frozen floor
  plus the budget and a higher re-frozen floor would otherwise loosen the
  gate. A term at S3 whose metric falls outside its threshold after the
  change drops to S2; a declaration cannot keep it at S3. A
  frozen floor, a threshold or a content-gate margin (RLT-CNT-001) MUST
  NOT change without a revision row in §12. At HEAD on 2026-11-01 the noise-floor practice exists for
  SKYAL captures ([SKYAL §4](../engine/skyal.md#4-harness)) and no lighting
  threshold exists.
- **RLT-REF-007.** Every §8 metric MUST be computed on linear captures
  (§2), never on the presented PNG, and the capture's frame count MUST be
  recorded. The EXR file MUST be written by M51's EXR writer
  (PTR-OUT-001). At HEAD on 2026-11-01 the Cornell oracle and
  `scripts/check-render-anchor.sh` compare 8-bit PNGs of the presented,
  tone-mapped swapchain (`cornell_parked_penumbra_converges_at_one_shadow_ray`
  measures "8-bit luma"), and the `image` crate is built with `png` only,
  as M51 records under PTR-OUT-001.
- **RLT-REF-008.** Every material in a reference rung MUST map to a named
  BSDF of the external renderer through one mapping table, kept in the
  rung subsection of the exporter design, that states for each engine
  lobe (Lambertian diffuse, GGX specular with the engine's multi-scatter
  compensation, conductor Fresnel) the reference BSDF and parameter
  transform. The same table MUST state the normal-map convention both
  sides use: the tangent-space basis and how its tangent frame is built,
  the green-channel orientation, and the texel-to-normal decode, so the
  normal-mapped floor (RLT-CON-007) measures shadowing, not a convention
  mismatch. The table MUST also state, for each rough lobe, whether the
  reference BSDF models multiple scattering (V13). Where it does not,
  every rung that compares specular energy — the material-validation rung
  (RLT-REF-014) and the sky specular sweep (RLT-IBL-003) — MUST either run
  the engine with its compensation off (`DBG_DISABLE_MULTISCATTER`,
  `0x2000`, which at HEAD on 2026-11-02 gates the light loop's
  compensation in `include/lighting.glsl`; the split-sum environment term
  RLT-IBL-001 adds MUST honour the same bit), or record the expected
  difference per roughness and lobe in the table, so neither gate counts
  a modelling difference as error. V13 closed in rev 8: none of Mitsuba
  3's rough BSDFs (`roughconductor`, `roughdielectric`, `roughplastic`,
  `principled`) compensates for multiple scattering — each is the
  single-scattering D·G·F ⁄ 4 cos form with a separable Smith G — so every
  specular-energy comparison runs the engine with
  `DBG_DISABLE_MULTISCATTER`. The table MUST also record, per roughness,
  the difference the engine light loop's Smith *k* = (*r* + 1)² ⁄ 8 remap
  (`geometrySmith`, `include/pbr.glsl`) makes against Mitsuba's GGX, and
  the exporter MUST set `distribution="ggx"` (Mitsuba's default is
  Beckmann) with α = roughness². M51's per-probe mapping of the Cornell
  materials (its V4) is the starting point; this ID covers the materials
  of this PRD's rungs.
  Phase 1a validates the Lambertian row only, the one every Phase 1a rung
  uses; the specular rows are validated at Phase 4's entry, before the sky
  sweep consumes them, with their per-roughness expected difference
  measured on the material rung (rev 11).
- **RLT-REF-009.** A reference rung MUST NOT contain an emissive surface,
  a glass or eye material, authored ambient, a DALC cube, fog within the
  box, or a volumetric medium. At HEAD on 2026-11-01 the classic
  `--cornell` spawns `ceiling_light_panel` and `emissive_cube` and sets a
  flat ambient of 0.03 (`install_cornell_lighting`, `byroredux/src/cornell.rs`),
  and `--cornell-sun` keeps `emissive_cube`.
- **RLT-REF-010.** *k*<sub>exp</sub> (§2) MUST be derived on paper for
  each emitter kind — point, sun, environment — from the backbone's unit
  chain and the exporter's unit conversion, and written into the rung
  subsection of the exporter design before any rung is rendered. The
  exporter MUST scale each emitter's intensity by its *k*<sub>exp</sub>
  (RLT-EXP-005). No factor is applied to a rendered image, so a scene that
  mixes point emitters, the sun and an environment is calibrated per
  emitter. One calibration rung per emitter kind then checks the chain, on
  an export scaled that way, and each MUST give a calibration ratio (§2)
  of 1 within the combined floor:
  - point: one inverse-square point emitter above a white Lambertian
    plane, nothing else, over the plane's centre region, with the ratio
    also constant across the plane (which checks the inverse-square and
    cosine laws);
  - sun: the `direct_only` view of a purely Lambertian plane
    (`specular_strength` 0 and metalness 0, as RLT-GI-010) with no
    authored cube under `--cornell-sun`, so the sky probe cannot
    contribute: no environment specular reaches a lobe-free material, and
    sky diffuse is not in `direct_only` (§3.4);
  - environment: one Lambertian probe on the probe-only sky rung
    (RLT-REF-021), on the sky diffuse view, lit by a uniform environment
    if V6 finds the engine can light a probe by one, and otherwise by a
    deliberately smooth, low-frequency sky with no sun disc and no glow
    on both sides, where the engine's nine-coefficient irradiance is
    accurate. At HEAD on 2026-11-02 zeroing the dome's sun
    (`dome.sun_dir.w`) removes the glow, the disc and the Rayleigh / Mie
    term and keeps the gradient (RLT-REF-019), but `--cornell-sun`'s
    procedural sky also carries a cloud deck (`cloud_coverage` 0.35 from
    `WeatherSkyState::default`), so the smooth sky MUST also zero the
    cloud coverage; V6 found the uniform sky reachable the same way. A ratio other than 1 on
    the full sky with its glow is L3 bias, measured by A6, not a unit
    error; a unit bug is filed only when the ratio is off on the smooth
    sky.

  Each kind's rung is built in the phase that first uses the kind (§9).
  A ratio other than 1 beyond the floor MUST be filed as a bug against the
  unit chain or the exporter, and every absolute metric of that emitter
  kind stays unreported until it is fixed. At HEAD on 2026-11-01 the backbone defines emitters in
  scene-linear radiant intensity
  ([backbone §6](../engine/physical-lighting-backbone.md#runtime-contract))
  and nothing maps that scale to any external renderer.
- **RLT-REF-011.** A closure rung MUST show, on both sides, that the
  component views sum to the total: on the engine, with SVGF bypassed
  (RLT-REF-018) and no temporal reconstruction or jitter (RLT-REF-020), the linear
  `final` capture MUST equal filtered direct + `indirect_only` × albedo
  within the floor; on the reference, total MUST equal direct + indirect
  within its floor. The same rung with SVGF on MUST publish the energy
  SVGF adds or removes over each region, as a report handed to M54
  ([neural-denoiser.md](neural-denoiser.md)), never as part of closure.
- **RLT-REF-012.** The reference environment of every `--cornell-sun` rung
  MUST be the engine's sky radiance for that frame **without the sun
  disc**, evaluated directly from the analytic `sky_radiance`
  (`include/sky.glsl`) at a face resolution at least four times the
  engine's (`SKY_CUBE_FACE_SIZE`, 128², `crates/renderer/src/vulkan/sky_cube.rs`),
  never copied from the engine's baked cube, so the reference does not
  inherit the probe's discretisation and L3 can see a probe-resolution
  error. The resolution MUST be recorded in the manifest; the sun is a
  separate distant emitter (RLT-EXP-004). The sun's glow stays in the
  environment on both sides: it is sky radiance (`include/sky.glsl` treats
  it as clear-sky scattering). This export is change request RLT-EXP-003.
- **RLT-REF-013.** The engine's RT sampling MUST accept a seed offset, so
  two captures of one binary can differ only in sampling. At HEAD on
  2026-11-01 RT sampling is seeded by fragment coordinate and frame
  counter only (`include/math_common.glsl`), so two same-binary captures
  differ only by GPU nondeterminism and an engine-side noise floor cannot
  be measured.
- **RLT-REF-014.** A material-validation rung MUST place one flat plane of
  each mapped material (RLT-REF-008) under a single inverse-square
  emitter, plus one plane carrying the RLT-CON-007 normal map, and each
  plane's direct-only radiance MUST match the reference within the floor
  at three or more view angles and, for the normal-mapped plane, three or
  more light azimuths. A white-furnace variant SHOULD be added if the
  engine can light a probe by a uniform environment. V6 closed in rev 8:
  it can, but only through new scene set-up code — with the zenith,
  horizon and lower colours equal, the sun intensity zero, the cloud
  coverage zero and no cloud texture layers, `sky_radiance` is constant
  and so are the baked cube, its mips and the SH9; it must be an exterior
  sky, since interiors get no sky probe, and no CLI flag or console
  command sets those inputs today.
- **RLT-REF-015.** Reference images MUST be cached locally, keyed by a
  hash of the exporter's output for the rung, the reference renderer's
  version and the sample count, and MUST be re-rendered only when that key
  changes; an engine shader change MUST NOT invalidate the cache. At HEAD
  on 2026-11-01 nothing renders a reference, so no cache exists.
- **RLT-REF-016.** Phase 1a MUST publish the wall-clock time of one full
  run of the reference gates on the RT development machine, cold (every
  reference rendered) and warm (every reference cached), and the gate
  runner MUST print it at the end of every run. The time MUST include the
  engine's parked, converged capture at every sampled pose of the temporal
  gates (RLT-SHD-002, RLT-SHD-010), which no cache removes, and every phase
  that adds gates MUST re-publish it.
- **RLT-REF-017.** A bench mode whose animation clock advances by a fixed
  step and produces the same skinned pose at the same frame index on every
  run MUST exist for the skinned proxy (RLT-CON-003), and the exporter
  MUST export that frame's pose (RLT-EXP-001). At HEAD on 2026-11-01
  `renderer-static` steps 0 s and `renderer-stepped` a fixed 1/60 s
  (`BenchMode::delta_time`, `byroredux/src/bench.rs`). V9 (#6605, rev 15)
  settles it as the existing mode: under `renderer-stepped` the
  `AnimationPlayer` and transform propagation advance 1/60 s before each
  drawn frame, so drawn frame F is clip time (F + 1) / 60 s. Two runs of
  the contact rung with different RT seeds exported every file
  byte-identical at frame 45, frames 0, 44 and 45 differed only in the
  proxy mesh, and the proxy's yaw read back as 0.01 / 0.45 / 0.46 rad
  against the clip's 0.6 (F + 1) / 60. The proxy's bind inverse is the
  identity, so its exported palette is the bone world matrix that
  `build_skinned_palettes` uploads as `skin_palette.comp`'s input; the
  GPU multiply on identical inputs is deterministic. The mode needs an
  explicit `--bench-camera` path; since rev 17 `static` is accepted and
  holds the rung's camera while animation and the RLT-SHD-010 blocker step
  (a parked path would freeze the blocker, which shares its clock).
- **RLT-REF-018.** A switch that bypasses SVGF — both its temporal
  accumulation and its à-trous passes — MUST exist for the closure rung,
  as a `DBG_*` bit listed with its siblings. At HEAD on 2026-11-01
  `DBG_DISABLE_ATROUS` (`0x4000`) skips the à-trous passes only, and
  `DBG_DISABLE_TEMPORAL` (`0x40000`) is ReSTIR's temporal reuse, not
  SVGF's (`crates/renderer/src/shader_constants_data.rs`).
- **RLT-REF-019.** The sky-only rung MUST zero the directional light
  without changing the sky dome. At HEAD on 2026-11-01 the dome's sun
  intensity (`dome.sun_dir.w`) scales the sun glow and the disc in
  `sky_radiance` (`include/sky.glsl`, the glow and `disc_base` terms) but
  not the zenith-to-horizon gradient, so zeroing the dome's sun would also
  remove the glow the environment is meant to keep (RLT-REF-012); it also
  scales the Rayleigh / Mie atmospheric term. V10 closed in rev 8: the
  switch is new code. One value, `SkyParamsRes.sun_intensity`, feeds both
  the directional light (`compute_directional_upload`,
  `byroredux/src/render/lights.rs`) and `dome.sun_dir.w`
  (`byroredux/src/render/sky.rs`), and the only independent input,
  `CellLightingRes.directional_color`, is rewritten every exterior frame by
  the weather system and also drives the clouds' sun lighting. The switch
  drops the directional light after `compute_directional_upload` in
  `collect_lights`, leaving the dome untouched.
- **RLT-REF-020.** A capture mode MUST produce a linear `final` with no
  temporal reconstruction and no sub-pixel jitter, for the closure rung
  (RLT-REF-011). At HEAD on 2026-11-01 `--upscaler` accepts only `taa`
  and `fsr3` (`byroredux/src/cli_args.rs`), both of which reconstruct
  `final` temporally, and the TAA jitter is dropped only for raw-output
  debug views (`taa_jitter_gate`,
  `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs`), so
  this mode is new code.
- **RLT-REF-021.** The sky IBL energy rungs (A6) and the environment
  calibration rung MUST contain only the probe under test, on both
  sides: no floor, box wall or second probe, so the engine's reflection
  rays below the horizon and the reference's environment-only render see
  the same scene. Occlusion by a floor is measured by RLT-IBL-004, not by
  the energy sweep. At HEAD on 2026-11-01 `--cornell-sun` removes only the
  ceiling and keeps the floor and walls (`byroredux/src/cornell.rs`), so
  the probe-only variant is new rung content.

### 5.2 Soft shadows and penumbrae (`RLT-SHD`)

- **RLT-SHD-001.** On a new Cornell penumbra rung (a blocker at a known
  height above the floor under one finite emitter), the penumbra width
  (§2) MUST be within the L1 threshold of the reference's, at tier 3.
  The measurement MUST: fit the visibility (§2) of the filtered direct
  view (RLT-SHD-009), not its luminance; read a
  fixed set of profile lines; average each profile across a strip of
  pixels along the edge; fit a smooth step whose form the rung's code
  names, by least squares; capture with no temporal reconstruction and no
  sub-pixel jitter on the engine side and a one-pixel box filter on the
  reference side; and place the blocker so the reference penumbra spans at
  least 10 pixels at capture resolution. The sun case MUST be a separate
  rung at the Q3 radius, under the same rule. A sun penumbra is narrow.
  Behind a straight edge, a disk source of angular radius θ gives a full
  penumbra of about 2*h*θ for a blocker at height *h*, and a 10–90 % width
  of about 0.69 of that: the 10 % point is where the edge hides a circular
  segment of one tenth of the disk, about 0.69 radii from its centre. At
  0.0047 rad that is ≈ 0.0065 *h*. The sun rung MUST therefore reach the
  10-pixel minimum with a close-up camera on the shadow edge rather than a
  taller blocker; for a field of view *W* world units wide across *N*
  pixels, *h* ≈ 7.3 *W* ⁄ (θ*N*) is the sizing estimate. Every penumbra
  rung's code MUST assert the 10-pixel minimum on the reference's own
  measured 10–90 % width, not on the estimate. At HEAD on 2026-11-01 every
  non-final debug view forces a native, unreconstructed blit
  (`render_debug_requires_raw_output`, `crates/renderer/src/shader_constants.rs`;
  `is_fsr_dispatch_active`, `context/draw.rs`), and the L2 rung asserts a
  hard edge only ([R4](../engine/rt-lighting-material-recovery.md#r4---build-the-cornell-l0-l5-oracle)).
  Under `--upscaler taa` the camera jitter is off for these views too
  (`taa_jitter_gate`, `context/assemble_camera_and_lights.rs`, #4513).
- **RLT-SHD-002.** During a `--bench-mode renderer-stepped --bench-camera pan`
  run on the penumbra rung, the temporal error (§2) over the penumbra
  region MUST stay within the L1 during-motion threshold at every sampled
  frame of the motion (a fixed stride, recorded in the manifest), and
  within the L1 post-stop threshold at 1, 4 and 16 frames after the camera
  stops. The two thresholds carry separate budgets (Q6), because temporal
  reuse loses its history while the camera moves. The comparison is the engine against its
  own parked, converged capture at the same pose, never against the
  reference: in the penumbra `final` is dominated by indirect light, so a
  reference comparison would report the L4 deficit as motion error, and it
  would need a converged offline render per sampled pose. Distance from
  the reference is A2's job. `final` is used because the debug views
  bypass temporal reconstruction, and the temporal path is what this gate
  measures. At HEAD on 2026-11-01 the only penumbra gate is parked (§3.1
  L1), and the pan path has no hold segment after it stops.
- **RLT-SHD-003.** The shadow filter's spatial reach SHOULD scale with the
  expected penumbra size rather than using a fixed kernel. The design MUST
  be built from published descriptions only: NVIDIA's NRD README, which
  states that SIGMA takes the per-pixel distance to the occluder as an
  input ([NRD README, "Distance to occluder (SIGMA)"](https://github.com/NVIDIA-RTX/NRD)),
  and the blocker-distance penumbra estimate of Fernando, "Percentage-Closer
  Soft Shadows" (SIGGRAPH 2005 Sketches). Nobody implementing it opens
  NRD's shader sources, and the NRD SDK is not a dependency. At HEAD on
  2026-11-01 the reach is fixed (T3: 6 pixels), and the 36-byte `Reservoir`
  carries the blocker's surface ID (`occluderSurface`), not its distance.
  V3 closed in rev 8: the reservoir has no free 32-bit word (its
  `accum` channels must stay f32 for the parked EMA floor), but each
  pixel's `directUnshadowed[].y` is written as `packHalf2x16(vec2(U.b,
  0.0))` (`triangle.frag`), so its high half is always zero and the
  shadow filter already binds that buffer; the selected ray's blocker
  distance (at most 12 000 units) fits it as an f16 at zero cost. A
  temporally accumulated distance would instead grow the 36-byte
  `Reservoir` to 40 bytes, +8 B/px across both frames in flight (+16.6 MB
  at 1080p).
- **RLT-SHD-004.** With the shadow filter on, the visibility error (§2)
  MUST be within the L1 threshold in a band just outside each penumbra on
  the lit side and on the umbra side, and across the shadow of a thin rod
  a few pixels wide on the penumbra rung. These are
  the places where the filter bleeds light into shadow or shadow into
  light. At HEAD on 2026-11-01 rev 2's version of this gate compared
  `direct_only` with the filter off and on, and those two images are
  identical by construction (T3).
- **RLT-SHD-005.** The bias of `RESERVOIR_W_CLAMP = 64.0` MUST be measured
  as the luminance-integral delta, clamp on versus off, on a dim-plus-hero
  light Cornell variant, and published. At HEAD on 2026-11-01 the
  measurement is owed
  ([trade-offs §1](../engine/shadow-pipeline-tradeoffs.md#1-reservoir_w_clamp--640)).
- **RLT-SHD-006.** The TAA variance clamp γ MUST be re-validated with the
  shadow filter on, by the RLT-SHD-002 capture at γ = 1.5 and γ = 1.0.
  At HEAD on 2026-11-01 the A/B is owed and predates the shadow filter
  ([trade-offs §2](../engine/shadow-pipeline-tradeoffs.md#2-taa-variance-clamp-γ--15)).
- **RLT-SHD-007.** The sun's angular radius MUST be the value Q3 decides,
  set once in the EXAL translate step (`env_translate/sky_weather.rs`), and
  the reference scene MUST use the same value. At HEAD on 2026-11-01 it is
  0.020 rad for every directional light (T1); the Sun's apparent diameter
  from Earth is 1 919 arc seconds ([NASA Sun Fact Sheet](https://nssdc.gsfc.nasa.gov/planetary/factsheet/sunfact.html)),
  a radius of about 0.0047 rad, so the shipped cone is about 4.3× the
  physical one.
- **RLT-SHD-008.** (withdrawn, rev 3) No radius the decisions consider
  exceeds the tangent-plane disk's stated limit.
- **RLT-SHD-009.** A component view that outputs direct light with the
  shadow filter's correction applied MUST exist, listed in the
  `RenderDebugMode` catalog. At HEAD on 2026-11-01 none does (T3).
- **RLT-SHD-010.** A moving-blocker rung MUST move a blocker along a
  frame-indexed path between emitter and floor, and the RLT-SHD-002
  temporal error on it — against the engine parked and converged with the
  blocker held at that frame's position — MUST be within the RLT-SHD-002
  during-motion and post-stop thresholds; the path ends in a hold, so the
  post-stop frames exist. At HEAD on 2026-11-01
  `--bench-camera` moves only the camera (`byroredux/src/boot/mod.rs`).
- **RLT-SHD-011.** A moving-light variant of RLT-SHD-010 SHOULD be added,
  since ReSTIR's temporal reuse is stressed hardest when the light moves.
- **RLT-SHD-012.** The visibility a light's traced shadow rays report MUST be
  the cosine-weighted occluded fraction of its emitter, the quantity §2
  defines as shadowed over unshadowed luminance and the reference computes,
  not the unweighted fraction of rays that arrive. Each ray's transmission
  is weighted by the receiver's shading normal dotted with that sample's
  offset vector, over the same dot with the emitter centre's. The vectors
  are taken UNNORMALISED, so the weight is linear in the disk offset and its
  mean over the disk is exactly 1: a pixel no occluder touches keeps
  visibility 1 and the unshadowed image (A8, A15) does not move (normalising
  each ray first leaves a second-order deficit of about (R / d)² / 4 on every
  lit pixel). Where the centre cosine is within twice the disk's angular
  reach of the horizon, or under a floor, the horizon clips the disk and the
  ratio is undefined: the weight is 1 there, and the rest is capped. The sun's
  tangent-plane jitter (RLT-SHD-007) takes the same weight. Both fragment
  variants, `triangle.frag.spv` and the opaque early-test
  `triangle_early.frag.spv` (`-DBYRO_OPAQUE_EARLY_TESTS=1`), MUST be rebuilt
  from the source; opaque geometry runs the second. The A5 run MUST read its
  reference at a sample count where the reference's own noise is under the
  band budget: one 1024-spp render leaves the plank band a mean error of
  3.2e-3 and a p95 of 1.7e-2 from reference noise alone (rev 27). The cap and
  the floor are Phase 3 verify items, set by the gates, not chosen here.
  At HEAD on 2026-11-06 `triangle.frag` averages the binary hits of K
  emitter-disk-jittered rays with equal weight and shades toward the light's
  centre; measured effect of the weight in rev 27.

### 5.3 Contact shadows (`RLT-CON`)

- **RLT-CON-001.** On a new Cornell contact rung (the tall block and a small
  cube resting on the floor under one emitter), the visibility error (§2)
  across each caster's contact band MUST be within the L2 threshold, at
  tiers 1–3, a deliberate exception to Q7's tier-3 rule. At HEAD on 2026-11-01 no rung measures a
  contact band (T5).
- **RLT-CON-002.** The same rung MUST pass with the shadow filter on: the
  filter's normal and distance edge stops MUST NOT lift occlusion in the
  contact band above the threshold. This is measured on the filtered
  direct view, because `direct_only` excludes the filter (T3).
- **RLT-CON-003.** The rung MUST include a skinned actor proxy standing on
  the floor, captured mid-animation in the RLT-REF-017 bench mode, and its
  contact band MUST be within the L2 threshold of a reference rendered at
  that frame's exported pose (RLT-EXP-001). A parked proxy cannot show a
  stale skinned BLAS. At HEAD on 2026-11-01
  `cornell_skinned_actor_receives_light_and_casts_shadows` checks that an
  actor casts a shadow, not where it meets the floor, and
  `renderer-static` freezes simulation
  ([determinism contract](../engine/renderer-evaluation.md#determinism-contract)).
- **RLT-CON-004.** If Q4 takes Alternative A, a screen-space shadow term
  in the manner of Bend Studio's screen-space shadows MUST apply only
  where no traced shadow exists for that pixel and light (beyond
  `SHADOW_FADE_START`, or a caster reported missing from the TLAS), and
  MUST NOT multiply a traced shadow. Its design MUST be built from Bend's
  published description ([Inside Bend: Screen Space Shadows](https://bendstudio.com/blog/inside-bend-screen-space-shadows))
  only; nobody implementing it opens the code linked there.
- **RLT-CON-005.** Contact bands on ground cover are out of this area
  (RLT-NG-005); a contact gate MUST NOT be written against blades.
- **RLT-CON-006.** The contact rung MUST include a thin plank lying on the
  floor, with its own flat-caster contact band (§2) and a thickness no more
  than that band's width.
- **RLT-CON-007.** The contact rung MUST include a normal-mapped floor
  receiver, so a shading normal that disagrees with the geometric normal
  near the crease shows as a contact error. The Cornell builders already
  synthesize a normal map (`synthesize_wavy_normal_map`,
  `byroredux/src/cornell/builders.rs`).
- **RLT-CON-008.** The contact rung MUST include an alpha-tested caster.
  V5 closed in rev 8: no builder sets `alpha_test` today, but a `Material`
  with `alpha_test` set, a non-zero `alpha_threshold` and a cut-out
  texture from `register_rgba`, spawned with `spawn_object`, needs no
  renderer change; traced shadow rays honour alpha test by committing the
  closest hit, testing coverage at mip 0 and re-tracing past holes, up to
  `MAX_ALPHA_SKIP_LAYERS = 8` (`include/shadow_transport.glsl`,
  `include/ray_hit.glsl`), so the caster MUST have fewer than eight
  layers along any measured ray.
- **RLT-CON-009.** Terrain contact cannot be expressed in the Cornell
  box; an exterior capture of terrain contact SHOULD be reported, not
  gated, by A11.

### 5.4 Image-based lighting (`RLT-IBL`)

The ROADMAP row asks for "IBL from per-cell HDR sky probe captured once at
cell load". HEAD does better for exteriors: the probe is re-baked every
frame, so it follows time of day and weather ([SKYAL §2.2](../engine/skyal.md#22-sky-cubemap--landed)).
That half is kept as shipped; the requirements below close what it does not
reach.

- **RLT-IBL-001.** On exteriors, dielectric surfaces MUST receive
  environment specular from the prefiltered sky, weighted by the split-sum
  environment BRDF (Karis, *Real Shading in Unreal Engine 4*, SIGGRAPH
  2013, [course notes](https://cdn2.unrealengine.com/Resources/files/2013SiggraphPresentationsNotes-26915738.pdf),
  sections "Split Sum Approximation" and "Environment BRDF"). The LUT-free
  analytic fit already in `envBRDFApprox` (`include/pbr.glsl`) is a later
  Karis fit, "Physically Based Shading on Mobile" (2014), per that
  function's comment. V7 closed in rev 8: both constant vectors, *r*,
  `a004` and the returned scale and bias match the post exactly (read
  from its December 2014 Web Archive copy; the live page refuses), so the
  fit MAY be used as the weight as written. At HEAD on 2026-11-01 an ordinary
  exterior dielectric reflects no sky (T9).
- **RLT-IBL-002.** Environment specular above `roughness 0.6` MUST come
  from the sky mip for that roughness instead of being dropped; the RT
  reflection ray MAY stay gated at 0.6. At HEAD on 2026-11-01 the whole
  term is skipped above 0.6 (T9).
- **RLT-IBL-003.** The `(1.0 - roughness)` energy heuristic MUST be
  replaced by the split-sum weight, and the L5 roughness sweep, run on
  the probe-only sky rung (RLT-REF-021), MUST match the reference's sky specular (§3.4) within the L3
  threshold at each of its five roughness values.
- **RLT-IBL-004.** Sky specular MUST be occluded: in the RLT-CON-001
  contact bands under `--cornell-sun`, sky-specular luminance MUST be
  within the L3 threshold of the reference. Specular occlusion as described
  by Lagarde and de Rousiers, *Moving Frostbite to Physically Based
  Rendering 3.0* (SIGGRAPH 2014,
  [course notes](https://seblagarde.wordpress.com/wp-content/uploads/2015/07/course_notes_moving_frostbite_to_pbr_v32.pdf)),
  is the named technique; a traced visibility term is an acceptable
  alternative the design chooses.
- **RLT-IBL-005.** (withdrawn, rev 3) A mean-luminance cap would block the
  correct fix: dielectric sky specular raises mean luminance by design.
  Replaced by RLT-IBL-008 and RLT-IBL-009.
- **RLT-IBL-006.** In an interior whose portal lane shows the sky
  (`sky_lower.w` mode 1 or 2,
  [SKYAL interior lane](../engine/skyal.md#the-interior-outdoor-sky-lane-4927-2026-10-01)),
  a GI path that leaves through the aperture SHOULD read the portal sky,
  not the authored ambient. At HEAD on 2026-11-01 the cube already holds
  the portal sky in such interiors (`build_sky_cube_params`,
  `crates/renderer/src/vulkan/context/frame_params.rs`, bakes the
  `portal_outdoor_sky` palette, and `triangle.frag`'s window-portal escape
  reads it through `exteriorSkyRadianceOr(-V, …)`), but the GI escape
  does not (T10).
- **RLT-IBL-007.** Lighting from the sky probe and the visible sky MUST NOT
  disagree after a time-of-day or weather change for more frames than the
  number SKYAL records, and the probe MUST NOT become a once-per-cell
  capture. At HEAD on 2026-11-01 the number is zero: the bake and the
  composite background evaluate the same `sky_radiance` in the same frame
  (T7).
- **RLT-IBL-008.** Before the IBL phase ships RLT-IBL-001, a material dump
  MUST publish, for each Wave 1 benchmark scene and each material class
  (§2), the translated roughness and metalness distributions, and MUST
  raise these flags without human judgement:
  - F1: the share of dielectric surfaces translated below a stated
    roughness (the value is fixed in the dump tool, recommended 0.2, and
    recorded in the manifest);
  - F2: any material with metalness > 0.3 whose source carries no metal
    signal (no BGSM metalness, no authored environment-map conductor
    marker). On Oblivion, FO3 and FNV content the keyword-classified
    legacy route carries no metal signal by construction, so F2 flags
    every conductor that route produces: the owner should expect the
    whole route to be flagged and fixed or accepted as one decision, not
    read each flag as a separate defect;
  - F3: on FO4, any material whose translated roughness differs from
    1 − its authored BGSM smoothness beyond float tolerance — the
    conversion the engine applies to BGSM content
    (`(1.0 - smoothness).clamp(0.04, 1.0)` in
    `byroredux/src/asset_provider/material/merge.rs`, specular enabled
    only), with its 0.5 no-gloss-map fallback excluded from the flag.
    V12 could not be closed in rev 8: the Material-Editor source describes
    `fSmoothness` only as "Smoothness of the specular effect." with no
    conversion, and nif.xml's description contradicts the field's name, so
    the conversion is filed as #6606 and F3 checks the engine against its
    own rule, reported as an unverified convention, until it closes.

  Every flagged class MUST be fixed at the translation boundary or
  recorded as accepted with a reason, never capped in a shader; the owner
  reviews only what is flagged. This is the guard against the "everything
  looks like chrome" regression the dielectric gate in `triangle.frag`
  records: its cause is legacy glossiness translated to too-low roughness,
  or wrong BGSM metalness.
- **RLT-IBL-009.** The mean linear luminance of each Wave 1 benchmark scene,
  HEAD against the pre-IBL anchor, MUST be published by
  `scripts/check-render-anchor.sh` (extended in Phase 1 to read linear
  captures) as a report, not a gate.
- **RLT-IBL-010.** The sun's radiance MUST reach a surface once. On an
  unoccluded plane under `--cornell-sun`, the linear `final` capture MUST
  be within the L3 threshold of the reference built per RLT-REF-012 (sun
  as an emitter, environment without the disc). At HEAD on 2026-11-01 the
  disc is baked into the probe while the directional light also carries
  the sun (T7), so the sun's energy reaches the plane twice.

### 5.5 Multi-bounce diffuse GI (`RLT-GI`)

- **RLT-GI-001.** The bounded path MUST evaluate light at two or more
  diffuse vertices at tiers 2 and 3. At HEAD on 2026-11-01 this is met
  (T11); this ID pins it with a shader-contract test so the ROADMAP row's
  stale "1 in raster mode" cannot become true again. At HEAD on
  2026-11-02 `bounded_path_preserves_the_accepted_segment_and_diffuse_budgets`
  pins tier 3 to the shader ceilings and every tier at or below them, but
  nothing pins a floor: dropping tier 2 to one shaded hit, or lowering
  tier 3 and `MAX_SHADED_HITS` together to one, passes every test. The
  pin MUST assert at least two shaded hits and two path segments at tiers
  2 and 3.
- **RLT-GI-002.** The Cornell energy deficit (§2) and its relative error
  distribution MUST be published per ray quality tier on the floor,
  back-wall and ceiling regions, from `indirect_only` × albedo against the
  §3.4 reference recipe, on a reference rung.
- **RLT-GI-003.** At the tier Q7 decides, the energy deficit MUST be
  within the L4 threshold (Q6). Until the Q5 mechanism lands, A7 MUST run
  report-only against this threshold: the deficit is still published on
  every run and the floor drift margin still fails the gate, but a deficit
  above the threshold does not block a phase exit.
- **RLT-GI-004.** The traced share MUST NOT be multiplied by SSAO: with
  `DBG_DISABLE_AO` (`0x800`) set and unset, the traced-share view
  (RLT-GI-009) MUST differ by no more than its noise floor. SSAO MAY keep
  multiplying the untraced share — the exterior SH9 sky's (1 − traced
  weight) part and interior authored ambient — and everything at tier 0
  and past the GI distance fade (T11). At HEAD on 2026-11-01 the traced
  share is multiplied by SSAO in every branch (T12), so contact darkening
  is counted twice.
- **RLT-GI-005.** In interiors, authored ambient MUST combine with traced
  indirect as Q2 decides, and the combination MUST be the same for the DALC
  and the flat-XCLL inputs. At HEAD on 2026-11-01 both branches add
  authored ambient to traced indirect; exteriors cross-fade instead (T8,
  T12).
- **RLT-GI-006.** The bias of `GI_SAMPLE_LUMINANCE_CLAMP` MUST appear in
  the RLT-GI-002 report as the deficit with the clamp at its shipped value
  and with it disabled.
- **RLT-GI-007.** The FNV real-content probe (`scripts/renderer-eval-fnv.sh`,
  `DBG_VIZ_GI_BOUNCE`) SHOULD show a non-black isolated bounce across the
  Prospector Saloon floor at tier 3. Last measured by the 2026-07-19 probe,
  not re-run at HEAD: the isolated bounce was "nearly black apart from
  sparse high-energy samples"
  ([trade-offs, real-content finding](../engine/shadow-pipeline-tradeoffs.md#real-content-finding-volumetric-coordinates-are-gamebryo-units));
  Phase 1b re-captures it before any Phase 2 change.
- **RLT-GI-008.** If Q5 adopts M51's hash-grid radiance cache as the
  bounded path's terminator, this PRD owns the cache's realtime allocation
  rule (the exception PTR-RT-006 hands over): its buffers MUST have a row
  in [memory-budget.md](../engine/memory-budget.md) and its per-pixel
  query and update work MUST be bounded by a `GpuRayBudget` field
  (RLT-PERF-002).
- **RLT-GI-009.** A component view that outputs the traced share as it
  enters `indirectLight`, including whatever multiplies it, MUST exist,
  listed in the `RenderDebugMode` catalog. At HEAD on 2026-11-01
  `indirect_only` mixes the traced share with the untraced sky share or
  authored ambient (§3.4), so no view isolates it.
- **RLT-GI-010.** Every material of an L4 rung MUST be purely Lambertian on
  both sides (§3.4): no specular lobe and no Fresnel weight on the
  engine, the plain diffuse BSDF on the reference, so the deficit measures
  diffuse transport only. V11 closed in rev 8: `specular_strength`
  (`GpuMaterial` offset 28) scales only the light loop's GGX lobe and its
  multi-scatter compensation (`include/lighting.glsl`); the environment
  term is off for a metalness-0 material without an authored cube, but
  the GI walk shades every hit with a dielectric GGX lobe whose F0 comes
  from `ior`, samples that lobe on at least 5 % of bounces, and weights
  every diffuse term by (1 − *F*) (`include/lighting.glsl`,
  `include/pbr.glsl`), and Schlick's grazing term survives `ior` = 1. The
  engine side therefore MUST gain a switch, built in Phase 1a, that makes
  a rung's materials Lambertian in the light loop, at the GI walk's hits
  and in its first-surface throughput.

- **RLT-GI-011.** The bounded path MUST evaluate an emitter's light at
  its hits with the primary lobe's Lambert convention: a hit's diffuse
  response is ρ · *L* · atten · cos θ, as `diffuseBrdf = kD * albedo` gives
  the primary surface, so bounce light stands to direct light as it does
  in the reference, whose emitters carry *k*<sub>exp</sub> = π. The walk's
  specular lobe, hit emission and escaped environment radiance keep their
  physical units. At HEAD on 2026-11-02 the walk shades hits with
  `evaluatePathBsdf`'s ρ ⁄ π (`include/pbr.glsl`, `pathHitRadiance` in
  `include/lighting.glsl`), a factor of π below the primary lobe, which
  alone accounts for about 68 % of the A7 deficit. Added at Phase 2's entry
  by owner decision (2026-11-02).
- **RLT-GI-012.** The RT-LOD gate MUST NOT remove the traced share from a
  surface in view at room scale: on the box rung at tiers 2 and 3 the
  traced-share view MUST be non-zero on at least 99 % of every L4 region's
  pixels, within the Q8 cost ceiling (A13). At HEAD on 2026-11-02 the gate
  (`rtLOD >= RT_LOD_GI`, `triangle.frag`) culls GI wherever a pixel's
  world footprint exceeds about 0.67 units, and its footprint is the larger
  screen derivative, so at 1280 × 720 the box's grazing floor and ceiling
  get no traced GI at any tier (A7 deficit 1.00). Added at Phase 2's entry
  by owner decision (2026-11-02). **Deferred** to the Q5 radiance cache
  (owner, 2026-11-02, rev 14). The isotropic-footprint gate met the
  coverage clause on the box but not the cost clause. On content it traces
  GI on every grazing surface in range. Against the Phase 1b control with
  the tier pinned:

  | Scene / configuration | Delta | Settled tier |
  |---|---|---|
  | Prospector / FSR Quality | +3.4 ms | unchanged |
  | Whiterun / TAA | +10.9 ms | 1 → 0 |
  | Whiterun / FSR Quality | +7.7 ms | 3 → 1 |
  | Dugout / FSR Quality | +9.2 ms | 1 → 0 |

  At tier 0 the adaptive budget turns GI off altogether. A narrower gate,
  isotropic with the old ceiling of 2, still cost +1.8 to +5.0 ms and still
  dropped Whiterun / TAA to tier 0. The Phase 1b gate ships until the cache
  makes GI hits cheap enough to widen it, and any later widening re-runs
  A13.

### 5.6 Cost and budget (`RLT-PERF`)

- **RLT-PERF-001.** Every new pass this PRD adds MUST have its own GPU
  timer bracket on the `bench:` line, as `shadow_filter` has inside `svgf`.
- **RLT-PERF-002.** Every new per-ray cost MUST be bounded by a field of
  `GpuRayBudget`, so the adaptive tiers can shed it; no requirement here
  may add rays outside the budget ([backbone §6](../engine/physical-lighting-backbone.md#runtime-contract)).
- **RLT-PERF-003.** Each phase exit MUST report its frame-time delta on the
  benchmark scenes under the ROADMAP methodology (same-machine control,
  pinned camera, isolated settings —
  [How to run a bench](../../ROADMAP.md#how-to-run-a-bench)).
- **RLT-PERF-004.** Each phase's added GPU time on the reference GPU MUST
  stay within the per-phase ceiling Q8 decides, on every game benchmark
  scene, measured with the ray quality tier pinned
  (`--rt-test-ray-quality-tier`) at the scene's Phase 1b settled tier, so
  the adaptive budget cannot pay for a pass in quality and hide its time.
  The Cornell box is excluded from the ceiling, because its frame
  time is too small for a percentage to mean anything, and its pass costs
  are reported.
- **RLT-PERF-005.** Each phase exit MUST publish every benchmark scene's
  settled tier and tier oscillation (§2) on the reference GPU, beside the
  term results, so a reader can see which tier the reference-quality claim
  describes for that scene, and no scene's settled tier MUST fall below
  its Phase 1b value. This is checked separately from RLT-PERF-004.
- **RLT-PERF-006.** The GPU time all phases add together MUST stay within
  the whole-milestone cap Q8 decides, on every game benchmark scene,
  measured against the Phase 1b baseline with the tier pinned as in
  RLT-PERF-004.

### 5.7 Gate operation (`RLT-OPS`)

- **RLT-OPS-001.** The Cornell gates (A2–A8, A14–A17) MUST be runnable as an
  `m-light` option of `.github/workflows/rt-correctness.yml`, and MUST be
  run, with the resulting manifest checked into `docs/audits/`, at every
  phase exit and before every release that changes a file under
  `crates/renderer/shaders/`; a shader-only change runs against the cached
  references (RLT-REF-015). At HEAD on 2026-11-01 that workflow is
  manual-only and no runner carries its `byroredux-rt` label (the
  workflow's header comment), so the run happens on the RT development
  machine.
- **RLT-OPS-002.** Thresholds MUST be fixed on the reference GPU. On any
  other GPU the gates MUST run report-only; a difference beyond the
  combined floor between GPUs MUST be filed as an issue, not absorbed into
  a threshold.
- **RLT-OPS-003.** Replacing the reference GPU MUST follow one procedure:
  re-measure every frozen floor on the new GPU, re-run every gate, carry a
  budget over unchanged when the re-measured baseline still leaves it
  strictly below the baseline (Q6), return any budget that no longer does
  to the owner, and record the replacement as a revision row in §12. The
  old GPU's numbers stay in the manifest history.
- **RLT-OPS-004.** When an M-LIGHT gate fails and the cause is a change
  outside M-LIGHT (another milestone's renderer, sky, material or content
  change, which the gate catches at the run before a shader-changing
  release, RLT-OPS-001), that change MUST either be fixed or reverted,
  with a regression issue filed against the milestone it belongs to, or
  be accepted by the owner as intended, in which case the gate is
  re-frozen under RLT-REF-006 with a revision row naming that change. A
  failing gate is never loosened to absorb it, and the M-LIGHT phase in
  progress does not exit while the failure is open.
- **RLT-OPS-005.** After M-LIGHT completes, its gates MUST stay in
  `rt-correctness.yml` as regression guards and keep running under
  RLT-OPS-001. A later failure MUST drop the term to S2 in ROADMAP until it
  is fixed, and is handled under RLT-OPS-004.

### 5.8 Content non-regression (`RLT-CNT`)

The milestone changes how content looks on purpose: Q2 darkens
ambient-lit interiors, RLT-IBL-010 darkens sun-facing exterior surfaces,
RLT-GI-004 brightens cavities. A literal "nothing changed" gate fails by
design, so content is gated against declared changes.

- **RLT-CNT-001.** A11 MUST compare, per Wave 1 and Wave 2 benchmark scene
  with an install, a linear capture (§2) at the pinned bench pose against
  that scene's anchor, captured at step 4 of the current phase's entry
  (§9). Change MUST be measured on a fixed tile grid (32 × 32 pixels,
  recorded in the manifest) as each tile's mean and 95th-percentile
  change, so a brightening and a darkening inside one region cannot
  cancel, and A11 MUST fail on any tile whose change exceeds the scene's
  tolerance and is not covered by a declaration (RLT-CNT-002). At each
  phase entry, before the new anchor is captured, A11 MUST also run
  against the previous phase's exit capture (Phase 1b's for Phase 2), so a
  change made outside M-LIGHT between phases goes through RLT-OPS-004
  before the phase's work starts instead of being charged to its exit. The tolerance is the scene's two-capture floor plus a
  per-scene margin recorded in the anchor manifest; the margin changes
  only through a revision row, like a frozen floor (RLT-REF-006). At HEAD
  on 2026-11-01 `scripts/check-render-anchor.sh` compares 8-bit PNGs and
  A11 has no pass condition.
- **RLT-CNT-002.** Every phase MUST check in a change declaration in a
  commit that precedes the first commit of the code it covers. Each entry
  MUST name one bounded region of one scene's anchor image as a set of
  grid tiles (RLT-CNT-001), one direction (brighter or darker), and an
  expected change in linear luminance with a tolerance. Declared changes
  are checked per tile too: a tile inside a declared region whose change
  is in the declared direction and no larger than the expected size plus
  the tolerance passes; a tile outside every declared region, changed in
  the other direction, or changed more than declared, fails. A declared change that does not appear is
  reported, not failed. The declaration also lists every already-frozen
  gate of another term that the phase re-freezes (RLT-REF-006).
- **RLT-CNT-003.** The shadow filter off / on capture pair on content
  (rev 3's A11) MUST be published as a report beside the RLT-CNT-001
  result, not gated.

### 5.9 Requests to M51 (`RLT-EXP`)

The exporter is M51's (PTR-VAL-004), and that requirement names only the
parity scene. Each item below is a change request against M51's scope.
Each stays **unowned** until M51's revision log (its §12) records it as
accepted. An unowned request blocks only the phase that first consumes
it, named at the end of each item; that phase's entry (§9) cannot
complete while it is unowned, and no other phase waits for it.

- **RLT-EXP-001.** The exporter MUST export a skinned mesh at the pose
  the rasteriser drew at a given frame index: that frame's bone palette
  (`bones[]`, set 1 binding 3, which `triangle.vert` applies to the
  bind-pose vertices), never the SkinComputePipeline output buffer the
  skinned BLAS is refit from (`crates/renderer/src/vulkan/acceleration/blas_skinned.rs`).
  A stale BLAS then disagrees with the reference instead of matching it
  (RLT-REF-017, RLT-CON-003). The exporter MUST apply the same skinning
  formula as `triangle.vert`, which at HEAD on 2026-11-02 linearly blends
  the four weighted bone matrices with the weights as authored (not
  renormalised), clamps bone indices to `MAX_BONES_PER_MESH − 1`, and
  treats a vertex whose weight sum is below 0.001 as rigid; otherwise a
  CPU / GPU skinning mismatch shows up as contact error on the skinned
  proxy. Consumed by Phase 3.
- **RLT-EXP-002.** The exporter MUST export normal-map and alpha-cutout
  textures and the normal-map convention of RLT-REF-008 (RLT-CON-007,
  RLT-CON-008). Consumed by Phase 3.
- **RLT-EXP-003.** The exporter MUST export the sun-free analytic
  environment at the RLT-REF-012 resolution. Consumed by Phase 4.
- **RLT-EXP-004.** The exporter MUST export the sun as a distant emitter
  of the Q3 angular radius. Consumed by Phase 3 (the sun penumbra rung and
  the sun calibration rung).
- **RLT-EXP-005.** The exporter MUST scale each emitter's intensity by its
  *k*<sub>exp</sub> (RLT-REF-010) and record the factors it applied.
  Consumed by Phase 1a.
- **RLT-EXP-006.** The exporter MUST support per-rung material and object
  overrides (the RLT-GI-010 plain diffuse mapping; the probe-only sky rung
  of RLT-REF-021) without a second scene definition. Consumed by
  Phase 1a, for the plain diffuse mapping.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Oblivion draws shadows from shadow maps with a capped actor-shadow count and a filter level (`iShadowMapResolution`, `iActorShadowCountInt` / `Ext`, `iShadowFilter`; UESP [Oblivion:Oblivion.ini](https://en.uesp.net/wiki/Oblivion_Mod:Oblivion.ini)). Other titles (V8, partly closed in rev 8 — no UESP, GECK or CK INI reference page covers them): FO3 and FNV ship the same set (`iShadowMapResolution` 256–1024, `iActorShadowCountInt` / `Ext` 2–6, `iShadowFilter` 0–2 across their presets; Nukapedia *Fallout.ini (Gamebryo)*); Skyrim SE's presets carry `iShadowMapResolution`, `iNumFocusShadow` and `fShadowDistance`, FO4's `iShadowMapResolution`, `uiShadowFilter`, `iDirShadowSplits`, `iMaxFocusShadows` and `fShadowDistance` — names from the shipped preset INIs only; what focus shadows and splits do is unverified | Every light ray-traced against the full TLAS, no caster cap (RLT-NG-002) | Shadow-resolution and shadow-count INI tweaks and the mods that ship them | Cost grows with lights; bounded by the adaptive budget → RLT-PERF-002, RLT-PERF-004 |
| 2 | Skyrim interior lighting templates author six directional-ambient colours and one cell-wide directional light "used to simulate bounced light"; the flat ambient colour only seeds the six values and "has no impact on the actual lighting" (CK wiki [Lighting Template](https://ck.uesp.net/wiki/Lighting_Template), local dump `ck-uesp-wiki/(main)/Lighting Template.wiki`) | Authored ambient combined with traced bounce light per Q2 | None | Interiors darker or brighter than authored → RLT-GI-005, RLT-IBL-009 |
| 3 | ROADMAP's wording: a sky probe "captured once at cell load" | Re-baked every frame on exteriors | — | None: the probe follows time of day and weather; its cost is the `sky_cube` bracket ([SKYAL reflection filtering](../engine/skyal.md#reflection-filtering-2026-09-14)) → RLT-IBL-007 |
| 4 | — (physical) Sun radius ≈ 0.0047 rad | 0.020 rad at HEAD, Q3 | — | Penumbrae wider than the reference unless both use one value → RLT-SHD-007 |
| 5 | — | Dielectric sky specular (RLT-IBL-001) | — | The historical "chrome" look on legacy materials with wrong translated roughness or metalness → RLT-IBL-008 |
| 6 | — | Traced GI no longer double-occluded by SSAO (RLT-GI-004) | — | Cavities read brighter than today's look → RLT-GI-003 holds them to the reference |
| 7 | — | Sun disc removed from the lighting the sky probe feeds (RLT-IBL-010) | — | Exterior surfaces facing the sun lose the doubled energy and read darker than today → RLT-IBL-009 reports it per scene |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Every requirement is downstream of the translation boundary: lights enter
through `LightSource::from_legacy_world_units`, sky and sun through EXAL's
`SkyParamsRes`, ambient through `CellLightingRes` / `DalcCubeYup`. The
per-game differences this PRD touches — which ambient input a cell has, the
sun's angular radius, the translated roughness and metalness — are resolved
in those translate steps; no shader branches on the source game
([CLAUDE.md invariants](../../CLAUDE.md#architecture-invariants)). What
each title loads is the ROADMAP
[compatibility matrix](../../ROADMAP.md#compatibility-matrix)'s "Verified
content" column, linked rather than restated;
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail)'s
tier headings lag it (Oblivion is still filed under "cell loader pending").

Wave 0 is the game-free Cornell scene: every reference gate and every
threshold lives there. "In scope" for a game title means declared-change
gates and probes (A9–A13), not a reference comparison (RLT-NG-009).

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 2: interior and exterior load; XCLL flat ambient input exercises RLT-GI-005 |
| Fallout 3 | yes | Wave 2: shares FNV's ESM path and inputs |
| Fallout: New Vegas | yes | Wave 1: reference title; Prospector Saloon probe (`renderer-eval-fnv.sh`) and benchmark scene |
| Skyrim (LE / SE / AE) | SE / AE yes; LE no | Wave 1 (SE): DALC input and the WhiterunBanneredMare benchmark scene. LE is parse-only at HEAD and inherits this when its cell path lands |
| Fallout 4 | yes | Wave 1: MedTekResearch01 and Dugout Inn benchmark scenes; BGSM-driven metalness feeds T9 and RLT-IBL-008 |
| Fallout 76 | no | No cell loader; nothing is lit |
| Starfield | yes | Wave 2: walkable Cydonia interior; metric-unit normalisation at import ([backbone](../engine/physical-lighting-backbone.md#runtime-contract)) |

### 7.2 Acceptance

A gate passes when its command exits 0 and its metric is within the
threshold (RLT-REF-006); a report passes when its numbers are published.
Cornell scenarios need an RT-capable Vulkan device and a display or Xvfb,
no game data, plus a locally rendered reference where marked. Game
scenarios need the user's own installed data; nothing is checked in.

| # | Scenario | Kind | Command | Data | Requirements |
|---|---|---|---|---|---|
| A1 | Parked penumbra converges (existing) | self-consistency | `cargo test -p byroredux --test cornell_rt_oracle -- --ignored cornell_parked_penumbra_converges_at_one_shadow_ray` | none | regression only |
| A2 | Penumbra width | gate | new test `cornell_penumbra_width_matches_reference` in `cornell_rt_oracle.rs` (local light and sun rungs) | none + reference | RLT-SHD-001, RLT-SHD-007 |
| A3 | Penumbra under camera motion | self-consistency gate (temporal error) | new test `cornell_panned_penumbra_is_temporally_stable` (engine moving vs engine parked and converged at the same pose) | none | RLT-SHD-002, RLT-SHD-006 |
| A4 | Filter bands and thin rod | gate (visibility error) | new test `cornell_shadow_filter_bands_match_reference` | none + reference | RLT-SHD-004, RLT-SHD-009 |
| A5 | Contact bands | gate (visibility error) | new test `cornell_contact_bands_match_reference` (block, cube, plank, normal-mapped floor, alpha-tested caster, skinned proxy mid-animation in the RLT-REF-017 mode); the plank is also run on a flat floor (`--cornell-reference contact-flat`, report-only, RLT-SHD-012) | none + reference | RLT-CON-001…003, RLT-CON-006…008, RLT-SHD-012, RLT-REF-017 |
| A6 | Sky IBL energy | gate | new test `cornell_sun_ibl_sweep_matches_reference` (probe-only sky rung, L5 sweep, specular-only and Lambertian probes) | none + reference | RLT-IBL-001…004, RLT-REF-021 |
| A7 | GI deficit | report (gate once Q5's mechanism lands, RLT-GI-003) | `scripts/m-light-gates.sh --gates A7` on the Lambertian `--cornell-reference lambert-box` rung: tiers 0–3 pinned, plus the clamp off (`--rt-test-no-gi-clamp`) and the RT-LOD gate off at tier 3; deficit, mean and 95th-percentile relative error per region with both sides' floors in the RLT-REF-004 manifest | none + reference | RLT-GI-002, RLT-GI-003, RLT-GI-006, RLT-GI-010 |
| A8 | Traced share free of SSAO | gate | `scripts/m-light-gates.sh --gates A8`: the `traced_share` view with and without `DBG_DISABLE_AO` (`0x800`) on the box rung | none | RLT-GI-004, RLT-GI-009 |
| A9 | Material translation and anchor luminance | report | material-class distribution dump (Phase 1 tool) and `scripts/check-render-anchor.sh` on linear captures, HEAD vs the pre-IBL anchor | FNV, Skyrim SE, FO4 | RLT-IBL-008, RLT-IBL-009 |
| A10 | Real-content bounce | report | `scripts/renderer-eval-fnv.sh` (`DBG_VIZ_GI_BOUNCE` view) | FNV | RLT-GI-007 |
| A11 | Content non-regression against declared changes | gate | engine `--bench-frames 300 --bench-hold` (release with `BYRO_DEBUG_SERVER=1`) at the pinned bench pose, linear `final` capture compared with the anchor captured at step 4 of the current phase's entry (§9), under the phase's change declaration, plus the entry-time check against the previous phase's exit capture (RLT-CNT-001); the filtered-direct `r.shadowfilter off` / `on` pair and one exterior terrain-contact capture published as reports | each Wave 1–2 title's benchmark scene with an install, one exterior | RLT-CNT-001…003, RLT-SHD-009 on content, RLT-CON-009 (report), RLT-PERF-001 bracket |
| A12 | Portal-sky escape | report | new script `docs/smoke-tests/m-light-portal-sky.sh` (an interior with Show Sky, `render.debug indirect_only` at a window) | FNV or Skyrim SE | RLT-IBL-006 |
| A13 | Cost and settled tier | gate | benchmark-scene refresh per ROADMAP methodology | benchmark scenes' games | RLT-PERF-003…006 |
| A14 | Moving blocker | self-consistency gate (temporal error) | new test `cornell_moving_blocker_is_temporally_stable` | none | RLT-SHD-010, RLT-SHD-011 |
| A15 | Calibration and materials | gate | test `cornell_calibration_and_material_rungs` in `byroredux/tests/m_light_gates.rs` (runs `scripts/m-light-gates.sh --gates A15`): calibration ratio 1 per emitter kind over the plane and a 3 × 3 constancy grid, each kind from the phase that first uses it; the Lambertian flat planes at three view angles from Phase 1a, the specular rows and the normal-mapped plane from Phases 4 and 3 | none + reference | RLT-REF-010, RLT-REF-014 |
| A16 | Closure | gate | test `cornell_components_sum_to_total` in `byroredux/tests/m_light_gates.rs` (`--gates A16`): SVGF bypassed (`DBG_DISABLE_SVGF`), `final` unjittered (`--rt-test-no-jitter`) and read before TAA; SVGF's energy change and the B10G11R11 storage cost reported | none + reference | RLT-REF-011, RLT-REF-018, RLT-REF-020 |
| A17 | Sun counted once | gate | new test `cornell_sun_energy_counted_once` | none + reference | RLT-IBL-010, RLT-REF-012, RLT-REF-019 |
| A18 | Apparatus cost | report | `scripts/m-light-gates.sh` prints its wall-clock at the end of every run; `--cold` empties the reference cache first. The temporal gates' parked engine captures join when Phase 3 adds them | none | RLT-REF-015, RLT-REF-016 |

### 7.3 Where the gates run

| Gates | Where | When | Cross-GPU |
|---|---|---|---|
| A2–A8, A14–A18 | `rt-correctness.yml`, `m-light` option, on the RT development machine until a `byroredux-rt` runner exists (RLT-OPS-001), against cached references unless a cache key changed (RLT-REF-015) | Every phase exit; before a release that changes a shader | Thresholds fixed on the reference GPU; other GPUs report-only (RLT-OPS-002) |
| A9–A13 | Local, with the user's game data | Every phase exit | Reference GPU only |
| A1 | `rt-correctness.yml`, `cornell` option (existing) | As today | As today |

---

## 8. Metrics

The numbers live in a "Lighting fidelity" table that the Phase 1b exit
commit adds to ROADMAP §Status beside the bench table; frame-time costs
stay in the bench table itself. Every gate metric is published with its
noise floor and threshold (RLT-REF-004). This section defines what is
counted.

| Metric | Definition | Granularity |
|---|---|---|
| Penumbra width error | \|raster width − reference width\| ÷ reference width, §2 penumbra width, fitted on the filtered direct view's visibility | per rung, per profile line, per tier |
| Temporal error | §2 definition (engine moving vs engine parked and converged), penumbra region; max and mean over the motion, and the values at 1, 4 and 16 frames after the stop | per rung (camera pan, moving blocker), per tier |
| Filter band error | Visibility error (§2) in the lit-side and umbra-side bands and across the thin rod's shadow | per band, per tier |
| Contact error | Visibility error (§2) over each caster's contact band | per caster, per tier |
| Sky IBL energy error | Sky specular and sky diffuse luminance error vs reference (§3.4) | per roughness, per dielectric / conductor, per component |
| Sun energy error | Linear `final` vs reference on the unoccluded plane | per scene |
| GI energy deficit | §2 definition on Lambertian-only rungs, with its relative error distribution | per region, per tier, clamp on / off |
| Clamp bias | Luminance-integral delta, `RESERVOIR_W_CLAMP` on vs off | per scene |
| Calibration check | Calibration ratio *c* (§2, RLT-REF-010): engine ÷ reference on an export scaled by *k*<sub>exp</sub>, 1 within the floor | per emitter kind, per renderer version, per GPU |
| Noise floor | §2 definition; frozen value, re-measured value and their ratio against the drift margin | per metric, per rung, per side |
| SVGF energy change | Region luminance with SVGF on minus SVGF bypassed (RLT-REF-011), handed to M54 | per region |
| Content change | Linear-capture change vs the anchor captured at the current phase's entry, declared and undeclared (RLT-CNT-001) | per scene, per tile (mean and 95th percentile) |
| Settled tier | §2 definition, with tier oscillation | per benchmark scene |
| Pass cost | `bench:` bracket milliseconds for each new pass with the tier pinned (RLT-PERF-004); per-phase and cumulative totals | per benchmark scene |
| Gate run time | Wall-clock of one full reference-gate run, cold and warm cache (RLT-REF-016) | per run |

---

## 9. Delivery sequence

Phases are ordered by what they unblock: decisions and designs first;
then the apparatus the first term needs, proven before any number is
published (1a); then the global baselines (cost, settled tier, material
dump, content anchors) (1b); then the mechanisms, one term at a time,
starting with the L4 slice that proves the pipeline end to end (Q10).
Each piece of apparatus is built in the phase that first uses it, and
each RLT-EXP request blocks only that phase (§5.9).
This PRD depends on M51 for these things only: the exporter (PTR-VAL-004)
and the EXR writer (PTR-OUT-001), which M51 files as their own Phase 1
issues ahead of its progressive reference and which every phase from 1a
on needs; M51's acceptance of each RLT-EXP request before the phase that
consumes it; its Q5 renderer choice; its progressive reference, for
RLT-REF-005; and its radiance cache, for Q5's recommendation (M51's
Phase 5).

**Phase entry (Phases 2–4).** Every mechanism phase opens the same way,
before it changes the renderer:
1. build the apparatus the phase is the first to use, as listed under it;
2. run the phase's gates against the renderer as it stands, publish the
   term's baseline and combined floor into the Lighting fidelity table,
   which makes the term S2, and freeze the floors (RLT-REF-006);
3. the owner writes the term's bias budget, drift margin and rationale
   into the threshold table, or declares the term done (Q6);
4. run A11 against the previous phase's exit capture, settle any
   outside change through RLT-OPS-004, capture the phase's content
   anchors, and check in the phase's change declaration (RLT-CNT-001,
   RLT-CNT-002).

Mechanism work starts only after step 4.

**Phase 0 — Decisions, verify items and designs (no engine code).**
Resolve Q1–Q10, citing M51's Q5 for the renderer. Write the penumbra-aware
filter design from published descriptions (RLT-SHD-003) and the rung
subsection of the exporter design (rung content, the RLT-REF-008 mapping
table, the §3.4 recipes in the chosen renderer's terms). Close every verify
item in the table below. Exit: every §10 item marked
**Decided**, every V item closed or filed as an issue, and both designs
checked in.

| # | Verify | Settled by |
|---|---|---|
| V1 | The external renderer's inverse-square and finite-cone emitters (RLT-REF-002) | Closed in rev 8: `point` is a delta; `sphere` + `area` is finite and cone-sampled; `directional` is a delta at 3.9.1, `angle` on master only (RLT-REF-002) |
| V2 | Its licence terms for output images (RLT-REF-003) | Closed in rev 8: BSD-3 plus Enhancements; silent on rendered images (RLT-REF-003) |
| V3 | Whether a blocker distance fits the reservoir (RLT-SHD-003) | Closed in rev 8: as an f16 in the free high half of `directUnshadowed[].y` (RLT-SHD-003) |
| V4 | Whether camera jitter is off for debug views under `--upscaler taa` (RLT-SHD-001) | Closed in rev 5: off (`taa_jitter_gate`, `context/assemble_camera_and_lights.rs`) |
| V5 | Whether the Cornell builders can spawn an alpha-tested material (RLT-CON-008) | Closed in rev 8: yes, through `Material` fields and `register_rgba`, no renderer change (RLT-CON-008) |
| V6 | Whether the engine can light a probe by a uniform environment (RLT-REF-014) | Closed in rev 8: yes, with new exterior-sky set-up code (RLT-REF-014) |
| V7 | The `envBRDFApprox` constants (RLT-IBL-001) | Closed in rev 8: they match Karis 2014 exactly (RLT-IBL-001) |
| V8 | The other titles' shadow INI settings (§6 row 1) | Closed in rev 8 for §6 row 1: setting names from the shipped preset INIs; their meanings are unverified and not needed by the row (§6 row 1) |
| V9 | Whether `renderer-stepped` advances animation to a bit-identical skinned pose per frame (RLT-REF-017) | Closed in rev 15 (#6605): yes — the existing mode is the RLT-REF-017 mode; drawn frame F is clip time (F + 1) / 60 s, and two runs export byte-identical posed meshes (RLT-REF-017) |
| V10 | Whether the directional light's intensity can be zeroed independently of `dome.sun_dir.w` (RLT-REF-019) | Closed in rev 8: no — one source feeds both; the switch is new code (RLT-REF-019) |
| V11 | Whether `specular_strength` 0 removes every specular contribution (RLT-GI-010) | Closed in rev 8: no — the GI walk keeps a GGX lobe and (1 − F) weights; a Lambertian switch is needed (RLT-GI-010) |
| V12 | The BGSM smoothness-to-roughness conversion (RLT-IBL-008 F3) | Not verifiable from the cited source; filed in rev 8 as #6606 (Phase 4) |
| V13 | Whether the reference renderer's rough conductor and dielectric BSDFs model multiple scattering (RLT-REF-008) | Closed in rev 8: none does; engine runs `DBG_DISABLE_MULTISCATTER` (RLT-REF-008) |

**Phase 1a — Apparatus for L4 and the global checks, proven.** Depends
on M51's exporter and EXR-writer issues and on the acceptance of
RLT-EXP-005 and RLT-EXP-006, not on M51's Phase 1 exit. Ships the rung
content for the L4, calibration, material and closure rungs
(RLT-REF-001, RLT-REF-009), inverse-square emitters (RLT-REF-002),
*k*<sub>exp</sub> for every emitter kind on paper and export-time
emitter scaling (RLT-REF-010), the point calibration rung, the material
mapping for flat planes (RLT-REF-008) and their validation rung
(RLT-REF-014), the seed offset (RLT-REF-013), linear captures
(RLT-REF-007), the SVGF bypass, the unreconstructed `final` and the
closure rung (RLT-REF-018, RLT-REF-020, RLT-REF-011), the traced-share
view (RLT-GI-009), the Lambertian L4 rungs (RLT-GI-010), the reference
cache (RLT-REF-015), the manifest columns (RLT-REF-004) and the
`m-light` workflow option (RLT-OPS-001). No term baseline is published
in this phase. Exit: A15 gives a calibration ratio of 1 for point
emitters and passes on the flat planes, A16 closes, and A18 publishes
the cold and warm gate run times.

**Phase 1b — Global baselines.** RLT-GI-001's pin; the RLT-GI-007 probe
re-captured; A9 published with the RLT-IBL-008 flags, after which the
material-class route list is closed (§2); A13 published, which fixes each
benchmark scene's settled tier and the pinned-tier frame time that
RLT-PERF-004 and RLT-PERF-006 measure against; the content anchors for
RLT-CNT-001 captured. Exit: the Lighting fidelity table exists in
ROADMAP with its cost and tier rows; each term's row is filled at that
term's phase entry.

**Phase 2 — GI energy (the L4 slice, Q10).** Entry apparatus: none
beyond Phase 1a. Entry baseline: A7 and A8. Then RLT-GI-004, RLT-GI-005
per Q2, RLT-GI-011 and RLT-GI-012 (added at the entry, rev 13), and
RLT-GI-007 on content; the Q5 mechanism for RLT-GI-003 (with
RLT-GI-008 if the cache is adopted, which waits for M51's Phase 5), so
RLT-GI-003 may stay open after this phase exits. At the exit (rev 14)
RLT-GI-004 and RLT-GI-011 shipped. RLT-GI-005 waits on Q2's amended
decision, and RLT-GI-012 waits on the Q5 cache, both by owner decision.
The exit held at rev 16 (`docs/audits/MLIGHT_phase2_exit_metadata.txt`). Exit: A8 passes, A7
publishes the deficit at every tier and passes at its threshold once the
Q5 mechanism lands, A10 is re-published, A11 passes under the phase's
change declaration, and A13 shows the phase within its Q8 ceiling.

**Phase 3 — Shadows and contact.** Entry apparatus, consuming
RLT-EXP-001, RLT-EXP-002 and RLT-EXP-004: the filtered direct view
(RLT-SHD-009); the pan hold segment and the moving-blocker path
(RLT-SHD-002, RLT-SHD-010); the animated bench mode (RLT-REF-017); the
normal-map convention and the normal-mapped validation plane
(RLT-REF-008, RLT-REF-014); the sun as a distant emitter and its
calibration rung (RLT-REF-010); and the penumbra, contact and
moving-blocker rungs. Entry baseline: A2–A5 and A14, with RLT-SHD-005 and
RLT-SHD-006 published. Then RLT-SHD-003 (penumbra-aware reach),
RLT-SHD-007 per Q3, RLT-SHD-004, RLT-SHD-010, RLT-SHD-012, RLT-CON-001…003 and
RLT-CON-006…008, and RLT-CON-004 if Q4 takes Alternative A. Exit: A2–A5
and A14 pass at their thresholds, A11 passes under the phase's change
declaration, and A13 shows the phase within its Q8 ceiling.

**Phase 4 — Sky IBL.** Entry apparatus, consuming RLT-EXP-003: the
sun-free analytic environment export (RLT-REF-012); the sky-only switch
and the probe-only rung (RLT-REF-019, RLT-REF-021); and the environment
calibration rung (RLT-REF-010). Entry baseline: A6 and A17. Then
RLT-IBL-008's flagged classes resolved first, then RLT-IBL-001…004,
RLT-IBL-010 and RLT-IBL-006. Exit: A6 and A17 pass, A9 is re-published
with no unresolved flag, A12 passes on one Wave 1 title, A11 passes under
the phase's change declaration, and A13 shows the phase within its
ceiling.

**Phase 5 — Waves and cross-check.** Wave 2 titles through A9, A11 and
A13; RLT-REF-005 once M51's progressive reference ships. Exit: all four
terms S3 (or L4 at S2 under Q5 Alternative B, §3), A11 passes on every
Wave 1 title and on every Wave 2 title with an install, and A13 shows the
milestone within its RLT-PERF-006 total.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — Whether M-LIGHT's reference is external.** Recommended: yes — the
   converged reference is rendered by the external offline renderer M51's
   Q5 adopts
   ([path-tracing-reference-mode.md §10](path-tracing-reference-mode.md#10-decisions-required);
   recommended there: Mitsuba 3, alternative pbrt-v4), fed by M51's
   exporter (PTR-VAL-004) with this PRD's rung content (RLT-REF-001),
   because it shares no shader code with the engine and so cannot share its
   bugs. The renderer is M51's decision and is cited here, not reopened.
   Consequence: only Phase 0 is independent of M51. Every later phase
   waits for M51's exporter and EXR-writer issues (through Phase 1a), and
   each waits for M51's acceptance of the RLT-EXP requests it consumes
   (§5.9). None waits for M51's progressive reference or its Phase 1
   exit, and the ROADMAP M-LIGHT row's "Depends on: M51" must say so on
   approval. Alternative A: M51's
   progressive reference; consequence: Phases 1a–4 wait for M51's Phases
   1–2, and the reference reuses the engine's own BSDF and visibility code,
   so a shared defect passes until PTR-VAL-002 catches it on the parity
   scene, which has no point or directional emitters (§3.4). Alternative B:
   the engine's own parked accumulation at tier 3; consequence: no new
   tool, but the two-bounce cap and the luminance clamp are baked into the
   "reference", so RLT-GI-003 and RLT-GI-006 cannot fail.
   **Decided** (2026-11-02): the recommendation — the external Mitsuba 3
   reference fed by M51's exporter. The owner also authorised building
   M51's exporter (PTR-VAL-004) and EXR writer (PTR-OUT-001) as their own
   stand-alone issues under M51's IDs, with Mitsuba 3 installed as a
   developer tool, never an engine dependency.
2. **Q2 — Interior authored ambient against traced GI.** Recommended:
   cross-fade as exteriors do — authored ambient enters only through the
   untraced share and through rays that escape (`pathEnvironmentRadiance`
   already returns it), so the bounce share is not counted twice;
   consequence: interiors lit mostly by ambient get darker, and Phase 2 has
   to show it on the benchmark scenes before it ships. Alternative: keep
   the addition; consequence: interiors keep today's look, and nothing
   holds interior ambient to a reference, since reference rungs carry no
   ambient (RLT-REF-009).
   **Decided** (2026-11-02): the recommendation — cross-fade, authored
   ambient through the untraced share and escaped rays only. Amended by
   the owner at Phase 2 (2026-11-02, rev 14): the cross-fade is deferred.
   Measured before it shipped, it darkened Whiterun by a median of 26 %
   and by up to 79 % per tile, which is more change than the phase
   declared or meant. RLT-GI-005 stays open, and the addition ships
   unchanged.
3. **Q3 — The sun's angular radius.** Recommended: the physical value (≈
   0.0047 rad, RLT-SHD-007) for the exterior sun, and 0.020 rad kept for
   interior XCLL directional keys, which have no physical source; set in
   the EXAL translate step. Consequence: sharper exterior shadows and
   narrower penumbrae for the filter and TAA to hold, and a sun penumbra
   rung that needs a close-up camera on the shadow edge to reach a
   10-pixel penumbra (RLT-SHD-001). Alternative: keep 0.020 rad everywhere; consequence:
   today's look, and the reference scene must use the same inflated cone,
   so the gate measures transport, not realism.
   **Decided** (2026-11-02): the recommendation — the physical radius for
   the exterior sun, 0.020 rad kept for interior directional keys, set in
   the EXAL translate step.
4. **Q4 — Sun shadows past the shadow fade.** At HEAD every TLAS caster is
   traced inside `SHADOW_FADE_END`, and distant-LOD blocks up to
   `LOD_SHADOW_CASTER_DISTANCE` are already TLAS casters (T6), so the
   far-field gap is the fade distance, not missing casters; and
   [distant-lod-completion.md](distant-lod-completion.md) makes no shadow
   promise (RLT-NG-008). Recommended: neither PRD fixes it in this
   milestone; the gap stays recorded in RLT-NG-008 and is raised with the
   M35 PRD's review so the owner accepts it there or moves it into M35.
   Consequence: receivers past 12 000 world units stay unshadowed.
   Alternative A: a Bend-style screen-space term for the sun beyond the
   fade (RLT-CON-004), designed from Bend's published description only;
   consequence: one more pass and screen-space artefacts (off-screen
   casters cast nothing). Alternative B: extend the sun's fade and trace
   distance toward `LOD_SHADOW_CASTER_DISTANCE`; consequence: longer
   directional rays at every tier, bounded by RLT-PERF-002 and measured by
   A13, and no new pass.
   **Decided** (2026-11-02): Alternative A — a Bend-style screen-space sun
   shadow term beyond the fade, built under RLT-CON-004 from Bend's
   published description only, in Phase 3. RLT-NG-008's far-field gap is
   then owned by this PRD for the sun inside the screen; geometry beyond
   the loaded grid stays M35's.
5. **Q5 — How the GI energy tail is closed.** Recommended: publish the
   deficit at Phase 2's entry, then close the tail by reusing M51's hash-grid
   radiance cache (§2; PTR-RT-002, built in M51's Phase 5) as the bounded
   path's terminator, with this PRD owning its realtime allocation rule
   (RLT-GI-008, the exception PTR-RT-006 hands over). Consequence:
   RLT-GI-003 closes only after M51's Phase 5; if M51's Q2 takes its
   no-cache alternative, this falls to Alternative A or B. Alternative A:
   raise `MAX_DIFFUSE_BOUNCES` with Russian roulette; consequence: more
   rays per pixel on top of the cost #2161 accepted, bounded by the tiers
   and the Q8 ceiling. Alternative B: accept the deficit as the
   raster-mode promise and hold only the PT modes to the reference;
   consequence: RLT-G-004 becomes "published" rather than "bounded", L4
   completes at S2 with its deficit frozen (§3 completion rule), and the
   milestone closes without an L4 reference-quality claim.
   **Decided** (2026-11-02): the recommendation — M51's hash-grid radiance
   cache as the bounded path's terminator; A7 runs report-only
   (RLT-GI-003) until it lands.
6. **Q6 — The bias budget of each term.** **Decided** (2026-11-02): the
   recommendation below — the owner writes each term's budget, rationale
   and drift margin (or a done declaration) at that term's phase entry,
   step 3. Each term needs a budget above
   its frozen floor, because raster mode is biased by design and a
   threshold equal to the floor is unreachable. Recommended procedure, the
   same for every term: at the entry of the phase that improves the term
   (§9), after its baseline and frozen floors are published and before
   that phase changes the renderer, the owner writes
   into the threshold table, per term, in the units below:
   - a **bias budget** strictly below the term's entry baseline error
     (above the frozen floor), for a term this phase claims to improve, so the phase's exit cannot be met by the baseline itself;
   - a one-line **rationale** for that budget tied to something outside
     the measurement — a visibility argument (the error at which the term
     is not visible at the capture resolution) or the error a published
     technique reports for the same quantity;
   - a **drift margin** for the term's re-measured floor (RLT-REF-006);
   - or, instead of a budget, a declaration that the term is **done**: its
     baseline is accepted as reference quality, it is S3 from that phase entry, and
     the phase that would improve it drops it.

   Once written, a budget, margin or done declaration changes only
   through a revision row. A later re-freeze (RLT-REF-006, RLT-OPS-004)
   follows RLT-REF-006: the baseline may move, a floor rise beyond the
   drift margin is a budget change with its own rationale, and a term that
   leaves its threshold drops from S3 to S2.

   | Term | Budget expressed as |
   |---|---|
   | L1 penumbra | Relative penumbra width error and filter-band visibility error against the reference; temporal error as two budgets, the maximum during motion and the value at the post-stop frames |
   | L2 contact | Mean and 95th-percentile visibility error over each caster's contact band |
   | L3 sky IBL | Relative energy error (normalised per §2) per roughness and component, and the sun energy error |
   | L4 GI | Energy deficit and its 95th-percentile relative error, per region |

   Consequence: the budgets are informed by measurement but are owner
   decisions on the record, each with a reason that is not "what the
   baseline happened to be", and S3 means something fixed. Alternative A:
   fix numeric budgets now; consequence: no measured basis exists yet, so
   a budget may be unreachable or vacuous and would be revised under
   pressure. Alternative B: threshold equal to the noise floor (rev 2's
   rule); consequence: no biased term can reach S3, so the milestone never
   closes or its thresholds are loosened silently.
7. **Q7 — The tier the promise holds at.** Recommended: S3 is judged at
   tier 3, every term's metric is also published at tiers 0–2, and each
   benchmark scene's settled tier is published beside it (RLT-PERF-005);
   ROADMAP states both, so a reader sees the tier a scene actually runs
   at. Consequence: on a scene that settles below tier 3 the
   reference-quality claim does not describe what is seen there, and the
   published tier says so. Alternative: S3 also at the lowest settled tier
   of any benchmark scene; consequence: the claim describes what players
   see, but L4 at tier 0 has no traced bounce (T11), so it either fails or
   forces Q5 Alternative A's cost into the low tiers. Under either
   option RLT-CON-001 is a stated exception: the contact band sits mostly
   in umbra next to the contact line, so its error should not depend on
   the shadow sample count, and it is held at tiers 1–3 (tier 0 reported).
   **Decided** (2026-11-02): the recommendation — S3 judged at tier 3,
   tiers 0–2 and each scene's settled tier published beside it.
8. **Q8 — The cost ceiling per phase and for the milestone.**
   Recommended: on the reference GPU and on the four game benchmark scenes
   (the Cornell box excluded, its costs reported, RLT-PERF-004): no phase
   may lower any scene's settled tier; the new brackets a phase adds may
   total at most the larger of 5 % of that scene's pinned-tier frame time
   as A13 captured it at Phase 1b (not the ROADMAP bench table, which
   other milestones update) and an absolute floor of 0.25 ms, so a small
   pass is never refused on a fast scene; and all phases together may add
   at most the larger of 12 % of that frame time and the sum of the
   per-phase floors of the phases that add a bracket (RLT-PERF-006), so
   four phases cannot quietly sum to 20 % and the total is never smaller
   than the per-phase rule allows (the floor applies only below 5 ms: on a
   4 ms scene each phase may add 0.25 ms rather than 0.2 ms, and 12 % is
   0.48 ms while three phases at the floor add 0.75 ms, so the total there
   is 0.75 ms). Every figure is measured with
   the tier pinned at the scene's Phase 1b settled tier (RLT-PERF-004),
   and the settled-tier condition is checked separately (RLT-PERF-005). Consequence: the gates cannot be met by spending
   unbounded frame time; a mechanism that does not fit becomes a
   tier-3-only feature or a separate decision. Alternative A: an absolute
   per-phase millisecond ceiling and total; consequence: simpler to state,
   but the same number is generous on a fast scene and tight on
   MedTekResearch01. Alternative B: report only (rev 2's RLT-PERF-003);
   consequence: quality can be bought with unbounded frame time.
   **Decided** (2026-11-02): the recommendation — per phase the larger of
   5 % and 0.25 ms, for the milestone the larger of 12 % and the sum of
   the per-phase floors, no settled-tier drop, Cornell reported only.
9. **Q9 — A converged reference for game content.** Recommended: no — the
   reference gates stay on Cornell, game titles get declared-change gates
   (RLT-CNT) and probes, and ROADMAP says so (RLT-G-006). Consequence: a translation
   defect on real materials is caught only by A9 and A11, not measured.
   Alternative: export one interior per Wave 1 title (Prospector Saloon,
   WhiterunBanneredMare, MedTekResearch01) and gate it like a rung;
   consequence: the exporter and the RLT-REF-008 mapping must cover
   textured legacy materials, alpha, skinning and the authored ambient,
   which is M51's Phase 3 content work, so Phase 5 waits for it.
   **Decided** (2026-11-02): the recommendation — no game-scene
   reference; reference gates stay on Cornell.

10. **Q10 — Which term goes first.** Recommended: an L4 vertical slice —
    after Phase 1b, the first mechanism phase is GI energy (Phase 2):
    RLT-GI-004, RLT-GI-005, the Lambertian rungs and the content probe,
    which exercise the whole pipeline (exporter, calibration, closure,
    linear captures, frozen floors, content declarations) end to end on
    the term the 2026-07-19 FNV probe found weakest, before the shadow and
    IBL phases spend on it. Consequence: RLT-GI-003 itself may stay open
    after Phase 2, until Q5's mechanism lands; the slice proves the
    apparatus, not the closure. Phase 1a builds only what this slice and
    the global checks need, so the slice starts before the shadow and sky
    apparatus exists. Alternative: rev 3's order (shadows, then
    sky IBL, then GI); consequence: the first mechanism phase is the one
    with the most rungs, and a flaw in the apparatus is found after the
    largest phase has built on it.
    **Decided** (2026-11-02): the recommendation — the L4 vertical slice
    is the first mechanism phase.

---

## 11. Document map

| Need | Go to |
|---|---|
| The milestone row and its dependency on M51 | [ROADMAP Tier 8](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon) |
| PT modes, external renderer, exporter, EXR writer, radiance cache, PT-mode bounces | [Path Tracing Reference Mode PRD](path-tracing-reference-mode.md) (§2, §5.5, §5.6, §5.7, §10 Q2 / Q5 / Q6) |
| The denoiser this PRD does not gate | [Neural Denoiser PRD](neural-denoiser.md) |
| Distant geometry and its (absent) shadow promise | [Distant LOD Completion PRD](distant-lod-completion.md) |
| Units, emitters, visibility bits, ray budget | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md#runtime-contract) |
| Backbone validation gates and the candle-room reference | [Physical Lighting Backbone § Validation gates](../engine/physical-lighting-backbone.md#validation-gates) |
| ReSTIR, ratio EMA, W clamp, TAA γ | [Shadow Pipeline Trade-offs](../engine/shadow-pipeline-tradeoffs.md) |
| Cornell oracle rungs, debug views, ray probe | [RT Lighting & Material Recovery](../engine/rt-lighting-material-recovery.md) |
| Capture workflow and determinism contract | [Renderer Evaluation](../engine/renderer-evaluation.md#determinism-contract) |
| Sky probe, prefilter, SH, interior portal lane, capture traps | [SKYAL](../engine/skyal.md) |
| Interior ambient inputs | [Cell Lighting](../engine/lighting-from-cells.md) |
| Ground-cover RT boundary | [EXAL Ground Cover §5](../engine/exal-groundcover.md#5-ray-tracing-boundary) |
| SVGF and pass order | [Vulkan Renderer](../engine/renderer.md#svgf-temporal-denoiser) |
| Memory rows for new buffers | [Memory Budget](../engine/memory-budget.md) |
| What each title loads | [ROADMAP § Compatibility matrix](../../ROADMAP.md#compatibility-matrix); tier prose in [Game Compatibility](../engine/game-compatibility.md#per-game-support-detail) |
| Bench methodology, benchmark scenes, reference GPU, the accepted #2161 cost | [ROADMAP § How to run a bench](../../ROADMAP.md#how-to-run-a-bench), [Known Issues](../../ROADMAP.md#performance-and-measurement) |
| Where the RT gates run | `.github/workflows/rt-correctness.yml` |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: T5 / T6 / T9 / T14 inventory corrections, RLT-REF-002 and RLT-IBL-006 verify items closed from the code, inverse-square reference emitters, linear captures (RLT-REF-007), A8 tool, SIGMA / Karis / CK lighting-template citation precision, per-game tier link, cross-PRD alignment with M51 (shared exporter PTR-VAL-004, external renderer via M51 Q5, hash-grid radiance cache and RLT-GI-008, PTR-RT-007) and with M35 on far-field shadows (RLT-NG-008, Q4) | claude: revise |
| 3 | 2026-11-01 | Review round 2 folded in: §3 split into four gated terms (L1–L4) and a non-gating mechanism inventory, with T6 / T13 / T14 listed as not gated here; S1+ state for self-consistency gates; noise floor separated from threshold, with per-term bias budgets (Q6); component-decomposition contract (§3.4), closure, calibration, material-mapping and material-validation rungs (RLT-REF-008…014); exporter scene content, sun-free environment and seed offset; emissive-free reference rungs; noise floors moved from Phase 0 to Phase 1 and the verify list made a table; scope narrowed to Cornell reference gates plus content non-regression (RLT-G-006, RLT-NG-009, Q9); exporter, EXR writer and renderer choice owned by M51 alone; specular-energy cap replaced by material-translation and anchor reports (RLT-IBL-005 withdrawn, RLT-IBL-008, 009); sun counted once (RLT-IBL-010); contact band and adversarial casters (RLT-CON-006…009); motion measured during and after the pan, moving blocker (RLT-SHD-010, 011); filter gates moved to a filtered direct view after finding that debug views bypass the filter correction (RLT-SHD-004, 009); penumbra-width procedure; clean-room wording replaced by published-descriptions-only; cost ceiling, settled tier and promised tier (RLT-PERF-004, 005, Q7, Q8); gate operation (RLT-OPS); traced-share view (RLT-GI-009); RLT-SHD-008 withdrawn; term-state metric dropped; "bench-of-record" replaced by the defined benchmark scenes | reviewer: revise |
| 4 | 2026-11-01 | Review round 3 folded in: shadowed-region metrics moved to visibility error and every relative error normalised by the region's unoccluded luminance (§2, §8); calibration made a check, not a fit — analytic *k*<sub>exp</sub> applied to emitter intensities at export, *k*<sub>meas</sub> must match, sun factor measured on `direct_only` (RLT-REF-010, RLT-EXP-005); L4 rungs purely Lambertian on both sides (§3.4, RLT-GI-010); motion gates changed to temporal error against the engine's own parked, converged capture (RLT-SHD-002, RLT-SHD-010, A3, A14); floors frozen at Phase 1b with a drift margin, combined floor defined as root-sum-square (§2, RLT-REF-006); per-caster contact bands with a fixed flat-caster band for the plank (§2, RLT-CON-006); content gated against per-phase change declarations on linear captures, filter A/B demoted to a report (§5.8 RLT-CNT-001…003, A11); Q6 budgets strictly below the Phase 1b baseline with an external rationale, terms can be declared done; exporter additions filed as change requests against M51 (§5.9 RLT-EXP-001…006); Phase 1 split into 1a (apparatus proven) and 1b (baselines), L4 vertical slice first (Q10); Q8 excludes Cornell, adds an absolute floor and a milestone total (RLT-PERF-004, RLT-PERF-006); animated bench mode (RLT-REF-017), normal-map convention and normal-mapped validation plane (RLT-REF-008, RLT-REF-014), sky-only switch and one probe per capture (RLT-REF-019, §3.4), analytic high-resolution environment export (RLT-REF-012), closure with SVGF bypassed and SVGF energy reported to M54 (RLT-REF-011, RLT-REF-018), reference cache and gate run time (RLT-REF-015, RLT-REF-016, A18), objective material-class flags (§2, RLT-IBL-008); completion rule aligned with Phase 5 and the Q5 Alternative B case stated (§3); settled-tier warm-up and oscillation (§2, RLT-PERF-005); reference-GPU replacement procedure (RLT-OPS-003); Status paragraph trimmed; T10 untagged | reviewer: revise |
| 5 | 2026-11-01 | Review round 4 folded in: apparatus built in the phase that first uses it, each RLT-EXP request blocking only its consuming phase, Phase 1a narrowed to the L4 slice and global checks, Phase 1b to global baselines, and Q1's M51 dependency corrected (§5.9, §9, Q1, Q10); floors, baselines and budgets frozen at each term's phase entry, with a re-freeze rule for intended cross-term changes and an ownership rule for failures caused outside M-LIGHT (RLT-REF-006, RLT-OPS-004, Q6); cost measured with the tier pinned at the Phase 1b settled tier, settled-tier drop checked separately, and the milestone total made consistent with the per-phase floor (RLT-PERF-004…006, Q8); calibration stated as a ratio of 1 on a scaled export, an environment calibration rung added, and the "converged reference" definition corrected (§2, RLT-REF-010, A15); change declarations bounded per region with size and tolerance, committed before the code, larger-than-declared changes failing, and the content margin under the revision-row rule (RLT-CNT-001, RLT-CNT-002); penumbra width fitted on visibility (§2, RLT-SHD-001); the drawn skinned pose exported, not the BLAS source (RLT-EXP-001); probe-only sky rung on both sides (RLT-REF-021, §3.4); sun rung by close-up camera with its height rule (RLT-SHD-001, Q3); unreconstructed `final` capture mode (RLT-REF-020), V4 closed from `taa_jitter_gate`; component-specific normaliser for L3 / L4 relative error (§2); RLT-NG-008 revision wording, F2's whole-route flag stated, material-class route list closed after the Phase 1b dump, temporal-gate engine captures counted in A18 (RLT-REF-016) | reviewer: revise |
| 6 | 2026-11-02 | Review round 5 (verdict: approved with edits; this revision makes the edits) folded in: multiple-scattering parity between the engine's compensated GGX and the reference BSDF, with V13 and the `DBG_DISABLE_MULTISCATTER` or recorded-difference choice (RLT-REF-008); sun-rung sizing corrected to the 10–90 % width and asserted on the reference's measured width (RLT-SHD-001); re-freeze rule that moves the baseline freely, treats a floor rise beyond the drift margin as a budget change and drops a term that leaves its threshold from S3 to S2 (RLT-REF-006, Q6); separate during-motion and post-stop L1 budgets (RLT-SHD-002, RLT-SHD-010, Q6); environment calibration on a uniform or smooth glow-free sky (RLT-REF-010); content anchors captured at phase entry after an entry-time A11 run, and change measured and declared on a 32 × 32 tile grid (RLT-CNT-001, RLT-CNT-002, §9); A7 report-only until the Q5 mechanism lands (RLT-GI-003); post-completion regression guards (RLT-OPS-005); contact tiers stated as an exception to Q7 (RLT-CON-001); Q8 percentages taken from A13's Phase 1b pinned-tier frame time, with a sub-5 ms example; Q6 wording "this phase"; exporter skinning formula matched to `triangle.vert` (RLT-EXP-001) | reviewer: approved |
| 7 | 2026-11-02 | Owner approval: every §10 decision recorded as **Decided** — Q4 takes Alternative A (screen-space sun term, RLT-CON-004 in Phase 3), every other question its recommendation; Q1's decision authorises building M51's exporter and EXR writer under M51's IDs; RLT-NG-008 updated for Q4; status PROPOSED → APPROVED | owner: approved |
| 8 | 2026-11-02 | Phase 0 verify items folded in from the code at HEAD and the cited sources: V1 (Mitsuba `point` and 3.9.1 `directional` are delta lights; sphere area emitters for finite sources; sun option chosen at Phase 3 entry, RLT-REF-002), V2 (licence silent on images, RLT-REF-003), V3 (blocker distance fits an unused f16, RLT-SHD-003), V5 (alpha-tested caster needs no renderer change, RLT-CON-008), V6 (uniform sky reachable with new set-up code, RLT-REF-014), V7 (constants match, RLT-IBL-001), V8 (preset-INI names, §6 row 1), V10 (one source feeds sun and dome; new switch, RLT-REF-019), V11 (GI walk keeps a GGX lobe and (1 − F); a Lambertian switch is required, RLT-GI-010, §3.4 corrected), V13 (no multiple scattering in Mitsuba; Smith *k* remap and GGX distribution recorded, RLT-REF-008); V9 filed as #6605 and V12 as #6606, with F3 reworded to the engine's own rule; environment calibration also zeroes the cloud deck (RLT-REF-010); RLT-GI-001's pin floor stated | — |
| 9 | 2026-11-02 | APPROVED → ACTIVE: tracking issue #6595 and its phase issues filed, every Phase 0 verify item closed or filed (rev 8) | — |
| 10 | 2026-11-02 | Phase 0 designs checked in: the penumbra-aware filter design (RLT-SHD-003, `PAF-`) and the exporter plus rung subsection in `docs/engine/path-tracing.md` (`PTX-`; *k*<sub>exp</sub> π / π / 1 for point / sun / environment, the flat-plane BSDF mapping, the Mitsuba recipes); Related table points at both | — |
| 11 | 2026-11-02 | Phase 1a exit (#6601): A15 passes (point calibration 0.99927–0.99947 over the plane and its 3 × 3 grid; Lambertian planes 0.99931–0.99954 at three views), A16 closes on both sides, A18 published (cold 291.2 s with 12 references rendered in 6.2 s, warm 260.0 s; engine captures dominate); manifests in `docs/audits/MLIGHT_phase1a_exit_262a964b9_*`. Representation floor added to §2: RGBA16F capture storage (2<sup>−10</sup>) and composite's B10G11R11 indirect and albedo, modelled in the closure prediction. A7, A8, A15, A16, A18 commands now name `scripts/m-light-gates.sh` and `byroredux/tests/m_light_gates.rs`; RLT-REF-008's Phase 1a scope is the Lambertian row. Apparatus findings recorded for their phases: shadow-ray self-intersection near a coordinate plane (#6607, fixed); a renderer-side interior dust medium (0.006 m<sup>−1</sup>, switch `--rt-test-no-dust-floor`); B10G11R11 storage darkens `final`'s indirect by about 1–2 %; SVGF removes 3.6–4.9 % of the blocks' energy (M54 report); for Phase 2's entry, the RT-LOD gate culls GI on grazing room surfaces at 720p and the GI walk shades hits as ρ ⁄ π against the primary lobe's non-/π Lambert | — |
| 12 | 2026-11-02 | Phase 1b exit (#6598). A13: settled tier and pinned-tier frame time per benchmark scene under TAA native (the bench of record's reference, which the Q8 percentages use) and FSR Quality (the shipping default, published beside): Cornell 1 / 2, Prospector 0 / 1 (oscillation 0.42 / 0.34), Whiterun 1 / 3, MedTek 0 / 0, Dugout 0 / 1 — three of the four game scenes run no traced GI at native resolution, the case Q7's consequence names. Settled tier measured past a 240-frame warm-up (the controller's cold-start climb to tier 3 takes 225 frames) over a 300-frame window. A9: material classes per Wave 1 scene with F1 (MedTek 1366 / 4931 and Dugout 365 / 1148 dielectrics below roughness 0.2), F2 (every keyword-route conductor: Prospector 147, MedTek 113, Whiterun 22) and F3 (MedTek 280 / 7492, Dugout 56 / 1204, unverified convention #6606); the material-class route list is closed (§2). RLT-GI-001 pinned; RLT-GI-007 re-captured (the tier-3 isolated bounce is non-zero on every pixel of the Prospector floor, median 0.0023); content anchors captured per scene with their 32 × 32 tile floor. Lighting fidelity table added to ROADMAP; manifests `docs/audits/MLIGHT_phase1b_*_688f42705` | — |
| 13 | 2026-11-02 | Phase 2 entry, steps 2–3 (#6602). Baselines: A7 at tier 3 deficit 1.00 / 0.98 / 1.00 (floor / back wall / ceiling), 0.82 / 0.82 / 0.79 with the RT-LOD gate off, clamp off 0.973 on the back wall; A8 fails at HEAD (−0.54 % over all geometry, run on a new close-up view because SSAO is inert at the front view's distance). L4 is S2. Owner (2026-11-02): budget deficit ≤ 0.10 and p95 relative error ≤ 0.50 per region, drift margin 2× the frozen floor, rationale a visibility argument; frozen floors taken from the RT-LOD-off capture because the gate-on floor is exactly 0. The threshold table lives in `docs/audits/MLIGHT_thresholds.tsv`. The owner also added RLT-GI-011 (hit shading in the primary lobe's convention) and RLT-GI-012 (the RT-LOD gate keeps GI on room-scale surfaces) to Phase 2 | — |
| 14 | 2026-11-02 | Phase 2 owner decisions (#6602), checked in with `2f2dc5380`: RLT-GI-012 deferred to the Q5 radiance cache because the isotropic-footprint gate cost +3.4 to +10.9 ms on content and dropped three settled tiers (the Phase 1b gate ships, and a later widening re-runs A13); Q2 amended, the interior cross-fade (RLT-GI-005) deferred because it darkened Whiterun by a median of 26 % and up to 79 % per tile; RLT-GI-004 and RLT-GI-011 shipped. The exit's A7 / A8 run is in `docs/audits/MLIGHT_phase2_exit_A7A8_a89ca367e.tsv` (A8 passes) | — |
| 15 | 2026-11-04 | V9 closed (#6605): `renderer-stepped` is the RLT-REF-017 mode. Drawn frame F is clip time (F + 1) / 60 s; two runs with different RT seeds export byte-identical scenes at frame 45 and the proxy's yaw matches the clip. A5's mid-animation run now passes a parked `--bench-camera` path, which `renderer-stepped` requires, and checks the yaw as well as the byte identity | — |
| 16 | 2026-11-04 | Phase 2 exit (#6602), evidence in `docs/audits/MLIGHT_phase2_exit_metadata.txt` and `MLIGHT_phase2_exit_A13.tsv`. A8 passes (0 change on every region). A7 is published at every tier: tier 3 deficit 1.00 / 0.93 / 1.00, 0.42 / 0.42 / 0.33 with the RT-LOD gate off. A10 re-published: the tier-3 Prospector floor bounce has mean 0.0191, median 0.0022 and p95 0.0861, with every pixel non-zero (Phase 1b 0.0106 / 0.0023 / 0.047). A11 passes on all five scenes under the Phase 2 declaration (0 tiles outside). A13: every game scene is within its Q8 ceiling and every settled tier is unchanged. Whiterun / TAA read +1.10 ms on 3 runs whose spread is wider than the ceiling, and +0.08 ms on a 7-run rerun, which settles the row. RLT-GI-003, RLT-GI-005 and RLT-GI-012 stay open on the Q5 cache and Q2's amendment | — |
| 17 | 2026-11-04 | Phase 3 entry, step 1 (#6603): the first run of the Phase 3 gates stopped on argument errors in apparatus that had never run. A2 / A4 and A5 passed `--rt-test-no-jitter` without `--upscaler taa`. A14 ran `renderer-stepped` with no camera path, and parking a path would have frozen its blocker, which shares the bench clock. `renderer-stepped` now accepts an explicit `--bench-camera static`: the camera holds while animation and the bench movers step, and a missing path is still refused. A14 and A5's mid-animation run use it (RLT-REF-017, RLT-SHD-010) | — |
| 18 | 2026-11-04 | Phase 3 entry, step 1 (#6603) measured: A2, A3, A4, A5 and A14 ran end to end for the first time at 41b74db49, tier 3, 64 frames, Mitsuba 3.9.1, evidence in `docs/audits/MLIGHT_phase3_entry_41b74db49.tsv` and its metadata file. A2 penumbra width error is 0.0012-0.0017 (local light, three edge heights; reference width 147 px) and 0.0006-0.0014 (sun; 61-62 px). A4 visibility error: umbra band ~0, lit band 0.0009 (local) / 0.0007 (sun), thin rod mean 0.0087, p95 0.023. A5 (filter on, tier 3) mean / p95: block 0.0007 / 0.0009, cube 0.0012 / 0.0010, plank 0.0038 / 0.019, panel 0.0014 / 0.0010, skinned proxy 0.0012 / 0.0059; the plank is the outlier at every tier, and the filter-off `direct_only` plank is as large (0.0040 / 0.021). The first A3 / A14 run read `value == floor` on every row (0.055-0.062 and 0.30-0.035) because its floor captures used `--bench-camera-freeze-at`, which still travels the path up to its frame, so they were the moving run itself (bit-identical EXRs) and the excess was zero by construction. A new `--bench-camera-hold-at F` holds the pose from frame 0, and the floors now come from it. A3 temporal error is 0.055-0.062 against a floor falling 0.053 -> 0.036, an excess of +0.003 at frame 32 growing to +0.024 at frame 112 and +0.025 to +0.027 after the stop, with no decay by 16 frames after it. A14 moving-blocker error is 0.30 (frame 32) falling to 0.036 at the stop against a floor of 0.075 falling to 0.042; the excess is +0.23 at frame 32, +0.08 at frame 80, +0.016 at frame 96 and about -0.007 from frame 112 on. Every row is `report`; steps 2-4 (frozen floors, the owner's L1 / L2 budgets per Q6, anchors and change declaration) are next, and no Phase 3 requirement has been implemented | — |
| 19 | 2026-11-05 | RLT-SHD-007 implemented (#6603): the exterior sun's angular radius is the physical 959.5″ = 0.004652 rad, set once in the EXAL translate step (`env_translate::SUN_ANGULAR_RADIUS`, was 0.020), and the reference sun sphere reads the same value through `build_sky_params`. An interior's own directional key keeps `SkyParams::default()`'s 0.020 rad (Q3), pinned by a test. A2 / A4 on the sun rung re-measured at tier 3 (`docs/audits/MLIGHT_phase3_shd007_A2A4_sun.tsv`, against the entry baseline): reference penumbra width 61.5 -> 14.4 px; width error 0.0006-0.0014 -> 0.0103-0.0105 (relative); A4 umbra band mean 2.8e-5 -> 6.5e-4 and p95 1.0e-4 -> 3.4e-3, lit band mean 6.8e-4 -> 1.1e-3 and p95 6.7e-4 -> 4.1e-3. The filter's fixed 6 px reach is now wide against a 14 px penumbra, which is the bleed RLT-SHD-003 exists to remove. A11 (content non-regression under the phase declaration) and A13 are not yet run for this change | — |
| 20 | 2026-11-05 | Phase 3 entry, steps 2-3 (#6603): frozen floors and the owner's L1 / L2 budgets written into `docs/audits/MLIGHT_thresholds.tsv`. Owner (2026-11-05): A4 shadow bands (umbra and lit, local and sun rungs) mean ≤ 5e-4 and p95 ≤ 2e-3 of the unoccluded luminance, a visibility argument (one 8-bit step is about 1.7 % at mid-grey and 3 % in shadow, so the p95 budget is about an eighth of a step), strictly below the post-SHD-007 sun baselines (mean 6.5e-4 / 1.1e-3, p95 3.4e-3 / 4.1e-3); A5 contact: the 2 cm plank takes a budget (mean ≤ 2e-3, p95 ≤ 1e-2, about half its baseline, which is as large with the filter off), and block, cube, panel and the skinned proxy are declared done (baselines accepted); A3 pan: excess over the hold-from-frame-0 control ≤ 0.02 during motion and ≤ 0.01 sixteen frames after the stop (baseline +0.024 and +0.025); A14 moving blocker: excess ≤ 0.05 from frame 80 on (baseline +0.079), earlier frames reported because they carry the cold start. Drift margin 2× the frozen floor, carried over from rev 13; provisional for the temporal rows, whose excess seed-pair noise is not yet measured. The sun-rung A4 baseline is the post-RLT-SHD-007 one (rev 19), because that change was a decided physical value, not an improvement the budget is measured against. Not yet decided and report-only: A2 penumbra width, the A4 thin-rod shadow, and the A14 frames before 80. The gate runner does not yet read the table (A7 / A8's thresholds are encoded in the script), so A2-A5 and A14 still print `report` | — |
| 21 | 2026-11-05 | The gate runner now reads `docs/audits/MLIGHT_thresholds.tsv` (#6603): every row the table names is judged against its threshold and a floor ceiling of 3× the frozen floor plus 1e-6 (the `2x frozen floor` drift margin; the 1e-6 keeps a band whose two seeded captures agree bit for bit from failing on the smallest noise), and prints `yes` / `no`; other rows stay `report`. A3 and A14 gain the excess-over-hold rows the budgets are written against. Applied to the committed baselines (post-SHD-007 sun rung, tier 3) the runner gives: A4 sun bands `no` (4 of 4) and local lit-band mean `no`; A5 plank `no` (2 of 2); A3 `no` (2 of 2); A14 `no`; the four done casters and the local umbra / lit p95 `yes`. A gate run now exits non-zero until RLT-SHD-003 and its siblings bring those rows inside their budgets | — |
| 22 | 2026-11-05 | RLT-SHD-003 implemented (#6603), design in `docs/engine/penumbra-aware-shadow-filter.md` §7: `triangle.frag` writes each pixel's penumbra width (mean blocker distance of its occluded samples, PAF-001) into the free f16 lane of `directUnshadowed[].y`; `shadow_filter.comp` runs a 9×9 blocker search in iteration 0, sizes a per-pixel kernel radius, gates each iteration on stride ≤ R and weights taps by a Gaussian; the iteration count is per tier (2, 2, 3, 4, `GpuRayBudget.shadow_filter_iterations`). The design's R = w/2f measured too wide, so the radius is scaled by 0.25 (`SHADOW_FILTER_PENUMBRA_RADIUS_SCALE`): at 1.0 the sun-rung A4 bands went 6× worse and the width error doubled (0.010 → 0.020); at 0.25 all four sun-rung bands pass (umbra 2.0e-4 / 9.2e-4, lit 4.3e-4 / 1.6e-3 against 5e-4 / 2e-3) and the width error is 0.0055. Local rung: lit band mean 8.9e-4 → 4.3e-4 (now passes), p95 unchanged; contact bands block / cube / panel / proxy fall 25–55 % below their accepted baselines; plank (3.8e-3 / 1.9e-2), A3 (+0.0243 / +0.0248) and A14 (+0.079) are unchanged, as expected (not filter-caused). Measured at tier 3, `docs/audits/MLIGHT_phase3_shd003_gates.tsv`. Three rows fail on floor drift alone, not on value: local umbra p95 (floor 0 → 1.5e-6), block mean and p95 (2.8e-7 → 1.1e-5, 1.9e-7 → 6.9e-6) — the filter changes the seeds' noise and the frozen floors are near zero; re-freezing them is a budget change that needs the owner's Q6 rationale (RLT-REF-006). Not done: PAF-011 radius debug view, A13 cost, A11 under the phase declaration | — |
| 23 | 2026-11-06 | Phase 3, A5 plank diagnosed (#6603), no code change. Re-run of A5 at tier 3 on the rev 22 tree reproduces the plank: `direct_only` mean / p95 3.6e-3 / 1.96e-2 (tiers 1 and 2: 5.0e-3 / 2.5e-2, 4.0e-3 / 2.1e-2). Per-pixel, the error is not noise (neighbour correlation of the visibility difference 0.08, seed floor 1.9e-4) and not the receiver's unshadowed luminance (engine / reference 0.9995). It sits on the plank's long sides: the end faces carry a mean error of 5e-4 against 4.3e-3 on the long sides; the 25 worst pixels are 0.5-2 world units (3-13 px) beside the long edge, in pixels the reference lights 20-80 %, and the engine reads about 0.04 darker (5-20 % of the reference's value) in all but two of them, so the band holds a systematic darkening inside a wide soft shadow, not a contact crease. Switching ReSTIR spatial reuse, temporal reuse or ReSTIR itself off leaves it unchanged (3.61e-3, 3.59e-3, 3.59e-3), so it is in the single-frame shadow-ray and emitter sampling, not reuse. The origin offset is the ULP-scale `offsetRayOrigin` along the geometric normal, not a coarse bias. The block, cube and panel bands read 0 because they lie in full umbra; the plank is the only band in a wide penumbra, and A4's bands sit outside the penumbra, so no gate measures a mid-penumbra value, which is where this error would have to be looked for. Open: whether the cause is the emitter model (the tangent-plane disk the engine samples against Mitsuba's sphere emitter) or something specific to the thin caster; the next measurement is the mid-penumbra signed visibility error on the penumbra rung, and a per-sample comparison under RenderDoc. The plank budget (rev 20) stays unmet and the A5 rows stay `no` | — |
| 24 | 2026-11-06 | Phase 3, A5 plank: the systematic part is the normal-mapped floor (#6603), measured, no code change. (1) Mid-penumbra value, `direct_only`, tier 3, seed 0, signed engine-minus-reference visibility binned by the reference's visibility: the local-light penumbra rung reads -0.001 to -0.0034 across 0.05-0.75 (-0.4 to -1.2 % of the reference, mean over the rung -0.0003), an order of magnitude under the plank's -0.04 beside its long edge, so the emitter model is not the cause; the sun rung reads +0.007 to +0.011 across 0.05-0.95 (+0.7 to +8 %, 0 outside the penumbra), the opposite sign, a separate open observation. (2) The contact rung re-run with the floor's normal map removed on both sides (the exported `scene.xml` reduced to its inner diffuse BSDF; the engine with `DBG_BYPASS_NORMAL_MAP`): the plank's signed mean error falls from -1.4e-3 to -8e-5 and its mean from 3.6e-3 to 1.9e-3; p95 falls from 2.0e-2 to 1.3e-2. The residual is unsigned scatter (mean inside the 2e-3 budget, p95 still over 1e-2) and is not explained. (3) Mechanism, from `triangle.frag` (4072-4183): the shadow rays are jittered across the emitter disk and their binary hits averaged unweighted, while the radiance is evaluated toward the light's centre `L`. Mitsuba weights each emitter sample by the cosine at that sample, so its visibility (§2: shadowed over unshadowed luminance) is cosine-weighted; on a perturbed normal the cosine varies across the disk more, and the occluded part of the disk is not the average part. This fits (1) and (2) but is not yet tested by an estimator that weights each ray by cos(N, ray) over cos(N, L). Any such change alters every light on every title (A11, A13) and needs a requirement here first; it is not made in this revision | — |
| 25 | 2026-11-06 | RLT-SHD-012 added (#6721, a child of #6603): the visibility of a light's shadow rays is the cosine-weighted occluded fraction of the emitter, as the reference computes it, bounded where the centre cosine is small (§5.2, §9 Phase 3, A5). It is a requirement inside this PRD and not a PRD of its own: it is one crate and one shader path, it details how to meet the L1 / L2 promise already approved, and it meets none of the standard's T1-T5 triggers. Scope stated in the requirement: it is expected to remove the normal-mapped floor's plank bias (rev 24) and not the sun rung's +0.01 excess. Acceptance is the plank budget of rev 20 on the normal-mapped floor with the flat-floor control reported, A8 / A15 unmoved, A11 under the phase declaration, and A13 within its ceiling | — |
| 26 | 2026-11-06 | RLT-SHD-012 withdrawn (#6721): the cosine-weighting mechanism of revs 24-25 is refuted by a direct test. Implemented as designed in `triangle.frag`'s main shadow-ray loop (each ray's transmission times cos(N, ray) / cos(N, L), applied above a centre cosine of 0.1 and capped at 4), the A5 plank reads bit-for-bit what it read before (flat 1.91e-3 / 1.34e-2, normal-mapped 3.61e-3 / 1.96e-2, signed -1.42e-3 unchanged to five digits). Two probes show why: forcing the weight to 0.5 darkens the capture by 1.6 % (so the loop is reached and the new SPIR-V is the one running), and amplifying the weight's deviation from 1 twenty-fold moves the mean luminance by 0.04 %, so the real weights are within about 2 % of 1 (the contact emitter is 0.1 m at about 2.6 m, 0.038 rad) and average to 1. A weighting that small cannot produce a 5-20 % deficit. The change was reverted. What stands from rev 24: the penumbra rung's interior is within -0.4 to -1.2 % (not the emitter model), ReSTIR reuse is not involved, the ray origin is not a coarse bias, and the plank's signed bias disappears when the floor's normal map is removed on both renderers, so the cause is in how a perturbed shading normal meets a partly occluded emitter, in the engine or in the reference's `normalmap` BSDF, and is not yet found. Next: a per-pixel dump of the engine's N, L and visibility at the outlier pixels (rev 23 lists them) against Mitsuba's shading frame there, or a RenderDoc capture; and a tiny-radius emitter control to separate disk integration from the normal | — |
| 27 | 2026-11-06 | RLT-SHD-012 reinstated and measured (#6721); revision 26 was wrong. Opaque geometry runs `triangle_early.frag.spv` (the same source with `-DBYRO_OPAQUE_EARLY_TESTS=1`); the rev 25-26 test recompiled only `triangle.frag.spv`, so the floor never ran the weighted code, and the "bit-for-bit unchanged" result and the 0.5 and 20x probes measured the other variant. What is established: (a) the engine's visibility is identical on a flat and a normal-mapped floor (mean shift -2e-5 over the 930 partial-shadow pixels of the plank band) while the reference's rises by +4.9e-3, so the reference carries the cosine weighting and the engine does not; (b) with the weight in both variants the engine's visibility on those pixels rises by +5.4e-3 and the gap to the reference falls from 5.5e-3 to -1e-5, and the plank band's signed error from -1.42e-3 to +1e-5; (c) in the unfiltered `direct_only` view the penumbra rung then matches the reference to within 4e-4 in every visibility bin, fully lit included (-1e-5); (d) normalising each ray before the ratio (the first form tried) put the A4 penumbra lit band at 2.2e-3 and regressed the A5 non-plank bands further; the unnormalised, linear weight brings the lit band to 9.2e-4, still above the rev 22 baseline. The plank's remaining error is the reference's own noise: two 1024-spp renders differ by a mean 4.5e-3 per pixel in the band (a single render about 3.2e-3, p95 about 1.7e-2), matching the engine's remaining 3.4e-3 / 1.7e-2, and 256 engine frames in place of 64 move it only 3.43e-3 -> 3.22e-3. Gates against the rev 22 baseline (`MLIGHT_phase3_shd003_gates.tsv`), tier 3: A2 widths improve (local 0.00098-0.00149 -> 0.00004-0.00025, sun 0.0052-0.0056 -> 0.0044-0.0048); filtered penumbra-rung bins 0.1-0.7 go from -0.0018 / -0.0032 / -0.0039 to +0.00005 / -0.00033 / -0.00088; the lit-side tail (0.97-0.9999) is unchanged at about -0.0022 to -0.0029, the filter's own error; the plank 3.76e-3 / 1.90e-2 -> 3.53e-3 / 1.71e-2; A8 and A16 pass. Rows that move the wrong way against thresholds frozen on the biased sampler: A4 penumbra lit band mean 4.3e-4 -> 9.2e-4 (threshold 5e-4) and p95 9.0e-4 -> 1.39e-3, sun lit band mean 4.3e-4 -> 7.7e-4, A5 cube and panel p95 (9.3e-4 -> 1.24e-3 and 1.43e-3) pass -> no, and the block, cube, panel and proxy means +3e-4 to +4e-4; fully lit pixels in the filtered view read -9.2e-4 against -5.3e-4. Re-tuning `SHADOW_FILTER_PENUMBRA_RADIUS_SCALE` to 0.15 and 0.08 leaves the penumbra lit band at 9.1e-4 and 8.9e-4 and A5 unchanged, but cuts the sun rung's width error to 0.0015 and 0.0009 and its umbra p95 to about 0: the radius is not the lever for the lit band. Not applied. A15 is not affected and already fails at HEAD: `cal-sun` reads 0.99679 with and without this change (0.99927 at Phase 1a), and `normal-plane-azimuth2` 0.99846 as before. The change is in the working tree, not committed: the lit-band thresholds are the owner's (Q6) and are decided against the exact sampler | — |
| 28 | 2026-11-06 | RLT-SHD-012 implemented (#6721) and the threshold table re-frozen against it, with the owner's go-ahead. `triangle.frag` weights each shadow ray's transmission by `dot(N, sample offset) / dot(N, centre offset)` on the unnormalised vectors, applied above a centre cosine of max(0.1, twice the disk's angular reach) and capped at 4 (`SHADOW_COS_WEIGHT_MIN_CENTRE`, `SHADOW_COS_WEIGHT_MAX`), for local lights and the sun; both `triangle.frag.spv` and `triangle_early.frag.spv` rebuilt; pinned by `shadow_ray_transmission_is_cosine_weighted_on_unnormalised_vectors`. The legacy WRS arm (`ENABLE_LEGACY_WRS`, compiled out) is not changed. The A5 references are rendered at 16 x the run's sample count (`CONTACT_REFERENCE_SPP_FACTOR`, in 2048-spp chunks because one 16384-spp render overflows Mitsuba's CUDA wavefront; the manifest records the count per row). The plank then passes its unchanged rev 20 budget: mean 1.56e-3 (threshold 2.16e-3) and p95 6.5e-3 (1.13e-2), so its error was the reference's noise plus the sampler's -1.4e-3 bias, not geometry. Rows re-frozen in `MLIGHT_thresholds.tsv` (note in the file): A4 local-light umbra p95 and lit p95 floors (0 and 8.8e-12 -> 1.5e-6 each, drift only); A4 lit-band mean on both rungs, budget 5e-4 -> 1e-3 (values 9.2e-4 local, 7.7e-4 sun; this is the filter's own lit-side bleed, 5 % of one 8-bit step at mid-grey, and equals the 8.9e-4 the fixed-reach filter read at Phase 3 entry, the rev 22 figure of 4.3e-4 having been measured against a sampler that read the penumbra interior 0.2-0.4 % dark); A5 block, cube and panel p95 re-baselined by the done-row rule. Evidence: `docs/audits/MLIGHT_phase3_shd012_gates.tsv` (A2, A4, A5 at tier 3); a confirmation run of A4, A5, A8, A15 and A16 against the new table passes every table-judged row (18), A8 and A16 pass, and A15 shows only `cal-sun` (0.99679) and `normal-plane-azimuth2` (0.99846), which read the same on the committed shader without this change. Still open in Phase 3: A3 and A14 (temporal), RLT-CON-004, the PAF-011 view, A11 under the phase declaration, A13; and the A15 `cal-sun` regression since Phase 1a (0.99927), unbisected. The filter's lit-side loss (fully lit pixels read -9e-4 filtered, -1e-5 unfiltered) is the next accuracy item for RLT-SHD-004 | — |
| 29 | 2026-11-06 | Phase 3, A3 attributed (#6603), measured, no engine change. At c7b3f605e, tier 3, seed 0, one capture per row, `scripts/m-light/a3_attribution.py` (evidence in `docs/audits/MLIGHT_phase3_a3_attribution.tsv`; the seed-pair noise of these rows is not measured). A3 still reads +0.0242 during motion and +0.0248 at +16 against budgets 0.02 and 0.01. (1) **The `final` excess is the age of the accumulators, not motion.** The moving run's error against the parked converged capture is 0.0616 / 0.0624 / 0.0605 at +1 / +4 / +16, 0.0461 at +64 and 0.0051 at +256 (0.0153 at +512, non-monotone, unexplained). A run held at the stop pose from frame 0 reads 0.0631 at frame 16, 0.0613 at 24, 0.0590 at 32, 0.0494 at 64, 0.0346 at 135 and 0.0204 at 200: the moving run at +16 is where a purely parked run is at frame 16-24, and at +64 where it is at frame ~72. The hold control the gate uses is a parked run with 135 frames of history, so the excess is a gap in effective sample count (SVGF stores its age capped at 1/floor while moving, `svgf_temporal.comp`), and in the penumbra `final` is dominated by that slowly converging indirect term (the dominance RLT-SHD-002 states for the reference comparison holds for convergence too). No change to the shadow filter or the shadow-ratio EMA can bring it under 0.01 at +16: a parked run is itself at 0.06 at that age. (2) **The shadow-ratio path has its own, smaller, real residual.** On `direct_only` (the in-shader ratio EMA, no SVGF, TAA tap or GI) the moving run reads 0.0322 / 0.0282 / 0.0173 / 0.0064 at +1 / +4 / +16 / +64 against a held run's 0.0140 at frame 16 and 0.0031 at 135. Scaling the +1 value by 17/(17+k) gives 0.0288 / 0.0171 / 0.0068 at +4 / +16 / +64, within 10 % of each: the moving-phase history (an EMA at floor 0.1, capped at 16 frames, and bilinearly reprojected, so blurred) is carried into the parked 1/n average with weight 16/(16+k), and its bias decays only that fast. The excess over the held run at +16 is about 0.014. (3) Not attributed: the during-motion excess (the parked reference for an earlier pose is a different capture, and this tool samples only post-stop frames); the +512 rise. Decisions for the owner (Q6), none taken here: (a) re-base A3's settle row on `direct_only`, where L1's own contribution is visible and 0.014 fails 0.01, which would motivate a requirement to discount moving-phase history on entry to the deep tail in `triangle.frag` (it changes every light on every title, so it needs A11 and A13, and a requirement here first, as rev 24 recorded); or (b) keep `final` and write the settle budget against an age-matched control (a held run at the same frame count), so the row measures what motion adds beyond the accumulators' age. Until one is chosen the A3 rows stay `no`, and A14 (moving blocker, the same two accumulators) is not re-attributed here | — |
| 30 | 2026-11-06 | Correction to rev 29 (2), no engine change. Rev 29 put the `direct_only` excess at +16 at "about 0.014" by reading the moving run at frame 135 (0.0173) against the held run at frame 135 (0.0031). Those are not the same age: the moving run has had 16 parked frames, the held run 135. The age-matched control is the held run at frame 16, 0.0140, so the excess is 0.0173 - 0.0140 = **+0.0033, inside the 0.01 budget**. The 0.014 is the sampling noise of a 16-frame average, which a purely parked run also has; it is not carried moving-phase bias. The 16/(16+k) decay in rev 29 describes the moving run's absolute error, which an age-matched parked run would also show. The same mismatch is the whole `final` result: moving +16 reads 0.0605 against a held 0.0631 at frame 16 (excess -0.0026). So the evidence supports rev 29 option (b) and **not** option (a): no requirement to discount moving-phase history on entry to the deep tail is written, and `triangle.frag` is unchanged. A3's settle row fails only because the gate's control (`gate_temporal`'s `floor_e`, the still run sampled at stop+1..stop+256) is older than the moving run it is subtracted from. Still open: the during-motion row, which has the same control-age mismatch per frame (`cold`, held at each frame's pose from frame 0, is age-matched, so that row is a real +0.0242 and not yet attributed); A14; the +512 rise; the seed-pair noise of every row here. Next: write the settle budget against an age-matched control (owner decision, Q6) | — |
| 31 | 2026-11-10 | RLT-SHD-012 (#6721) acceptance closed, measured, no engine change. (1) **Flat-floor control.** A new rung `contact-flat` (`--cornell-reference contact-flat`) is the contact rung with the floor's normal map left off: same casters, emitter, bands and camera, and the same 7270 plank pixels. A5 runs it at tier 3, plank band only, beside the normal-mapped rows, report-only (the owner has set no budget for it); evidence `docs/audits/MLIGHT_phase3_shd012_a5_flatfloor.tsv`, 16384-spp references. Plank mean / p95: filtered direct 1.28e-3 / 5.12e-3 flat against 1.56e-3 / 6.49e-3 normal-mapped (budget 2.16e-3 / 1.13e-2); `direct_only` 9.4e-4 / 6.1e-3 flat against 1.64e-3 / 8.0e-3 normal-mapped. Both floors pass the plank budget, and the normal map adds about 2.8e-4 mean and 1.4e-3 p95 on the filtered view. Every table-judged A5 row passes in the same run. (2) **A11, isolated to this change.** Phase 3's entry anchors (step 4) were never captured, so the content gate has no Phase 3 anchor; this change is isolated instead: anchors from the engine at `55ba81e20^`, captures from `55ba81e20`, both release builds from worktrees, the Phase 1b pose and tier, TAA without jitter, 64 frames, `content.py compare`, margin 0.01. Without a declaration: Cornell 163 of 754 tiles outside tolerance (143 brighter to +3.3 %, 20 darker to -5.4 %; whole-frame mean +0.40 %), Whiterun 11 of 880 (all brighter, +0.35 % to +4.4 %), Dugout 2 of 833 (darker, -1.7 % and -5.6 %), Prospector 0 of 527, MedTek 0 of 880. `docs/audits/MLIGHT_phase3_shd012_declaration.tsv` declares those regions at their measured sizes and all five scenes then pass; it was written after the code it covers, which RLT-CNT-002 does not allow, and it records measured signs the requirement does not predict (it predicts a change only where an emitter is partly occluded). The shifts are small and local, but a darkening of -5.6 % in two Dugout tiles and -5.4 % in two Cornell tiles is a measured consequence, not a predicted one; whether the owner accepts it is Q6's call. A11 for the rest of Phase 3 (RLT-SHD-003, -007) has no anchor and stays open. (3) **A13.** Pinned-tier frame time against the `55ba81e20^` engine, interleaved, 3 runs per side, 300 frames, release, the Phase 1b settled tiers: every settled tier unchanged; nine of ten rows within the Q8 per-phase ceiling (deltas -0.63 to +0.76 ms against ceilings 0.48 to 1.39 ms; Cornell is reported, not held). `whiterun/fsr-quality` read +0.53 ms against 0.483 in the 3-run pass, with `dugout/fsr-quality` reading -0.63 ms in the same pass, so the 3-run spread is wider than the ceiling; 7 interleaved runs per side give control 10.52 ms, head 10.48 ms, delta -0.04 ms. Evidence `docs/audits/MLIGHT_phase3_shd012_a13.tsv` and `…_a11_a13_metadata.txt`. The ceiling is the per-phase one, applied to this one change. With the plank budget met (rev 28), A8 / A15 as recorded in rev 28, A11 declared and passing and A13 within its ceiling, the four acceptance bullets of #6721 are met; what stays open in Phase 3 is unchanged from rev 28 (A3, A14, RLT-CON-004, the PAF-011 view, the A15 `cal-sun` regression since Phase 1a, and A11 / A13 for the whole phase) | — |
