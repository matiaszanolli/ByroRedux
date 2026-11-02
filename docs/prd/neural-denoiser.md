# Neural Denoiser — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (ground truth aligned with the path-tracing reference PRD's
progressive reference, interactive PT mode and relMSE; the nested
shadow-filter bracket; ray-quality-tier pinning; the shader toolchain's
plain `-V` invocation; RFC 2119 wording and split observables; the
alternative reading of "30 % runtime"; the ROADMAP sequencing Q5 changes).
Revision 1 was the first draft of the M54
promise: a small learned spatial filter that replaces the SVGF à-trous
stage for indirect lighting, the data it is trained on, how it runs on
Vulkan without a vendor SDK, and how it is measured. The decisions it needs
from the project owner are collected in §10; nothing below is built as a
result of this document yet. Statements about how an external
specification, paper, library or tool behaves cite external sources; a
claim that could not be verified is marked **Phase 0: verify** instead of
being stated as fact.

**Scope**: The promise is that, on hardware that supports it, the spatial
half of the indirect-lighting denoiser can be a small neural network (about
1–2 MB of weights, per the
[ROADMAP M54 row](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon))
that matches or beats the current à-trous filter's image quality against a
converged target image (§3.2) at no more than 70 % of its GPU time, runs as an
engine-owned Vulkan compute pass with no proprietary SDK, and falls back to
the à-trous filter everywhere else. In scope: the inference pass, the
weights file and its provenance, the training-data capture, the offline
training tool, the ground-truth target, the evaluation harness and the
interaction with SVGF temporal accumulation, TAA and FSR 3.1. Out of scope:
denoising the direct-shadow signal, replacing SVGF temporal accumulation,
replacing TAA or FSR, any hosted training or telemetry service, and any
per-game network.

**Related**: this document states the *promise*; the designs below own the
*how*. No design owns the neural pass yet; Phase 0 writes it.

