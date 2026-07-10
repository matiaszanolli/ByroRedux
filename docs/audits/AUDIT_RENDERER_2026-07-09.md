# Renderer Audit — 2026-07-09

**Scope:** Full 21-dimension deep audit of the ByroRedux Vulkan renderer at HEAD `0c4e9176`, per `/audit-renderer` (`--depth deep`, all dimensions). Combines a prior high-rigor pass that had already completed Dimensions 1, 2, 3, and 5 (found in `/tmp/audit/renderer/` from an earlier interrupted run and preserved rather than redone) with a freshly-run pass covering Dimensions 4, 6–21.

**Method:** Every dimension ran `cargo test` against its area, diffed committed `.spv` binaries against fresh `glslangValidator -V` recompiles, checked `git log`/`git show` on entry points, and cross-referenced `docs/engine/shader-pipeline.md` + `docs/engine/memory-budget.md` as the authoritative layout/budget references. All findings were deduplicated against the live GitHub issue list and prior `docs/audits/` reports.

---

## Executive Summary

**21/21 dimensions audited. 5 MEDIUM findings, ~30 LOW findings, 0 CRITICAL, 0 HIGH.** This is a renderer in good health — every load-bearing correctness invariant (AS/SSBO index contract, GPU-struct byte lockstep, sync/barrier scopes, deferred-destroy lifecycles) held under adversarial re-derivation, not just re-reading of comments. Two independently-discovered **shading correctness bugs** are the most significant findings — both are visual-only (not crashes, not memory-unsafe) and share a root cause worth calling out explicitly:

### The headline finding: a recurring sun-direction sign bug, found twice

Two dimensions independently discovered the **same class of bug** in two different files: a shader assumes `sun_direction` means "pointing *from* the sun" when the entire rest of the codebase (composite sun-disc, water caustics, main directional lighting, the Rust-side `compute_sun_arc`) uses "pointing *toward* the sun."

- **VOL-D16-01** (MEDIUM): `volumetrics_inject.comp`'s sun shadow ray is cast into the ground instead of toward the sun, silently zeroing sun-driven volumetric god-rays — the flagship feature that motivated flipping `VOLUMETRIC_OUTPUT_CONSUMED = true`.
- **SKY-D18-01** (MEDIUM): the `#890` Effect_Lit shading path in `triangle.frag` negates the same `direction_angle` value, inverting the sun hemisphere for `BSEffectShaderProperty` surfaces (Skyrim spell FX, FO4 magic/power-armor glow).

