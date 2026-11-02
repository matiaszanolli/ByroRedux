# Reference-Quality Lighting — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 3, same day, folds in review
round 2: §3 rebuilt around the four gated terms with the mechanisms moved
to a non-gating inventory, the noise floor separated from the threshold
(per-term bias budgets, Q6), a component-decomposition contract (§3.4),
radiometric calibration, the exporter's missing scene content (materials,
environment, sun, emissive policy), the sun counted once, noise floors
moved out of Phase 0, the scope sentence narrowed to what is measured, the
exporter / EXR writer / external renderer handed to M51 alone, and the
shadow-filter gates corrected (at HEAD the debug views bypass the filter's
correction, so rev 2's on / off A/B compared identical images). The
M-LIGHT promise is written against the code at HEAD rather than the
ROADMAP row, which predates most of what has shipped. The decisions it
needs from the project owner are collected in §10; nothing below is built
as a result of this document yet. Statements about how an original engine,
a published technique or an external tool behaves cite external sources
(UESP and the Creation Kit wikis, the cited papers and vendor
documentation); a claim that could not be verified is marked **Phase 0:
verify** instead of being stated as fact.

**Scope**: M-LIGHT ([ROADMAP Tier 8](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon))
closes the gap between "lit correctly" and "lit as well as the data allows"
for the four lighting terms the row names: soft shadow penumbrae, contact
shadows, image-based lighting from the sky, and multi-bounce diffuse GI. It
promises two different things, and keeps them apart. On the self-contained
Cornell scene, each term is *measured* against a converged external
reference and held within an owner-decided bias budget by a runnable gate.
On game content, the same terms are held by non-regression gates and
content probes; no game scene is compared to a reference unless Q9 takes
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
| External reference renderer (M51 §10 Q5), the Cornell exporter (PTR-VAL-004), the EXR writer (PTR-OUT-001), the hash-grid radiance cache (PTR-RT-002) | [Path Tracing Reference Mode PRD](path-tracing-reference-mode.md) alone. This PRD extends the exporter's scene content for its rungs (RLT-REF-001) and adds a rung subsection to the exporter section of `docs/engine/path-tracing.md`, which M51's Phase 0 writes |
| Component-decomposition contract, calibration, rung content policy | This PRD, §3.4 and RLT-REF-008…014 |
| Penumbra-aware shadow filtering | none yet — Phase 0 writes the design (RLT-SHD-003) |

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
| **Converged reference** | An image of a rung rendered by the external renderer (M51 §10 Q5) to negligible noise, from the exporter's scene (RLT-REF-001) with the mapped BSDFs (RLT-REF-008), and scaled by the calibration factor (RLT-REF-010) before any absolute comparison. |
| **Raster mode** | The shipped hybrid renderer: rasterised primary visibility, ray-query shadows, reflections and GI ([renderer.md](../engine/renderer.md)). The mode this PRD measures. |
| **Progressive reference** / **interactive PT mode** | M51's two path-tracing modes, as [path-tracing-reference-mode.md §2](path-tracing-reference-mode.md#2-terminology) defines them. Neither exists at HEAD. "PT mode" in the ROADMAP row means the interactive one. |
| **Hash-grid radiance cache** | M51's world-space radiance cache, defined in [path-tracing-reference-mode.md §2](path-tracing-reference-mode.md#2-terminology) and required by PTR-RT-002; the ROADMAP rows' "SHARC" names this component whatever its provenance under M51's Q2. |
| **Rung** | A Cornell scene variant built for one measurement, in the sense of the L0–L5 oracle ([R4](../engine/rt-lighting-material-recovery.md#r4---build-the-cornell-l0-l5-oracle)). A **reference rung** is one that has a converged reference and obeys RLT-REF-009. |
| **Component view** | An engine debug view used as one side of a metric; §3.4 states what it contains and the reference recipe that produces the same quantity. |
| **Linear capture** | A raster-mode image of scene-linear radiance taken at composite's HDR output, before bloom, exposure, grading and tone mapping, averaged over a stated number of consecutive frames and written by the EXR writer (RLT-REF-007), as opposed to the 8-bit PNG of the presented swapchain that `screenshot` writes. |
| **Calibration factor** | The scalar *k* that maps the reference's radiance scale onto the engine's, measured on the calibration rung (RLT-REF-010) and applied before every absolute metric. |
| **Noise floor** | A metric's value between two captures of the same side that differ only in sampling seed (RLT-REF-006, RLT-REF-013). The lower bound of any threshold, never the threshold itself. |
| **Bias budget** | The error a term may carry above its noise floor, decided per term by the owner (Q6). Raster mode is biased by design (bounded path, luminance clamp, W clamp, SSAO, SVGF), so the budget is a product decision, not a measurement. |
| **Threshold** | A term's noise floor plus its bias budget. A gate passes when its metric is within the threshold. |
| **Self-consistency gate** | A gate that compares the engine with itself (two engine captures, an engine A/B, an isolated GPU unit test) rather than with the converged reference. It earns state S1+, never S2. |
| **Shadow ratio** | The per-pixel ratio Σrad·V ÷ Σrad the direct term is shaded by (Heitz et al. 2018), stored in the ReSTIR reservoir as `accum` ([trade-offs §3](../engine/shadow-pipeline-tradeoffs.md#3-restir-di-replaced-the-transient-m--8-estimator)). |
| **Shadow filter** | `shadow_filter.comp`: the edge-aware spatial filter over the shadow ratio. It writes a correction U·(filtered − raw) that `composite.frag` adds to the direct attachment. |
| **Profile line** | A world-space segment on a receiver, perpendicular to a shadow edge, fixed in the rung's code and recorded in the manifest. |
| **Penumbra width** | The world-space distance along a profile line between the 10 % and 90 % points of a smooth step fitted to the direct-term luminance profile (RLT-SHD-001). |
| **Contact band** | The receiver strip of fixed world-space width that borders the line where a caster touches the receiver. Its width is set once in the rung's code and is no more than one tenth of the shortest caster's height in that rung, so the band measures contact, not penumbra. |
| **Sky probe** | SKYAL's baked sky cubemap (`sky_cube.comp`), its GGX mip chain (`sky_prefilter.comp`) and its nine SH coefficients (`sky_irradiance.comp`). |
| **Bounded path** | The GI walk in `triangle.frag`: up to `MAX_PATH_SEGMENTS` segments, `MAX_SHADED_HITS` light-evaluated vertices, `MAX_DIFFUSE_BOUNCES` diffuse events. |
| **Energy deficit** | 1 − (indirect luminance in raster mode ÷ indirect luminance in the converged reference), over a named region. Positive means raster mode is darker; negative means it is brighter. |
| **Relative error distribution** | Per pixel, \|raster − reference\| ÷ reference over a region, reported as its mean and its 95th percentile, so a light leak and an underestimate cannot cancel in one ratio. |
| **Ray quality tier** | The adaptive budget tier 0–3 (`AdaptiveRayBudget`, [backbone §6](../engine/physical-lighting-backbone.md#runtime-contract)); pinned in tests with `--rt-test-ray-quality-tier`. The **settled tier** of a scene is the tier the budget holds after warm-up on the reference GPU. |
| **Reference GPU** | The RTX 4070 Ti that the ROADMAP bench table is measured on ([How to run a bench](../../ROADMAP.md#how-to-run-a-bench)). |
| **Benchmark scenes** | The scenes of the ROADMAP bench table: the Cornell box, FNV Prospector Saloon, Skyrim SE WhiterunBanneredMare, FO4 MedTekResearch01 and FO4 Dugout Inn ([How to run a bench](../../ROADMAP.md#how-to-run-a-bench)). |

---

## 3. Core model

Each of the four terms has one **state**, from a closed set:

| State | Meaning | Reported as |
|---|---|---|
| **S0** | Absent: the term is not computed | "absent" |
| **S1** | Shipped, unmeasured: computed, no gate | "shipped, unmeasured" |
| **S1+** | Self-consistent: computed, and only a self-consistency gate holds it | "self-gated: <gate>" |
| **S2** | Measured: a gate reports its distance from the converged reference on a §3.4 quantity, with its noise floor | "measured: <metric> = <value> (floor <value>)" |
| **S3** | Reference quality: the gate's metric is within the term's threshold (noise floor + Q6 budget) at the tier Q7 decides, on the reference GPU | "reference: <metric> within <threshold>" |

M-LIGHT is complete when all four terms of §3.1 are S3 on the Cornell
reference rungs, every non-regression gate in §7.2 passes on each Wave 1
title, and every phase stayed within its cost ceiling (RLT-PERF-004). The
mechanisms of §3.2 carry no state; rows that no gate here covers are listed
in §3.3 with their owner.

### 3.1 The four gated terms

The table below is the state at HEAD on 2026-11-01. Its Today column is
hand-written; at Phase 1 exit the generated Lighting fidelity table
(RLT-REF-004, §8) replaces it, and this table is not maintained by hand
after that.

| # | Term | Gate | State | Today |
|---|---|---|---|---|
| L1 | Soft penumbra | A2 (width), A3 (camera motion), A14 (moving blocker), A4 (filter bands) | S1+ | `cornell_parked_penumbra_converges_at_one_shadow_ray` (`byroredux/tests/cornell_rt_oracle.rs`, A1) compares a parked one-ray capture with an 8-ray engine capture in 8-bit luma: engine against engine, no reference |
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
| T10 | No interior probe; escaped GI rays and the untraced share read the authored DALC cube or XCLL ambient | L4 (report only, RLT-IBL-006) | `pathEnvironmentRadiance`, `include/lighting.glsl` (gated on `jitter.w`) |
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
| `indirect_only` | The single-frame, pre-SVGF `indirectLight`, **not** multiplied by the local albedo: exteriors (SH9 sky × (1 − traced weight) + traced) × AO; DALC cells (cube + traced) × AO; other cells ambient × max(AO, 0.3) + traced × AO | Authored ambient zero and no DALC (RLT-REF-009); averaged over the linear capture's frame count; multiplied by the exporter's albedo for the region's object (every L4 region is one flat-albedo object) | Reference total − reference direct, both at the same sample count |
| Traced share (new, RLT-GI-009) | The traced share as it enters `indirectLight`, including whatever multiplies it | As `indirect_only` | As `indirect_only` on interior rungs, where traced share and indirect coincide |
| Sky specular | `direct_only` on the sky-only rung: environment specular alone | `--cornell-sun` with the sun's intensity zero; probes with diffuse albedo zero | Environment-only render of the same probes with the specular part of the mapped BSDF (RLT-REF-008) |
| Sky diffuse | `indirect_only` × albedo on the sky-only rung | As sky specular, with Lambertian probes | Environment-only render with Lambertian probes |
| `final` | Direct + filter correction + SVGF'd indirect × albedo + composite terms | Closure rung (RLT-REF-011): fog out of range, no volumetric media, no caustic casters | Reference total |

---

## 4. Goals and non-goals

### 4.1 Goals

- **RLT-G-001 — Penumbrae match the reference, parked and moving.** Penumbra
  width, the penumbra error during and after a camera pan, and the error
  under a moving blocker are within the L1 threshold on the Cornell
  reference rungs.
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
  are held by non-regression gates and probes, and ROADMAP never describes
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
  no shadow promise, so far-field shadowing is owned by **neither** PRD at
  rev 3. Recorded here so the gap is visible, and decided in Q4.
- **RLT-NG-009.** A converged reference of any game scene, unless Q9 takes
  its alternative. Game content is gated for non-regression (RLT-G-006).

---

## 5. Requirements by area

### 5.1 Reference and measurement (`RLT-REF`)

- **RLT-REF-001.** The exporter M51 owns (PTR-VAL-004) MUST write every
  reference rung of this PRD with: geometry; per-object BSDF parameters
  (albedo, roughness, metalness) under the RLT-REF-008 mapping; the normal
  map and alpha-cutout texture of the RLT-CON-007 and RLT-CON-008 rungs;
  the rung's emitters under RLT-REF-002; the sun as a distant emitter of
  the Q3 angular radius; the environment under RLT-REF-012; the camera;
  and, for the skinned rung, the pose drawn in the captured frame. This PRD
  owns the rung content; M51 owns the tool. At HEAD on 2026-11-01 no
  exporter exists; the scene is authored only in `cornell.rs` and
  `cornell/builders.rs`.
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
  the manifest MUST record the largest *r*/*d* used. **Phase 0: verify**
  that the external renderer's point and sphere-area emitters are
  inverse-square with a finite radius, and which emitter reproduces a
  directional light of finite angular radius (settled by the emitter
  documentation of the renderer M51's Q5 adopts).
- **RLT-REF-003.** Reference images MUST NOT be checked in when the
  reference renderer's licence forbids it; the exporter and a pinned
  renderer version MUST regenerate them locally. **Phase 0: verify** the
  chosen renderer's licence terms for output images.
- **RLT-REF-004.** A metric manifest (`manifest.tsv` from
  `scripts/renderer-eval.sh`, extended) MUST record, per case, each §8
  metric with its noise floor and threshold, the git revision, the ray
  quality tier, the GPU and driver identity, the calibration factor, and
  the sampling seeds of both sides. At HEAD on 2026-11-01 the manifest
  records PNG hashes and the `bench:` summary only
  ([renderer-evaluation](../engine/renderer-evaluation.md#quick-start)).
- **RLT-REF-005.** Once M51 ships its progressive reference, its converged
  capture of each RLT reference rung SHOULD agree with the external
  reference within the two references' combined noise floor; a
  disagreement MUST be filed against M51 or the exporter, not absorbed into
  a threshold. M51's own parity check on its parity scene is PTR-VAL-002;
  this ID covers the RLT rungs only.
- **RLT-REF-006.** Every §8 metric MUST have a noise floor measured per
  rung on both sides — two engine captures with different sampling seeds
  (RLT-REF-013) and two differently-seeded reference renders — and every
  threshold MUST equal that floor plus the term's Q6 bias budget. A
  threshold MUST NOT be below its floor, and once Phase 2 starts MUST NOT
  be raised without a revision row in §12. At HEAD on 2026-11-01 the
  noise-floor practice exists for SKYAL captures
  ([SKYAL §4](../engine/skyal.md#4-harness)) and no lighting threshold
  exists.
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
  transform. M51's per-probe mapping of the Cornell materials (its V4) is
  the starting point; this ID covers the materials of this PRD's rungs.
- **RLT-REF-009.** A reference rung MUST NOT contain an emissive surface,
  a glass or eye material, authored ambient, a DALC cube, fog within the
  box, or a volumetric medium. At HEAD on 2026-11-01 the classic
  `--cornell` spawns `ceiling_light_panel` and `emissive_cube` and sets a
  flat ambient of 0.03 (`install_cornell_lighting`, `byroredux/src/cornell.rs`),
  and `--cornell-sun` keeps `emissive_cube`.
- **RLT-REF-010.** A calibration rung — one inverse-square point emitter
  above a white Lambertian plane, nothing else — MUST produce the
  calibration factor *k* as the ratio of engine to reference radiance over
  the plane's centre region, and MUST show that ratio constant across the
  plane within the noise floor, which also checks the inverse-square and
  cosine laws. A second calibration rung with the sun alone MUST produce
  the sun's factor. Each factor MUST be recorded in the manifest and
  applied before every absolute metric. At HEAD on 2026-11-01 the
  backbone defines emitters in scene-linear radiant intensity
  ([backbone §6](../engine/physical-lighting-backbone.md#runtime-contract))
  and nothing maps that scale to any external renderer.
- **RLT-REF-011.** A closure rung MUST show, on both sides, that the
  component views sum to the total: on the engine, the linear `final`
  capture MUST equal filtered direct + `indirect_only` × albedo within the
  noise floor; on the reference, total MUST equal direct + indirect within
  its noise floor.
- **RLT-REF-012.** The exporter MUST write, as the reference environment
  of every `--cornell-sun` rung, the engine's sky radiance for that frame
  **without the sun disc**, and the sun as a separate distant emitter
  (RLT-REF-001). The sun's glow stays in the environment on both sides: it
  is sky radiance (`include/sky.glsl` treats it as clear-sky scattering).
- **RLT-REF-013.** The engine's RT sampling MUST accept a seed offset, so
  two captures of one binary can differ only in sampling. At HEAD on
  2026-11-01 RT sampling is seeded by fragment coordinate and frame
  counter only (`include/math_common.glsl`), so two same-binary captures
  differ only by GPU nondeterminism and an engine-side noise floor cannot
  be measured.
- **RLT-REF-014.** A material-validation rung MUST place one flat plane of
  each mapped material (RLT-REF-008) under a single inverse-square
  emitter, and its direct-only radiance MUST match the reference within
  the noise floor at three or more view angles. A white-furnace variant
  SHOULD be added if the engine can light a probe by a uniform
  environment; **Phase 0: verify** whether it can (settled by reading the
  sky-probe inputs in `sky_cube.rs` and `frame_params.rs`).

### 5.2 Soft shadows and penumbrae (`RLT-SHD`)

- **RLT-SHD-001.** On a new Cornell penumbra rung (a blocker at a known
  height above the floor under one finite emitter), the penumbra width
  (§2) MUST be within the L1 threshold of the reference's, at tier 3.
  The measurement MUST: use the filtered direct view (RLT-SHD-009); read a
  fixed set of profile lines; average each profile across a strip of
  pixels along the edge; fit a smooth step whose form the rung's code
  names, by least squares; capture with no temporal reconstruction and no
  sub-pixel jitter on the engine side and a one-pixel box filter on the
  reference side; and place the blocker so the reference penumbra spans at
  least 10 pixels at capture resolution. The sun case MUST be a separate
  rung at the Q3 radius, under the same rule. At HEAD on 2026-11-01 every
  non-final debug view forces a native, unreconstructed blit
  (`render_debug_requires_raw_output`, `crates/renderer/src/shader_constants.rs`;
  `is_fsr_dispatch_active`, `context/draw.rs`), and the L2 rung asserts a
  hard edge only ([R4](../engine/rt-lighting-material-recovery.md#r4---build-the-cornell-l0-l5-oracle)).
  **Phase 0: verify** whether camera jitter is also off for these views
  under `--upscaler taa` (settled by reading the jitter gate beside
  `is_fsr_dispatch_active` in `context/draw.rs`).
- **RLT-SHD-002.** During a `--bench-mode renderer-stepped --bench-camera pan`
  run on the penumbra rung, the relative RMSE of the linear `final`
  capture against the reference at the same pose, over the penumbra
  region, MUST be within the L1 threshold at every sampled frame of the
  motion (a fixed stride, recorded in the manifest) and at 1, 4 and 16
  frames after the camera stops. `final` is used because the debug views
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
  **Phase 0: verify** whether a blocker distance fits the reservoir or
  needs its own screen-sized image (settled by the reservoir layout test
  `reservoir_glsl_copies_stay_in_lockstep` and a byte budget).
- **RLT-SHD-004.** With the shadow filter on, the filtered direct view
  MUST be within the L1 threshold of the reference in a band just outside
  each penumbra on the lit side and on the umbra side, and across the
  shadow of a thin rod a few pixels wide on the penumbra rung. These are
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
  measurement on it MUST be within the L1 threshold. At HEAD on 2026-11-01
  `--bench-camera` moves only the camera (`byroredux/src/boot/mod.rs`).
- **RLT-SHD-011.** A moving-light variant of RLT-SHD-010 SHOULD be added,
  since ReSTIR's temporal reuse is stressed hardest when the light moves.

### 5.3 Contact shadows (`RLT-CON`)

- **RLT-CON-001.** On a new Cornell contact rung (the tall block and a small
  cube resting on the floor under one emitter), the filtered direct view's
  luminance across each contact band MUST be within the L2 threshold of
  the reference, at tiers 1–3. At HEAD on 2026-11-01 no rung measures a
  contact band (T5).
- **RLT-CON-002.** The same rung MUST pass with the shadow filter on: the
  filter's normal and distance edge stops MUST NOT lift occlusion in the
  contact band above the threshold. This is measured on the filtered
  direct view, because `direct_only` excludes the filter (T3).
- **RLT-CON-003.** The rung MUST include a skinned actor proxy standing on
  the floor, captured mid-animation in a bench mode whose animation clock
  advances, and its contact band MUST be within the L2 threshold of a
  reference rendered at the exported pose. A parked proxy cannot show a
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
  floor whose thickness is no more than the contact band's width.
- **RLT-CON-007.** The contact rung MUST include a normal-mapped floor
  receiver, so a shading normal that disagrees with the geometric normal
  near the crease shows as a contact error. The Cornell builders already
  synthesize a normal map (`synthesize_wavy_normal_map`,
  `byroredux/src/cornell/builders.rs`).
- **RLT-CON-008.** The contact rung MUST include an alpha-tested caster.
  **Phase 0: verify** whether the Cornell builders can spawn an
  alpha-tested material (settled by reading `cornell/builders.rs`).
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
  function's comment; **Phase 0: verify** its constants against that post
  before it is used as the weight. At HEAD on 2026-11-01 an ordinary
  exterior dielectric reflects no sky (T9).
- **RLT-IBL-002.** Environment specular above `roughness 0.6` MUST come
  from the sky mip for that roughness instead of being dropped; the RT
  reflection ray MAY stay gated at 0.6. At HEAD on 2026-11-01 the whole
  term is skipped above 0.6 (T9).
- **RLT-IBL-003.** The `(1.0 - roughness)` energy heuristic MUST be
  replaced by the split-sum weight, and the `--cornell-sun` L5 roughness
  sweep MUST match the reference's sky specular (§3.4) within the L3
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
- **RLT-IBL-008.** Before Phase 3 ships RLT-IBL-001, the translated
  roughness and metalness distributions of each canonical material class
  MUST be published for each Wave 1 benchmark scene, and a class the owner
  marks as mistranslated in review MUST be fixed at the translation
  boundary, never by a shader-side cap. This is the guard against the
  "everything looks like chrome" regression the dielectric gate in
  `triangle.frag` records: its cause is legacy glossiness translated to
  too-low roughness, or wrong BGSM metalness.
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
  stale "1 in raster mode" cannot become true again.
- **RLT-GI-002.** The Cornell energy deficit (§2) and its relative error
  distribution MUST be published per ray quality tier on the floor,
  back-wall and ceiling regions, from `indirect_only` × albedo against the
  §3.4 reference recipe, on a reference rung.
- **RLT-GI-003.** At the tier Q7 decides, the energy deficit MUST be
  within the L4 threshold (Q6).
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
  Phase 1 re-captures it before any Phase 4 change.
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
  stay within the ceiling Q8 decides, on every benchmark scene.
- **RLT-PERF-005.** Each phase exit MUST publish every benchmark scene's
  settled tier on the reference GPU, beside the term results, so a
  reader can see which tier the reference-quality claim describes for
  that scene.

### 5.7 Gate operation (`RLT-OPS`)

- **RLT-OPS-001.** The Cornell gates (A2–A8, A14–A17) MUST be runnable as an
  `m-light` option of `.github/workflows/rt-correctness.yml`, and MUST be
  run, with the resulting manifest checked into `docs/audits/`, at every
  phase exit and before every release that changes a file under
  `crates/renderer/shaders/`. At HEAD on 2026-11-01 that workflow is
  manual-only and no runner carries its `byroredux-rt` label (the
  workflow's header comment), so the run happens on the RT development
  machine.
- **RLT-OPS-002.** Thresholds MUST be fixed on the reference GPU. On any
  other GPU the gates MUST run report-only; a difference beyond the noise
  floor between GPUs MUST be filed as an issue, not absorbed into a
  threshold.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Oblivion draws shadows from shadow maps with a capped actor-shadow count and a filter level (`iShadowMapResolution`, `iActorShadowCountInt` / `Ext`, `iShadowFilter`; UESP [Oblivion:Oblivion.ini](https://en.uesp.net/wiki/Oblivion_Mod:Oblivion.ini)). Other titles: **Phase 0: verify** from their INI references | Every light ray-traced against the full TLAS, no caster cap (RLT-NG-002) | Shadow-resolution and shadow-count INI tweaks and the mods that ship them | Cost grows with lights; bounded by the adaptive budget → RLT-PERF-002, RLT-PERF-004 |
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
threshold lives there. "In scope" for a game title means non-regression
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
| A3 | Penumbra under camera motion | gate | new test `cornell_panned_penumbra_tracks_reference` | none + reference | RLT-SHD-002, RLT-SHD-006 |
| A4 | Filter bands and thin rod | gate | new test `cornell_shadow_filter_bands_match_reference` | none + reference | RLT-SHD-004, RLT-SHD-009 |
| A5 | Contact bands | gate | new test `cornell_contact_bands_match_reference` (block, cube, plank, normal-mapped floor, alpha-tested caster, skinned proxy mid-animation) | none + reference | RLT-CON-001…003, RLT-CON-006…008 |
| A6 | Sky IBL energy | gate | new test `cornell_sun_ibl_sweep_matches_reference` (`--cornell-sun`, L5 sweep, specular-only and Lambertian probes) | none + reference | RLT-IBL-001…004 |
| A7 | GI deficit | gate | `scripts/renderer-eval.sh` with `BYROREDUX_RENDER_EVAL_FRAMES="64 1024"`, extended in Phase 1 to pin `--rt-test-ray-quality-tier` 0–3 and write the RLT-REF-004 columns | none + reference | RLT-GI-002, RLT-GI-003, RLT-GI-006 |
| A8 | Traced share free of SSAO | gate | `scripts/renderer-eval.sh` with a new `--cornell` case pair: the traced-share view with and without `DBG_DISABLE_AO` (`0x800`) in its `debug_flags` column | none | RLT-GI-004, RLT-GI-009 |
| A9 | Material translation and anchor luminance | report | material-class distribution dump (Phase 1 tool) and `scripts/check-render-anchor.sh` on linear captures, HEAD vs the pre-IBL anchor | FNV, Skyrim SE, FO4 | RLT-IBL-008, RLT-IBL-009 |
| A10 | Real-content bounce | report | `scripts/renderer-eval-fnv.sh` (`DBG_VIZ_GI_BOUNCE` view) | FNV | RLT-GI-007 |
| A11 | Content non-regression capture | gate | engine `--bench-frames 300 --bench-hold` (release with `BYRO_DEBUG_SERVER=1`), then `byro-dbg`: `render.debug` set to the filtered direct view, `r.shadowfilter off`, `screenshot a.png`, `r.shadowfilter on`, `screenshot b.png`, `render.debug final`, `stats`; plus one exterior terrain-contact capture | each Wave 1–2 title's benchmark scene, one exterior | RLT-SHD-009 on content, RLT-CON-009 (report), RLT-PERF-001 bracket |
| A12 | Portal-sky escape | report | new script `docs/smoke-tests/m-light-portal-sky.sh` (an interior with Show Sky, `render.debug indirect_only` at a window) | FNV or Skyrim SE | RLT-IBL-006 |
| A13 | Cost and settled tier | gate | benchmark-scene refresh per ROADMAP methodology | benchmark scenes' games | RLT-PERF-003…005 |
| A14 | Moving blocker | gate | new test `cornell_moving_blocker_tracks_reference` | none + reference | RLT-SHD-010, RLT-SHD-011 |
| A15 | Calibration and materials | gate | new test `cornell_calibration_and_material_rungs` | none + reference | RLT-REF-010, RLT-REF-014 |
| A16 | Closure | gate | new test `cornell_components_sum_to_total` | none + reference | RLT-REF-011 |
| A17 | Sun counted once | gate | new test `cornell_sun_energy_counted_once` | none + reference | RLT-IBL-010, RLT-REF-012 |

### 7.3 Where the gates run

| Gates | Where | When | Cross-GPU |
|---|---|---|---|
| A2–A8, A14–A17 | `rt-correctness.yml`, `m-light` option, on the RT development machine until a `byroredux-rt` runner exists (RLT-OPS-001) | Every phase exit; before a release that changes a shader | Thresholds fixed on the reference GPU; other GPUs report-only (RLT-OPS-002) |
| A9–A13 | Local, with the user's game data | Every phase exit | Reference GPU only |
| A1 | `rt-correctness.yml`, `cornell` option (existing) | As today | As today |

---

## 8. Metrics

The numbers live in a "Lighting fidelity" table that the Phase 1 exit
commit adds to ROADMAP §Status beside the bench table; frame-time costs
stay in the bench table itself. Every gate metric is published with its
noise floor and threshold (RLT-REF-004). This section defines what is
counted.

| Metric | Definition | Granularity |
|---|---|---|
| Penumbra width error | \|raster width − reference width\| ÷ reference width, §2 penumbra width, filtered direct view | per rung, per profile line, per tier |
| Motion error | Relative RMSE of the linear `final` capture vs the reference at the same pose, penumbra region; max and mean over the motion, and the values at 1, 4 and 16 frames after the stop | per rung (camera pan, moving blocker), per tier |
| Filter band error | Relative error distribution in the lit-side and umbra-side bands and across the thin rod's shadow | per band, per tier |
| Contact error | Relative error distribution over the contact band, filtered direct view | per caster class, per tier |
| Sky IBL energy error | Sky specular and sky diffuse luminance error vs reference (§3.4) | per roughness, per dielectric / conductor, per component |
| Sun energy error | Linear `final` vs reference on the unoccluded plane | per scene |
| GI energy deficit | §2 definition, with its relative error distribution | per region, per tier, clamp on / off |
| Clamp bias | Luminance-integral delta, `RESERVOIR_W_CLAMP` on vs off | per scene |
| Calibration factor | *k* for point emitters and for the sun (RLT-REF-010) | per renderer version, per GPU |
| Noise floor | §2 definition | per metric, per rung, per side |
| Settled tier | §2 definition | per benchmark scene |
| Pass cost | `bench:` bracket milliseconds for each new pass | per benchmark scene |

---

## 9. Delivery sequence

Phases are ordered by what they unblock: decisions and designs first; then
the measurement apparatus and the baseline, because every threshold
depends on measured noise floors; then the mechanisms, one term at a time.
This PRD depends on M51 for four things only: the exporter (PTR-VAL-004)
and the EXR writer (PTR-OUT-001), which M51 files as their own Phase 1
issues ahead of its progressive reference; its Q5 renderer choice; its
progressive reference, for RLT-REF-005; and its radiance cache, for Q5's
recommendation (M51's Phase 5).

**Phase 0 — Decisions, verify items and designs (no engine code).**
Resolve Q1–Q9, citing M51's Q5 for the renderer. Write the penumbra-aware
filter design from published descriptions (RLT-SHD-003) and the rung
subsection of the exporter design (rung content, the RLT-REF-008 mapping
table, the §3.4 recipes in the chosen renderer's terms). Close every verify
item in the table below. Exit: every §10 item marked
**Decided**, every V item closed or filed as an issue, and both designs
checked in.

| # | Verify | Settled by |
|---|---|---|
| V1 | The external renderer's inverse-square and finite-cone emitters (RLT-REF-002) | Its emitter documentation |
| V2 | Its licence terms for output images (RLT-REF-003) | Its licence file |
| V3 | Whether a blocker distance fits the reservoir (RLT-SHD-003) | `reservoir_glsl_copies_stay_in_lockstep` and a byte budget |
| V4 | Whether camera jitter is off for debug views under `--upscaler taa` (RLT-SHD-001) | The jitter gate in `context/draw.rs` |
| V5 | Whether the Cornell builders can spawn an alpha-tested material (RLT-CON-008) | `cornell/builders.rs` |
| V6 | Whether the engine can light a probe by a uniform environment (RLT-REF-014) | `sky_cube.rs`, `frame_params.rs` |
| V7 | The `envBRDFApprox` constants (RLT-IBL-001) | Karis 2014, "Physically Based Shading on Mobile" |
| V8 | The other titles' shadow INI settings (§6 row 1) | Their UESP / GECK INI references |

**Phase 1 — Measurement apparatus and baseline.** Depends on M51's
exporter and EXR-writer issues, not on M51's Phase 1 exit. The rung
content (RLT-REF-001, RLT-REF-009), emitters (RLT-REF-002), environment
export (RLT-REF-012), seed offset (RLT-REF-013), linear captures
(RLT-REF-007), calibration and material rungs (RLT-REF-010, RLT-REF-014),
closure rung (RLT-REF-011), the two new component views (RLT-SHD-009,
RLT-GI-009), the moving-blocker path and the pan hold segment
(RLT-SHD-002, RLT-SHD-010), the manifest columns (RLT-REF-004), the
`m-light` workflow option (RLT-OPS-001), and gates A2–A8 and A14–A17 run
against HEAD's renderer unchanged; RLT-GI-001's pin; RLT-SHD-005 and
RLT-SHD-006 published; the RLT-GI-007 probe re-captured; A9 and A13
published. Exit: A15 and A16 pass; the Lighting fidelity table in ROADMAP
holds, per term, the baseline value and its noise floor, so every term is
S2; and the owner has written the four bias budgets into the threshold
table under Q6's procedure.

**Phase 2 — Shadows and contact.** RLT-SHD-003 (penumbra-aware reach),
RLT-SHD-007 per Q3, RLT-SHD-004, RLT-SHD-010, RLT-CON-001…003 and
RLT-CON-006…008, RLT-CON-004 if Q4 takes Alternative A. Exit: A2–A5 and
A14 pass at their thresholds, and A13 shows the phase within its Q8
ceiling.

**Phase 3 — Sky IBL.** RLT-IBL-008 first, then RLT-IBL-001…004,
RLT-IBL-010 and RLT-IBL-006. Exit: A6 and A17 pass, A9 is re-published,
A12 passes on one Wave 1 title, and A13 shows the phase within its
ceiling.

**Phase 4 — GI energy.** RLT-GI-004, RLT-GI-005 per Q2, and the Q5
mechanism for RLT-GI-003 (with RLT-GI-008 if the cache is adopted, which
waits for M51's Phase 5); RLT-GI-007 on content. Exit: A7 and A8 pass, A10
is re-published, and A13 shows the phase within its ceiling.

**Phase 5 — Waves and cross-check.** Wave 2 titles through A9, A11 and
A13; RLT-REF-005 once M51's progressive reference ships. Exit: all four
terms S3, and A11 passes on every Wave 1 and Wave 2 title with an install.

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
   Consequence: Phases 0 and 2–4 do not wait for M51, Phase 1 waits only
   for M51's exporter and EXR-writer issues, and the ROADMAP M-LIGHT row's
   "Depends on: M51" must say so on approval. Alternative A: M51's
   progressive reference; consequence: Phases 1–4 wait for M51's Phases
   1–2, and the reference reuses the engine's own BSDF and visibility code,
   so a shared defect passes until PTR-VAL-002 catches it on the parity
   scene, which has no point or directional emitters (§3.4). Alternative B:
   the engine's own parked accumulation at tier 3; consequence: no new
   tool, but the two-bounce cap and the luminance clamp are baked into the
   "reference", so RLT-GI-003 and RLT-GI-006 cannot fail.
2. **Q2 — Interior authored ambient against traced GI.** Recommended:
   cross-fade as exteriors do — authored ambient enters only through the
   untraced share and through rays that escape (`pathEnvironmentRadiance`
   already returns it), so the bounce share is not counted twice;
   consequence: interiors lit mostly by ambient get darker, and Phase 4 has
   to show it on the benchmark scenes before it ships. Alternative: keep
   the addition; consequence: interiors keep today's look, and nothing
   holds interior ambient to a reference, since reference rungs carry no
   ambient (RLT-REF-009).
3. **Q3 — The sun's angular radius.** Recommended: the physical value (≈
   0.0047 rad, RLT-SHD-007) for the exterior sun, and 0.020 rad kept for
   interior XCLL directional keys, which have no physical source; set in
   the EXAL translate step. Consequence: sharper exterior shadows and
   narrower penumbrae for the filter and TAA to hold, and a sun penumbra
   rung whose geometry must be tall enough for a 10-pixel penumbra
   (RLT-SHD-001). Alternative: keep 0.020 rad everywhere; consequence:
   today's look, and the reference scene must use the same inflated cone,
   so the gate measures transport, not realism.
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
5. **Q5 — How the GI energy tail is closed.** Recommended: publish the
   deficit in Phase 1, then close the tail by reusing M51's hash-grid
   radiance cache (§2; PTR-RT-002, built in M51's Phase 5) as the bounded
   path's terminator, with this PRD owning its realtime allocation rule
   (RLT-GI-008, the exception PTR-RT-006 hands over). Consequence:
   RLT-GI-003 closes only after M51's Phase 5; if M51's Q2 takes its
   no-cache alternative, this falls to Alternative A or B. Alternative A:
   raise `MAX_DIFFUSE_BOUNCES` with Russian roulette; consequence: more
   rays per pixel on top of the cost #2161 accepted, bounded by the tiers
   and the Q8 ceiling. Alternative B: accept the deficit as the
   raster-mode promise and hold only the PT modes to the reference;
   consequence: RLT-G-004 becomes "published" rather than "bounded", and
   L4 cannot reach S3.
6. **Q6 — The bias budget of each term.** Each term needs a budget above
   its noise floor, because raster mode is biased by design and a
   threshold equal to the floor is unreachable. Recommended procedure, the
   same for every term: the owner writes each budget into the threshold
   table at Phase 1 exit, after the baseline and noise floors are
   published and before any Phase 2 change, stated in the units below;
   after Phase 2 starts a budget can only be raised by a revision row.

   | Term | Budget expressed as |
   |---|---|
   | L1 penumbra | Relative penumbra width error; motion error at the post-stop frames |
   | L2 contact | Mean and 95th-percentile relative error over the contact band |
   | L3 sky IBL | Relative energy error per roughness and component, and the sun energy error |
   | L4 GI | Energy deficit and its 95th-percentile relative error, per region |

   Consequence: the budgets are informed by measurement but are owner
   decisions on the record, and S3 means something fixed. Alternative A:
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
   forces Q5 Alternative A's cost into the low tiers.
8. **Q8 — The cost ceiling per phase.** Recommended: on the reference GPU,
   no phase may lower any benchmark scene's settled tier, and the new
   brackets a phase adds may total at most 5 % of that scene's TAA-native
   frame time in the ROADMAP bench table. Consequence: the gates cannot be
   met by spending unbounded frame time; a mechanism that does not fit
   becomes a tier-3-only feature or a separate decision. Alternative A: an
   absolute per-phase millisecond ceiling; consequence: simpler to state,
   but the same number is generous on Cornell and tight on
   MedTekResearch01. Alternative B: report only (rev 2's RLT-PERF-003);
   consequence: quality can be bought with unbounded frame time.
9. **Q9 — A converged reference for game content.** Recommended: no — the
   reference gates stay on Cornell, game titles get non-regression gates
   and probes, and ROADMAP says so (RLT-G-006). Consequence: a translation
   defect on real materials is caught only by A9 and A11, not measured.
   Alternative: export one interior per Wave 1 title (Prospector Saloon,
   WhiterunBanneredMare, MedTekResearch01) and gate it like a rung;
   consequence: the exporter and the RLT-REF-008 mapping must cover
   textured legacy materials, alpha, skinning and the authored ambient,
   which is M51's Phase 3 content work, so Phase 5 waits for it.

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
