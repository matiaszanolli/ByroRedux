# Reference-Quality Post-Process — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (GECK motion-blur citation, xEdit IMGS `Sky / Blur Radius` enum vs
the IMAD `DoF Flags` bit, the FO76-only NAM5 / NAM6 form-version gate, the
settings inventory, bloom-vs-FSR contract wording and the exposure-row
consequence, the Q8 / SKYAL scope, the Jimenez 2014 slides located, the
shutter and DOF-edge requirements made testable, the M51 path-tracing
interaction, the ROADMAP row's composite-layering wording). The PRD covers
the remainder of milestone M58 after the bloom pyramid, the exposure meter
and the ACES / AgX display-transform switch shipped. Statements about how an original engine,
format, paper or tool behaves cite external sources (the Creation Kit and
GECK wikis, UESP, xEdit's record definitions, the published talks and
papers); a claim that could not be verified is marked **Phase 0: verify**
instead of being stated as fact.

**Scope**: The post-process chain between composite's render-extent linear
HDR scene and the swapchain write, for every target title: the bloom filter
upgrade and its placement against the upscaler, depth of field (authored and
lens-driven), per-object motion blur from the existing motion vectors, 3D
LUT grading and how it composes with the authored IMGS / IMAD grade, a third
display transform, and the vignette / film-grain finish. It also covers the
parser-boundary decode of the authored image-space fields these effects
consume (depth of field, vignette, LUT path) and their translation into the
one canonical `ImageSpace` / `ImageSpaceModifier` pair. Outside it: the
shipped Stage 1 colour pipeline as such (it is inventoried, not re-promised),
the IMGS HDR block, renderer-injector mods, path-traced lens simulation
(M51) and any engine-invented per-cell colour mood.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Per-frame pass order, bloom, TAA, presentation pass | [Vulkan Renderer § Per-frame draw](../engine/renderer.md#per-frame-draw), [§ Bloom (M58)](../engine/renderer.md#bloom-m58) |
| Shader set and submission order | [Shader Pipeline § Per-Frame Submission Order](../engine/shader-pipeline.md#per-frame-submission-order) |
| Upscaler boundary, FSR input contract, post-upscale DOF decision | [FSR 3.1 integration plan](../engine/fsr3-upscaler-integration-plan.md) |
| Bloom sky-gain constraint; authored mood is the base | [SKYAL](../engine/skyal.md) |
| IMGS / IMAD → canonical image space (weather, worldspace, cell) | [EXAL § 3](../engine/exal.md#3-the-single-boundary-proposed) |
| Colour pipeline research plan (LUT bake tool, lens stack) | [Cinematic Rendering R&D Plan § Stage 1](../../RENDERING-PLAN.md#stage-1--color-pipeline-and-physical-camera) |
| Bloom VRAM footprint | [Memory budget § Bloom](../engine/memory-budget.md#bloom) |
| Path-traced modes that bypass or own the post chain (M51) | [Path Tracing Reference Mode — PRD](path-tracing-reference-mode.md) (PROPOSED) |
| DOF / motion-blur pass internals, LUT stage, lens finish | **none yet** — Phase 0 writes `docs/engine/post-process.md` |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `PPX-` prefix.

---

## 1. Product thesis

ByroRedux renders Bethesda content with ray-traced light; the last stretch
of the frame decides whether that reads as a game or as a photograph. The
ROADMAP frames M58 as "make existing content look as good as it can: beauty
only", building on the RT investment rather than a parallel pipeline
([ROADMAP Tier 8](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon)).
Per-pixel parity with Bethesda's look is explicitly not a goal
([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).

M58 is mostly shipped (§3). What is left is the part of a modern post chain
that the engine either approximates (bloom's box filter, the IMAD
full-screen motion-blur streak) or lacks (depth of field, velocity motion
blur, LUT grading, vignette, grain). Three of those effects are also
**authored data** the engine currently drops on the floor: Skyrim, Fallout 4
and Fallout 76 image spaces author depth of field, and Fallout 4 / 76 author
a vignette and a LUT path (§5.8).

The corollary that shapes every requirement: **every post effect has
exactly one parameter source per frame — authored, engine or user — and the
engine says which**. Authored values come only through the parser-boundary
decode and EXAL's translation into the canonical image-space types; the
renderer never branches on the game, never invents a look the content did
not author, and never runs an effect the content and the user both left off.
The engine will not map Bethesda's HDR tuning onto its physical exposure
and bloom, will not reproduce Bethesda's blur kernels, and, if the owner
takes §10 Q8's recommendation, will not re-colour a world by cell type.

The ROADMAP row describes M58 as a "single compute dispatch chain layered
onto the existing composite pass". The FSR 3.1 split came later and moved
presentation post-processing after the upscale
([FSR plan § 1.2](../engine/fsr3-upscaler-integration-plan.md#12-target-frame-graph)).
This PRD keeps the "compute dispatches, no new render pass" half of that
line (PPX-FG-007). It replaces "layered onto composite" with the FSR plan's
placement (PPX-FG-001), and the Phase 4 rewrite of the M58 row records the
change.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Presentation pass** | `presentation.rs` / `presentation.frag`: the output-extent fullscreen pass that applies IMAD lens effects, exposure, the IMGS / IMAD grade, chroma compress, the display transform, underwater extinction and fade, then writes the swapchain ([renderer.md](../engine/renderer.md#per-frame-draw) step 24). |
| **Upscale tap** | `record_upscale_pass`: the FSR 3.1 dispatch, or the native copy under `--upscaler taa`. Everything before it runs at the render extent, everything after it at the output extent (`FrameExtentSet::render` / `::output`). |
| **Pre-upscale / post-upscale** | Which side of the upscale tap a pass records on. |
| **Exposure texel** | `ExposureResource`: the per-frame-in-flight 1×1 exposure written by `exposure_meter.comp` and read by both the FSR dispatch and the presentation pass. |
| **Display transform** | `TonemapOp` (`crates/renderer/src/tonemap.rs`): the HDR→display curve. Output is linear display light; the `B8G8R8A8_SRGB` swapchain encodes. |
| **Image space (IMGS)** | The base grade of a cell, weather or worldspace; canonical form `ImageSpace` (`crates/core/src/imagespace.rs`). |
| **Image-space modifier (IMAD)** | A timed, keyed modifier composed onto the base; canonical per-frame form `ImageSpaceModifier`, sampled by `image_space_modifier_system` (`crates/scripting/src/cinematic.rs`). |
| **Authored grade** | Saturation, brightness, contrast and tint from IMGS + IMAD, applied by the presentation pass today (#5482). |
| **LUT** | A 3D colour lookup table sampled once per pixel; *authored* (an IMGS `TX00` path, §5.8) or *user* (a `.cube` file). |
| **CoC** | Circle of confusion: the per-pixel blur diameter depth of field derives from depth, focus and aperture. |
| **Lens DOF** | Depth of field driven by the existing `Camera::aperture` / `Camera::focus_dist` fields (`crates/core/src/ecs/components/camera.rs`). |
| **Authored DOF** | Depth of field driven by IMGS `DNAM` and IMAD `WNAM` / `XNAM` / `YNAM` (§5.8). |
| **Cinematic mode** | ROADMAP's name for the lens-DOF use case: a camera whose aperture is non-zero. No such mode exists at HEAD; §10 Q6 defines it. |
| **Raw-output debug view** | A render-debug mode for which `render_debug_requires_raw_output` is true; post passes must not touch it. |
| **Parameter source** | One of *authored*, *engine*, *user*, *off* — the closed vocabulary of §3.2. |

---

## 3. Core model

### 3.1 The post chain at HEAD

The inventory below is the state at HEAD on 2026-11-01, checked against the
code named in each row. The pass order is
`record_post_passes` in `crates/renderer/src/vulkan/context/post_passes.rs`:
composite → bloom → exposure meter → TAA (only under `--upscaler taa`) →
upscale tap → presentation. This table is hand-written; the Phase 1
commit that adds `post.status` (PPX-CTL-001) replaces it with that
command's output and deletes the column.

| # | Stage | Class | Today |
|---|---|---|---|
| 1 | Bloom pyramid | upgrade | Shipped, pre-upscale. 2×2 box downsample and 4-tap box upsample over `BLOOM_MIP_COUNT` levels (`bloom_downsample.comp`, `bloom_upsample.comp`), soft-knee bright pass on the seed level (`62a09fd9`), added back in place by `bloom_apply.comp` after composite (#2796). The header of `bloom_downsample.comp` records why the published Jimenez / Bjørge weights were not used. |
| 2 | Exposure meter | shipped | Frostbite EV100 geometric-mean meter of the post-bloom render-extent scene, per-FIF adaptation, default on since `a070baaad` (`exposure_meter.comp`; `--no-auto-exposure` in `byroredux/src/cli_args.rs`). |
| 3 | TAA / FSR | shipped (boundary) | FSR 3.1 Quality is the default `UpscalerMode`; the TAA pipeline is created only for `UpscalerMode::Taa` (`create_taa` in `context/init_stages.rs`). FSR's colour input is the post-bloom scene: `record_bloom_pass` writes bloom into `composite.scene_image` in place, and `record_upscale_pass` hands that same image to FSR (`post_passes.rs`). That departs from the plan's colour contract ("no bloom/tonemap/UI", [§ 1.4](../engine/fsr3-upscaler-integration-plan.md#14-fsr-input-contracts)), as the plan's status header records. The same section's Exposure row specifies a meter over the post-bloom scene. |
| 4 | IMAD lens effects | shipped | Blur radius, double vision, a horizontal three-tap "motion blur" streak and radial blur, in `sampleImageSpace` (`presentation.frag`), driven by `ImageSpaceModifier`. |
| 5 | Authored grade | shipped | Saturation → contrast (with shadow toe) → brightness → tint on exposed radiance (#5482), then the EV-dependent chroma compress (#5154), in `presentation.frag`. |
| 6 | Display transform | shipped (2 of 3) | `TonemapOp::{Aces, Agx}`; `--tonemap aces\|agx` (`cli_args.rs`) and the `tonemap` console command (`byroredux/src/commands/scene.rs`). Not a registered setting: `crates/core/src/settings/builtin.rs` registers `interface.*`, `gameplay.field_of_view`, `render.upscaler` and the `render.terrain_*` knobs, and no tonemap setting. The runtime selection is `ExposureTuning::agx`, a `bool` (`byroredux/src/components/tuning.rs`), so it cannot name a third operator. |
| 7 | Underwater extinction, fade | shipped | Post-tonemap mixes in `presentation.frag`. |
| 8 | Depth of field | new | Absent as a pass. `dof_effective_view_proj` (`context/frame_params.rs`) is a stochastic lens-jitter path that cannot produce bokeh through TAA (#4967), is forced to pinhole while FSR dispatches (`fsr_gated_dof`), and is dormant (nothing authors a non-zero aperture). |
| 9 | Velocity motion blur | new | Absent. Render-extent motion vectors exist (G-buffer, unjittered, skinned through the previous bone palette in `triangle.vert`). |
| 10 | 3D LUT grading | new | Absent: no colour-grading LUT is sampled anywhere in `crates/renderer/shaders/`. |
| 11 | Vignette, film grain | new | Absent from every shader. |
| 12 | Authored DOF / vignette / LUT decode | new | Not decoded: `parse_imad` (`crates/plugin/src/esm/records/misc/imagespace.rs`) reads no `WNAM` / `XNAM` / `YNAM` / `NAM5` / `NAM6`; `decode_image_space` (`crates/plugin/src/esm/records/misc/world.rs`) reads the grade only and skips `DNAM` on Skyrim+ and `TX00`. |

### 3.2 Parameter sources

Every post stage, every frame, has exactly one parameter source. It is
reported by `post.status` (PPX-CTL-001) and nowhere inferred.

| Source | Meaning | Reported as |
|---|---|---|
| **authored** | Values come from the frame's canonical `ImageSpace` / `ImageSpaceModifier`, which only the parser boundary and EXAL fill | `authored: <IMGS or IMAD editor ID>` |
| **engine** | An engine constant or a physical model (the bloom knee, the exposure meter) | `engine` |
| **user** | A registered setting or console override the user set | `user: <setting id>` |
| **off** | The stage records no dispatch this frame | `off` |

A user override wins over authored data for the stages the user can switch
(§5.9); authored data wins over engine defaults. A stage whose authored
input decodes to its identity value is `off`, not `authored`.

---

## 4. Goals and non-goals

### 4.1 Goals

- **PPX-G-001 — Published bloom filter.** Bloom uses the filter chosen in
  §10 Q1 with its published weights, without raising the SKYAL sky gain.
- **PPX-G-002 — Depth of field.** Authored and lens depth of field render
  through one post-upscale gather pass that is coherent with FSR.
- **PPX-G-003 — Per-object motion blur.** Moving objects and the camera
  blur along the existing motion vectors, off by default (§10 Q7).
- **PPX-G-004 — LUT grading.** Authored LUTs (Fallout 4 / 76) and user
  `.cube` LUTs grade the frame at the position §10 Q4 fixes, composed with
  the authored IMGS / IMAD grade.
- **PPX-G-005 — Three display transforms.** ACES, AgX and the transform
  §10 Q5 decides, selectable as a registered setting.
- **PPX-G-006 — Lens finish.** Authored and user vignette, and user film
  grain, applied after the display transform.
- **PPX-G-007 — Nothing silent.** Every stage reports its parameter source,
  and an authored field the engine does not consume is reported, not
  ignored.

### 4.2 Non-goals

- **PPX-NG-001.** Engine-invented per-cell colour moods ("interior warm,
  exterior cool, irradiated green" in the ROADMAP row), *proposed*,
  pending §10 Q8. The ROADMAP row promises them, so dropping them changes
  the milestone (README §1 T1) and is the owner's call. The argument for
  dropping them is the principle in
  [SKYAL § 2.4.1](../engine/skyal.md#241-the-games-temperature-and-mood-are-the-base-2026-10-20),
  which says the game's authored temperature and mood are the base and
  that SKYAL's own additions must not re-colour it. SKYAL decided that for
  its own terms (aerial perspective, clouds). It did not decide M58's
  grading row, and this PRD does not claim it did. Q8 asks whether to
  extend the rule to M58.
- **PPX-NG-002.** Mapping the IMGS HDR block (eye-adapt speed, bloom
  radius / threshold / scale, sunlight and sky scale, FO4 "Tonemap E" and
  "Middle Gray") onto the engine's exposure meter or bloom. The renderer's
  exposure and bloom are its own physical model and no mapping is sourced
  (`ImageSpace` doc, `crates/core/src/imagespace.rs`). These fields are
  reported as not consumed (PPX-IMS-006).
- **PPX-NG-003.** Reproducing Bethesda's blur kernels or the integer
  "Blur Radius 0–7" steps literally; per-pixel parity is anti-scope.
- **PPX-NG-004.** Renderer-injector mods (ENB, ReShade, Community Shaders).
  Their disposition is D3 in
  [LMC § 3.1](legacy-mod-compatibility.md#31-mechanism-catalog)
  row N5; user LUTs (§5.5) serve the most portable part of what they do.
- **PPX-NG-005.** Chromatic aberration, halation and FFT-convolution lens
  diffraction. They are RENDERING-PLAN Stage 1 research items, not in M58's
  promise; adding one is a T1 revision of this PRD.
- **PPX-NG-006.** Re-deciding the stochastic lens-jitter DOF. The FSR plan
  decided the post-upscale pass
  ([execution phase 6, item 4](../engine/fsr3-upscaler-integration-plan.md#execution-phase-6--image-quality-stabilization--complete-2026-07-24-with-one-item-deferred-for-want-of-hardware));
  this PRD implements that decision.
- **PPX-NG-007.** Starfield authored post data. Starfield's IMGS and IMAD
  carry their parameters in reflection (`REFL`) blobs, not typed
  sub-records: both are `wbEDID` + `wbReflection(Self, REFL)`, under the
  comment "subrecords checked against Starfield.esm"
  ([xEdit `wbDefinitionsSF1.pas`](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsSF1.pas)). Decoding them belongs to the
  Starfield CDB / reflection work, not M58.

---

## 5. Requirements by area

### 5.1 Frame-graph placement (`PPX-FG`)

- **PPX-FG-001.** Depth of field and velocity motion blur MUST record after
  the upscale tap and before the presentation pass, on the output-extent
  linear-HDR image, as the FSR plan's target frame graph places them
  ([§ 1.2](../engine/fsr3-upscaler-integration-plan.md#12-target-frame-graph)).
  Today neither pass exists.
- **PPX-FG-002.** Passes this PRD adds MUST NOT record between the
  exposure meter and the upscale tap, so FSR's colour input keeps the contract in
  [§ 1.4](../engine/fsr3-upscaler-integration-plan.md#14-fsr-input-contracts)
  (no tonemap, no UI). Checked by a source-order test over
  `record_post_passes`, in the style of the existing ones in
  `post_passes.rs`.
- **PPX-FG-003.** The LUT, vignette and grain stages MUST run inside the
  presentation pass or after it, never before the display transform unless
  §10 Q4 places the LUT scene-referred.
- **PPX-FG-004.** Bloom's placement MUST follow §10 Q2. Today it is
  pre-upscale, which the FSR plan's status header carries as known scope;
  if Q2 moves it, the same commit MUST restate the exposure meter's input
  in [renderer.md](../engine/renderer.md#per-frame-draw) step 21 and in the
  Exposure row of the FSR plan's
  [§ 1.4](../engine/fsr3-upscaler-integration-plan.md#14-fsr-input-contracts)
  (today: "the post-bloom composite scene"). It MUST also remove the bloom
  caveat from that plan's status header and § 1.2 item 6.
- **PPX-FG-005.** Every pass this PRD adds MUST return early under a
  raw-output debug view, with the same `render_debug_requires_raw_output`
  gate `record_taa_pass` and `record_bloom_pass` use.
- **PPX-FG-006.** A stage whose parameter source is `off` MUST record no
  dispatch, and its `TimerPass` row MUST report inactive.
- **PPX-FG-007.** Every pass this PRD adds MUST be a compute dispatch or
  part of the existing presentation pass. A new `vk::RenderPass` would be
  the "render-pass churn" the ROADMAP row rules out, so the check is that
  the count of render passes created in `context/init*.rs` is the same as
  at HEAD.
- **PPX-FG-008.** While an M51 path-traced mode is selected, the passes
  this PRD adds MUST follow that mode's contract in
  [Path Tracing Reference Mode](path-tracing-reference-mode.md). The
  progressive reference's written image MUST receive none of them
  (PTR-MODE-007). If the interactive mode resolves depth of field in its
  estimator (that PRD's Q4), the post DOF pass MUST be `off`, so the frame
  is never blurred twice. Today neither mode exists.

### 5.2 Bloom (`PPX-BLM`)

- **PPX-BLM-001.** The downsample and upsample filters MUST be the ones
  §10 Q1 selects, with tap offsets and weights lifted from the cited
  source, and each weight table MUST carry a pin test naming that source.
  Today both are box filters, and
  `bloom::tests::downsample_taps_match_the_documented_footprint` pins the
  box footprint.
- **PPX-BLM-002.** The upgraded pyramid MUST keep the soft-knee bright pass
  on the seed level only (`bright_pass_enable`) and its host mirror test
  `bloom::tests::bright_pass_mirrors_the_shader_expression` green.
- **PPX-BLM-003.** On the SKYAL capture
  ([§ 1.1](../engine/skyal.md#11-the-measured-defect)), the post-bloom ÷
  pre-bloom zenith ratio MUST stay within the tolerance Phase 0 fixes
  around the value SKYAL records after `62a09fd9`. It is measured with
  SKYAL's recipe: the pre-bloom value through `--render-debug-mode
  composite_term`, and the post-bloom value from the normal frame at the
  same camera. `composite_term` is a raw-output view and
  `record_bloom_pass` returns before dispatch under it
  (`post_passes.rs`), so that view cannot show post-bloom. If Q2 moves
  bloom post-upscale, Phase 3 restates the recipe for the output-extent
  image.
- **PPX-BLM-004.** The upgraded filter MUST NOT increase frame-to-frame
  luminance flicker of a sub-pixel bright feature under a slow camera pan,
  measured as the per-frame luma variance of a fixed bright-emitter region
  over the pan (method fixed in Phase 0). This is the pulsing defect the
  `bloom_downsample.comp` header names.
- **PPX-BLM-005.** `BLOOM_INTENSITY` MUST be re-derived for the new
  filter's DC gain, with the derivation in `shader_constants_data.rs`, not
  carried over; today's value is tuned for the box chain's gain (the
  `bloom_upsample.comp` DC-gain note).
- **PPX-BLM-006.** If Q1 selects Jimenez 2014, the seed level MUST keep
  the soft-knee bright pass (PPX-BLM-002), even though the source feeds
  bloom "non-thresholded color" (v18 slides, "Our bloom / veil core
  features"). The SKYAL sky-gain fix depends on the knee. The deviation is
  §6 row 6, and a code comment beside `bright_pass_enable` MUST name it.

### 5.3 Depth of field (`PPX-DOF`)

- **PPX-DOF-001.** Depth of field MUST be a scatter-as-gather pass with a
  tiled CoC pre-pass, following Jimenez, "Next Generation Post Processing in
  Call of Duty: Advanced Warfare", SIGGRAPH 2014 Advances in Real-Time
  Rendering ([iryoku.com](http://www.iryoku.com/next-generation-post-processing-in-call-of-duty-advanced-warfare);
  slides: [v18 .pptx](https://www.iryoku.com/downloads/Next-Generation-Post-Processing-in-Call-of-Duty-Advanced-Warfare-v18.pptx)).
- **PPX-DOF-002.** The CoC MUST be zero at the focus distance and MUST grow
  monotonically with |depth − focus| up to a clamp; pinned by a new host
  mirror test `post::dof::tests::coc_is_zero_at_focus_and_monotone`.
- **PPX-DOF-003.** Lens DOF MUST read `Camera::aperture` and
  `Camera::focus_dist`, not a second pair of fields. Today those fields feed
  only the dormant jitter path.
- **PPX-DOF-004.** While the DOF pass is active, `dof_effective_view_proj`
  MUST return the pinhole matrix in every upscaler mode, so the frame is
  never blurred twice; a new test
  `dof_jitter_is_pinhole_when_the_post_pass_owns_dof` pins it beside the
  existing `fsr_gated_dof` tests in `context/frame_params.rs`.
- **PPX-DOF-005.** Authored DOF MUST be read from the frame's
  `ImageSpaceModifier` (§5.8), and its translation into focus distance,
  transition range and maximum CoC MUST live in one function with a unit
  test per field. **Phase 0: verify** what Strength, Distance and Range mean
  quantitatively. The CK wiki says Distance is "the distance in units that
  contains a clean, non-blurred view", Range "the range in units where the
  depth blur begins", and that a Distance above Range blurs the foreground
  ([CK: ImageSpace Modifiers](https://ck.uesp.net/wiki/ImageSpace_Modifiers),
  local dump `ck-uesp-wiki/(main)/ImageSpace Modifiers.wiki`). Settled by a
  census of vanilla IMGS `DNAM` and IMAD `WNAM`/`XNAM`/`YNAM` values per
  game, plus a capture of one census IMAD in the original Skyrim SE showing
  where the blur starts.
- **PPX-DOF-006.** The IMGS `DNAM` `Sky / Blur Radius` field MUST be
  decoded into a canonical "no sky" boolean and a blur-radius step, carried
  to the DOF pass and applied as Phase 0 settles. The field is not a
  flag word. xEdit decodes it as a `u16` **enum** of 16 named values,
  `Radius 0`–`Radius 7` and `No Sky, Radius 0`–`7` (16384 … 17016), and FO4
  / FO76 add `0 = None`
  ([`wbDefinitionsTES5.pas`](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsTES5.pas),
  [`wbDefinitionsFO4.pas`](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsFO4.pas),
  [`wbDefinitionsFO76.pas`](https://github.com/TES5Edit/TES5Edit/blob/dev-4.1.6/Core/wbDefinitionsFO76.pas)).
  The decode MUST map named values only. Any other value MUST be reported
  by PPX-IMS-006, not guessed. **Phase 0: verify** what "No Sky" does: the
  CK wiki lists it with no description, and "exclude far-plane sky texels
  from the blur" is only the reading its name suggests. Settled by a
  capture in the original game of a census IMGS that uses a `No Sky`
  value.
- **PPX-DOF-007.** When several active IMADs author DOF, the composed value
  MUST follow the original's composition rule. **Phase 0: verify** it: the
  CK wiki states the rule for blur radius and double vision (highest wins)
  and tint (alpha-weighted average) but not for DOF.
- **PPX-DOF-008.** Depth of field MUST sample depth at the render extent and
  produce its CoC at the output extent. Under FSR Quality, the DOF-on
  frame MUST add no more outlier pixels at depth edges than a threshold
  fixed in Phase 0. The measure is the native-AA DOF-on capture of the same
  camera path, compared with `image_metrics::compare` in the
  `upscaler_quality` harness style (Cornell with an authored aperture,
  committed threshold).
- **PPX-DOF-009.** The IMAD `DNAM` DOF controls MUST be decoded with the
  curves. These are `DoF Use Target`, present from FO3 to FO76, and the
  `DoF Flags` bits `0x01` `Mode - Front`, `0x02` `Mode - Back` and `0x04`
  `No Sky`, present on FO3, Skyrim, FO4 and FO76. Skyrim's definition
  also names bits `0x08`–`0x20` "Blur Radius Bit 2…0". FNV's IMAD `DNAM`
  has an unused byte where the flags would be (xEdit
  `wbDefinitionsFO3/FNV/TES5/FO4/FO76.pas`, IMAD `DNAM`). Each value MUST
  reach the DOF pass or be reported by PPX-IMS-006. **Phase 0: verify**
  what "Use Target" and the front / back modes mean. The CK wiki lists
  them without descriptions.

### 5.4 Motion blur (`PPX-MB`)

- **PPX-MB-001.** Velocity motion blur MUST be a tile-max / neighbour-max
  reconstruction filter, following McGuire et al., "A Reconstruction Filter
  for Plausible Motion Blur", I3D 2012 (method summary in
  [NVIDIA GameWorks' sample](https://archive.docs.nvidia.com/gameworks/content/gameworkslibrary/graphicssamples/d3d_samples/motionblurd3dadvancedsample.htm),
  TileMax → NeighborMax → gather)
  and Jimenez 2014 (PPX-DOF-001).
- **PPX-MB-002.** It MUST read the existing G-buffer motion vectors, scaled
  from render to output extent, and MUST NOT add a second velocity target.
- **PPX-MB-003.** A frame on which `signal_temporal_discontinuity` fires
  (camera cut, cell transition, resize) MUST apply no motion blur; a new test
  pins it.
- **PPX-MB-004.** Camera-rotation blur and object blur MUST be separately
  switchable, because the motion vector mixes both and §10 Q7 sets their
  defaults separately.
- **PPX-MB-005.** The shutter MUST be a user setting expressed as an
  exposure time in milliseconds, with a default fixed in Phase 0. The
  blur length MUST be the motion vector scaled by exposure time ÷ frame
  interval, so a surface moving at a fixed screen speed gets the same blur
  length at 30, 60 and 120 fps. A host test pins that at three frame
  intervals. A shutter written as a fraction of the frame interval (a
  shutter angle) cannot meet this, which is why it is not the unit.
- **PPX-MB-006.** The IMAD "FullScreen Motion Blur" channel MUST keep its
  shipped behaviour (the `lens.z` horizontal streak in `sampleImageSpace`)
  unless §10 Q9 routes it into the velocity filter. **Phase 0: verify**
  what the original draws. The two wikis disagree on the evidence. The
  GECK wiki (FO3 / FNV) says "How much to blur the screen while moving",
  from 0.0 "No Blur" to 10.0 "Maximum Blur"
  ([GECK: ImageSpace Modifiers](https://geck.uesp.net/wiki/ImageSpace_Modifiers),
  local dump `geck-uesp-wiki/(main)/ImageSpace Modifiers.wiki`). The CK
  wiki (Skyrim) gives only 0.0, 0.15 and 0.35 with no description. "While
  moving" points to a camera-motion-dependent blur, not a fixed streak.
  Settled by a capture in FNV of a census IMAD with a non-zero strength,
  once with the camera still and once panning.

### 5.5 Grading and LUT (`PPX-LUT`)

- **PPX-LUT-001.** The presentation chain MUST apply, in this order:
  exposure → authored grade → chroma compress → display transform →
  underwater → LUT (if display-referred, §10 Q4) → vignette → grain → fade.
  Today the chain ends display transform → underwater → fade. The
  order MUST be pinned by a source-order test over `presentation.frag`, the
  mitigation RENDERING-PLAN names for its own "LUT + tonemapper order"
  risk.
- **PPX-LUT-002.** A full-alpha fade MUST still produce exactly the fade
  colour: no grain, vignette or LUT shows through it.
- **PPX-LUT-003.** An identity LUT MUST reproduce the LUT-off frame within
  one 8-bit level per channel on every pixel of the golden scenes; checked
  by a `golden_frames` variant run with an identity LUT.
- **PPX-LUT-004.** User LUTs MUST load from Adobe `.cube` files
  (`LUT_3D_SIZE`, `DOMAIN_MIN` / `DOMAIN_MAX`). **Phase 0: verify** the
  specification text: the published URL
  (`https://wwwimages2.adobe.com/content/dam/acom/en/products/speedgrade/cc/pdfs/cube-lut-specification-1.0.pdf`)
  is reported dead since 2023
  ([FFmpeg trac #10431](https://ffmpeg.org/pipermail/ffmpeg-trac/2023-June/066367.html));
  settled by locating an archived copy and citing its archive URL (the file
  itself is not checked in).
- **PPX-LUT-005.** A malformed or oversized `.cube` MUST be refused with a
  console line naming the file and the failing keyword, and the frame MUST
  render with the LUT `off`.
- **PPX-LUT-006.** An authored LUT (§5.8) MUST be resolved through the
  texture provider like any other texture path, and a missing texture MUST
  report `authored LUT missing: <path>` once and render with the LUT `off`.
  **Phase 0: verify** the vanilla LUT texture's layout and encoding
  (dimensions, whether it is a 2D strip, colour space) by inspecting the
  `TX00` paths in `Fallout4.esm` and their DDS headers in the FO4 texture
  archives.
- **PPX-LUT-007.** Whether the authored LUT is display-referred or
  scene-referred in the original MUST be settled before PPX-LUT-001's order
  is final. **Phase 0: verify** with the same inspection as PPX-LUT-006: an
  identity-shaped table points to display-referred.
- **PPX-LUT-008.** At most one LUT MUST be active per frame; a user LUT
  replaces the authored one and `post.status` says so.

### 5.6 Display transform (`PPX-TM`)

- **PPX-TM-001.** The display transform MUST be a registered setting
  (`render.tonemap`) in `crates/core/src/settings/builtin.rs`, with
  `--tonemap` and the `tonemap` console command writing through it. Today
  it is a CLI flag and the `ExposureTuning::agx` boolean only. The setting
  MUST carry a `TonemapOp`, not a boolean, so that a third operator can be
  selected.
- **PPX-TM-002.** If §10 Q5 adds Tony McMapface
  ([h3r2tic/tony-mc-mapface](https://github.com/h3r2tic/tony-mc-mapface),
  MIT or Apache-2.0), it MUST be a third `TonemapOp` with a stable shader
  id, and its LUT MUST be sampled through the same 3D-LUT path as
  PPX-LUT-001. The repository describes it as mapping "HDR Rec.709/sRGB
  stimulus" to linear output, matching the presentation pass's
  linear-output convention. **Phase 0: verify** the shipped LUT's size,
  encoding and input domain from that repository.
- **PPX-TM-003.** Every operator MUST pass the existing behavioural tests in
  `renderer::tonemap` (`grey_input_stays_grey_balanced`,
  `grey_ramp_is_monotonic`, `outputs_are_bounded_and_black_maps_to_black`,
  `negative_input_maps_to_black_and_stays_monotone`); for a LUT-based
  operator the host mirror samples the same table.
- **PPX-TM-004.** A new operator's licence notice MUST be in
  `THIRD_PARTY_NOTICES.md` in the commit that adds it, as AgX's is.

### 5.7 Lens finish (`PPX-LENS`)

- **PPX-LENS-001.** Vignette MUST attenuate radially from screen centre with
  radius and strength parameters. A host mirror test MUST pin three
  things: the centre texel is unchanged, attenuation is monotone in
  radius, and strength zero is the identity.
- **PPX-LENS-002.** Authored vignette (FO4 / FO76, §5.8) MUST drive it when
  present. **Phase 0: verify** the units of "Vignette Radius" and "Vignette
  Strength" (xEdit gives the fields, not their meaning) by a census of
  vanilla values.
- **PPX-LENS-003.** Film grain amplitude MUST be a function of the pixel's
  display luminance, pinned by a host mirror test over a grey ramp. Model: a real-time
  approximation of Newson, Delon and Galerne, "A Stochastic Film Grain Model
  for Resolution-Independent Rendering", Computer Graphics Forum 36(8), 2017
  ([doi:10.1111/cgf.13159](https://doi.org/10.1111/cgf.13159)), the
  reference RENDERING-PLAN names.
- **PPX-LENS-004.** Grain MUST be generated at the output extent, after the
  upscale tap, so FSR never sees it.
- **PPX-LENS-005.** The grain pattern MUST change every frame and MUST be
  seeded only from the frame counter, so two `--bench-frames` runs of the
  same scene produce byte-identical screenshots with grain on.
- **PPX-LENS-006.** Grain MUST be `off` in the default settings (§10 Q7).

### 5.8 Authored image-space translation (`PPX-IMS`)

Per-game differences in this area live in the parser decode
(`imagespace.rs`, `world.rs`) and EXAL's `translate_weather` /
`ImageSpaceSources`; the renderer sees only the canonical types.

| Field | FO3 / FNV | Skyrim | FO4 | FO76 | Source |
|---|---|---|---|---|---|
| IMGS DOF (Strength, Distance, Range, `Sky / Blur Radius` enum) | — | `DNAM` | `DNAM` | `DNAM` | xEdit `wbDefinitionsTES5/FO4/FO76.pas`; UESP [Skyrim IMGS](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/IMGS) |
| IMGS vignette (Radius, Strength) | — | — | `DNAM` tail | `DNAM` tail | xEdit `wbDefinitionsFO4.pas`, `wbDefinitionsFO76.pas` |
| IMGS LUT path | — | — | `TX00` | `TX00` | same |
| IMAD DOF curves | `WNAM` `XNAM` `YNAM` | same | same | same | xEdit `wbDefinitionsFO3/FNV/TES5/FO4.pas`; UESP [Skyrim IMAD](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/IMAD) |
| IMAD DOF controls (`DoF Use Target`, `DoF Flags`) | `DNAM` (FO3 flags; FNV none) | `DNAM` | `DNAM` | `DNAM` | xEdit IMAD `DNAM` in each file (PPX-DOF-009) |
| IMAD vignette curves | — | — | `NAM5` `NAM6` | `NAM5` `NAM6` (`wbFromVersion(118, …)`) | xEdit `wbDefinitionsFO4.pas` (no version gate), `wbDefinitionsFO76.pas` (form version ≥ 118) |

FO3 / FNV IMGS carry no DOF block (xEdit `wbDefinitionsFNV.pas` IMGS field
list). Oblivion has no IMGS or IMAD record (UESP's Oblivion record list,
local dump `uesp-wiki/Oblivion Mod/Mod File Format/`).

- **PPX-IMS-001.** `parse_imad` MUST decode `WNAM` / `XNAM` / `YNAM` into
  keyed scalar curves on `ImadRecord`, for every game whose IMAD carries
  them; a new test `parses_dof_curves` uses a synthetic record.
- **PPX-IMS-002.** `parse_imad` MUST decode `NAM5` / `NAM6` on FO4, and on
  FO76 for records of form version ≥ 118, the gate xEdit's FO76 definition
  applies. A synthetic-record test covers each side of the gate.
- **PPX-IMS-003.** `decode_image_space` MUST decode IMGS `DNAM` DOF on
  Skyrim, FO4 and FO76, and the vignette tail and `TX00` on FO4 and FO76,
  into new fields of the canonical `ImageSpace`. Today it reads `DNAM` only
  as the FO3 / FNV grade block and skips it elsewhere.
- **PPX-IMS-004.** The new fields MUST be added to the existing
  `ImageSpace` / `ImageSpaceModifier` types in `crates/core/src/imagespace.rs`
  with identity defaults, not to a new type (the #3861 one-type rule), and
  `defaults_are_the_identity_transform` MUST cover them.
- **PPX-IMS-005.** `image_space_modifier_system` MUST compose the new
  channels onto the base with the rules PPX-DOF-007 and PPX-LENS-002 settle,
  each with a unit test.
- **PPX-IMS-006.** A census command or test MUST list, per game, the IMGS
  and IMAD fields the engine decodes but does not consume (today: the HDR
  block, PPX-NG-002), so "not consumed" is a reported fact.
- **PPX-IMS-007.** A real-data floor test in the existing opt-in corpus lane
  MUST pin, per game, how many vanilla IMGS / IMAD records author non-identity
  DOF, vignette or LUT values, so a decode regression drops the count.
- **PPX-IMS-008.** Saves MUST NOT change shape for this area unless a new
  field lands in a saved type; if one does (`ImadRecord` rides in
  `CinematicPresentationState`), the save shape guard and `FORMAT_MAJOR`
  policy apply in the same commit.

### 5.9 Settings, control and observability (`PPX-CTL`)

- **PPX-CTL-001.** A `post.status` console command MUST print one line per
  stage of §3.1 with its parameter source (§3.2) for the current frame.
- **PPX-CTL-002.** Each user-switchable stage (bloom upgrade if Q2 makes it
  switchable, DOF, motion blur's two halves, LUT, display transform,
  vignette, grain) MUST be a registered setting persisted through
  `settings-io`, and MAY appear in `assets/graphics_presets.toml`.
- **PPX-CTL-003.** A `dof` console command MUST set the lens-DOF aperture
  and focus distance on the active camera for cinematic mode, and print the
  current values with no arguments.
- **PPX-CTL-004.** Every new pass MUST have a `TimerPass` row, so its GPU
  time appears in `BYRO_PROFILE=1` output and the debug overlay's pass list.
- **PPX-CTL-005.** `r.health` MUST stay `CLEAN` with every stage on, since
  the presentation pass's non-finite counters sample the output of the
  post-upscale passes.

### 5.10 Performance (`PPX-PERF`)

- **PPX-PERF-001.** With every new stage `off`, the frame MUST record the
  same dispatches as HEAD apart from the bloom filter change, checked by the
  `TimerPass` active flags.
- **PPX-PERF-002.** Each stage's GPU cost MUST be measured at the output
  resolutions §10 Q3 names, with the bench methodology of
  [How to run a bench](../../ROADMAP.md#how-to-run-a-bench) (same-machine
  control, pinned camera), and MUST meet the budget §10 Q3 sets.
- **PPX-PERF-003.** The new passes MUST NOT allocate per frame; their images
  MUST be sized at swapchain creation and on resize, and their footprint
  added to [memory-budget.md](../engine/memory-budget.md#bloom).

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | IMGS HDR block tunes bloom threshold, scale and radius and eye adaptation per image space ([CK: ImageSpace](https://ck.uesp.net/wiki/ImageSpace)) | One physical bloom and exposure model; HDR block ignored (PPX-NG-002) | Mods that retune IMGS bloom / adaptation | A mod's bloom edit silently does nothing → reported by PPX-IMS-006 |
| 2 | DOF blur in eight radius steps (xEdit `Sky / Blur Radius` enum, `Radius 0`–`7`); raising Strength above the 0–1 range "provides an awesome contrast-like effect, but does not look right as a depth of field effect" (CK and GECK wikis, ImageSpace Modifiers § Depth Of Field) | Physically sized CoC gather (PPX-DOF-001, PPX-NG-003) | — | Authored over-driven DOF looks different → clamp and translation unit-tested (PPX-DOF-005) |
| 3 | IMAD "FullScreen Motion Blur": "how much to blur the screen while moving", 0–10 (GECK wiki); CK wiki undescribed | Shipped horizontal streak kept (PPX-MB-006) | — | Not the original look → Phase 0 verify, §10 Q9 |
| 4 | Post look changed by renderer injectors (ENB, ReShade) hooking the original renderer (LMC row N5) | Native user LUT, vignette, grain (§5.5, §5.7); injectors not loaded | — (injectors are D3, not redundant) | Injector presets do not carry over → reported as D3 by the LMC preflight (PPX-NG-004) |
| 5 | FO4 / FO76 IMGS name a LUT texture (`TX00`, xEdit); how the original applies it is unverified | Authored LUT applied at the position §10 Q4 fixes, after Redux's own display transform | — | Wrong domain shifts colour → PPX-LUT-007 verifies the domain first |
| 6 | Jimenez 2014 bloom takes "non-thresholded color" as input (v18 slides) | Seed level keeps the soft-knee bright pass (PPX-BLM-006) | — | Weaker glow on mid-luminance surfaces than the source intends; deliberate, because without the knee the sky regains SKYAL's measured gain → PPX-BLM-003 guards it |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

The engine-side stages (bloom, velocity motion blur, lens DOF, user LUT,
display transform, grain) are game-agnostic and apply to every title the
renderer draws. Only the authored inputs differ, and they differ in the
translate step (§5.8). Waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail).

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes (engine stages only) | No IMGS / IMAD records exist, so nothing is authored; wave 2 |
| Fallout 3 | yes | IMAD DOF curves; grade already translated; wave 1 with FNV |
| Fallout: New Vegas | yes | Reference title; IMAD DOF curves; wave 1 |
| Skyrim (LE / SE / AE) | yes | IMGS + IMAD DOF; SKYAL bloom capture lives here; wave 1 |
| Fallout 4 | yes | IMGS + IMAD DOF, vignette and the only authored LUT; wave 1 |
| Fallout 76 | yes (decode only) | Same IMGS / IMAD family as FO4; rendered checks wait on FO76 cell loading ([game-compatibility.md](../engine/game-compatibility.md#exterior-readiness--fo76-cell-loading)); wave 3 |
| Starfield | yes (engine stages only) | Authored post data is in `REFL` blobs (PPX-NG-007); wave 2 |

### 7.2 Acceptance

Game data is the local installs named by the `--game` profiles; nothing is
checked in. A scenario passes when:

1. **Unit and source pins.** `cargo test -p byroredux-renderer` passes the
   existing `tonemap::tests::*` and `bloom::tests::*` plus the new pins
   named in §5 (filter weights, CoC, chain order, pinhole-under-post-DOF,
   discontinuity zeroes motion blur). No game data.
2. **Parser decode.** `cargo test -p byroredux-plugin` passes the new
   synthetic IMGS / IMAD tests (PPX-IMS-001..003); the opt-in corpus floor
   (PPX-IMS-007) passes on FNV, FO3, Skyrim SE and FO4 data.
3. **No regression with stages off.** `golden_frames` (`cube_demo_golden_frame`,
   `combustion_lab_golden_frame`, `--ignored`, Vulkan device) passes against
   baselines regenerated only for the bloom filter change, and
   `upscaler_quality::fsr_presets_track_the_native_reference_on_every_camera_path`
   stays within its thresholds.
4. **Bloom sky gain.** The SKYAL § 1.1 capture recipe on Skyrim SE, read
   through `composite_term`, meets PPX-BLM-003.
5. **Live stages.** New script `docs/smoke-tests/m58-post-process.sh`
   (written in Phase 2) boots FNV, Skyrim SE and FO4 cells with
   `--bench-hold` and `BYRO_DEBUG_SERVER=1`, then through `byro-dbg`: runs
   `post.status` and checks every source line; turns each stage on in turn
   (`dof`, the motion-blur, LUT, vignette and grain settings, `tonemap`)
   and takes a `screenshot`; requires `r.health` `CLEAN` and each stage's
   timer active only while on. Missing data is `SKIP` with exit 77, per the
   [smoke contract](../smoke-tests/README.md).
6. **Authored data.** The same script loads one census IMGS / IMAD per game
   that authors DOF (and, on FO4, vignette and LUT), and `post.status`
   reports `authored: <editor ID>` for those stages.

---

## 8. Metrics

Numbers live in ROADMAP (the M58 row for coverage, the bench-of-record
section under [How to run a bench](../../ROADMAP.md#how-to-run-a-bench) for
cost); this document defines them.

| Metric | Definition | Granularity |
|---|---|---|
| Authored post coverage | IMGS / IMAD post fields decoded *and* consumed ÷ post fields the game's records define (§5.8 table) | per game |
| Authored record coverage | vanilla records authoring non-identity DOF / vignette / LUT that render with source `authored` ÷ such records (PPX-IMS-007 census) | per game |
| Stage GPU cost | `TimerPass` milliseconds per stage, bench methodology | per stage, per output resolution |
| Post-chain delta | sum of post-stage GPU ms with all defaults on − HEAD's | per bench scene |
| Bloom sky gain | post-bloom ÷ pre-bloom zenith luminance on the SKYAL capture | per filter revision |
| Highlight flicker | luma variance of the PPX-BLM-004 region over the pan | per filter revision |

---

## 9. Delivery sequence

M58 depends on nothing still open: FSR 3.1, the exposure meter and SKYAL's
bloom fix have shipped. PPX-LENS-002 and PPX-LUT-006 need FO4 cell
rendering, which ships.

**Phase 0 — Census, sources and design (no engine code).** Resolve the §10
decisions. Close every **Phase 0: verify** item: DOF field semantics and
composition (PPX-DOF-005, PPX-DOF-007), the IMGS `No Sky` value and the
IMAD DOF modes (PPX-DOF-006, PPX-DOF-009), the IMAD full-screen motion blur
(PPX-MB-006), the `.cube` specification text (PPX-LUT-004), the FO4 LUT
texture layout and domain (PPX-LUT-006, PPX-LUT-007), vignette units
(PPX-LENS-002) and the Tony McMapface LUT format (PPX-TM-002). Record the
weights of the bloom filter §10 Q1 selects, from its primary source. For
Jimenez 2014 that source is the v18 deck PPX-DOF-001 links: its "Bloom /
Veil" bonus section covers the 13-fetch, 36-texel downsample, the 3×3
tent upsample and the partial Karis average on the mip 0 → 1 step. It
was located during review round 1, and the weights are still to be
transcribed. Fix the PPX-DOF-008 outlier threshold. Run the
per-game census of authored DOF / vignette / LUT values from local data and
fix the PPX-BLM-003 tolerance, the PPX-BLM-004 method and the shutter
default. Write `docs/engine/post-process.md`. Exit: the design is checked
in, every verify item is closed with a citation or filed as an issue, and
the census table per game is in the design.

**Phase 1 — Authored decode and observability.** PPX-IMS-001..008,
PPX-CTL-001, PPX-CTL-002 for the display transform (PPX-TM-001), PPX-FG-002
and PPX-FG-005 pins. Exit: acceptance scenarios 1 and 2 pass and
`post.status` lists every §3.1 stage on FNV, Skyrim SE and FO4.

**Phase 2 — Depth of field and motion blur.** PPX-FG-001, PPX-FG-006..008,
§5.3, §5.4, PPX-CTL-003..005, PPX-PERF-003; the new smoke script's DOF and
motion-blur legs. Exit: scenarios 3, 5 and 6 pass for DOF and motion blur,
and their stage GPU cost is published within the §10 Q3 budget.

**Phase 3 — Bloom upgrade and placement.** §5.2 and PPX-FG-004 per §10 Q1
and Q2. Exit: scenario 4 passes, the PPX-BLM-004 flicker metric is no
worse than HEAD's, and the regenerated goldens are reviewed.

**Phase 4 — Grade, LUT, display transform, lens finish.** §5.5, PPX-TM-002..004,
§5.7. Exit: scenarios 1, 3 (PPX-LUT-003 variant), 5 and 6 pass for the LUT,
transform, vignette and grain legs; the post-chain delta is published in
ROADMAP and the M58 row is rewritten to match.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — Bloom filter.** Recommended: Jimenez 2014's pyramid, with a
   13-fetch downsample, a partial Karis average on the first step and a
   3×3 tent upsample (v18 slides, "Bloom / Veil"). It has the same pyramid
   shape that ships and targets the sub-pixel pulsing the
   `bloom_downsample.comp` header names ("eliminates the pulsating
   artifacts", same slides). Its weights are in a deck that can be
   downloaded (PPX-DOF-001 links). Consequence: the source's
   non-thresholded input is not adopted (PPX-BLM-006), and this changes
   the ROADMAP row's "Kawase-blur bloom (5-pass dual filter)" wording.
   Alternative A: Bjørge's dual
   filter (ARM, "Bandwidth-Efficient Rendering", SIGGRAPH 2015), the ROADMAP
   row's wording; it is designed to save bandwidth, which matters less on
   the RT baseline GPU, and this draft did not locate the primary slides
   (Phase 0 would have to). Alternative B: FFT convolution with a lens
   diffraction kernel (RENDERING-PLAN's target); most physical, highest
   cost, and it needs a sourced kernel.
2. **Q2 — Bloom placement.** Recommended: move bloom post-upscale, as the
   FSR plan's target graph (§ 1.2 item 6) and colour contract (§ 1.4,
   "no bloom") state, and order it after DOF and motion blur. That
   ordering is this PRD's choice, not the plan's: the plan's graph has a
   single "Output-resolution bloom and post" node. Out-of-focus highlights
   then bloom as blurred discs. Consequence: bloom runs on more pixels
   under FSR presets. FSR needs the exposure texel before the upscale, so
   the exposure meter must meter the pre-bloom scene (or bloom's output
   one frame late). That rewrites the § 1.4 Exposure row and renderer.md
   step 21 (PPX-FG-004). Alternative: keep it pre-upscale. The meter is
   unchanged and bloom precedes DOF. The FSR plan's § 1.2 item 6 and § 1.4
   colour row are then amended to describe pre-upscale bloom as the
   contract, instead of carrying it as known scope.
3. **Q3 — Performance budget.** Recommended: stages that are on by
   default together add no more than RENDERING-PLAN Stage 1's acceptance
   delta at 1440p on the dev GPU; DOF and motion blur, which are opt-in,
   get their own per-stage budget fixed in Phase 0 from a measured
   baseline. Alternative: one budget for the whole chain with everything on;
   simpler, but it lets an opt-in effect starve the always-on ones.
4. **Q4 — LUT position.** Recommended: display-referred, after the display
   transform, if PPX-LUT-007 shows the FO4 LUT is display-referred, because
   that is what LUTs authored on LDR output expect and user `.cube` files
   from grading tools usually target. Alternative: scene-referred, in a log
   encoding before the transform; grades survive a change of display
   transform, but every authored LUT needs a conversion step.
5. **Q5 — Tony McMapface.** Recommended: add it as the third operator once
   the 3D-LUT path exists (Phase 4); licence-compatible, and it is the
   "boring" neutral reference the ROADMAP row names. Alternative: stop at
   ACES and AgX; the row's tone-map scope is then closed as shipped.
6. **Q6 — What cinematic mode is.** Recommended: no new mode; lens DOF is
   the `Camera` aperture / focus fields set by the `dof` command, a setting,
   or a scripted camera. M51's path-traced modes would read the same
   fields if [that PRD](path-tracing-reference-mode.md)'s Q4 puts depth
   of field in the estimator (PPX-FG-008).
   Alternative: a dedicated photo mode with its own UI and camera; more
   usable, but it is a UI feature that needs M48-era menu work.
7. **Q7 — Defaults.** Recommended: authored effects on (DOF, vignette, LUT
   where the content authors them), engine extras off (velocity motion
   blur both halves, grain, Tony McMapface), ACES stays the default
   transform. Alternative: object motion blur on by default with camera blur
   off; closer to modern titles, but it changes every existing capture and
   golden.
8. **Q8 — The ROADMAP row's per-cell mood promise.** This is a T1 change
   to what M58 promises, so it is the owner's call. SKYAL has not
   decided it. Recommended: drop "per-cell-type mood — interior warm,
   exterior cool, irradiated green" from M58 (PPX-NG-001), extending to M58
   the principle SKYAL § 2.4.1 set for its own terms: the authored
   temperature and mood are the base. A grade the content did not author
   is also what NIFAL's no-fabrication rule forbids at the boundary
   ([nifal.md § Passthroughs](../engine/nifal.md#passthroughs--parked--dropped-inventory-surveyed-2026-06-02)).
   Consequence: RENDERING-PLAN Stage 1's "one LUT per game/weather
   profile" also narrows to authored and user LUTs. Alternative A: keep
   it as an opt-in user LUT pack shipped with the engine, off by default
   and reported as `user`. It costs nothing at the boundary, but the
   project has to make the LUT assets. Alternative B: keep it as written,
   on by default. That contradicts SKYAL § 2.4.1's principle on every
   cell, and the PRD's corollary (§1) would need an exception.
9. **Q9 — IMAD full-screen motion blur.** Recommended: keep the shipped
   streak until the PPX-MB-006 capture settles what the original draws,
   then decide. The GECK wiki's "while moving" already points away from a
   static streak. Alternative: drive the velocity filter's shutter from the IMAD
   strength; one blur, but it ties an authored scripted effect to a
   setting the user may have turned off.

---

## 11. Document map

| Need | Go to |
|---|---|
| Pass order, bloom and TAA as built | [renderer.md](../engine/renderer.md#per-frame-draw) |
| Shader list and submission order | [shader-pipeline.md](../engine/shader-pipeline.md#per-frame-submission-order) |
| Upscaler frame graph, FSR input contract, DOF decision | [fsr3-upscaler-integration-plan.md](../engine/fsr3-upscaler-integration-plan.md#12-target-frame-graph) |
| Bloom sky-gain capture and the authored-mood rule | [skyal.md](../engine/skyal.md#11-the-measured-defect) |
| Where IMGS / IMAD become the canonical image space | [exal.md](../engine/exal.md#3-the-single-boundary-proposed) |
| Colour-pipeline research plan (Stage 1) | [RENDERING-PLAN.md](../../RENDERING-PLAN.md#stage-1--color-pipeline-and-physical-camera) |
| Renderer-injector mod disposition | [legacy-mod-compatibility.md](legacy-mod-compatibility.md#31-mechanism-catalog) |
| Path-traced modes and their post-chain contract (M51) | [path-tracing-reference-mode.md](path-tracing-reference-mode.md) |
| Bench methodology | [ROADMAP.md](../../ROADMAP.md#how-to-run-a-bench) |
| Smoke-test contract | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: GECK motion-blur citation, IMGS `Sky / Blur Radius` enum and IMAD `DoF Flags` (PPX-DOF-006, new PPX-DOF-009), FO76-only NAM5 / NAM6 gate, settings inventory, bloom-vs-FSR §1.4 wording and Exposure-row consequence, Q2 ordering, Q8 / SKYAL scope, Jimenez slides located (PPX-BLM-006, §6 row 6), testable shutter / DOF-edge / vignette / grain (PPX-MB-005, PPX-DOF-008, PPX-LENS-001, PPX-LENS-003, new PPX-LENS-005/006), M51 interaction (new PPX-FG-008), compute-only passes (new PPX-FG-007) | claude: revise |