Both were introduced by *recent* additions guessing the wrong convention, both are invisible to `cargo test` (visual-only), and — per **CORN-D21-01** — the Cornell-box RT reference harness cannot catch either one because it never exercises directional/sun lighting at all. **Fixing these requires RenderDoc/visual confirmation before landing a change** (per the project's no-speculative-Vulkan-fixes policy) — see the Needs-RenderDoc section.

### A mislabeled commit shipped real renderer changes

Commit `977eb95a`, titled *"Add Scripting Subsystem Audit report for 2026-07-06,"* actually ships a substantial renderer arc: TLAS per-instance shadow-mask buckets, the volumetrics Phase 2b light-injection rewrite (including the sun-direction bug above), the `VOLUMETRIC_OUTPUT_CONSUMED = true` flip, an RL-03 per-light ambient-fill shading defect (**REN-D2-01**, MEDIUM — see below), a material-classifier change (**MAT-D6-01**), and left `composite.frag.spv` stale (**REN-D3-01**, MEDIUM). Six dimensions independently traced this commit. This is a git-hygiene/traceability problem, not a code-quality one — the shipped code is mostly correct — but it defeats `git blame`/bisect for anyone tracing a regression, and the misleading title meant no prior audit's dedup pass looked here.

### Other MEDIUM findings

- **REN-D2-01**: RL-03 per-light ambient fill in `triangle.frag` is missing its own documented directional-light gate — every exterior fragment gets an unshadowed, N·L-independent ~15% sun-tinted fill added on top of the real (shadowed) lighting, washing out shadow contrast on exteriors.
- **REN-D5-01**: The batched texture-upload path releases pooled staging buffers under the *requested* size rather than the *actual allocation* size, so the documented 128 MB staging budget is enforced against an under-counted ledger — real BAR-heap retention can exceed the documented cap after repeated cell transitions.

### Everything else: clean

Dimensions 1 (BLAS/TLAS), 3 (GPU-struct layout), 4 (sync/barriers), 7 (material dedup), 9 (GPU skinning), 10 (camera-relative precision), 11 (pipeline/render-pass), 12 (command recording), 13 (TAA), 14 (caustics), 17 (Disney BSDF/soft shadows), 19 (tangent-space), and 20 (debug/telemetry) returned **zero or near-zero findings above LOW** — every regression guard named in the audit checklist was independently re-derived from source and confirmed intact, not merely re-read from a comment. Two dimensions (11, 19) independently discovered the same SPIR-V build-hygiene issue (`triangle.vert.spv` compiled to a different SPIR-V target version than its siblings) — corroborating cross-dimension evidence, not two separate bugs.

### Pipeline areas affected

| Area | Dimensions | Status |
|---|---|---|
| Ray tracing (AS, SSBO, ray queries) | 1, 2 | Clean |
| GPU-struct layout | 3 | Clean |
| Sync/barriers | 4 | Clean |
| Memory/lifecycle | 5 | 1 MEDIUM (staging pool), 2 LOW |
| NIFAL material translation | 6 | 2 LOW |
| Material table dedup | 7 | Clean |
| Denoiser/composite | 8 | 2 LOW (dead code) |
| GPU skinning | 9 | Clean (2 informational) |
| Camera precision | 10 | 1 LOW (doc) |
| Pipeline/render-pass | 11 | 2 LOW (build hygiene, doc) |
| Command recording | 12 | 2 LOW (doc) |
| TAA | 13 | 1 LOW (latent), 1 informational |
| Caustics | 14 | 1 LOW, 1 informational |
| Water | 15 | 3 LOW |
| Volumetrics/bloom | 16 | **1 MEDIUM (sun-direction bug)**, 2 LOW |
| Disney BSDF/soft shadows | 17 | Clean (1 informational) |
| Sky/weather | 18 | **1 MEDIUM (sun-direction bug)** |
| Tangent-space | 19 | Clean (1 informational, corroborates #11) |
| Debug/telemetry | 20 | 2 LOW (doc) |
| Cornell harness | 21 | 2 LOW |

---

## RT Pipeline Assessment

**BLAS/TLAS (Dim 1):** Clean. Build geometry, build-flag constants, the `instance_custom_index`↔SSBO-index CRITICAL contract, TLAS BUILD/UPDATE decision safety, transform conversion, empty-TLAS-at-frame-0, device-address usage, build→read barriers, LRU/shrink wiring, and deferred BLAS destruction (`#a476b256`) all independently re-derived and confirmed. Three LOW doc-rot findings (memory-budget.md call-site misattribution, a stale byte-size figure in a comment, a missing hardening pin on a new TLAS shadow-mask truncation site).

**SSBO/ray queries (Dim 2):** Clean. The SSBO custom-index contract, all `rayQueryInitializeEXT` sites, shadow/reflection/GI ray mechanics, glass/IOR refraction guards (#789/#820/#1438), RT gating, noise determinism, ReSTIR-DI spatial reuse (`#d523b9b3`), and the BC1 punch-through alpha guard (`#ae285062`) all confirmed intact. One MEDIUM (RL-03 ambient fill, see Executive Summary) and several LOW/doc findings, all introduced by the same `977eb95a` churn.

**Denoiser stability (Dim 8):** Clean logic. SVGF history ping-pong, disocclusion rejection, firefly-clamp hoist (`48906670`), à-trous slot parity (verified no off-by-one), and the composite reassembly order (direct + denoised-indirect → volumetric attenuate → bloom → tone-map) are all correct. Two LOW findings: the composite fog fallback branch is now dead code post-`VOLUMETRIC_OUTPUT_CONSUMED=true` (author's own comment says to remove it), and the #865 XCLL interior fog curve was never actually reachable for the interiors it targets.

**Overall RT verdict:** the acceleration-structure/SSBO/ray-query/denoiser core — the highest-blast-radius, silently-corrupting-if-wrong subsystem in the renderer — has no correctness defects. The MEDIUM findings in this area are shading-energy bugs (wrong light contribution), not memory-safety or indexing bugs.

## GPU-Struct & Memory Assessment

**GPU-struct layout (Dim 3):** Clean and thoroughly test-pinned — 63 targeted tests green, all 5 `GpuInstance` GLSL declaration sites byte-identical, `GpuMaterial`'s 75 scalar fields offset-pinned both by size and by GLSL declaration order (`#1657`, catches within-vec4 reorders a size-only pin would miss). The one real defect found here — `composite.frag.spv` stale after `977eb95a` removed a volumetric gate from the GLSL without recompiling — is MEDIUM and cross-referenced by 3 other dimensions (2, 8, 15) that touch the same file.

**Memory/lifecycle (Dim 5):** Mostly clean — deferred-destroy queues (BLAS, BLAS-scratch, textures), the allocator-vs-ECS teardown ordering (`#1406`/`#1477`/`#1483`), and the DDS 16/24-bpp expansion lifecycle (`#1542`) all hold. One MEDIUM: the batched texture-upload path under-counts staging-buffer capacity on release, so the documented 128 MB budget is enforced against a shrinking ledger rather than real retained bytes — bounded, not a leak, but pressures the scarce BAR heap silently. Two LOW doc-rot findings on stale "needs a follow-up" comments describing gaps that have since been filled.

---

## Findings (by severity)

### MEDIUM

#### REN-D2-01: RL-03 per-light ambient fill missing its own directional gate
- **Location**: `triangle.frag:2191-2197`
- **Introduced by**: `977eb95a`
- Exterior fragments get an unshadowed, N·L-independent ~15% sun-tinted ambient fill added on top of proper RT-shadowed lighting, because the fill's own contract comment ("point/spot only") isn't enforced by a `lightType` gate in code. Washes out shadow contrast on exteriors; interiors unaffected.
- **Fix**: add the promised `lightType >= 1.5` skip (or hoist the fill inside the point/spot arms). One-line shader change + recompile; bench on `--grid` A/B.

#### REN-D3-01: `composite.frag.spv` stale relative to source
- **Location**: `composite.frag.spv` (built `9c10f14e`) vs `composite.frag` (edited `977eb95a`)
- Byte-proven via recompile diff. Zero behavioral divergence *today* (the removed gate is a no-op given the pinned host constant), but the source↔binary contract is violated, and the next unrelated recompile will silently ship the change.
- **Fix**: `glslangValidator -V -I crates/renderer/shaders composite.frag -o composite.frag.spv`, commit the binary. Cross-referenced by Dim 2, 8, 15.

#### REN-D5-01: Batched texture-upload path under-counts staging-buffer capacity on release
- **Location**: `texture_registry.rs:803-827,884`, `vulkan/texture.rs:186,201,401`
- `flush_pending_uploads` releases pooled staging buffers using the requested upload size rather than the actual (possibly larger, best-fit-reused) allocation size, so the pool's recorded capacity shrinks monotonically on reuse. The documented 128 MB retained-staging budget is enforced against this shrunken ledger.
- **Fix**: compute release capacity the same way the sync path does (`staging.allocation.size()`), or have `record_dds_upload` return the allocation size instead of the requested size.

#### VOL-D16-01: Volumetric sun shadow ray cast in the wrong hemisphere
- **Location**: `volumetrics_inject.comp:270,313-327`, fed by `draw.rs:695-698`
- **Introduced by**: `977eb95a` (Phase 2b rewrite)
- The shader documents/assumes `sun_dir` points *from* the sun and negates it into `light_in`, which is then used as the shadow-ray direction. The host actually feeds `sun_direction` as *toward* the sun (the convention `water.frag`, `composite.frag`'s sun-disc, and the main directional lighting path all use). The ray is cast downward into the terrain/floor instead of toward the sun, so the opaque shadow-mask ray hits immediately on essentially every froxel → sun in-scatter is zeroed. The Henyey-Greenstein phase term is *coincidentally correct* through the same double-negation — this is purely a ray-direction bug, not a phase bug (explicitly checked and disproved as a phase issue).
- **Impact**: daytime exterior sun shafts and interior sun-through-window god-rays — the entire reason `VOLUMETRIC_OUTPUT_CONSUMED` was flipped `true` — produce ~zero sun contribution. Point/spot lantern glow (unaffected, different code path) can mask the regression visually.
- **Fix (pending RenderDoc confirmation)**: cast the visibility ray toward the sun (use `-light_in`, i.e. the un-negated host value) while keeping `light_in` as-is for the HG cosine, so the correct phase result is preserved. Do not fix by negating the host value — that breaks the HG term instead.

#### SKY-D18-01: Effect_Lit shading path negates sun direction, inverting the hemisphere
- **Location**: `triangle.frag:606-609` (`MAT_FLAG_EFFECT_LIT` / `#890` block)
- **Introduced by**: `#890`, commit `2aa2817a`
- Same bug class as VOL-D16-01 in a different file: this path computes `dot(N, -Ldir)` while the main directional-lighting path two thousand lines later in the same file correctly computes `dot(N, Ldir)` on the identical `direction_angle` value. Confirmed via a physical check: at solar noon a +Y floor should be fully lit (main path: `dot(up,+up)=1`); the effect-lit path instead computes `dot(up,-up)→0` (dark).
- **Impact**: `BSEffectShaderProperty` Effect_Lit surfaces (Skyrim spell FX, FO4 magic/power-armor ambient glow) under an exterior sun get their additive scene-lit term computed against the wrong hemisphere — sunlit side goes dark, shadow side lights up. Narrow surface class, additive-only, so not catastrophic, but a definite inversion.
- **Fix (pending RenderDoc confirmation)**: `float NdotL = max(dot(N, Ldir), 0.0);` (drop the negation). Recompile `triangle.frag.spv`.

### LOW (representative selection — full detail in per-dimension reports under `/tmp/audit/renderer/` prior to cleanup, and cross-referenced against `docs/audits/` history)

**Build hygiene / SPIR-V:**
- **REN-D11-01 / M-NORMALS-D19-N01** (found independently by 2 dimensions): `triangle.vert.spv` is SPIR-V 1.5 while every sibling shader is SPIR-V 1.0; the documented plain `-V` recompile command produces a byte-different (1.0) binary. No functional impact; reproducibility hazard. `triangle.frag.spv` additionally ships as SPIR-V 1.0 despite using ray-query extensions — a latent portability concern on stricter drivers.

**Documentation rot** (largest category, ~15 findings): stale call-site attributions in `memory-budget.md`/`shader-pipeline.md` (BLAS/TLAS shrink call sites, `dof_params.zw` meaning, GpuMaterial file pointer, offset-row mislabeling, instance-flag table missing bit 8, descriptor-table missing bindings 15-17, `GpuLight` header size); stale in-code lockstep-protocol comments (`gpu_types.rs` naming a wrong 5th GLSL site, `material.rs` module doc describing the pre-generated-header world, pre-Session-35 file paths); contradictory/self-negating comments (egui_pass.rs's begin/end rationale contradicts its own next line; gpu_timers.rs describes a non-existent "ran_this_frame" API; a `draw.rs` volumetrics comment still says the composite output is discarded post-`977eb95a`); a `GpuLight` shader-struct-sync tracking comment missing the newest 4th GLSL declaration site (`volumetrics_inject.comp`).

**Test/hardening coverage gaps** (~6 findings): the `#1234` named-macro fix in `caustic_splat.comp` has no anti-literal regression test; a new `SHADOW_MASK_*` truncation site lacks the 8-bit ceiling pin its 24-bit sibling has; `generated_header_contains_all_defines` omits 10 emitted constants including the new `SHADOW_MASK_*` pair; the TAA jitter gate omits the `taa_failed` check present on TAA's other two gates (currently unreachable — `dispatch` is infallible today).

**NIFAL/material** (2 findings): a `977eb95a` classifier change (correct, tested) shipped under the same mislabeled commit; the classifier's `"scrap"` keyword is an unbounded substring match that could over-match genuine scrap-metal clutter (unconfirmed, low-confidence).

**Other tech debt** (~5 findings): `draw.rs`/`draw_frame` LOC figures in an existing tracking issue have drifted stale-low; the submission-order doc is missing `cluster_cull.comp` and its new cross-pass dependency on volumetrics; `wave_amplitude`/`wave_frequency` are parsed but never forwarded to the water shader (documented deferral); the Cornell-box harness has no directional/sun-lighting variant (see below); the Cornell glass-probe docstring misstates `finalAlpha` and the refraction-path gate.

**Meta-finding — CORN-D21-01**: the Cornell-box reference harness used for bisecting lighting regressions exercises *only point lights* — it never inserts a `SkyParamsRes` or a non-zero directional light, so it could not have caught either sun-direction bug above. Suggested: add a `--cornell-sun` variant.

---

## Prioritized Fix Order

1. **Confirm and fix the two sun-direction bugs** (VOL-D16-01, SKY-D18-01) — visual-only but they invert a flagship feature (sun god-rays) and a whole material class (Effect_Lit). RenderDoc/visual confirmation first, then a one-line shader fix each + recompile.
2. **Recompile `composite.frag.spv`** (REN-D3-01) — mechanical, zero-risk, closes a source↔binary drift before it compounds.
3. **Fix the RL-03 ambient-fill gate** (REN-D2-01) — one-line shader fix, meaningfully improves exterior shadow contrast.
4. **Fix the staging-pool capacity ledger** (REN-D5-01) — bounded but silently pressures the BAR heap; straightforward fix (use `allocation.size()` consistently).
5. **Add a `--cornell-sun` harness variant** (CORN-D21-01) — would have caught both MEDIUM sun bugs and prevents recurrence of this bug class.
6. **Doc-rot sweep** — batch-fix the ~15 documentation findings across `shader-pipeline.md`, `memory-budget.md`, and in-code comments; low effort, high value for future audits (several findings this pass exist *because* prior doc-rot pointed auditors at stale line numbers/behaviors).
7. **Hardening/test-coverage gaps** — add the missing regression pins (SHADOW_MASK ceiling assert, caustic anti-literal test, generated-header value-pin completeness, GpuLight 4th-site sync).
8. **SPIR-V target-env unification** — pin one `--target-env` for the whole shader set so the documented recompile command reproduces every binary.
9. **Everything else** — LOW-severity, no urgency.

---

## Needs-RenderDoc

The following are invisible to `cargo test` and require a live Vulkan capture (RTX 4070 Ti dev target) before any code change lands, per the project's no-speculative-Vulkan-fixes policy:

- **VOL-D16-01 / SKY-D18-01**: visually confirm sun god-rays/Effect_Lit shading are actually inverted as predicted, on a daytime exterior cell and a Skyrim-magic-effect NIF respectively, before landing the one-line fixes.
- Cross-render-pass visibility of `COMPUTE_SHADER` availability for the TLAS + cluster-buffer reads feeding the new volumetrics inject dispatch, across an intervening graphics render pass (Dim 4) — sound per the Vulkan memory-dependency model on paper, unconfirmed on hardware.
- Interior-godray two-pass shadow-ray `cullMask` semantics (real-window vs. ceiling-gap disambiguation) — a shading-correctness question, not sync (Dim 4/16).
- Runtime ghosting/disocclusion behavior in SVGF (cf. the existing open "Renderer Ghosting Investigation" memory note) — statically correct, visual outcome unconfirmed (Dim 8).
- Z-fighting at water shorelines given `depth_bias_enable(false)` (Dim 15).
- Actual GPU pixel-level precision at `|world| ≈ 176k–1M` for the camera-relative render-origin system (Dim 10) — statically/unit-test verified, visual outcome at the precision ceiling unconfirmed.
- Skinned-BLAS compute→AS-build input barrier access-flag choice (`SHADER_READ` vs `ACCELERATION_STRUCTURE_READ_KHR`) — per an explicit prior design decision (`#1436`), no validation-layer hazard observed but not capture-confirmed (Dim 9).
- SPIR-V 1.0 + ray-query extension combination on `triangle.frag.spv` — tolerated by the dev driver, portability on stricter drivers unconfirmed (Dim 19).

---

## Dedup Note

All findings were checked against `gh issue list` (200-issue window) and `docs/audits/` history. Two candidate findings were investigated and explicitly disproved before being excluded (egui texture-upload queue-family mismatch; `timestamp_supported` gate conservatism) — recorded in Dimension 20's detail for transparency, not included above since they are non-findings. Three "documented-not-fixed" gaps (`#1793`'s missing-rigid-BLAS recovery path and multi-cell-burst false-eviction, both budget-gated and unreachable on the dev GPU) were recast as regression guards per instructions, not re-reported as new.
