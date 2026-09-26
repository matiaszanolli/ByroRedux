# Opaque early-depth experiment — rejected by the visual gate

**Follow-up:** [ReSTIR light-index remapping and the corrected optimization](PERFORMANCE_RESTIR_LIGHT_HISTORY_2026-09-26.md). This report preserves the earlier failed experiment; its restored-source disposition describes that earlier stage.

**Base HEAD:** `b7491072f47aebf67fc35e76e6cfeffedd5831bf`. **Follow-up to:** [dense-scene investigation](AUDIT_PERFORMANCE_2026-09-26b.md). **Outcome:** measurable GPU savings, but a reproducible lighting-history artifact prevents enabling the optimization. Production code and the release executable have been restored to the base revision. This report does not claim a shipped speedup.

The complete source prototype, including tests and an optional narrow depth-prepass diagnostic, is preserved in [opaque_early_tests_2026-09-26.patch](experiments/opaque_early_tests_2026-09-26.patch). It applies cleanly to the recorded HEAD. The extra SPIR-V artifact is regenerated from the shared shader source; instructions are below. No GitHub issues, commits or pushes were made.

## What was implemented

The prototype compiles a separate main fragment module with `layout(early_fragment_tests) in;`. It admits only default lit material kind 0, no alpha blending, zero alpha-test threshold, enabled depth test/write, authored LESS or LESSEQUAL comparison, filled polygons, and no decal classification. Eligibility is evaluated from each current draw, including animated state. Unknown material kinds, effects, glass, cutouts and wireframe retain the original shader.

An explicit pipeline-key axis prevents incompatible instances/batches from merging. Opaque sort slot 6 groups that axis while preserving the existing state/mesh/depth ordering; this is not a front-to-back sort experiment. Initialization, render-pass recreation and teardown own the added pipeline. Shader-module and partial-pipeline error cleanup were included.

For the shader-only A/B, `BYRO_DISABLE_OPAQUE_EARLY_TESTS=1` replaces the experimental module with the ordinary module while retaining identical classification, sorting and batching. The two SPIR-V modules are byte-identical after removing the EarlyFragmentTests execution-mode instruction. Material calculations and resource interfaces therefore do not differ between them.

## Timing results

RTX 4070 Ti, release Rust 1.96.0 build, serial Xvfb runs, 1280×720 Native AA, pinned ray tier 0, renderer-stepped/pan, 180 frames. GI is already disabled at that tier. Normal captures retain direct shadows, reflection/glass and history reuse. Three runs per scene/variant, with order reversed on repeat two. No separate warmup exclusion; these are short controlled captures, not steady-state FPS guarantees.

Correction established during the follow-up: the GPU fields on `bench:` are **last-completed-frame snapshots**, not capture averages (`byroredux/src/app_events.rs`). The GPU values below are medians of those endpoint snapshots across repeats; wall values are benchmark aggregates. Ranges describe repeat variation, not confidence intervals. The follow-up uses means of every available per-frame GPU readback instead. GPU and host wall intervals must not be added.

| Scene | Late-test main GPU ms | Early-test main GPU ms | Main reduction | Wall ms, late → early |
|---|---:|---:|---:|---:|
| MedTekResearch01 | 66.572 (63.651–71.340) | 45.863 (44.721–48.176) | 31.1% | 101.55 → 82.65 |
| CambridgeKendallHospital01 | 19.320 (18.797–19.424) | 16.787 (15.937–17.046) | 13.1% | 72.42 → 68.50 |

MedTek's wall reduction is 18.6%; Kendall's is 5.4%. The main-pass savings exceed the observed variation, but they fail the image-quality requirement below.

All six MedTek captures share state hash `92b7960742fa16e1`, 39,560 entities, 286 lights, 13,038 TLAS instances and 1,342 batches / 86 GPU calls. All six Kendall captures share `81ddac243a471e2b`, 49,988 entities, 214 lights, 15,589 TLAS instances and 420 batches / 24 calls.

