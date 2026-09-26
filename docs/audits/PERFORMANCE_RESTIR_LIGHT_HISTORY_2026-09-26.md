# ReSTIR light identity and opaque early-depth optimization

Follow-up to [the rejected early-depth prototype](PERFORMANCE_OPAQUE_EARLY_TESTS_2026-09-26.md), based on `b7491072f47aebf67fc35e76e6cfeffedd5831bf`.

## Cause and retained changes

ReSTIR persisted a selected **array index**, while both the app and renderer sorted lights by current intensity × range. Animation changed that order every frame. Instrumented Kendall Hospital captures found roughly 190–200 of 214 slots changing position identity between successive frames. The shader combined an old reservoir's weight with a different current light. Both temporal and spatial reuse made this mistake. Early depth rejection exposed the error as a coherent dark region on the central column.

Authored emitters now carry their ECS identity through collection and sorting. The directional source has a separate identity namespace. The renderer constructs a previous-frame-index → current-frame-index table **after** sorting and the upload cap; both reuse paths translate through it before reading current light data. Missing, duplicate, or unspecified identities reject selection reuse. Transported combustion samples currently have no persistent emitter identity and remain eligible for fresh sampling.

The mapping uses the same previous frame-in-flight slot as the reservoir buffer. Upload dirty hashes include it: an unchanged current light array can still need a different mapping. CPU scratch storage is reused. `GpuLight` grows from 64 to 80 bytes; all four GLSL mirrors and their compiled consumers are updated. The light SSBO header is 4112 bytes, with the mapping at offset 16 and lights beginning at a 16-byte-aligned offset.

Each current reservoir buffer is cleared before rendering, with explicit transfer/fragment barriers. Unwritten pixels therefore cannot retain selections from an older frame to which the one-frame mapping no longer applies. At 1280×720 this clears 29.49 MB per frame; the performance captures include that cost in wall time.

The retained early-test pipeline accepts only filled, default-lit, non-blended geometry with zero alpha-test threshold, enabled depth test/write, LESS or LESSEQUAL comparison, and no decal classification. Eligibility participates in sorting, batching and pipeline selection. Cutouts, effects, wireframe and other material kinds retain their original shader. `BYRO_DISABLE_OPAQUE_EARLY_TESTS=1` selects the late-test module while retaining identical draw classification/order for A/B measurement. The experimental depth prepass is **not** retained.

This implements the frame-to-frame light-index correspondence required for reservoir reuse; NVIDIA's [RTXDI integration documentation](https://github.com/NVIDIA-RTX/RTXDI/blob/main/Doc/Integration.md) describes the equivalent integration contract. No NVIDIA source code was copied.

## Verification

RTX 4070 Ti, release Rust 1.96.0, serial Xvfb captures, 1280×720 Native AA, pinned ray tier 0, renderer-stepped/pan, 180 frames. Three repeats per scene/variant, reversing A/B order on repeat two. Both variants include the history repair, so these pairs isolate early depth rejection. No ray-budget or resolution reduction was made.

**GPU methodology:** `bench:` GPU fields are last-completed-frame snapshots, not capture averages. The table instead uses the median across three **per-capture arithmetic means of all 178 available `gpu_geometry phases` readbacks before the first benchmark summary**, with no warmup trimming. Wall time is the engine's 180-frame benchmark aggregate. GPU and wall intervals are not additive. Bracket ranges are observed repeat variation, not confidence intervals.

| Scene | Late-test main GPU ms (range) | Early-test main GPU ms (range) | Main reduction | Wall ms, late → early | Wall reduction |
|---|---:|---:|---:|---:|---:|
| MedTekResearch01 | 64.154 (59.589–64.587) | 44.786 (40.783–44.856) | 30.2% | 101.26 → 83.86 | 17.2% |
| CambridgeKendallHospital01 | 23.495 (23.418–23.836) | 17.578 (17.140–17.875) | 25.2% | 73.10 → 67.44 | 7.7% |

MedTek A/B runs share state hash `92b7960742fa16e1`, 286 lights and 1342 batches / 86 calls. Kendall shares `81ddac243a471e2b`, 214 lights and 420 batches / 24 calls. [Capture data](BENCH_restir_light_history_2026-09-26.tsv) retains both the per-capture GPU means and the endpoint snapshots as separately named columns. Diagnostics and validation are excluded from the timing table.

### Visual check

The same fixed Kendall column region (x=520..629, y=0..179) measures 0.13795–0.13796 mean display RGB across all three early runs, versus 0.14138–0.14139 late and 0.14138 with reuse disabled. The earlier broken early shader measured 0.07382. The abrupt dark boundary is gone. The remaining approximately 2.4% region-mean difference is recorded rather than claiming pixel identity.

For repeat one, Kendall early/late full-frame RGB MAE is 0.001743, and 0.00586% of pixels differ by more than 0.1 in any channel (the failed prototype had 1.61%). Early/early repeat MAE is 0.000667; late/late is 0.000580. MedTek early/late MAE is 0.001406; its large-difference fraction is 0.0156%, comparable to the 0.0166–0.0173% within-variant repeats. These are normalized display-space measurements, not physical irradiance or an unbiased transport oracle.

Durable full-resolution Kendall evidence: [late tests](experiments/restir_light_history_2026-09-26/kendall-late.png), [corrected early tests](experiments/restir_light_history_2026-09-26/kendall-early.png), [reuse disabled](experiments/restir_light_history_2026-09-26/kendall-raw.png).

### Automated and Vulkan checks

- 1,245 renderer library tests pass (one ignored); 210 application render tests pass (two ignored), including light reordering under animation, duplicate/missing/unknown identities, shrinking lists, frame-slot mapping, SSBO layout agreement, early-test eligibility and batch separation.
- All 36 standard shader artifacts plus the early-test variant reproduce with glslang 16.2.0. Changed modules pass `spirv-val`.
- Kendall with `BYRO_VALIDATION=1`: clean exit and zero VUID / synchronization-hazard messages. GPU-assisted validation was not enabled.
- Release build succeeds. The corrected early-test path is enabled by default; the diagnostic disable switch remains available.

The optional prototype depth prepass and diagnostic reservoir-color output have been removed. No commit or push was made.

The original diagnostic and all final commands, logs and PNGs are retained in `/tmp/restir-fix-2026-09-26/`. The older experiment remains intact in `/tmp/opaque-early-2026-09-26/`.

The independent initial-estimator normalization concern recorded in the earlier report is unchanged. These captures verify the history-index repair and the performance/visual behavior of the early-test optimization; they are not a claim that every ReSTIR estimator bias or overlapping-fragment history ownership issue has been eliminated.
