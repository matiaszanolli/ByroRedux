# ByroRedux Renderer Audit — 2026-07-25

Scope: all 22 `/audit-renderer` dimensions, run split across three tiers for
reliability (CRITICAL: dims 1–3, one agent per dim 4–10, MEDIUM: dims 11–22).
Anchored at commit `ca7a4e0e`. Every dimension agent verified its checklist
against live code (grep symbols, read functions/tests) and
`docs/engine/shader-pipeline.md` / `docs/engine/memory-budget.md`, then ran
`cargo test -p byroredux-renderer --lib` (**428 passed / 0 failed** in every
pass) plus, where relevant, `cargo test -p byroredux --bins`
(693 passed / 0 failed / 4 ignored) and
`scripts/check-shader-artifacts.sh` (21/21 SPIR-V artifacts byte-identical).
No speculative Vulkan render-pass/pipeline/barrier changes are proposed
anywhere in this report; any such observation is explicitly deferred to the
Needs-RenderDoc section per standing project policy.

This merge synthesizes the nine dimension reports; no finding was
re-investigated — synthesis and deduplication only.

## Executive Summary

**0 CRITICAL, 0 HIGH, 1 MEDIUM, 13 LOW** new findings, plus a handful of
INFO-level/below-floor observations and three pre-existing open issues
reconfirmed (not re-filed). Five full dimensions (14 Caustic splat, 15 Water,
17 Disney BSDF/soft shadows, 18 Sky/weather, 19 Tangent-space) came back
**fully clean — 0 findings** — a real, notable result across roughly a
quarter of the audit surface, not a coverage gap.

The one MEDIUM finding is a single underlying story that showed up
independently in five different dimension agents' output: **the new FSR 3.1
presentation pass** (`crates/fsr3-sys`, `presentation.rs`, `frame_upscaler.rs`,
`exposure.rs`, landed 2026-07-22→24) restructured the tail of the frame —
G-buffer grew 6→8 color attachments, ACES tone-mapping moved out of
`composite.frag` into `presentation.frag`, and two new passes (upscale +
present) now run between composite and egui — and this restructuring isn't
yet reflected in `docs/engine/shader-pipeline.md`, `_audit-common.md`'s file
listing, or the `/audit-renderer` SKILL.md checklist itself. The code side is
uniformly correct and test-pinned; every instance of this is pure
documentation/checklist drift, consolidated below into one item rather than
repeated per-dimension.

Of the 13 LOW findings, all but one (`OBS-1`, an un-RAII-guarded staging
buffer in `upload_terrain_tiles`) are documentation/comment/checklist drift
with zero runtime impact today. Two of those LOW items (`D10-01`, the
render-origin/TLAS-absolute comment, and `D10-02`, the missing
`fragWorldPosRel` regression test) are flagged as **latent hazards** — no bug
exists today, but a plausible future change (an RT hit-position
reconstruction; a varying rename) would silently reintroduce a real bug that
current tests would not catch.

No CRITICAL or HIGH defect was found anywhere in the RT pipeline, GPU-struct
layout, synchronization, GPU memory lifecycle, material system, denoiser,
skinning, or camera-precision code this pass.

