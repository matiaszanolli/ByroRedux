# Renderer Audit — 2026-09-20

Deep audit, all 12 dimensions, delta window `--since=2026-09-16`
(`AUDIT_RENDERER_2026-09-16.md` was the baseline). HEAD `052891f22`.
Orchestrator: 12 dimension auditors (3 concurrent waves), guard-first,
`-j 4` throughout; two live sync-validation engine runs (Cornell + Skyrim
`WhiterunBanneredMare` + FNV `GSProspectorSaloonInterior`) and one real-data
corpus sweep supplemented the cargo-test guards.

**Totals: 0 CRITICAL · 3 HIGH · 5 MEDIUM · 26 LOW** (34 findings; 1 prior
MEDIUM re-graded "not reproducible at HEAD").

## Executive Summary

The window's substantive changes all landed correctly: #3572's TAA-tap move
(D4/D7/D11 all trace it sound end-to-end), the GpuMaterial intern-once and
byte-view work (#4442/#4443/#4445), the detail/tint neutral fixes (#4422/#4423,
verified against real vanilla data), the 1 GiB BLAS budget ceiling +
blended-actor shadow policy (`84bbc44ed`), and the HUD triple-buffer overlay
(`dc306a6a0`). The prior report's two HIGH findings are confirmed fixed with
cross-game-measured gates.

What the audit found instead is three real spec/safety violations that only a
validation run or hostile input can surface — **the water pipeline has shipped
a push-constant range violation on the default path** (2 VUID-layout-10069
errors every session start, reproduced this session), **the DDS untrusted-input
path can allocate a 4 GiB image from a 128-byte file** (u32 mip-size wrap) or
panic in release, and **Skyrim texture uploads overrun their staging buffer by
one BC1 block** on deep mip chains (live validation, 10 occurrences). Around
them: one functional TAA-mode gap (jitter applied while the resolve is skipped
under raw-view), one light-canonicalizer miss on the dominant meshed-lamp path,
and a long tail of #3572/#4315 doc-rot — the second consecutive audit where a
pass moved and its documentation stayed behind; two new skill-sync rules below
address that class structurally.

## RT Pipeline Assessment (AS, SSBO indexing, ray-query safety, denoiser)