The saved original executable was also measured once per scene with matching launch settings: MedTek 65.419 ms main / 101.79 ms wall, Kendall 19.439 ms main / 72.52 ms wall. It has fewer batches (1,212 and 398 respectively). Its state hashes differ because `byroredux/src/bench.rs` hashes the ordered draw-command sequence, which the new classification changes. These single-run checks include the batching tradeoff; they are not interchangeable with the identical-order shader-only A/B.

A single paired FNV Prospector control measured 11.736 → 7.969 ms main and 36.68 → 32.89 ms wall. This is contextual evidence, not a repeated small-scene performance envelope.

All **29 completed captures**, including diagnostics with different settings, are recorded in [BENCH_opaque_early_tests_2026-09-26.tsv](BENCH_opaque_early_tests_2026-09-26.tsv). Normalization and depth-prepass experiments have separate columns; do not pool their measurements with the twelve normal timing runs. Raw logs, commands, executables and all PNGs remain in `/tmp/opaque-early-2026-09-26/`.

## Why the optimization was rejected

The upper portion of Kendall's central column becomes markedly darker with early tests. It repeats across all three normal captures and is substantially larger than repeat-run variation. This is a visible correctness difference, not something a faster timer can override.

Full-resolution evidence:

- [Late-test baseline](experiments/opaque_early_tests_2026-09-26/kendall-late.png)
- [Early-test result](experiments/opaque_early_tests_2026-09-26/kendall-early.png)
- [Early tests with history reuse disabled](experiments/opaque_early_tests_2026-09-26/kendall-no-reuse.png)

Measurements use normalized display RGB, not physical luminance. The fixed diagnostic region is x=520..629, y=0..179 in the 1280×720 PNG:

| Configuration | Mean RGB in column region |
|---|---:|
| Late tests, normal history | 0.14090 |
| Early tests, normal history | 0.07382 |
| Main history reuse disabled | 0.14138 |
| Early tests, spatial reuse disabled only | 0.08177 |
| Early tests, temporal reuse disabled only | 0.07550 |
| Early tests plus narrow depth prepass | 0.07383 |

The full-frame early/late RGB MAE is 0.00570, with 1.61% of pixels differing by more than 0.1 in at least one channel. Late/late repeat MAE is approximately 0.00064, and the column's mean remains approximately 0.1409. The difference is reproducible and localized.

The following controls narrow the cause:

1. **Coverage:** static `material_lobe` captures are pixel-identical between early and late variants in both MedTek and Kendall. The visible problem is not reproduced as missing/cutout geometry in these views.
2. **History:** at the same moving-camera snapshot, Kendall's early and late screenshots are pixel-identical when `DBG_DISABLE_RESTIR` (`32768`) disables both temporal and spatial reuse. The raw current-frame estimator is retained. This isolates the difference to interaction with history; it does not prove the raw estimator is physically correct in every respect.
3. **Individual reuse paths:** disabling only spatial (`65536`) or only temporal (`262144`) reuse does not remove the dark region. Disabling just one is not an acceptable repair.
4. **Depth prepass:** a vertex-only prepass for the certified early-test batches with LESSEQUAL depth, matching vertex shader, transforms, culling and depth bias, does not remove it. This falsifies the proposed prepass as a sufficient fix. That experiment is optional in the preserved patch and was never accepted as production code. Its diagnostic direct draws were not added to ordinary draw-call telemetry, so its call count is not a total submission count.
5. **Validation:** Vulkan core/synchronization validation reports no VUID or synchronization-hazard messages for the FSR material-view capture, TAA normal capture, or prepass capture. Clean validation does not establish lighting correctness or resolve shader-level history ownership.

Source inspection confirms that overlapping fragment invocations write a per-pixel reservoir SSBO, while reuse reads prior reservoirs. Early rejection changes which invocations can contribute history. **The exact cause of the darkening is not established.** Reservoir ownership, history payload consistency and reuse weights are further investigation targets; no individual race, reprojection error or weight formula is claimed as the proven cause.