| Dimension | Result |
|---|---|
| 1 — Acceleration Structures (BLAS/TLAS) | 9 PASS / 2 LOW (doc drift) |
| 2 — SSBO/Index + RT ray queries | 11 PASS / 0 |
| 3 — GPU-struct layout lockstep | 8 PASS / 2 LOW (doc drift) |
| 4 — Synchronization & barriers | 7 PASS / 0 (1 doc-drift note, folded into MEDIUM) |
| 5 — GPU memory & resource lifecycle | 8 PASS / 1 LOW (real code gap) + 1 tracked (existing) |
| 6 — NIFAL material canonical translation | 5 PASS / 0 |
| 7 — Material table (R1 dedup) | 6 PASS / 2 LOW + 1 INFO |
| 8 — Denoiser & composite | mostly PASS / 1 INFO (needs RenderDoc) + doc-drift folded into MEDIUM |
| 9 — GPU skinning + BLAS refit (M29) | 6 PASS / 0 above floor; 2 LOW checklist/CLAUDE.md corrections + 3 INFO |
| 10 — Camera-relative render origin & f32 precision | 4 PASS-with-caveats / 3 LOW (2 latent-hazard) |
| 11 — Pipeline state & render pass/G-buffer | 1 MEDIUM (primary FSR item) + folded LOW |
| 12 — Command buffer recording | folded into MEDIUM |
| 13 — TAA | 1 LOW (informational, superseded mechanism) |
| 14 — Caustic splat | 0 — clean |
| 15 — Water + water-side caustics | 0 — clean |
| 16 — Volumetrics & bloom | 0 new (1 existing issue reconfirmed) |
| 17 — Disney BSDF/PBR + soft shadows | 0 — clean |
| 18 — Sky/weather/exterior lighting | 0 — clean |
| 19 — Tangent-space & normal maps | 0 — clean |
| 20 — Debug overlay & GPU telemetry | folded into MEDIUM (duplicate of dim 8's note) |
| 21 — Cornell-box RT harness | 0 new (2 existing issues reconfirmed) |
| 22 — Light animation translation | 1 LOW (checklist test-name drift) |

---

## RT Pipeline Assessment

**BLAS/TLAS correctness (Dim 1): solid.** Build-geometry contract
(`R32G32B32_SFLOAT`/`UINT32`/`OPAQUE`), the three build-flag constants
(`STATIC_BLAS_FLAGS`, `SKINNED_BLAS_FLAGS`, `UPDATABLE_AS_FLAGS`), and
`validate_refit_flags`/`validate_refit_counts` guards are all intact and
test-pinned. `instance_custom_index` correctly carries the shared-map-compacted
SSBO index (not the raw enumerate index), const-asserted `< 2^24` at two sites.
TLAS BUILD-vs-UPDATE decisioning, the empty-TLAS-valid-from-frame-0 invariant,
and column-major→VK-row-major transform conversion are all regression-tested.
Deferred BLAS destruction (#a476b256) is correctly wired through
`pending_destroy_blas`/`DeferredDestroyQueue` everywhere; the handful of
direct `destroy_acceleration_structure` calls found are all either
build-failure rollback or post-compaction-copy teardown of an AS never
referenced by an in-flight command buffer — not live-AS destruction. Two
previously-documented deferred gaps (#1793, PERF-D3-NEW-02 — no per-frame
recovery for a permanently-missing rigid BLAS; a shared frame-counter false-evict
race during a synchronous multi-cell `--grid` burst) remain present and
unreachable on the 12 GB dev card; not re-reported as new.

**SSBO indexing + ray queries (Dim 2): 11/11 clean.** Every ray-query hit site
indexes via `rayQueryGetIntersectionInstanceCustomIndexEXT`, never
`gl_InstanceID`. Shadow rays correctly use
`TerminateOnFirstHitEXT | OpaqueEXT`. The glass/thin-glass IOR gate
(`glassIORAllowed = isGlass && !isThinGlass && rtEnabled && !isWindow && rtLOD < RT_LOD_IOR`)
matches spec verbatim, with all three thin-glass classification tests green.
ReSTIR-DI's spatial normal-cone gate (`SPATIAL_NORMAL_COS = cos 25°`) and
stable surface-ID tagging (`surfaceId & RESERVOIR_SURFACE_MASK`) are both
correct and tested. BC1 punch-through alpha handling matches the CPU-side
`format_has_alpha` gate. No RNG source found other than deterministic
interleaved-gradient noise.

**GPU-skinning → BLAS refit chain (Dim 9): sound.** Palette compute always
dispatches before the vertex-skin compute, with a correctly-scoped
`SHADER_WRITE → SHADER_READ` barrier between them and before
`record_skinned_blas_refit`. The #1790 scratch-serialize dst-mask guard, the
COMPUTE→AS-BUILD `SHADER_READ` (not `AS_READ`) access-flag correction from
#1436, and the refit-vs-rebuild flag/count validation are all intact.
`SkinPushConstants` stays in 3×`u32` lockstep with the GLSL push-constant
block. Bone-palette overflow degrades to bind-pose rendering rather than
truncating silently. Two checklist premises were found stale but the
*underlying code is correct*: `VERTEX_STRIDE_FLOATS` is now 26 (104 B, not the
checklist's/CLAUDE.md's 25/100 B — a tangent lane was added under #783), and
the "skinned output buffer usage flags" checklist item has the #681 fix
direction backwards (the fix was *removing* an unused `VERTEX_BUFFER` usage
bit, not adding one).

**Camera-relative render origin / f32 precision (Dim 10): the two coordinate
conventions (raster render-origin-relative, RT absolute) are never mixed** —
traced through every consumer (`triangle.vert/frag`, `cluster_cull.comp`,
`caustic_splat.comp`, `water.vert/frag`, `volumetrics_inject.comp`,
`ssao.comp`, `composite.frag`, `raytrace.glsl`). CPU-side rebase
(`rebase_model_matrix`) and the skinned vertex-shader rebase both use the same
current-frame origin for current and previous transforms; the previous-frame
VP is separately origin-corrected so motion vectors survive cell-boundary
crossings. `RT_ABSOLUTE_PRECISION_CEILING` (2^20) is enforced via a
debug-assert with a real predicate and test coverage. Two LOW findings here
are genuine **latent hazards** rather than live bugs (see Findings below) and
the whole dimension's failure mode is invisible below ~100k world units from
origin — confirming the rendered result at MarkarthWorld scale needs a
RenderDoc capture, not something `cargo test` can reach.

**Denoiser/composite (Dim 8): SVGF ping-pong, motion vectors, mesh-ID
encoding, the firefly clamp ordering (before the `hasHistory` branch, #1639),
alpha-blend aux-MRT alpha lanes, and the caustic-accumulator
double-count guard are all correct and tested.** One INFO-level deviation from
Schied 2017 §5 (à-trous output isn't fed back into temporal history) is
deliberate per in-code comments but needs a RenderDoc capture to assess
practical convergence-speed impact — see Needs-RenderDoc below.

## GPU-Struct & Memory Assessment

**Layout lockstep (Dim 3): all size/offset pins hold.** `GpuInstance` = 112 B,
`GpuCamera` = 336 B, `GpuMaterial` = 300 B — all `cargo test`-pinned and
matching `shader-pipeline.md`. `GpuMaterial` remains a strict 75-field,
all-scalar (no `[f32;3]`) struct with zero padding bytes, so
`hash_gpu_material_fields`/`as_bytes()` fully determine the dedup key. The
5-site `GpuInstance` shader mirror (`bindings.glsl`, `triangle.vert`,
`ui.vert`, `water.vert`, `caustic_splat.comp`) carries `surfaceId` at the same
relative offset everywhere — no drift. `MAX_INSTANCES`/`MAX_MATERIALS` match
`memory-budget.md` exactly. One repurposed field (`GpuInstance` offset 92,
`_pad_id0` → per-draw optical IOR for the caustic pass) is correctly wired and
byte-identical everywhere, but its *name* still says padding at 4 of 5 shader
sites and its row in `shader-pipeline.md` still says "padding" — LOW,
see Findings.

**Memory lifecycle (Dim 5): 8/8 checklist items pass.** `gpu-allocator`
`MemoryLocation` usage is correctly segregated (`GpuOnly` for every
image/AS-backing allocation, `CpuToGpu` only for staging/per-frame-mapped
buffers, `GpuToCpu` only for screenshot readback). The `AllocatorResource`
ECS-teardown ordering (#1406/#1477/#1483) is enforced by an explicit `Drop`
body that runs on both panic-unwind and `CloseRequested` paths. BLAS/TLAS
scratch high-water-mark growth is monotone; shrink is correctly gated to
cell-unload (BLAS scratch) vs. end-of-`draw_frame` (TLAS scratch) — matching
`memory-budget.md`, not the (stale) SKILL.md checklist wording (see Findings,
`R1-02`). The #1782 deferred-BLAS-scratch-destruction guard and the #1390
`device_wait_idle`-before-free-on-TLAS-resize guard are both intact.
Deferred-destroy countdown (`DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT`) ticks
in the correct order relative to the two-slot fence wait. `VulkanContext::Drop`
reverse-order teardown is complete and correctly sequenced (framebuffers
before render pass, image views before swapchain, allocator before device).
The one real (non-doc) gap found: `SceneBuffers::upload_terrain_tiles`'s
transient staging buffer is not wrapped in the crate's own `StagingGuard` RAII
type that every other staging site uses — an early `?` return between
`allocate`/`bind_buffer_memory`/`mapped_slice_mut` leaks the `VkBuffer` (and,
for the last two, the allocator slab). Bounded impact (cell-transition-only
frequency, requires an allocator OOM/bind failure to trigger) but a real,
mechanical fix. See `OBS-1` in Findings.

**Material table / R1 dedup (Dim 7): fully correct.** Intern stability,
over-cap handling (`overflow_count`, return material 0, `Once`-gated warn),
slot-0 neutral-default seeding, and the byte-exact Hash/Eq key are all pinned.
Debug builds re-verify byte-equality on every dedup hit rather than trusting
the hash alone. No per-instance PBR/alpha/UV-transform/Skyrim-variant field
remains on `GpuInstance` — the R1 Phase 6 closeout still holds structurally
and by test.

**NIFAL material canonical translation (Dim 6): mature and clean, 5/5.**
Exactly two `translate_material` construction call sites exist repo-wide (loose
NIF load, REFR placement); `Material::metalness`/`roughness` are plain
resolved `f32` with an idempotent `resolve_pbr()`; `EmissiveSource` is
resolved once at translate-time with no render-time per-game branch; no
`match`/`if` on a game discriminator exists between `Material` and
`MaterialTable::intern`; particle emitter overrides never touch color, only
kinematics/size.

---

## Findings

### CRITICAL

None found.

### HIGH

None found.

### MEDIUM

#### M-1: Doc drift — the FSR 3.1 presentation pass isn't reflected in `shader-pipeline.md`, `_audit-common.md`, or the `/audit-renderer` checklist

- **Severity**: MEDIUM
- **Dimensions**: 4, 8, 11, 12, 20 (five independent dimension agents surfaced pieces of this same underlying drift)
- **Status**: NEW (consolidated from `REN-11-01`, `REN-11-02`, `REN-12-01`, `REN-20-01`, and matching notes in the Dim 4 and Dim 8 scratch reports)
- **Description**: The 2026-07-22→24 FSR 3.1 work (`crates/fsr3-sys`, `crates/renderer/src/vulkan/presentation.rs`, `frame_upscaler.rs`, `exposure.rs`) restructured the tail of the frame. The code side is internally consistent and fully test-pinned; only the reference docs and the audit checklist itself lag:
  - **G-buffer grew 6 → 8 color attachments.** `shader-pipeline.md`'s "G-Buffer Layout" table still lists six (HDR, normal, motion, mesh_id, raw_indirect, albedo). `gbuffer.rs::GBuffer` now also owns `reactive` and `transparency` attachments (`FSR_MASK_FORMAT = R8_UNORM`), and `context/helpers.rs::create_render_pass` explicitly builds an 8-entry `color_refs` array; all three graphics pipelines' blend-attachment arrays are correctly 8-wide in lockstep (`reflect::tests::triangle_frag_declares_eight_color_outputs` passes).
  - **ACES tone-mapping moved out of `composite.frag` into `presentation.frag`.** Composite now emits render-resolution linear HDR (`HDR_FORMAT = R16G16B16A16_SFLOAT`, `final_layout = SHADER_READ_ONLY_OPTIMAL`) with no tone-map step; `presentation.frag` applies ACES to the *upscaled* image using its own `PresentationPushConstants.exposure`.
  - **Composite no longer writes the swapchain; `presentation.rs` does.** Composite's render pass targets an intermediate HDR image; `presentation.rs`'s render pass owns `final_layout(PRESENT_SRC_KHR)` and binds `swapchain_views`. Two stale comments still credit composite with this: `context/draw.rs`'s egui-pass comment ("Composite already wrote the swapchain image and left it in PRESENT_SRC_KHR" — flagged independently by both Dim 8 and Dim 20 as the exact same line) and the Dim-4 checklist's "composite's outgoing dstStage = NONE" wording (that property now belongs to `presentation.rs`'s `outgoing` dependency).
  - **Submission order gained two steps.** `shader-pipeline.md`'s "Per-Frame Submission Order" stops at step 16–17 (composite → egui). The actual order (`context/post_passes.rs::record_post_passes`) is: SVGF → caustic splat → volumetrics → TAA (gated) → SSAO → bloom → composite → **`frame_upscaler.record`** (FSR 3.1 SDK dispatch or native-blit fallback) → **`presentation.dispatch`** (exposure + ACES + underwater → swapchain) → egui → screenshot copy.
  - **`CompositeParams.underwater` and `depth_params.y` (exposure) are dead uploads.** Both are populated every frame in `draw.rs` and `composite.frag`'s UBO declares both, but `composite.frag`'s `main()` reads neither — the real consumers are `presentation.frag`'s independently-sourced `params.underwater`/`params.exposure` push constants. Doc comments on both fields still assert composite consumes them; contrast `fog_color`/`fog_params`, which *are* correctly annotated reserved-and-unconsumed (#1926/#1927).
  - **`_audit-common.md`'s `VulkanContext` file-listing row is stale.** It lists only `(mod.rs, draw.rs, resize.rs, resources.rs, helpers.rs, screenshot.rs)`; the directory also now contains `geometry_pass.rs`, `post_passes.rs`, and `skinned_blas_refit.rs` (all part of the same #1857/FSR3-era file split).
- **Evidence**: `gbuffer.rs:64-72,234-244`; `context/helpers.rs:86-122`; `pipeline.rs:336-359,640-649,822-850`; `context/post_passes.rs:537-608`; `presentation.rs:136-150`; `context/draw.rs` egui-pass comment (~line 2192-2196); `composite.frag` (grep `depth_params` → only `.x`/`.z` read, never `.y`).
- **Impact**: Zero runtime impact — every piece of this is internally consistent, correctly synced (each new pass declares its own `SubpassDependency` pair, mirroring the pre-existing #1433 egui pattern), correctly torn down in reverse order (`context/mod.rs` Drop, ~line 3285-3300), and correctly instrumented (GPU timer coverage below). The impact is entirely for a future contributor or auditor: tracing a swapchain-write bug by looking in `composite.rs`, or adding a new G-buffer consumer against the stale 6-attachment table, would send them to the wrong place.
- **Positive note**: GPU telemetry was *not* left behind by this refactor — `gpu_timers.rs`'s `QUERIES_PER_FRAME = 28` already includes the two new timed pairs (`cmd_upscale_start/_end`, `cmd_presentation_start/_end`), correctly wired into `post_passes.rs`. Flagged as a real risk area that was checked and came back clean.
- **Suggested Fix**: One doc pass covering `docs/engine/shader-pipeline.md` (G-Buffer table → 8 attachments incl. `reactive`/`transparency`; submission-order list extended with upscale + presentation steps; note ACES's new home), `_audit-common.md`'s `VulkanContext` file row, and the two stale "composite wrote the swapchain" comments (`context/draw.rs`, and the Dim-4/SKILL.md checklist wording). Either drop `CompositeParams.underwater`/`depth_params.y` or re-annotate them "reserved (moved to presentation.frag)" to match the `fog_*` precedent.

### LOW

#### L-1: AS-eviction telemetry ("`missing_blas` counters") is described as surfacing via a `mem.stats` command that doesn't exist
- **Dimension**: 1 (Acceleration Structures)
- **Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs` (`missing_skinned_blas`/`missing_rigid_blas`/`missing_ssbo_instance`, `build_tlas`)
- **Status**: NEW
- **Description**: The three cause-counters do increment correctly and are aggregated into a genuinely useful rate-limited (`log::warn!`, once/sec) diagnostic line. But no `mem.stats` console command exists anywhere in the codebase (only `stats`, `mem.frag`, `ctx.scratch`, `sys.accesses`, `entities`, `systems`, `help` are registered), and none of those read the three counters — they're local to a single `build_tlas` call, not persisted on any resource a command could read.
- **Impact**: No functional impact; an operator trying to use the documented `mem.stats` command to check RT-shadow-missing-BLAS counts would find nothing.
- **Related**: Same class as `L-6` (material dedup telemetry, different subsystem, same "`mem`"-named-command-that-doesn't-exist pattern).
- **Suggested Fix**: Either add the three counters to a persistent resource surfaced by an existing command (`stats` or `ctx.scratch`), or correct the doc/log-comment to describe the actual rate-limited-log mechanism.

#### L-2: `shrink_tlas_scratch_to_fit`'s documented call site ("cell-unload") contradicts both the code and `memory-budget.md`
- **Dimension**: 1 (Acceleration Structures / LRU wiring)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (call site, end of `draw_frame`) vs. `.claude/commands/audit-renderer/SKILL.md`'s Dimension-1 checklist
- **Status**: NEW (doc-drift in the audit tooling itself, not in project docs)
- **Description**: The checklist says `shrink_tlas_scratch_to_fit` runs "at cell-unload (#1226)". It actually runs only from the end of every `draw_frame` (post `current_frame` increment), never from `cell_loader/unload.rs` (which calls the *different* `shrink_blas_scratch_to_fit` instead). The code matches `docs/engine/memory-budget.md`'s own corrected wording (`#1911`/`REN-D1-01`) exactly; only the SKILL.md checklist text is stale.
- **Impact**: No code defect. Risk is purely that a future auditor, trusting the checklist, could flag correct code as regressed — or "fix" the call site to match the wrong description and reintroduce the #1782-class use-after-free that `memory-budget.md` warns about.
- **Suggested Fix**: Update the SKILL.md Dimension-1 bullet to match `memory-budget.md` (draw_frame-end for both `shrink_tlas_to_fit`/`shrink_tlas_scratch_to_fit`; keep "cell-unload (#1226)" only for `shrink_blas_scratch_to_fit`).

#### L-3: `GpuInstance` offset 92 is documented and named as padding, but is live per-draw optical IOR data
- **Dimensions**: 3 (GPU-struct layout), 7 (Material table)
- **Location**: `docs/engine/shader-pipeline.md` (`GpuInstance` table, offset-92 row); `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`_pad_id0`); `crates/renderer/src/vulkan/context/draw.rs` (`_pad_id0: draw_cmd.ior`); `crates/renderer/shaders/caustic_splat.comp` (reads it as `ior`); `include/bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert` (all still say `_padId0`)
- **Status**: NEW
- **Description**: `shader-pipeline.md` lists offset 92 as `*(padding)*`. It is actually populated every draw from `draw_cmd.ior` and consumed by `caustic_splat.comp` as `float ior;` — real, load-bearing data occupying a byte-identical slot at all 5 shader/Rust mirror sites, but named `_pad_id0`/`_padId0` at 4 of those 5 sites (the exception: `gpu_types.rs`'s doc comment, which correctly explains the repurposing).
- **Impact**: No functional defect today (offsets agree everywhere, byte-pinned by tests) — but a reader trusting `shader-pipeline.md`'s byte table would believe offset 92 is free for a future field, when it is already spoken for; and a future editor searching for "unused padding" to reuse could silently break caustic refraction.
- **Suggested Fix**: Rename `_pad_id0`/`_padId0` → `ior` at all 5 sites, and update `shader-pipeline.md`'s table row from "(padding)" to "`ior` — per-draw optical IOR, repurposed padding slot; consumed only by `caustic_splat.comp`".

#### L-4: Audit checklist's "13 `DBG_*` bits" count is stale — the shared catalog now has 23
- **Dimension**: 3 (GPU-struct layout / flag-constant catalog)
- **Location**: `crates/renderer/src/shader_constants_data.rs` (`DBG_BITS` catalog) vs. `/audit-renderer` SKILL.md Dimension 3
- **Status**: NEW
- **Description**: The checklist describes 13 bits spanning `0x1`…`0x1000`. The catalog (the actual single source of truth, hash `8eaade44`) now lists 23 entries spanning to `0x400000` — 10 Session-49-era ReSTIR/SVGF/FSR additions post-date the "13" figure. The catalog mechanism itself (value-pinning + no-redeclare guard) is correctly fixed post-#1860; `generated_header_contains_all_defines`, `triangle_frag_dbg_bits_not_redeclared`, and `dbg_bits_catalog_covers_every_dbg_constant` all pass, confirming every current bit is covered by the mechanism the checklist describes.
- **Impact**: No functional impact — purely a stale count in the checklist text.
- **Suggested Fix**: Update the checklist bullet to "currently 23, `0x1`…`0x400000`".

#### L-5 (real code gap, not doc-only): `SceneBuffers::upload_terrain_tiles` staging buffer isn't RAII-guarded
- **Dimension**: 5 (GPU memory & resource lifecycle)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs`, fn `upload_terrain_tiles`
- **Status**: NEW
- **Description**: The transient staging buffer is created via `device.create_buffer`, then three fallible (`?`-propagating) steps follow before the unconditional teardown at the bottom of the function: `allocator.allocate(...)?`, `bind_buffer_memory(...)?`, `mapped_slice_mut().context(...)?`. An early return from any of these leaks the `VkBuffer` (and, for the last two, the gpu-allocator slab). Every other staging site in the crate wraps this exact window in `buffer::StagingGuard`; this one doesn't.
- **Impact**: Bounded — cell-transition-only frequency, and the trigger requires an allocator OOM or bind failure. Still a genuine resource leak on that path, and the only one of its kind found in the audit (every sibling staging site is correctly guarded).
- **Suggested Fix**: Mechanical — construct a `StagingGuard` immediately after the successful `allocate` + `bind`, and call `guard.destroy()` at the end, matching every other staging call site in the crate.

#### L-6: Material dedup telemetry doc/log comments reference a nonexistent `mem` command
- **Dimension**: 7 (Material table)
- **Location**: `crates/renderer/src/vulkan/material.rs` (`overflow_count`/`collision_count` doc comments, `INTERN_OVERFLOW_WARNED` log text), `byroredux/src/main.rs` (debug-assert message)
- **Status**: NEW
- **Description**: Multiple comments/log lines say "via the `mem` command" / "Run `mem` to confirm". The actual operator-facing surface is the **`ctx.scratch`** console command (`commands/world_info.rs`), which does correctly print `materials: N unique / M interned (X× dedup)` plus an `OVERFLOW n → id 0` suffix when non-zero. Only `mem.frag` (fragmentation) and `ctx.scratch` are registered; there is no bare `mem` command.
- **Impact**: No functional impact — the real surfacing mechanism (`ctx.scratch`) works and is correctly gated by `debug_assert_eq!(overflow_count, 0)` (#1428). Purely misleading operator-facing text.
- **Related**: Same pattern as `L-1` (AS telemetry), different subsystem.
- **Suggested Fix**: Update the doc comments/log text to say `ctx.scratch`, not `mem`.

#### L-7: TAA checklist describes a retired mechanism (#1497's progressive alpha floor), not the current one
- **Dimension**: 13 (TAA)
- **Location**: `crates/renderer/src/vulkan/taa.rs` (`upload_params`), `crates/renderer/shaders/taa.comp`
- **Status**: NEW (informational — confirmed NOT a regression of #1497's original bug)
- **Description**: The checklist describes "moving-pixel accumulation α floored under a parked camera, regression guard #1497" — commit `c6342845`'s per-pixel floor driven by a `static_frames` counter. A later refactor (`e5d02f83`) deleted `static_frames` entirely and hardcoded `let alpha = 0.1;`, replacing the mechanism with a per-pixel octahedral-normal surface-consistency disocclusion test (`dot(currNormal, prevNormal) < 0.85`) alongside the existing mesh-ID/offscreen/alpha-blend checks. The in-code rationale is explicit and deliberate ("driving this weight toward 1/256 while parked turns any invalid final-colour sample into a persistent translucent after-image").
- **Impact**: None — the original #1497 hazard cannot recur (the mechanism that produced it no longer exists), and the replacement is architecturally sound and test-pinned (`taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces` passes). Only the checklist wording is stale.
- **Suggested Fix**: Update the Dimension 13 checklist bullet to describe the current flat-α + normal-validated-disocclusion design.

#### L-8 (latent hazard): stale comment invites a future RT hit-position bug via `GpuInstance.model`
- **Dimension**: 10 (Camera-relative render origin)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs`, frustum-culled-instance comment above the `GpuInstance` push in `draw_frame`
- **Status**: NEW
- **Description**: Since the render-origin rebase work, `GpuInstance.model` is render-origin-**relative** while the TLAS it's paired with is absolute. The comment above the push still promises RT hit shaders "the right material / **transform** (#516)". Today the only RT reader of `.model` (`raytrace.glsl::getHitTriNormal`) is translation-invariant (`cross(w1-w0, w2-w0)`), so nothing is wrong yet — but the comment as written invites a future RT hit-position reconstruction from `hitInst.model` that would land `renderOrigin` (up to ~176k units on MarkarthWorld) away from the true hit.
- **Impact**: No current corruption; a latent trap for the next RT feature that reads `.model` as a world-position transform.
- **Suggested Fix**: Amend the comment: "rotation/scale valid for RT; translation is origin-relative — add `renderOrigin` before using as a world position."

#### L-9 (latent hazard): no regression-guard test for the `fragWorldPosRel` render-origin convention
- **Dimension**: 10 (Camera-relative render origin)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` (absent sibling); `crates/renderer/shaders/triangle.vert`/`triangle.frag`
- **Status**: NEW
- **Description**: The #1496 split (relative varying `fragWorldPosRel`, `main()`-top reconstruction to absolute `fragWorldPos`, four derivative consumers reading the relative name only) is enforced solely by shader comments — unlike the sibling #1486 convention, which has a static source-check test (`triangle_vert_skinned_branch_rebases_render_origin`). `grep -rn fragWorldPosRel --include=*.rs` returns zero hits repo-wide. A refactor that renames the varying or switches a derivative consumer to the absolute local compiles clean and passes all 428 renderer tests.
- **Impact**: Silent regression to pre-#1496 derivative ULP noise (~0.0156 u at `|world| ≥ 131k` → faceted/banded normal-map and POM shading) — invisible near the origin and therefore invisible to CI; needs a large-world scene/RenderDoc to observe.
- **Suggested Fix**: Add a static source-check test mirroring #1486's, asserting `triangle.vert` contains `fragWorldPosRel = worldPos.xyz`, `triangle.frag` contains `fragWorldPosRel + renderOrigin.xyz`, and the four known derivative call sites spell `fragWorldPosRel`.

#### L-10: `GpuCamera.render_origin.w` documentation is inconsistent — some sites say "unused", but it carries the FSR-reset-flag payload
- **Dimension**: 10 (Camera-relative render origin)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`render_origin:` field comment), `crates/renderer/shaders/water.vert`, `crates/renderer/shaders/cluster_cull.comp`, `docs/engine/shader-pipeline.md` ("Coordinate Spaces & Precision")
- **Status**: NEW
- **Description**: `draw.rs` uploads `if fsr_reset_pending { 1.0 } else { 0.0 }` into `render_origin.w`, and `triangle.frag` reads `clamp(renderOrigin.w, 0.0, 1.0)` in the FSR-reset debug view — `gpu_types.rs` and `triangle.vert` document this correctly. But the `draw.rs` comment says "(xyz; w unused)" two lines before explaining the FSR payload, and `water.vert`/`cluster_cull.comp` still say "w unused"; `shader-pipeline.md`'s spec section documents only `xyz`.
- **Impact**: Documentation only — same class of trap as the already-tracked `VolumetricsParams.render_origin.w` case (#1928/`REN-D10-01`, which packs `is_exterior`), inviting a future repurposing collision.
- **Suggested Fix**: Fix the `draw.rs` comment and the two shader comments to describe the FSR-reset payload; add `w` to the doc's coordinate-spaces section.

#### L-11: Light-animation checklist cites a test name that was consolidated/renamed
- **Dimension**: 22 (Light animation translation)
- **Location**: `crates/core/src/ecs/components/light.rs` (test at `light_anim.rs:236`, formerly `fallout4_shadow_spotlight_is_not_slow_pulse`)
- **Status**: NEW
- **Description**: The checklist cites `fallout4_shadow_spotlight_is_not_slow_pulse`. The current suite consolidated this into a broader, multi-game test, `shadow_spotlight_bit_never_leaks_into_animation_on_any_game`, which covers FO4 and other games in one assertion set. The sibling test `fallout4_real_flicker_and_pulse_map_to_shared_behavior` still exists verbatim.
- **Impact**: None — coverage is provably stronger (any-game, not FO4-only) under a different name than the checklist expects.
- **Suggested Fix**: Update the checklist's cited test name.

#### L-12: `VERTEX_STRIDE_FLOATS` is 26 (104 B), not the 25/100 B still cited in the checklist and in this project's own `CLAUDE.md`
- **Dimension**: 9 (GPU skinning)
- **Location**: `crates/renderer/src/shader_constants_data.rs` (`VERTEX_STRIDE_FLOATS = 26`); `crates/renderer/src/vertex.rs` (`Vertex`, 104 B after the `[f32;4]` tangent lane, #783/M-NORMALS); project `CLAUDE.md`'s Quick Reference (`vertex.rs` line still says "9 attribute descriptions, 100 B (19 f32 + 4 u32 + 8 u8)")
- **Status**: NEW
- **Description**: The stride constant, its GLSL-side macro, and every consumer (`skin_compute.rs`, `skin_vertices.comp`) all correctly derive from `VERTEX_STRIDE_FLOATS = 26` and are cross-pinned by three tests plus a byte-identical `.spv` artifact check. Only the human-facing docs (audit checklist + this repo's own `CLAUDE.md`) still say 25/100 B, predating the tangent-lane addition.
- **Impact**: None functionally. `CLAUDE.md` is checked into the repo and read by every future session, so this is worth fixing at the source, not just in the audit checklist.
- **Suggested Fix**: Update `CLAUDE.md`'s `vertex.rs` line to "104 B (19 f32 + 4 u32 + tangent `[f32;4]` + 8 u8)" or the current accurate breakdown, and correct the SKILL.md checklist's "25/100 B" figure.

#### L-13: Checklist has the #681 skin-buffer usage-flags fix direction backwards
- **Dimension**: 9 (GPU skinning)
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs` (`SkinComputePipeline::create_slot`)
- **Status**: NEW (checklist-only correction; code is correct)
- **Description**: The checklist implies the skinned-output buffer is missing a needed `VERTEX_BUFFER` usage flag. In fact commit `b99ae91e` ("Fix #681 (MEM-2-6): drop unused `VERTEX_BUFFER` from skin_compute output") deliberately *removed* it — M29.3 raster still inline-skins in `triangle.vert`, nothing binds the slot buffer as a VBO, and the omission narrows the memory-type mask `gpu-allocator` must satisfy. The current flags (`STORAGE_BUFFER | SHADER_DEVICE_ADDRESS | ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`, no `VERTEX_BUFFER`) are correct as-is.
- **Impact**: None — code is correct; only the checklist's framing of #681 is inverted.
- **Suggested Fix**: Correct the checklist to state the flag was deliberately *removed*, and note it should only be re-added in the same commit that lands a Phase-3 raster bind path consuming the slot buffer as a VBO.

### INFO / Below Reporting Floor (preserved, no action required)

- **D7-03** (Dim 7): three per-instance duplicates of material data remain by
  design (`texture_index` for the UI-quad path, `avg_albedo_{r,g,b}` and `ior`
  for the caustic pass's own descriptor set) — all documented in `gpu_types.rs`
  and sourced from the same `DrawCommand` fields that feed `to_gpu_material`,
  so there is no divergence risk.
- **OBS-2** (Dim 5): `TextureRegistry` is still grow-only with no slot reuse on
  cell revisit — already tracked as #2030/MEM-D3-01 in `memory-budget.md`, not
  re-reported.
- **Dim 9 INFO**: (1) `blas_skinned.rs` derives `vertex_stride` from
  `size_of::<Vertex>()` while `skin_compute.rs` uses `VERTEX_STRIDE_BYTES` —
  equivalent, cross-pinned, just two spellings of one contract. (2) The
  `Once`-gated `SKIN_DROPOUT_DUMPED` warn in `skinned.rs` is the bone-*dropout*
  diagnostic, not the slot-pool overflow guard (that's `SkinSlotPool`'s own
  `overflow_warned`) — worth knowing if a future audit greps the wrong file
  for the overflow warning.
- **Existing issues reconfirmed still open, not re-filed**: #1938
  (`VOL-D16-02`, stale "composite multiplies by 0.0" comment — code doesn't do
  this, only the comment; simply relocated under the #1857 file split, same
  substance); #1942 (`CORN-D21-01`, Cornell harness exercises only point
  lights, no directional/volumetric coverage); #1943 (`CORN-D21-02`, `glass()`
  docstring/comment inaccuracies in `cornell.rs`).
- **Dim 1's two already-tracked deferred gaps** (#1793, PERF-D3-NEW-02) remain
  present and unreachable on the 12 GB dev card; not re-reported as new (see RT
  Pipeline Assessment above).

---

## Clean Dimensions (0 findings)

Five dimensions came back with **zero findings of any severity** this pass —
worth stating plainly rather than burying in the tally table:

- **Dimension 14 — Caustic splat (#321)**: per-FIF `R32_UINT` accumulator
  lifecycle, `imageAtomicAdd` fixed-point accumulation, and the named-constant
  source-pixel gate (`INSTANCE_FLAG_CAUSTIC_SOURCE`) all check out.
- **Dimension 15 — Water (M38) + water-side caustics**: Fresnel F0 correctly
  distinct from glass's IOR-derived F0; RT reflection/refraction miss
  fallbacks correct; dynamic `CULL_MODE`; water-caustic accumulator lifecycle
  independently correct.
- **Dimension 17 — Disney BSDF/PBR gating + soft shadows**: single
  `MAT_FLAG_PBR_BSDF` gate; anisotropic GGX clamping; sheen/diffuse split
  matches Disney-2012 convention; sun angular radius defaults and ceiling
  assert intact.
- **Dimension 18 — Sky/weather/exterior lighting (M33/M34)**: cloud-scroll
  rate correctly WTHR-driven; below-horizon ground color uses authored tint,
  not a fake fallback; fog/RT-shadow interior-vs-exterior gating correct.
- **Dimension 19 — Tangent-space & normal maps (M-NORMALS)**: Bethesda
  tangent/bitangent swap correctly decoded with explicit anti-re-flip
  commentary; FO4+ packed-tangent gating is feature-flag-based, not
  BSVER-band-based.

---

## Prioritized Fix Order

Correctness → safety → optimization/tidiness, per project convention. Nothing
in this pass rises to correctness-blocking; the order below reflects
risk-reduction value per unit of effort.

1. **`L-5`** — Wrap `upload_terrain_tiles`'s staging buffer in `StagingGuard`.
   The only real (non-doc) code gap found this pass; mechanical, low-risk fix
   that closes an actual leak path.
2. **`L-9`** — Add the missing `fragWorldPosRel` static source-check test.
   Cheap insurance against a large-world-only regression that would otherwise
   be invisible to CI.
3. **`L-8`** — Amend the `GpuInstance`/TLAS-transform comment in `draw.rs`.
   Also cheap, and closes off a plausible path to a real bug in the next RT
   feature that touches hit-position reconstruction.
4. **`L-3`** — Rename `_pad_id0`/`_padId0` → `ior` at all 5 sites and fix the
   `shader-pipeline.md` table row. Prevents a future "reuse this padding"
   mistake.
5. **`M-1`** — One consolidated documentation pass: `shader-pipeline.md`
   (G-buffer table + submission order + `CompositeParams` field annotations),
   `_audit-common.md`'s `VulkanContext` file row, and the two stale
   "composite wrote the swapchain" comments. Highest total word-count but
   lowest individual risk; batching it avoids five more piecemeal doc PRs.
6. **Remaining LOW documentation/checklist corrections** (`L-1`, `L-2`, `L-4`,
   `L-6`, `L-7`, `L-10`, `L-11`, `L-12`, `L-13`) — batch into the same or a
   follow-up doc pass; `L-12` (CLAUDE.md's stale vertex-byte-size) is worth
   prioritizing slightly above the rest since it's a checked-in project file
   read every session, not just an audit-tooling artifact.

## Needs-RenderDoc

Findings/observations whose failure mode is invisible to `cargo test` and
requires a validation-layer or RenderDoc capture (not `cargo test`) before any
change should be considered:

- **SVGF à-trous output not fed back into temporal history** (Dim 8,
  `svgf.rs::write_atrous_descriptor_sets`). Deviates from Schied 2017 §5
  (which feeds the first à-trous iteration back as next frame's history);
  deliberate per in-code comments ("post-filter integration"), but the
  practical cost (slower convergence / residual temporal noise) is purely
  visual and unquantified. INFO-level, not actionable without a capture.
- **Confirming the Dimension 10 render-origin/precision result at actual
  large-world scale.** Everything in that dimension was verified
  structurally + by static source pins; the failure mode only manifests at
  `|world| ≳ 100k` (e.g. MarkarthWorld ≈ −176k). Suggested repro:
  `cargo run --release -- --game skyrim --cell MarkarthWorld… --bench-frames N --bench-hold`,
  checked for no double-added origin on skinned actors, no derivative
  banding, and RT ray-bias headroom.
- **BVH quality decay across the 600-frame skinned-BLAS refit threshold**
  (Dim 9, `SKINNED_BLAS_REFIT_THRESHOLD`). The threshold is a policy choice;
  only a capture shows whether traversal cost actually climbs before a forced
  rebuild fires.
- **`should_skip_skin_gpu_refresh` (#1811) stale-palette risk on a
  just-woken NPC** (Dim 9). The reasoning for suppressing the refresh after
  `MAX_FRAMES_IN_FLIGHT` clean frames is sound, but a stale-palette artifact
  would only be visible in a capture, not a unit test.
- **`record_scratch_serialize_barrier`'s full per-frame serialization of
  skinned BLAS builds/refits** (Dim 9, #1797/D6-03). The throughput ceiling
  under a moving-crowd frame is real but unquantified; the decision not to
  shard scratch is already documented as deliberate.

No barrier/render-pass/pipeline defect was found in any dimension this pass —
the Needs-RenderDoc items above are all either INFO-level deviations from a
reference algorithm, or performance-policy questions, not sync-correctness
gaps.