**AS (D1): HEALTHY.** All core contracts verify against code and all guards
are green (128/128 acceleration suite, the TLAS barrier pin, both BLAS-recovery
pins, the #4111 resize-ordering pin). `84bbc44ed`'s budget ceiling is clamped
in code (`predicates.rs`), ledgered, and its admission-control gate is
test-pinned; `rt.integrity` runs per frame. Two LOW hygiene findings: the
`mask_divert_cause` doc/pin premise is stale after the blended-actor policy
change, and `census.actor_diverted_alpha_blend` is structurally dead while
`rt.masks` still publishes it (the #3305 console oracle lost its alpha signal).
Two live-scene A/Bs remain owed (#3305 efficacy; >1 GiB visible-set
degradation).

**SSBO indexing / ray queries (D2): PASS.** The `instance_custom_index`
contract holds at all five hit sites; the RT gate precedes every fragment ray
query (triangle + water); shadow/reflection/GI bias, thin-glass, ReSTIR-DI,
BC1 punch-through, skyCube PARTIALLY_BOUND gating, and the #1495/#1496
raster-relative vs RT-absolute split all trace clean (the one absolute-trig
consumer is world-anchored inside the 2^20 ceiling). The window's two prior
HIGHs (#4422/#4423) are fixed with extended guards and byte-fresh SPIR-V
(34/34 spv byte-match). One LOW: `DBG_BYPASS_DETAIL` is honored only in
`triangle.frag`'s primary combine — `rayHitAlbedo` takes no flags, so RT
debug A/Bs keep the detail term.

**Denoiser/resolve (D7): structurally sound.** #3572's tap is the right image
at all three construction sites with self-consistent post-bloom history on
both sides; coverage alpha survives bloom→TAA→presentation; raw-output policy
is closed on both ends. One MEDIUM functional gap: under `--upscaler taa` +
raw-view, Halton jitter is still applied while the resolve is skipped
(#3632 fixed the FSR arm's predicate only; reachable automatically via the
#2480 FSR-startup→TAA promotion). The rest is a second #3572 doc-rot cluster
(taa.rs module doc, taa.comp header, retired #4309/#2760 premises) plus
bloom.rs's pre-#2796 module doc.

## GPU-Struct & Memory Assessment

**Structs/material table (D3): PASS.** Every lockstep guard green and
non-vacuous (205 tests; 108 field offsets; the per-byte XOR hash-coverage
probe); GpuMaterial Rust↔GLSL re-diffed field-by-field in sync at 432 B;
GroundCover mirrors hand-diffed after #4497/#4498 including the new 22-bit
serial mask. The 2026-09-16 report's suggested neutral-value consumer check is
now real for detail and tint (three of four legs individually pinned); `dark`
remains untouched and the GLSL arithmetic itself is unpinned (a revert to
`× 2.0` passes every test — REN-3-01).

**Memory/lifecycle (D5): the window's changes are sound** — the three
load-bearing teardown orderings hold after #4188, #4089's poison recovery is
correct, and the HUD 3-buffer rotation is correctly sized (FIF+1),
registry-owned, teardown-covered, zero per-frame allocation. But the
untrusted-input floor fails: **REN-D5-01 (HIGH)** — `mip_size` unchecked u32
arithmetic + no dimension cap in `parse_dds` + a release-compiled `assert!` on
the payload check means a truncated or hostile archive DDS either panics the
engine or wraps to a 0-byte staging budget under a 4 GiB `vkCreateImage`
allocation (fix precedent in-repo: `menuxml/src/tex.rs`'s 8192 cap +
`TexError::AbsurdDimensions`). **REN-D9-03 (HIGH, surfaced by D9's Skyrim
run)** — `vkCmdCopyBufferToImage` mip regions exceed the staging buffer by
exactly +8 B (one BC1 block row tail; `div_ceil(w,4)` vs truncated-multiply on
final mips), 10 occurrences, Skyrim-only — a live spec violation on the
default texture-upload path. Two MEDIUMs: the new `write_rgba_inplace` hazard
contract claims an extent assert that does not exist (merged D4-05), and the
HUD rotation is missing from the #3643/#870 FIF-bump tripwire. Plus two LOWs
(ledger row for the ~24.9 MB @1080p overlay set; `Drop`'s SAFETY comment still
says "four" orderings).

**Skinning (D9): PASS at HEAD.** Every checklist invariant holds; the P3
`PickedUp` render-skip is sound but a two-site lockstep decision with no guard
(LOW). The prior unattributed VUID-08114 is **not reproducible at HEAD** —
two validated runs with live palette+dispatch+refit chains (FNV saloon,
`skin=91/1364`, refit 2.2 ms/frame; Skyrim BanneredMare) produced zero
occurrences; every skin-phase descriptor set is provably write-before-bind.
Re-graded: capture-the-message-and-diff if re-observed (recipes in dim_9
notes: bench-hold attach, equip churn, mid-run cell transition, taa mode).

## Findings

Severity-ordered; CRITICAL first (none), then HIGH, MEDIUM, LOW. IDs are
`REN-<dim>-2026-09-20-NN` (dim = owning dimension).

### HIGH

#### REN-D4-2026-09-20-01: water pipeline push-constant block is 28 B in GLSL against a 16 B declared range — two `VUID-VkGraphicsPipelineCreateInfo-layout-10069` errors fire on every session start, on the default render path
- **Severity**: HIGH (Vulkan spec violation + validation error in normal operation)
- **Dimension**: Pipeline/RenderPass · **Status**: NEW
- **Location**: `shaders/water.vert:163-166` + `shaders/water.frag:138-141` (`WaterDrawPush { uint waterIndex; uvec3 _reserved; }` — std430 aligns the uvec3 to 16, block spans [0,28]); `vulkan/water.rs:473-477` (range = `size_of::<WaterPush>()` = 16), `:198-203` (const-assert pins the wrong invariant — only the Rust struct is 16 B), `:784-793` (16 B pushed).
- **Description/Evidence**: Reproduced this session, `BYRO_VALIDATION=1 --cornell --bench-frames 45` (RTX 4070 Ti): exactly two validation errors, both at water-pipeline creation, vertex+fragment stages, push-constant block range [0,28] outside VkPushConstantRange [0,16]. Benign today only because `_reserved` is never read; the #3572 commit message is the only prior record ("the layout-10069 push-constant pair") — no issue tracks it. The errors also desensitize the validation signal every session start.
- **Impact**: A compliant driver may reject the pipeline or return garbage for the out-of-range word; invisible until a driver change or someone uses `_reserved`.
- **Suggested Fix**: Shrink the GLSL block to the real 16 B selector (drop `_reserved` or repack), fix the const-assert message, keep the push at 16 B; pin with a SPIR-V-reflection size check rather than Rust `size_of` alone (see skill-sync rule).
- **Related**: REN-D4-2026-09-20-02 (the other standing VUID, now re-graded); D8's addendum — `bind_pass`'s doc restates the false 16 B premise.

#### REN-D5-2026-09-20-01: DDS untrusted-input path — `mip_size` is unchecked `u32` arithmetic, `parse_dds` applies no dimension cap, and the payload check is a release `assert!`: a malformed archive texture panics the engine or wraps to a 0-byte budget under a 4 GiB image
- **Severity**: HIGH (untrusted-input reader: panic, or allocation sized from on-disk fields with no cap)
- **Dimension**: Memory/Lifecycle · **Status**: NEW
- **Location**: `vulkan/dds.rs:537-547` (`mip_size` plain-u32 multiply; `width >> mip_level` shift-overflows at mip_count ≥ 33; `mip_count` unclamped at `:276`), `dds.rs:259-330` (raw width/height, no cap), `vulkan/texture.rs:318-328` (`assert!` on payload length).
- **Description/Evidence**: At the NVIDIA 32768² limit, `32768×32768×4 = 2^32` wraps to 0 (BC likewise at 8192² blocks × 16 B): `total_data_size` returns 0, a 128-byte header-only file passes, and `vkCreateImage` allocates a 4 GiB device-local image. A truncated-but-plausible DDS instead panics in release via the `assert!`. Vanilla content never triggers it; the input is mod-authorable (BSA/BA2). No OOB read — a VRAM/OOM bomb and a panic, not corruption.
- **Suggested Fix**: Cap dimensions in `parse_dds` (in-repo precedent: `menuxml/src/tex.rs:88` 8192 cap + `TexError::AbsurdDimensions`), clamp `mip_count`, compute `mip_size` in u64/`checked_mul`, convert the `assert!` to an error so the caller falls back to the checkerboard; add the absurd-dimension test the skill asks for.
- **Related**: none prior.

#### REN-D9-2026-09-20-03 [owner Dim 5]: `vkCmdCopyBufferToImage` mip regions exceed the staging buffer by exactly 8 bytes on Skyrim texture uploads
- **Severity**: HIGH (Vulkan spec violation on the default path)
- **Dimension**: Memory/Lifecycle · **Status**: NEW (surfaced by D9's validated Skyrim run)
- **Location**: staging-region builder vs `total_data_size` budget on the DDS upload path (`vulkan/texture.rs`); observed as `pRegions[8]` 349536 > 349528 and `pRegions[10]` 5592416 > 5592408, ×10, deep-mip (≥11 mip) textures.
- **Description/Evidence**: Every overrun is exactly +8 B = one BC1 block row tail — the classic `div_ceil(w,4)` vs truncated-multiply disagreement on final mips between the region list and the staging allocation. FNV run clean; Skyrim-specific dimension class. Live validation output in dim_9 notes.
- **Suggested Fix**: Diff `total_data_size` against the sum of per-mip `region.size` for BC1/BC3 odd sub-block dimensions; unify the block math and add `debug_assert!(regions.iter().map(|r| r.size).sum::<u64>() <= staging_size)`.
- **Related**: REN-D5-2026-09-20-01 (same file, different mechanism).

### MEDIUM

#### REN-D7-2026-09-20-01: under `--upscaler taa` + a raw-output debug view, Halton jitter is still applied while the TAA resolve is skipped
- **Severity**: MEDIUM · **Dimension**: TAA · **Status**: NEW
- **Location**: `taa_jitter` gating vs `record_taa_pass` skip (`context/post_passes.rs`, `assemble_camera_and_lights.rs`); #3632 folded the raw-output predicate into `is_fsr_dispatch_active` only.
- **Description**: Raw oracles render jittered-but-unresolved (persistent shimmer); reachable automatically via the #2480 FSR-startup→TAA promotion (`init.rs:1754-1776`). The FSR arm is correct; only the TAA arm missed the fix.
- **Suggested Fix**: Fold the raw-output predicate into the TAA arm's jitter decision; guard `taa_and_fsr_negate_jitter_y_the_same_way`-style.

#### REN-D10-2026-09-20-01: `spawn_mesh_instance`'s ESM-light fallback bypasses `canonical_light_falloff_exponent` — the third LIGH spawn path missed by `6b4e6252c`
- **Severity**: MEDIUM (wrong per-game light decode; visual) · **Dimension**: Light Animation · **Status**: NEW
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:1426` (`ld.falloff_exponent` raw); canonicalized siblings at `synth_child.rs:394,500`.
- **Description**: Pre-Skyrim 32-byte LIGH always carries the 0.0 sentinel; this is the dominant meshed-lamp path (Prospector-Saloon class, gated `spawned_nif_lights == 0`), so Oblivion/FO3/FNV meshed lamps resolve k=1.0 via the "non-ESM last-resort net" instead of k=2.0.
- **Suggested Fix**: Thread a falloff lane through `PlacementCtx` like the three already-canonicalized light fields; pin all three sites with one guard.

#### REN-D5-2026-09-20-02 [merges D4-05]: `write_rgba_inplace`'s hazard contract claims an extent assertion that does not exist; unchecked `u32` size math
- **Severity**: MEDIUM · **Dimension**: Memory/Lifecycle · **Status**: NEW (`dc306a6a0`, in-window)
- **Location**: `vulkan/texture.rs` `overwrite_rgba_pixels`/`write_rgba_inplace` (only caller-supplied w/h vs pixels is checked; `Texture` stores no extent).
- **Description**: The doc delegates the hazard to the HUD's 3-slot rotation, but a second consumer relying on the documented assert would upload into a smaller image (GPU-side failure mode). Same finding independently surfaced by D4; merged.
- **Suggested Fix**: Store the creation extent on `Texture` (or pass it) and assert against it; checked size math.

#### REN-D5-2026-09-20-03: HUD 3-buffer rotation missing from the #3643/#870 frames-in-flight bump tripwire
- **Severity**: MEDIUM (latent; MAX_FRAMES_IN_FLIGHT == 2 today) · **Dimension**: Memory/Lifecycle · **Status**: NEW
- **Location**: `frames_in_flight_contract_names_every_dependent_resource` enumeration vs `hud.rs` 3-slot rotation (`upload_frame`, `(current+1)%3`).
- **Description**: The rotation's safety rests on the both-slots fence wait (one slot of margin); a future FIF ≥ 3 silently becomes a WAR use-after-free. The tripwire test that exists for exactly this class doesn't name the new rotation.
- **Suggested Fix**: Add the rotation to the tripwire's enumeration.

#### REN-D4-2026-09-20-02: standing VUID-08114 (skin-dispatch set) — re-graded: not reproducible at HEAD
- **Severity**: MEDIUM (observability; escalates to HIGH if it is a real never-written-descriptor draw) · **Status**: RE-GRADED (was unattributed in #3572's commit message)
- **Description/Evidence**: Two validated debug-binary runs with live skinning (FNV `GSProspectorSaloonInterior` `skin=91/1364` + full palette/dispatch/refit chain; Skyrim `WhiterunBanneredMare`) logged zero 08114. Code-side, every skin-phase descriptor set is provably write-before-bind (option-keyed #1197 caches, eager cluster-cull writes, seeded empty-TLAS binding, MorphSlot has no sets). Uncovered configs: bench-hold attach, equip churn, mid-run cell transition, taa mode.
- **Suggested Fix**: If re-observed, capture the full message (set+binding) and diff against the recorded recipes; do not carry as a standing error.

### LOW (26)

| ID | Dimension | Finding (one line) |
|---|---|---|
| REN-D1-01 | AS Correctness | `mask_divert_cause` doc + pin's "whole input space" claim stale after `84bbc44ed` (values still agree on every enumerated cell) |
| REN-D1-02 | AS Correctness | `census.actor_diverted_alpha_blend` structurally dead; `rt.masks` still publishes it; #3305 console oracle lost its alpha signal |
| REN-D2-01 | Ray Queries | `DBG_BYPASS_DETAIL` honored only in `triangle.frag` primary combine; `rayHitAlbedo` (`ray_hit.glsl:533`) takes no flags — RT debug A/Bs keep the detail term |
| REN-3-01 | GPU-Struct | Nothing pins the GLSL detail-combine (`detailSample / max(mat.detailNeutral,1e-4)`) or two-condition tint gate — a revert to `×2.0` passes every test |
| REN-3-02 | GPU-Struct | Four sibling hash views in `scene_buffer/descriptors.rs:462-538` hand-roll `from_raw_parts` after #4445 declared `byte_view` "the single place" (3 of 4 convertible) |
| REN-3-03 | GPU-Struct | `hash_gpu_material_fields` doc says "428 bytes" post-#4422; `gpu_material_size_claims` scanner misses it (no `Gpu*` type on the line) |
| REN-3-04 | GPU-Struct | `dark` role: bare sRGB multiply, no neutral, no test; producer arm live but census-zero on 4 games, never censused FO4+ |
| REN-D4-03 | Sync/Barriers | #3572 doc-rot cluster ×6: retired `fall_back_to_raw_hdr`/rebind prose, broken `taa_failed` intra-doc link, dangling #4006 comment, dead `rebind_hdr_views` kept alive only by the svgf anchor guard |
| REN-D4-04 | Pipeline/RenderPass | `shader-pipeline.md` authoritative submission order predates #3572 (TAA at step 15) and #4182 (step 5b "required by spec even for HOST_COHERENT") |
| REN-D5-04 | Memory/Lifecycle | No `memory-budget.md` ledger row for the 3 overlay textures (~24.9 MB @1080p); Scaleform section's #3429 prose covers only one of the two HUD drivers |
| REN-D5-05 | Memory/Lifecycle | `Drop`'s SAFETY comment still says "four" load-bearing orderings; #4188 made it three (`teardown.rs:244`) |
| REN-6-01 | NIFAL Material | Explicit `Height` flipbook leaves the parallax `.a`-vs-`.r` gate keyed on the normal's alpha; `active_has_alpha(FlipTextureRole::Height)` recorded (`anim_convert.rs:272`) but never read |
| REN-6-02 | NIFAL Material | `MaterialTextureHandles` producer copy-paste at `nif_loader.rs:1357` + `mesh_instance.rs:1165`, unshared/unpinned (the #2444/#2300 duplicate-construction class) |
| REN-D7-02 | TAA | Second #3572 doc-rot cluster: `taa.rs:3-14` module doc (pre-move order), `taa.comp:8-12` header, #4309 coverage-lane rationale, #2760 premise, FSR-promotion `error!` warns about open-#3572 crawl |
| REN-D7-03 | Bloom | `bloom.rs:1-21` module doc still describes pre-#2796 architecture |
| REN-D8-01 | Volumetrics | `VOLUMETRIC_OUTPUT_CONSUMED` doc credits the 3×3 XY blur deleted `5be840d2b` (2026-08-16) — the stated reason names a mechanism that no longer exists |
| REN-D8-02 | Volumetrics | `normalize_fog_tint` "black stays absorptive" doc + test contradict `3ce970a5a`'s shader branch (zero-tint + extinction now scatters neutrally); no GLSL-side test ties them |
| REN-D8-03 | Water | `traceWaterRay` header still quantifies the miss term as "~14%" — deleted by `7996edf61`'s normalization (0% at ramp end) |
| REN-D8-04 | Volumetrics | Failed combustion-moment drain consumes `combustion_moment_dirty` before the fallible read/zero/flush — stale moments persist one cycle (self-healing; one-line latch restore) |
| REN-D9-02 | Skinning | `PickedUp` render-skip is a two-site lockstep decision (`skinned.rs` palette skip vs `static_meshes.rs` draw skip) with no guard; only `NpcAppearanceHidden` sibling is tested |
| REN-D10-02 | Light Animation | GI-priority sort rationale (3 comment clusters + test doc) names `giHitIrradiance`, deleted in #4017; sort still load-bearing for overflow-drop policy + determinism |
| REN-D11-01 | FSR/Presentation | Nothing enforces "FSR dispatch ⇒ `scene_color` is `SHADER_READ_ONLY_OPTIMAL`" (`record_fsr_barriers_before` hard-codes it; `scene_color_layout` makes GENERAL representable) |
| REN-D11-02 | FSR/Presentation | Exposure-failure warn names "default exposure constant"; actual fallback is `NO_EXPOSURE_RESOURCE_FALLBACK` (1.0) not `DEFAULT_EXPOSURE` (0.85) — #2833's conflation resurrected (`init.rs:1000`) |
| REN-D11-03 | FSR/Presentation | FSR plan §1.4 + exal-groundcover §12.14 say ground cover "writes zero to both masks"; `groundcover_blade.frag:247` has written `0.9 × max(midTransition, cardTransition)` reactive since `fd0cd577c` |
| REN-D12-01 | Debug/Telemetry | #4315's 36→38 bracket bump left four `gpu_timers.rs` prose sites at 18/36; the #4210 row-count test only guards the table — doc-count rot recurred on the next bump |
| REN-D12-02 | Debug/Telemetry | `tex.dump` (`ae572745f`): zero test coverage (arg split, menu-path resolution, decode-failure paths) and no `debug-cli.md` row while `tex.missing`/`tex.loaded` are documented |

## Prioritized Fix Order

1. **REN-D5-01** (DDS cap + checked math + error-not-assert) — untrusted-input
   floor; small, self-contained, in-repo precedent.
2. **REN-D9-03** (BC1 staging +8 B) — live spec violation on Skyrim's default
   path; unify the block math, one debug assert.
3. **REN-D4-01** (water push-constant 16/28 B) — shrink the GLSL block, fix
   the const-assert, kill the two every-session validation errors; add a
   reflection-based push-constant pin.
4. **REN-D7-01** (TAA-arm raw-view jitter) — fold the predicate; small.
5. **REN-D10-01** (falloff canonicalizer third path) — one `PlacementCtx`
   lane + a three-site guard.
6. **REN-D5-02/03** (inplace extent assert + FIF tripwire) — harden the new
   HUD ownership shape.
7. The LOW tail in one or two doc/skill-sync commits (the doc-rot clusters
   REN-D4-03/04, REN-D7-02/03, REN-D12-01 are one mechanical sweep);
   REN-3-01's GLSL-arithmetic pin is the one LOW worth doing for real.

## Needs-RenderDoc

(Or `BYRO_VALIDATION=1` — both were used this session where possible.)

- #3305 A/B pre/post `84bbc44ed`: does the blended-actor policy actually
  restore creature shadows (depends on creature draws carrying
  `alpha_blend`)?
- The 1 GiB BLAS ceiling's degradation behavior on >1 GiB visible-set cells
  (`rt.integrity` observable).
- Runtime FSR boundary layouts (SDK-bump-voided 2026-07-25 validation only)
  and rendered reactive/T&C mask content (shader-text pins only; never run
  against the live upscaler — `m-exteriors.sh` hardcodes `--upscaler taa`).
- BC3-tint on-path visuals (no vanilla content fires #4423's new gate arm);
  #4422 perceptual result on FaceGen; origin bit-step at extreme |world|;
  reversed-Z flip (operator job).
- Dust-floor visuals, canary torch A/B (still owed per ROADMAP), deep-water
  A/B (Lake Mead/Potomac); bloom halo clamp under motion near bright edges.
- FP32 FSR permutation: carried scope, untested (no shaderFloat16-less GPU).

## Stale skill premises (for the next `/audit-renderer` sync)

1. **Doc-moves-with-pass rule (new, structural):** second consecutive audit
   where a pass moved without its docs — add to Phase 1: "a pass that moves
   must take `shader-pipeline.md`'s step list AND its own module doc / shader
   header with it" (#3572 took neither; REN-D4-04, REN-D7-02/03).
2. **Push-constant pin rule (new):** "push-constant block contained in
   declared range — pin via SPIR-V reflection, not Rust `size_of` alone"
   (REN-D4-01's const-assert pinned the wrong invariant).
3. Dim 1: mask bullet should note the blended-actor-inside-opaque policy
   (`84bbc44ed`) and name `MAX_BLAS_BUDGET_BYTES`; the "Paths untouched" delta
   claim in this run's coordination was wrong — always run the dimension's own
   `First step:`.
4. Dim 3: neutral-value bullet needs rewriting (detail + tint now pinned on
   three legs; `dark` remains); "two packed high bits" is now three
   (`TINT_ALPHA_WEIGHT_BIT`); note the size-claims scanner's same-line-type
   limit (REN-3-03).
5. Dim 5: dds bullet should point at REN-D5-01 + the menuxml cap precedent;
   add the MenuXml HUD overlay (rotation/extent/ledger) to the checklist; add
   "check `Drop`'s own SAFETY comment agrees"; write the guard command with
   `--lib`.
6. Dim 7: guard line is behind the file (add the #4305/#2760/#4309 guards);
   the raw-output TAA bullet needs "must also un-jitter".
7. Dim 8: "one TerminateOnFirstHit shadow ray per froxel" understates (up to
   10 traversals/froxel, `volumetrics.rs:604-611`, #2509); 2026-09-16's
   water reflection-miss arm (D2-01) is resolved by #4292 — do not re-file;
   name the new sub-areas (dust floor, reach canary, `WaterNormalEncoding`,
   `refrTraced` gate).
8. Dim 9: guard line mis-locates `bind_inverse_upload_failed_*` (renderer
   crate `draw.rs`/`dispatch_skin_and_cluster.rs`, not bin `app_frame.rs`);
   `bone_palette_overflow_tests` is its own file; record D4-02's
   not-reproducible-at-HEAD grade so 08114 isn't carried as standing.
9. Dim 12: skill checklist current (38/19, 13 modes); the 2026-09-16 report's
   "GpuMaterial 428 B" prose is stale vs the 432 B pin (Dim 3 owns).

## Guard posture

All named guards across the 12 dimensions exist, are not `#[ignore]`d, are
not vacuous, and pass: acceleration 128/128; `shader_contract` 103/103 (incl.
BSDF/light pins and the depth-convention suite — note the bare `depth_convention`
filter matches only 1 of 4); scene_buffer+material 205/205; resolve-chain
8/8 primary + 9/9 secondary; water/caustic/volumetrics/froxel 143/143;
upscaler/presentation/exposure/jitter 48 + fsr 28 + fsr3-sys 8; skinning 5
guard families across both crates; lighting 4 named guards + 14 secondary;
debug/telemetry 20 + 3; bin-crate guards via the 1.96.0 toolchain throughout;
34/34 SPIR-V byte-match via `check-shader-artifacts.sh`. Two live validated
engine runs and one real-data corpus sweep (FO4 6,616 BGSMs) supplement the
source-shape guards.