| Mechanism | Owning design |
|---|---|
| SVGF temporal + à-trous denoiser it plugs into | [Vulkan Renderer § SVGF](../engine/renderer.md#svgf-temporal-denoiser), [Shader Pipeline](../engine/shader-pipeline.md#per-frame-submission-order) |
| A/B captures, determinism, convergence frames | [Renderer Evaluation](../engine/renderer-evaluation.md) |
| Temporal reconstruction contract, reference laboratory | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md#runtime-contract) |
| TAA clamp and RT seed trade-offs the input noise depends on | [Shadow Pipeline Trade-offs](../engine/shadow-pipeline-tradeoffs.md) |
| FSR 3.1 frame graph and temporal-history rules | [FSR 3.1 Integration Plan](../engine/fsr3-upscaler-integration-plan.md#15-temporal-history-rules) |
| Screen-sized VRAM ledger | [Memory Budget](../engine/memory-budget.md#svgf-indirect-lighting-denoiser) |
| Converged path-traced reference and interactive PT mode (M51) | [Path Tracing Reference Mode — PRD](path-tracing-reference-mode.md) (progressive reference, PTR-OUT-001 to PTR-OUT-003, PTR-RT-003, PTR-VAL-005) |
| GI estimator changes the training depends on (M-LIGHT) | [Reference-Quality Lighting — PRD](reference-lighting.md) (its RLT-NG-004 leaves the spatial filter to M54) |
| Neural pass, weights format, capture format, training tool | none yet — Phase 0 writes `docs/engine/neural-denoiser.md` |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `NDN-` prefix.

---

## 1. Product thesis

The renderer is RT-first ([ROADMAP keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)).
Its indirect lighting is one stochastic sample per pixel, made watchable by
SVGF: a temporal accumulation pass and a hand-tuned, edge-stopping à-trous
wavelet filter (Schied et al. 2017). Hand-tuned edge stopping is a known
ceiling. Its kernel was already cut from five levels to three because the
wide kernel spread correlated GI error into wall-sized stains (the
`ATROUS_ITERATIONS` note in `crates/renderer/src/vulkan/svgf.rs`). Learned
reconstruction from the same per-pixel features is the published
alternative: Chaitanya et al. 2017 reconstruct low-sample global
illumination from the noisy image plus auxiliary channels such as depth and
normals, and Işık et al. 2021 report matching offline denoisers at 2–8
samples per pixel at interactive rates at 1080p (§11 has the sources).

M54 is for two audiences. A player on supported hardware gets cleaner
indirect light for less GPU time in the default raster mode. A user of
M51's interactive PT mode, which feeds the existing SVGF chain
(PTR-RT-003 in [the M51 PRD](path-tracing-reference-mode.md)),
gets a better image for the same samples per frame; at that scale the
network is a quality multiplier, not a speed-up (ROADMAP M54 row). M51's
progressive reference runs no denoiser and is not an M54 consumer; it is
M54's ground truth (§3.2).

The engine will **not** ship a vendor denoiser, will **not** depend on a
proprietary compute runtime, will **not** collect anyone's frames, and will
**not** train or ship one network per game. The corollary that shapes every
requirement below: **the network is a drop-in replacement for one stage,
measured against the stage it replaces.** It takes the inputs the à-trous
filter takes today, writes the image the à-trous filter writes today, is
scored against the same target image in the same run, and the à-trous filter
stays as the fallback and the control.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **SVGF temporal** | `svgf_temporal.comp`: motion-vector reprojection and accumulation of the 1-spp indirect, plus the luminance moments. Kept by M54. |
| **À-trous stage** | `svgf_atrous.comp`, dispatched `ATROUS_ITERATIONS` times after SVGF temporal; the stage M54 replaces. The code's name; this document does not coin another. |
| **Neural stage** | The M54 compute pass that replaces the à-trous stage: same inputs, same output image. |
| **Spatial stage** | Whichever of the à-trous stage or the neural stage is active on a frame. |
| **Spatial-stage time** | GPU time of the spatial stage alone, excluding SVGF temporal and the nested direct-shadow filter (NDN-MEAS-001). |
| **Feature buffers** | The per-pixel guides the spatial stage reads: the SVGF temporal output and moments, the octahedral normal, depth, mesh id and albedo (the à-trous bindings in `svgf_atrous.comp`). |
| **Weights file** | One binary file holding the network's parameters, a format version, the network topology identifier and a provenance manifest (NDN-NET). |
| **Provenance manifest** | The list, inside or beside the weights file, of every training scene with its source and licence, the training-tool revision and the training seed. |
| **Training pair** | One captured frame of feature buffers plus the matching target image, at the same render extent and camera. |
| **Target image** | What a training pair and an evaluation score against: converged, albedo-demodulated indirect light for the same frame from one tier of §3.2. Deliberately not called "reference image", which the sibling PRD reserves for the progressive reference's output (R1's source). |
| **Progressive reference**, **interactive PT mode**, **spp**, **relMSE** | As defined in [Path Tracing Reference Mode § Terminology](path-tracing-reference-mode.md#2-terminology); used here with those meanings and not redefined. |
| **Raster mode** | The shipped hybrid renderer, as defined in [Reference-Quality Lighting § Terminology](reference-lighting.md#2-terminology). |
| **Ray quality tier** | The adaptive GI / reflection budget tier 0–3 chosen at runtime by `AdaptiveRayBudget` (`crates/renderer/src/vulkan/scene_buffer/ray_budget.rs`), pinned with `--rt-test-ray-quality-tier` (`byroredux/src/cli_args.rs`). It changes the noise the spatial stage receives. |
| **Bench scene** | The Cornell scene plus every wave-1 (§7.1) scene of [ROADMAP's bench-of-record](../../ROADMAP.md#how-to-run-a-bench). |
| **Redistributable scene** | A scene whose every input is in this repository and authored for it (the Cornell scene in `byroredux/src/cornell.rs`, procedurally generated scenes). Captures of it can be checked in and trained on for shipped weights. |
| **Local scene** | A scene built from the user's installed game data. Captures of it stay on the user's machine. |
| **Cooperative matrix** | Subgroup-scoped matrix types and multiply-accumulate operations exposed by `VK_KHR_cooperative_matrix` / `SPV_KHR_cooperative_matrix` / `GLSL_KHR_cooperative_matrix` (§11). |

---

## 3. Core model

### 3.1 Pipeline stages and their state

The neural stage touches the stages below. The inventory is the state at
HEAD on 2026-11-01; it goes stale on the first M54 commit and is replaced by
the owning design's stage table once Phase 0 writes it.

| # | Stage | M54 disposition | Today |
|---|---|---|---|
| 1 | G-buffer feature attachments | Read only | `crates/renderer/src/vulkan/gbuffer.rs`: normal `R16G16_SNORM` (octahedral), motion `R16G16_SFLOAT`, mesh id `R32_UINT`, albedo-demodulated raw indirect and albedo `B10G11R11_UFLOAT_PACK32`. |
| 2 | SVGF temporal | Kept, unchanged | `svgf_temporal.comp`: reprojection with mesh-id and normal rejection, α schedule, moments (μ₁, μ₂, history age) in `RGBA16F` (`svgf.rs` module doc). |
| 3 | À-trous stage | Replaced where supported; stays as fallback and control | `svgf.rs` `ATROUS_ITERATIONS = 3` (strides 1, 2, 4, 5×5 B3 kernel); `svgf_atrous.comp` weights normal, depth, albedo, variance-keyed luminance and mesh-id match (Schied 2017 §4.3). |
| 4 | À-trous A/B toggle | Kept as the control | `DBG_DISABLE_ATROUS = 0x4000` (`crates/renderer/src/shader_constants_data.rs`) turns each iteration into a copy; `r.dbg` (`RenderDebugFlagsCommand`, `byroredux/src/commands/world_info.rs`) sets it live. |
| 5 | Composite input | Unchanged binding | Composite samples `SvgfPipeline::indirect_view`, the final à-trous ping-pong slot (`svgf.rs`). |
| 6 | Direct-shadow filter | Out of scope | `shadow_filter.comp`, its own à-trous chain, dispatched inside the SVGF timer bracket (`record_svgf_pass`, `crates/renderer/src/vulkan/context/post_passes.rs`). |
| 7 | TAA / FSR 3.1 | Unchanged consumers | Default upscaler is FSR 3.1 Quality (`UpscalerMode::default`, `crates/renderer/src/vulkan/upscaling.rs`); SVGF is built at the render extent (`create_svgf_and_restir` call in `crates/renderer/src/vulkan/context/init.rs`). |
| 8 | GPU timing | Gains a bracket | One `Svgf` bracket covers temporal, three à-trous dispatches and the nested shadow filter (`crates/renderer/src/vulkan/gpu_timers.rs`, `record_svgf_pass`); no bracket isolates the à-trous stage. The `bench:` line prints it as `gpu_svgf=` (`BENCH_GPU_KEYS`, `byroredux/src/main.rs`). The shadow filter also has its own nested bracket, printed as `gpu_shadow_filter=` (same array, #5959), so `gpu_svgf − gpu_shadow_filter` is temporal plus à-trous today. |
| 9 | Device features | Gains an optional extension | `shaderFloat16` is enabled when supported (`shader_float16_supported`, `crates/renderer/src/vulkan/device.rs`); no cooperative-matrix extension is requested anywhere in `crates/renderer/src`; the instance targets Vulkan 1.3 (`crates/renderer/src/vulkan/instance.rs`). The pinned `ash` 0.38.0+1.3.281 (`Cargo.lock`) exposes `ash::khr::cooperative_matrix`. |
| 10 | Quality harness | Reused | `byroredux/tests/image_metrics/mod.rs` (`compare`, `compare_linear`: SSIM, max, p95/p99, outlier %); `byroredux/tests/upscaler_quality.rs` scores candidates against a same-run reference on four moving camera paths, Cornell only, game content report-only. Both metrics take 8-bit `RgbImage`s, so they score presented PNGs, not floating-point captures. |
| 11 | Ray quality tier | Pinned in every capture and bench | `AdaptiveRayBudget` (`ray_budget.rs`) moves the tier at runtime from main-lighting GPU time; `--rt-test-ray-quality-tier 0..=3` (`cli_args.rs`) pins it. |

### 3.2 Reference tiers

The ground truth the network learns and is scored against (the target
image) comes from one of three tiers. Every training pair and every score
names its tier.

| Tier | Name | Source | Use | Reported as |
|---|---|---|---|---|
| **R0** | Same-estimator accumulation | SVGF temporal's progressive accumulation on a parked camera: the α floor drops and history converges 1/N until the 255 history-age cap; past the cap the update keeps α = 1/256, an exponential average of about 256 samples (`svgf_temporal.comp`, `params.w` branch and `ageCap`, at HEAD on 2026-11-01). It is the spatial stage's own input, converged. | Bring-up and Phase 1–2 training. It converges the current estimator, including its firefly clamp, so it scores variance removal, not bias. | `target=R0` |
| **R1** | M51's progressive reference (Phase 2 exit) | The sibling PRD's progressive reference image (PTR-OUT-001: 32-bit float OpenEXR, scene-linear), its indirect split (PTR-VAL-003), demodulated by the reference's first-non-specular-vertex albedo (PTR-OUT-003). Not available at HEAD on 2026-11-01: no path-tracing mode or `render.reference` command exists in `byroredux/src` or `crates/renderer/src`. Becomes available at that PRD's Phase 2 exit (trusted reference), on the Cornell scenes first. | Shipped weights and the M54 exit (Q5). | `target=R1` plus the reference manifest's spp and SHA-256 (PTR-OUT-002) |
| **R2** | External baseline | An offline denoiser run on the same capture, e.g. Intel Open Image Denoise (Apache-2.0, §11). | A comparison column only; never a training target. | `baseline=R2:<tool>` |

### 3.3 Data classes

| Class | May be checked in | May train shipped weights | May leave the machine |
|---|---|---|---|
| Redistributable scene capture | Yes | Yes | Yes |
| Local scene capture | No | Only under Q1 alternative A | No |
| Locally trained weights | No | — | No |
| Shipped weights | Yes | — | Yes |

---

## 4. Goals and non-goals

### 4.1 Goals

- **NDN-G-001 — Quality parity.** On every gated acceptance scenario (§7.2
  A4, and A9 once it exists) and every camera path in it, the neural
  stage's output has no higher relMSE against the same target image than
  the à-trous stage's (NDN-MEAS-008), and the presented frame scores at
  least the à-trous stage's linear SSIM and no worse a p99 error
  (NDN-MEAS-003). Game content (A5) is report-only and does not bind.
- **NDN-G-002 — Runtime.** On the development GPU at the default render
  extent, neural spatial-stage time is at most 70 % of à-trous spatial-stage
  time on every bench scene (§2) (the ROADMAP "30 % runtime" target, whose
  reading is Q4).
- **NDN-G-003 — No proprietary dependency.** The engine build and runtime
  link no vendor denoiser, ML runtime or compute SDK for M54; the neural
  stage is GLSL compiled with the existing shader toolchain.
- **NDN-G-004 — Clean fallback.** Every device that runs today's renderer
  still renders, with the à-trous stage, when the neural stage is
  unavailable.
- **NDN-G-005 — Shippable weights.** One weights file of at most 2 MiB ships
  with the engine, with a provenance manifest every entry of which is
  redistributable (Q1).
- **NDN-G-006 — Path-tracing multiplier.** With M51's interactive PT mode
  (its Phase 5) at its stated spp per frame, where the neural stage
  replaces à-trous as it does in raster mode (PTR-RT-003), the neural
  stage lowers the mode's relMSE
  against the progressive reference (PTR-VAL-005) below the à-trous stage's
  on the scenes of the sibling PRD's interactive-mode scenario (its A10).

### 4.2 Non-goals

- **NDN-NG-001.** Replacing SVGF temporal accumulation or making the network
  recurrent. The ROADMAP M54 row scopes the network to the SVGF *spatial*
  filter, and the FSR plan keeps SVGF as the sole temporal denoiser of the
  indirect signal ([§1.5](../engine/fsr3-upscaler-integration-plan.md#15-temporal-history-rules)).
- **NDN-NG-002.** Denoising direct shadows. `shadow_filter.comp` stays; the
  M54 row names indirect lighting only.
- **NDN-NG-003.** Replacing TAA or FSR 3.1 with a learned upscaler. DLSS is
  in the ROADMAP parking lot as M37.6; a learned upscaler is not M54.
- **NDN-NG-004.** Using NVIDIA NRD or Intel Open Image Denoise as the
  runtime. NRD is distributed under the NVIDIA RTX SDKs License, which is
  not an open-source licence (§11); the M54 row rules out proprietary SDK
  dependencies. OIDN is Apache-2.0, but its GPU devices are SYCL, CUDA, HIP
  and Metal, with no Vulkan compute device (§11), so it cannot run inside the
  Vulkan frame without a second compute runtime.
- **NDN-NG-005.** Hosted training, frame upload or telemetry. ROADMAP's
  anti-scope excludes hosted services and telemetry
  ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
- **NDN-NG-006.** Per-game networks or a game identifier as a network input.
  Per-game behaviour stops at the translation boundary (CLAUDE.md,
  Architecture Invariants; R5).
- **NDN-NG-007.** Training at runtime inside the engine process. Training is
  an offline tool (NDN-DATA-006).

---

## 5. Requirements by area

### 5.1 The pass (`NDN-PASS`)

- **NDN-PASS-001.** The neural stage MUST read only the feature buffers the
  à-trous stage reads today and MUST write the image composite samples
  through `SvgfPipeline::indirect_view`. Today `indirect_view` returns the
  final à-trous ping-pong slot (`svgf.rs`, at HEAD on 2026-11-01).
- **NDN-PASS-002.** The neural stage SHOULD add no G-buffer attachment; a
  feature channel it does add MUST be listed in the owning design with its
  ablation score on the §7.2 A4 scenario.
- **NDN-PASS-003.** The neural stage MUST run at the render extent, in the
  position the à-trous stage occupies: after SVGF temporal, before
  composite, ahead of TAA or FSR (the
  [FSR target frame graph](../engine/fsr3-upscaler-integration-plan.md#12-target-frame-graph)).
- **NDN-PASS-004.** A console command `r.denoiser [atrous|neural]` MUST read
  and switch the spatial stage at runtime, following the `r.shadowfilter`
  pattern (`ShadowFilterCommand`, `byroredux/src/commands/world_info.rs`),
  and MUST report `unavailable` with the reason when the neural stage could
  not be created.
- **NDN-PASS-005.** `DBG_DISABLE_ATROUS` MUST change only the à-trous
  stage (its iterations become copies) and leave neural-stage output
  unchanged; a separate debug bit MAY turn the neural stage into a copy for
  the same A/B.
- **NDN-PASS-006.** The engine MUST log, once at startup, which spatial
  stage is active and, when it is the à-trous stage, which of NDN-INF-003's
  conditions selected it.
- **NDN-PASS-007.** A neural-stage creation or dispatch failure MUST latch
  to the à-trous stage for the rest of the session, as the SVGF and TAA
  failure latches do today (`svgf_failed`, declared in
  `crates/renderer/src/vulkan/context/mod.rs`, set in
  `build_and_upload_instances.rs`); the latched state MUST be what
  `r.denoiser` reports.
- **NDN-PASS-008.** The neural stage's screen-sized images MUST be added to
  the SVGF ledger (`SVGF_BYTES_PER_PIXEL`, `svgf.rs`) and to
  [Memory Budget § SVGF](../engine/memory-budget.md#svgf-indirect-lighting-denoiser)
  in the commit that allocates them.
- **NDN-PASS-009.** On the frame a neural-stage failure latches, composite
  MUST sample an image the à-trous stage wrote that frame, never a stale or
  unwritten neural output (split from NDN-PASS-007, rev 2).

### 5.2 Inference backend (`NDN-INF`)

- **NDN-INF-001.** The neural stage MUST be GLSL compute compiled by
  `glslangValidator` to committed SPIR-V like every other shader, and MUST
  pass `scripts/check-shader-artifacts.sh`.
- **NDN-INF-002.** On devices that expose `VK_KHR_cooperative_matrix` with
  the feature `cooperativeMatrix` and a supported FP16 (M, N, K) shape the
  network can use, the neural stage MUST use cooperative-matrix operations
  for its matrix multiplies. The supported shapes are enumerated per device
  with `vkGetPhysicalDeviceCooperativeMatrixPropertiesKHR` (§11). Today no
  cooperative-matrix extension is requested (`device.rs`, at HEAD on
  2026-11-01).
- **NDN-INF-003.** The à-trous stage MUST be selected when any of these
  holds: the extension or feature is absent; no supported shape matches the
  network; `shaderFloat16` is unavailable; the weights file is missing or
  rejected (NDN-NET-003); the user chose `atrous`. The fallback path is a
  decision (Q3).
- **NDN-INF-004.** Whether or not `VK_KHR_cooperative_matrix` is present,
  device creation MUST NOT fail on its account and every existing feature
  MUST keep the enabled state it has today; the extension is optional, like the FidelityFX optional set
  (`FFX_OPTIONAL_EXTENSIONS`) in `device.rs`.
- **NDN-INF-005.** **Phase 0: verify** which (M, N, K, component type)
  combinations each device class the project targets reports. Observed on
  the development machine on 2026-11-01 with `vulkaninfo`: the RTX 4070 Ti
  (driver 580.178.04) and llvmpipe (Mesa 26.2.4) list
  `VK_KHR_cooperative_matrix` revision 2; the RADV Raphael integrated GPU on
  the same machine does not. Phase 0 MUST settle it with a checked-in table
  of `VkCooperativeMatrixPropertiesKHR` dumps per device class.
- **NDN-INF-006.** **Phase 0: verify** that the installed
  `glslangValidator` (16.4.0 on the development machine) compiles a
  `GL_KHR_cooperative_matrix` fixture to SPIR-V that passes `spirv-val` and
  that the engine's SPIR-V reflection (`crates/renderer/src/vulkan/reflect.rs`)
  accepts, **under the exact invocation `scripts/check-shader-artifacts.sh`
  uses for every shader** (`glslangValidator -V -I<shader_root>`, no
  `--target-env`). The GLSL extension specifies its mapping against
  SPIR-V 1.5 (§11), so whether plain `-V` emits a valid module, or the
  script needs a per-shader target environment, is the open point. Phase 0
  MUST settle it by compiling a standalone fixture outside the engine.
- **NDN-INF-007.** For a fixed device, driver, weights file and input, two
  runs of the §7.2 A4 Cornell capture MUST produce neural-stage output that
  is byte-identical or differs by no more than the tolerance the owning
  design states from a measurement; the golden tests MUST use that
  tolerance.

### 5.3 Network and weights (`NDN-NET`)

- **NDN-NET-001.** The shipped weights file MUST be at most 2 MiB (ROADMAP
  M54 row: "~1–2 MB weights").
- **NDN-NET-002.** Exactly one shipped weights file MUST serve every game;
  the network MUST take no game identifier or per-game constant as input
  (NDN-NG-006).
- **NDN-NET-003.** The weights file MUST carry a magic, a format version and
  a topology identifier, and the loader MUST reject a file whose version or
  topology does not match the compiled shader, with a log line naming both.
  Covered by new CPU tests `neural_weights_reject_version_mismatch` and
  `neural_weights_reject_topology_mismatch` in `byroredux-renderer`.
- **NDN-NET-004.** The weights file MUST carry a provenance manifest (§2)
  that lists every training scene with its data class (§3.3), its licence,
  the target tier used (§3.2), the training-tool commit and the seed.
- **NDN-NET-005.** A CPU test MUST fail if the shipped weights' provenance
  manifest names a scene whose data class is not "redistributable scene
  capture", unless Q1 is decided for alternative A.
- **NDN-NET-006.** The engine MUST load a user-supplied weights file from a
  path given on the command line or in settings, validated by NDN-NET-003,
  and MUST report in `r.denoiser` which file is active and its manifest's
  training-scene count.
- **NDN-NET-007.** The network topology MUST be described in the owning
  design with its layer list, parameter count and the published method it
  derives from (Chaitanya 2017, Işık 2021 or another cited source).

### 5.4 Training data and ground truth (`NDN-DATA`)

- **NDN-DATA-001.** The engine MUST provide a capture mode that writes, for
  one frame, every feature buffer the neural stage reads, the spatial
  stage's output and the target image, as linear floating-point files at
  the render extent, in the file format of PTR-OUT-001 (32-bit float
  OpenEXR; its writer route is the sibling PRD's verify item V5), so M51
  and M54 share one capture writer. OIDN's training toolkit also reads
  OpenEXR (§11). Today screenshots are tone-mapped 8-bit
  PNGs (`--screenshot`, `byroredux/src/boot/mod.rs`), and the only
  attachment readback is `depth.stats` (`DepthStatsCommand`,
  `byroredux/src/commands/depth.rs`), which reports statistics and writes
  no file; no feature-buffer export exists, at HEAD on 2026-11-01.
- **NDN-DATA-002.** Every capture MUST record the commit, render extent,
  bench mode, camera pose, frame index, SVGF history age, ray quality tier,
  upscaler, target tier and scene identity, in the shape of
  `run-metadata.txt` and `manifest.tsv` from
  [Renderer Evaluation](../engine/renderer-evaluation.md#quick-start), the
  shape PTR-OUT-002 also extends.
- **NDN-DATA-003.** Captures MUST be deterministic under
  `--bench-mode renderer-static`: two captures of the same case MUST hash
  identically on the same device, or differ within NDN-INF-007's tolerance
  ([determinism contract](../engine/renderer-evaluation.md#determinism-contract)).
- **NDN-DATA-004.** A procedurally generated redistributable scene set
  MUST exist beside the Cornell scene, generated from engine-owned
  primitives and a seed, covering at least: thin geometry, high-contrast
  emitters, coloured inter-reflection, alpha-tested cut-outs and large flat
  walls (the Dugout Inn stain case named in `svgf.rs`).
- **NDN-DATA-005.** Captures of local scenes MUST be written only under a
  user-chosen directory and MUST NOT be written into the repository tree;
  engine and tool code MUST NOT transmit them anywhere (NDN-NG-005).
- **NDN-DATA-006.** Training MUST be an offline tool run by the user,
  outside the engine process, that reads captures and writes a weights file
  with its provenance manifest. Its framework is a decision (Q2).
- **NDN-DATA-007.** The training tool MUST reproduce a shipped weights file
  bit-exactly, or within a stated tolerance on the §7.2 scores, from the
  checked-in redistributable captures, the manifest's tool commit and seed.
- **NDN-DATA-008.** **Phase 0: verify** whether weights trained on captures
  of an installed Bethesda game may be redistributed. The owner's reading of
  each title's end-user licence agreement MUST be recorded in Q1 before any
  local-scene capture trains a weights file that ships. This document makes
  no claim either way.
- **NDN-DATA-009.** The target half of an R0 training pair MUST be captured
  only on a frame where SVGF temporal reports progressive accumulation with
  the history age at its 255 cap; the input half MUST be captured at the
  same pose with the history age the manifest records (NDN-DATA-002), so
  a pair maps a partially accumulated input to the converged R0 value.
- **NDN-DATA-010.** **Phase 0: verify** that R1's demodulation (indirect
  split ÷ first-non-specular-vertex albedo, PTR-OUT-003) and the G-buffer
  albedo the spatial stage demodulates by (`ALBEDO_FORMAT`, `gbuffer.rs`)
  are the same quantity on the A4 scene, so an R1 target and a neural-stage
  output are comparable. Settled by a capture of both on Cornell once the
  sibling PRD's Phase 1 lands; a weights file MUST NOT be trained on R1
  before it is settled.
- **NDN-DATA-011.** Every capture MUST be taken with
  `--rt-test-ray-quality-tier` pinned, so the adaptive budget cannot change
  the input noise between the two halves of a pair; the provenance
  manifest (NDN-NET-004) lists the pair count per tier.

### 5.5 Temporal and upscaler interaction (`NDN-TMP`)

- **NDN-TMP-001.** The neural stage MUST NOT feed SVGF temporal history.
  Today the à-trous output is a post-filter and the temporal history
  feedback is untouched (`svgf_atrous.comp` header comment, at HEAD on
  2026-11-01).
- **NDN-TMP-002.** The neural stage MUST be stateless across frames, so a
  camera cut, cell transition, resize or `signal_temporal_discontinuity`
  (`crates/renderer/src/vulkan/context/mod.rs`) needs no reset for it.
- **NDN-TMP-003.** Switching the spatial stage with `r.denoiser` MUST NOT
  call `signal_temporal_discontinuity` or reset SVGF, TAA or FSR history.
- **NDN-TMP-004.** Under the default FSR 3.1 Quality upscaler and under TAA,
  the §7.2 moving-path scenario MUST score no worse with the neural stage
  than with the à-trous stage, measured after the upscaler.
- **NDN-TMP-005.** On a parked camera under `renderer-static`, the mean
  absolute linear delta between consecutive captured frames with the
  neural stage MUST NOT exceed the à-trous stage's.
- **NDN-TMP-006.** A change to TAA's `γ` or the RT seed scheme recorded in
  [Shadow Pipeline Trade-offs](../engine/shadow-pipeline-tradeoffs.md#audit-hook),
  to the GI estimator (for instance the multi-bounce GI the
  [Reference-Quality Lighting PRD](reference-lighting.md) promises), or to
  the ray quality tiers SHOULD re-run the §7.2 scores, because each changes
  the noise the network was trained on.

### 5.6 Measurement (`NDN-MEAS`)

- **NDN-MEAS-001.** A GPU timer bracket MUST isolate spatial-stage time, and
  the `bench:` line MUST print it as its own `gpu_<key>=` token through
  `BENCH_GPU_KEYS` (`byroredux/src/main.rs`). Today `gpu_svgf` covers SVGF
  temporal, the à-trous stage and the shadow filter, and the nested
  `gpu_shadow_filter` isolates only the shadow filter, so no token isolates
  the spatial stage (§3.1 row 8, at HEAD on 2026-11-01).
- **NDN-MEAS-002.** The bench line or a log line MUST name the active
  spatial stage, so a bench row cannot be read without knowing which stage
  it timed.
- **NDN-MEAS-003.** A new ignored test `denoiser_quality` in `byroredux/tests/`
  MUST score the à-trous stage's and the neural stage's presented frames
  against the same presented target capture with `compare_linear`
  (`byroredux/tests/image_metrics/mod.rs`) on the four camera paths
  `upscaler_quality.rs` uses, and print the table.
- **NDN-MEAS-004.** `scripts/renderer-eval.sh` MUST add a neural-stage
  capture beside `cornell_f64_no_atrous.png` at the final frame count.
- **NDN-MEAS-005.** Runtime claims MUST follow ROADMAP's bench methodology:
  a same-machine control, pinned `--camera-pos` / `--camera-forward`, an
  isolated `BYROREDUX_SETTINGS_PATH`, a pinned ray quality tier, and the
  same render extent for both stages
  ([How to run a bench](../../ROADMAP.md#how-to-run-a-bench)); a number
  added to the bench-of-record table MUST use that table's invocation shape.
- **NDN-MEAS-006.** **Phase 0: verify** the à-trous stage's cost on the bench
  scenes before the dedicated bracket exists, two ways on one `--bench-hold`
  run: `gpu_svgf − gpu_shadow_filter` (temporal plus à-trous, an upper
  bound), and the `gpu_svgf` difference between `r.dbg -0x4000` and
  `r.dbg +0x4000` (filter cost minus copy cost, a lower bound). Phase 0
  MUST publish both as a row under ROADMAP's bench methodology; a budget
  derived from the lower bound is conservative.
- **NDN-MEAS-007.** **Phase 0: verify** that the published inference cost
  of the candidate topology fits NDN-G-002's budget, from the inference
  timings Işık 2021 and Chaitanya 2017 (§11) report, scaled to the render
  extent and GPU class. If no published topology fits, Q4 MUST be decided
  before Phase 1 starts.
- **NDN-MEAS-008.** `denoiser_quality` MUST also report, per stage, the
  relMSE (sibling PRD §2, with its ε recorded) of the floating-point
  spatial-stage output capture (NDN-DATA-001) against the target image, so
  M54 and M51 publish estimator error in one metric.

---

## 6. Deliberate deviations and risks

M54 changes no original-engine behaviour the renderer reproduces; its
departures are from the renderer's current à-trous stage.

| # | Today (à-trous stage) | M54 behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Hand-tuned edge-stopping constants (`SIGMA_N`, `SIGMA_L`, `SIGMA_A`, `svgf_atrous.comp`) | Learned weights | Re-tuning the σ constants per artefact | Learned filters can invent or erase detail a fixed kernel cannot → scored against a reference, not against SVGF (NDN-G-001, NDN-MEAS-003) |
| 2 | Same image on every device | Neural on devices with cooperative matrices, à-trous elsewhere | — | Two looks across hardware; bug reports become device-dependent → active stage is logged and benched (NDN-PASS-006, NDN-MEAS-002) |
| 3 | No training data | Weights trained on redistributable scenes | — | Domain gap: Cornell and procedural scenes may not cover game content → report-only game-content scores (§7.2 A4) and local fine-tuning (Q1) |
| 4 | Filter tracks shader edits automatically | Weights trained against one noise distribution | — | A change to the GI estimator, TAA `γ` or seeds silently invalidates the training → NDN-TMP-006, provenance commit (NDN-NET-004) |
| 5 | R0 reference converges the current estimator | R1 needs M51 | — | Training on R0 bakes current bias into the target → R1 required for the exit (Q5, NDN-G-006) |
| 6 | No ML artefact in the repository | A binary weights file ships | — | Provenance and licence of training data → NDN-NET-004, NDN-NET-005, Q1 |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

The neural stage is a renderer pass with no per-game code; every title
reaches it through the same G-buffer. Per-game scope is about where
acceptance is measured, following the engine tiers in
[Game Compatibility](../engine/game-compatibility.md#per-game-support-detail).

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | Yes, measured later | Same pass, no per-game code. Tier 3 in game-compatibility.md; scored once its cells are part of the renderer-eval set. |
| Fallout 3 | Yes | Tier 1. Report-only game-content score, wave 2. |
| Fallout: New Vegas | Yes | Tier 1. `scripts/renderer-eval-fnv.sh` Prospector probe exists; report-only score, wave 1. |
| Skyrim (LE / SE / AE) | Yes | Tier 1 (SE). Report-only score, wave 1. |
| Fallout 4 | Yes | Tier 1. Dugout Inn is the stain case the à-trous kernel was cut for (`svgf.rs`); report-only score, wave 1. |
| Fallout 76 | Yes, measured later | Same pass. Tier 3; scored once a cell renders. |
| Starfield | Yes | Tier 2. Report-only score, wave 2. |

Wave 1 is the content that already has renderer-eval probes or a named
denoiser artefact; wave 2 follows when those titles have a pinned
bench pose. No title gets a per-game weights file (NDN-NET-002).

### 7.2 Acceptance

Game data is the user's local install; nothing game-derived is written
into the repository (NDN-DATA-005).

| # | Scenario | Command | Data | Pass rule |
|---|---|---|---|---|
| A1 | Weights validation | `cargo test -p byroredux-renderer --lib neural_weights` (new) | None | NDN-NET-003 rejections and NDN-NET-005 provenance check pass |
| A2 | Shader contract | `cargo test -p byroredux-renderer --lib name_diverging_glsl_rust_mirrors_stay_in_lockstep` (existing, extended to the neural push block) and `scripts/check-shader-artifacts.sh` | None | Both pass |
| A3 | Fallback selection | `cargo test -p byroredux-renderer --lib spatial_stage_selection` (new): each NDN-INF-003 condition selects à-trous | None | All conditions covered |
| A4 | Cornell quality | `cargo test --release -p byroredux --test denoiser_quality -- --ignored --nocapture` (new, NDN-MEAS-003) | None (engine-owned scene) | NDN-G-001 on all four paths (relMSE per NDN-MEAS-008, presented-frame SSIM / p99 per NDN-MEAS-003), under TAA and FSR Quality (NDN-TMP-004) |
| A5 | Game-content quality | Same test with `BYROREDUX_QUALITY_GAME` (the pattern of `fsr_presets_on_game_content_report_only`) | Local FNV, Skyrim SE, FO4 installs | Report-only table; regressions are filed, not gated |
| A6 | Runtime | `cargo run --release -- --game <profile> --cell <cell> --bench-frames 300 --bench-mode renderer-static --rt-test-ray-quality-tier <n> --camera-pos … --camera-forward …` with an isolated `BYROREDUX_SETTINGS_PATH`, once per spatial stage, plus the same-machine control (NDN-MEAS-005) | Local FNV, Skyrim SE, FO4 installs; Cornell | NDN-G-002 on the new spatial-stage token (NDN-MEAS-001) |
| A7 | Live A/B | `BYRO_DEBUG_SERVER=1 cargo run --release -- --cornell --bench-frames 60 --bench-hold`, then `byro-dbg`: `r.denoiser atrous`, `screenshot a.png`, `r.denoiser neural`, `screenshot n.png`, `r.denoiser` | None | Both screenshots exist; the last command names the active file and stage (NDN-PASS-004, NDN-NET-006) |
| A8 | Stability | `scripts/renderer-eval.sh` with `BYROREDUX_RENDER_EVAL_FRAMES="63 64"`, per stage | None | NDN-TMP-005 |
| A9 | PT multiplier | The sibling PRD's A10 (`scripts/renderer-eval.sh` interactive case), once per spatial stage | RT device; as that scenario names | NDN-G-006: PTR-VAL-005 relMSE lower with the neural stage |

---

## 8. Metrics

The numbers live in ROADMAP: spatial-stage times in the Bench-of-record
table ([How to run a bench](../../ROADMAP.md#how-to-run-a-bench)) as a new
column, quality scores in the M54 row of
[Tier 8](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon).

| Metric | Definition | Granularity |
|---|---|---|
| Spatial-stage time ratio | neural spatial-stage ms ÷ à-trous spatial-stage ms, same machine, scene, pose, extent | Per bench scene, per device class |
| Estimator error delta | neural relMSE − à-trous relMSE of the spatial-stage output against the same target image (NDN-MEAS-008) | Per scene, per camera path, per target tier |
| Presented quality delta | neural linear SSIM − à-trous linear SSIM of the presented frame against the same presented target; also p99 delta (NDN-MEAS-003) | Per scene, per camera path, per upscaler, per target tier |
| Temporal instability | mean absolute linear delta between consecutive parked-camera frames | Per scene, per stage |
| Neural coverage | device classes on which the neural stage is selected ÷ device classes in the NDN-INF-005 table | Per release |
| Weights size | bytes of the shipped weights file | Per weights release |
| Training corpus | redistributable training pairs in the provenance manifest | Per weights release |

---

## 9. Delivery sequence

M54 depends on M37 (SVGF, shipped) and M51 (path-tracing reference mode,
unbuilt) per its ROADMAP row, and ROADMAP's Tier 8 note has M54 wait
"until there is enough scene data to train on". Phases 1–3 need only M37
and train against R0, which is the sequencing Q5 asks the owner to
approve. Phase 4 needs the sibling PRD's Phase 2 exit (a trusted
progressive reference, R1) and, for A9, its Phase 5 exit (the
interactive PT mode).

**Phase 0 — Survey, design and decisions (no engine code).** Write
`docs/engine/neural-denoiser.md` (topology, weights format, capture format,
bracket, fallback). Close the **Phase 0: verify** items: device shape table
(NDN-INF-005), shader-toolchain fixture (NDN-INF-006), redistribution
reading (NDN-DATA-008), à-trous cost bounds (NDN-MEAS-006), published
inference cost (NDN-MEAS-007); NDN-DATA-010 is filed as an issue blocked
on the sibling PRD's Phase 1. Resolve Q1–Q6. Exit: the design is checked
in, the device table and the cost-bound row are published, and every §10
item carries **Decided**.

**Phase 1 — Measurement and capture.** The spatial-stage bracket and token
(NDN-MEAS-001, NDN-MEAS-002); the capture mode (NDN-DATA-001 to
NDN-DATA-003, NDN-DATA-009, NDN-DATA-011); the procedural scene set
(NDN-DATA-004); the `denoiser_quality` harness scoring the à-trous stage
against R0 (NDN-MEAS-003, NDN-MEAS-008). Exit: A4 runs and prints the à-trous baseline table, and
the à-trous spatial-stage time is published for every wave-1 bench scene.

**Phase 2 — Training tool and first weights.** The offline training tool
(NDN-DATA-006, NDN-DATA-007) and a first weights file trained on
redistributable R0 pairs with its manifest (NDN-NET-001, NDN-NET-004,
NDN-NET-007). Exit: the tool reproduces the weights from checked-in
captures within NDN-DATA-007's tolerance, and an offline evaluation of the
weights on held-out Cornell paths meets NDN-G-001 against R0.

**Phase 3 — Inference pass and fallback.** The neural stage
(NDN-PASS-001 to NDN-PASS-009), the cooperative-matrix path and fallback
(NDN-INF-001 to NDN-INF-004, NDN-INF-007), weights loading
(NDN-NET-002, NDN-NET-003, NDN-NET-005, NDN-NET-006), temporal rules
(NDN-TMP-001 to NDN-TMP-005). Default stays à-trous (Q6). Exit: A1–A4 and
A6–A8 pass on the development GPU, and the spatial-stage ratio is published
for every wave-1 scene.

**Phase 4 — Path-tracing reference and default.** Close NDN-DATA-010 and
retrain on R1 once the sibling PRD's progressive reference is trusted
(Q5); score A9 (NDN-G-006) once its interactive PT mode exists; run A5 on
wave 1 and wave 2 (report-only); switch the default per Q6. Exit:
NDN-G-001 holds against R1 on A4, NDN-G-002 holds on every bench scene,
A9 passes, and the ROADMAP M54 row carries the published metrics.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — What the shipped weights may be trained on.** Recommended: ship
   only weights trained on redistributable scenes (Cornell plus the
   procedural set), and let a user fine-tune locally on captures of their
   own install with weights that never leave the machine (NDN-DATA-005,
   NDN-NET-006). This keeps the keep-list stance "parse data, don't emulate
   engine … no copyright issues"
   ([keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)).
   Alternative A: ship weights trained on game captures; likely better on
   game content, but it puts a derivative of game assets in the repository,
   which needs the NDN-DATA-008 reading first and changes NDN-NET-005.
   Alternative B: ship no weights, local training only; no licence question,
   but the feature is off for everyone who does not run the training tool.
2. **Q2 — Training framework.** Recommended: an offline Python tool on
   PyTorch outside the cargo workspace, the shape OIDN's training toolkit
   uses (PyTorch 2.4+, EXR inputs, exported weights; §11), with the engine
   reading only the exported weights file. The repository already runs
   Python tooling (`scripts/*.py`). Alternative A: a Rust training crate in
   the workspace; one language, but a large new dependency tree in the
   lock file and in every workspace-wide build or audit unless it is kept
   out of the workspace members. Alternative B: training in Vulkan
   compute inside the engine; no new toolchain, but NDN-NG-007 is reversed
   and the engine gains an optimiser, a loss and a data loader.
3. **Q3 — Fallback without cooperative matrices.** Recommended: fall back to
   the à-trous stage; one neural shader to maintain, and the fallback is the
   tested control. Alternative: a second neural path on plain FP16
   subgroup arithmetic; neural output on more devices (the RADV iGPU in
   NDN-INF-005), but a second shader to keep in parity, and likely slower
   than the à-trous stage there, which fails NDN-G-002 on those devices.
4. **Q4 — What the "30 % runtime" target binds.** Recommended: it binds in
   raster mode as NDN-G-002 (spatial-stage time ≤ 70 % of à-trous, same
   machine and extent); in M51's interactive PT mode only quality binds
   (NDN-G-006). Alternative A: read "30 % runtime" as the neural stage
   costing at most 30 % of à-trous time; a stricter budget that rules out
   most published topologies before NDN-MEAS-007 has measured them, and
   NDN-G-002 changes to 30 %. Alternative B: quality-first everywhere,
   shipping a neural stage that is better but slower; a change to the
   ROADMAP M54 promise (trigger T1), and the default in raster mode stays
   à-trous.
5. **Q5 — Ground truth before M51.** Recommended: train and score on R0 for
   Phases 1–3 and require R1 for the Phase 4 exit; work starts now and the
   shipped claim is against path-traced truth. This changes ROADMAP's
   sequencing (the M54 row's M51 dependency and the Tier 8 note that M54
   "waits until there is enough scene data to train on"), so approving it
   means editing those lines. It also assumes the sibling PRD's Q1 keeps
   the progressive reference; if that PRD ships the interactive mode only,
   R1 does not exist and this PRD needs a revision. Alternative A: wait
   for M51 before any M54 work; ROADMAP stays as written, no throw-away
   weights, no progress until M51 ships. Alternative B: a dedicated
   long-accumulation mode without the 255 history-age cap; a better
   interim target, but it duplicates the progressive reference's
   accumulation and needs its own design. Alternative C: use the external
   converged Cornell reference of the
   [Reference-Quality Lighting PRD](reference-lighting.md) (its RLT-REF
   area) as an interim R1 on Cornell only; unbiased earlier, but it is
   produced outside the engine and, under that PRD's RLT-REF-003, may not
   be redistributable as training data for shipped weights.
6. **Q6 — Default stage.** Recommended: à-trous stays the default until
   Phase 4's exit holds, then the neural stage becomes the default on
   devices that select it, as FSR became the default only after its
   benchmark phase ([FSR plan § Execution phase 7](../engine/fsr3-upscaler-integration-plan.md#execution-phase-7--benchmark-default-switch-and-documentation--complete-2026-07-24)).
   Alternative: opt-in permanently; no default-change risk, and most users
   never see M54.

---

## 11. Document map

| Need | Go to |
|---|---|
| The stage being replaced and its inputs | [Vulkan Renderer § SVGF](../engine/renderer.md#svgf-temporal-denoiser), `crates/renderer/src/vulkan/svgf.rs`, `crates/renderer/shaders/svgf_atrous.comp` |
| Pass order and G-buffer layout | [Shader Pipeline § Per-Frame Submission Order](../engine/shader-pipeline.md#per-frame-submission-order), [§ G-Buffer Layout](../engine/shader-pipeline.md#g-buffer-layout) |
| A/B capture workflow and determinism | [Renderer Evaluation](../engine/renderer-evaluation.md#how-to-evaluate-a-renderer-change) |
| Reference laboratory and temporal reconstruction | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md#validation-gates) |
| Noise sources the network is trained on | [Shadow Pipeline Trade-offs](../engine/shadow-pipeline-tradeoffs.md) |
| TAA and FSR history rules | [FSR 3.1 Integration Plan](../engine/fsr3-upscaler-integration-plan.md#15-temporal-history-rules), [Vulkan Renderer § TAA](../engine/renderer.md#taa-m375) |
| VRAM ledger | [Memory Budget](../engine/memory-budget.md#svgf-indirect-lighting-denoiser) |
| Engine tiers per game | [Game Compatibility](../engine/game-compatibility.md#per-game-support-detail) |
| Bench methodology and published numbers | [ROADMAP § How to run a bench](../../ROADMAP.md#how-to-run-a-bench) |
| Milestone row, dependencies, anti-scope | [ROADMAP Tier 8](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon), [anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |
| Ground truth (R1), interactive PT mode, relMSE, capture file format | [Path Tracing Reference Mode — PRD](path-tracing-reference-mode.md) |
| GI estimator changes and the external converged reference | [Reference-Quality Lighting — PRD](reference-lighting.md) |
| Ray quality tier | `crates/renderer/src/vulkan/scene_buffer/ray_budget.rs`, `--rt-test-ray-quality-tier` in `byroredux/src/cli_args.rs` |
| SVGF (the filter being replaced) | Schied et al., "Spatiotemporal Variance-Guided Filtering: Real-Time Reconstruction for Path-Traced Global Illumination", HPG 2017, <https://doi.org/10.1145/3105762.3105770> |
| Recurrent learned reconstruction | Chaitanya, Kaplanyan, Schied, Salvi, Lefohn, Nowrouzezahrai, Aila, "Interactive Reconstruction of Monte Carlo Image Sequences using a Recurrent Denoising Autoencoder", ACM TOG 36(4):98, 2017, <https://doi.org/10.1145/3072959.3073601>, <https://research.nvidia.com/publication/2017-07_interactive-reconstruction-monte-carlo-image-sequences-using-recurrent> |
| Affinity-based learned kernels | Işık, Mullia, Fisher, Eisenmann, Gharbi, "Interactive Monte Carlo Denoising using Affinity of Neural Features", ACM TOG 40(4):37, 2021, <https://doi.org/10.1145/3450626.3459793> |
| Cooperative matrices | Khronos, `VK_KHR_cooperative_matrix` (extension 507, ratified, revision 2), <https://docs.vulkan.org/refpages/latest/refpages/source/VK_KHR_cooperative_matrix.html>; GLSL extension `GLSL_KHR_cooperative_matrix` (GLSL 4.50+, mapped against SPIR-V 1.5), <https://github.com/KhronosGroup/GLSL/blob/main/extensions/khr/GLSL_KHR_cooperative_matrix.txt>; feature struct `VkPhysicalDeviceCooperativeMatrixFeaturesKHR` (member `cooperativeMatrix`), <https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceCooperativeMatrixFeaturesKHR.html> |
| Intel Open Image Denoise (licence, devices, training toolkit) | <https://github.com/RenderKit/oidn> (Apache-2.0; SYCL, CUDA, HIP, Metal devices; PyTorch training toolkit exporting `.tza` weights) |
| NVIDIA NRD (licence) | <https://github.com/NVIDIA-RTX/NRD>, licence text <https://raw.githubusercontent.com/NVIDIA-RTX/NRD/master/LICENSE.txt> (NVIDIA RTX SDKs License) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: ground truth aligned with the M51 PRD (R1 = progressive reference at its Phase 2 exit, NDN-G-006 on the interactive PT mode via PTR-RT-003, relMSE as NDN-MEAS-008, OpenEXR capture format), "reference image" renamed "target image", nested `gpu_shadow_filter` bracket (§3.1 row 8, NDN-MEAS-001, NDN-MEAS-006), ray quality tier pinned (§3.1 row 11, NDN-DATA-011), check-shader-artifacts plain `-V` (NDN-INF-006), R0 pair halves (NDN-DATA-009), R1 demodulation verify (NDN-DATA-010), NDN-PASS-009 split from NDN-PASS-007, RFC 2119 wording (NDN-INF-004, NDN-INF-007, NDN-DATA-005, NDN-TMP-005), Q4 alternative reading, Q5 ROADMAP sequencing and alternative C, sibling PRD links | claude: revise |