## Separate normalization experiment, not retained

The initial light loop enumerates candidate lights, sums target weights, increments M for each light and later divides by M. For a categorical draw with q(i)=w(i)/sum(w), the inverse proposal weight is sum(w)/w(i). Counting the enumerated list again in the denominator introduces a light-count factor. This is a separate source-level estimator concern; the general importance-weight/reservoir context is described by [Bitterli et al., ReSTIR](https://research.nvidia.com/publication/2020-07_spatiotemporal-reservoir-resampling-real-time-ray-tracing-dynamic-direct).

A focused experiment treated the enumerated proposal as one initial sample. It passed an added analytic total-energy test, but **did not repair the early-test artifact**: the Kendall region measured 0.19961 late, 0.07490 early and 0.20113 with reuse disabled. It changed global lighting and was removed from the working tree and preserved prototype. The three `normalized-*` rows document this falsified repair; they are not final performance or correctness results. A normalization change needs its own transport-oracle review before adoption.

## Verification and disposition

- Original early-test prototype: release build, 1,239 renderer tests passed (one ignored), 209 binary render tests passed (two ignored).
- Reproducible shader compilation with glslang 16.2.0, SPIR-V validation, and an execution-mode/interface equivalence guard. The expanded normalization experiment passed 1,240 renderer tests before its extra test/change was removed.
- Twenty-nine completed GPU captures; normal-color inspection, pixel comparisons and Vulkan validation as described above. These do not constitute exhaustive material, resize or exterior coverage.
- Final production sources match the recorded HEAD. The normal release executable was rebuilt successfully after restoration. The archived patch passes `git apply --check`.

The timing evidence supports continuing work on opaque fragment cost. **Do not enable this prototype until history correctness is established against the raw estimator and a transport reference.** Further tuning of front-to-back order or occlusion should not hide this failed visual gate. No default ray-budget, resolution or image-quality reduction was retained.

## Reproducing the archived prototype

Apply only in a disposable checkout of the recorded revision; the patch intentionally reproduces the failed visual gate:

```sh
git apply docs/audits/experiments/opaque_early_tests_2026-09-26.patch
glslangValidator -V -Icrates/renderer/shaders crates/renderer/shaders/triangle.frag \
  -o crates/renderer/shaders/triangle.frag.spv
glslangValidator -V -Icrates/renderer/shaders -DBYRO_OPAQUE_EARLY_TESTS=1 \
  crates/renderer/shaders/triangle.frag -o crates/renderer/shaders/triangle_early.frag.spv
```

Build with the explicit Rust 1.96.0 toolchain described in AGENTS.md. Run the command below once with `BYRO_DISABLE_OPAQUE_EARLY_TESTS=1` for the late baseline, and once with that variable unset for early tests. Use different screenshot paths. Keep `BYRO_OPAQUE_DEPTH_PREPASS` unset for the principal A/B.

```sh
env -u WAYLAND_DISPLAY -u GDK_BACKEND -u BYROREDUX_FIXED_DT \
  XDG_SESSION_TYPE=x11 RUST_LOG=info BYRO_PROFILE=1 BYROREDUX_RENDER_DEBUG=0 \
  xvfb-run --auto-servernum target/release/byroredux \
  --game fo4 --cell CambridgeKendallHospital01 --window-size 1280x720 \
  --upscaler fsr3 --fsr-quality native-aa --rt-test-ray-quality-tier 0 \
  --bench-frames 180 --bench-mode renderer-stepped --bench-camera pan \
  --screenshot /tmp/opaque-test.png
```

For the isolated history control set `BYROREDUX_RENDER_DEBUG=32768` on both sides. For the prepass diagnostic set `BYRO_OPAQUE_DEPTH_PREPASS=1` on the early side; it also needs independent correctness review. For validation set `BYRO_VALIDATION=1` and keep those runs out of the performance aggregates.
