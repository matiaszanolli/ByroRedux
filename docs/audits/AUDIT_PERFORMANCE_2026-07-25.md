# Performance Audit — ByroRedux — 2026-07-25

**Scope**: all 9 dimensions, depth = deep. Part of a `comprehensive` audit-suite sweep.
**Hardware target**: RTX 4070 Ti (12 GB) + Ryzen 7950X (16c/32t). RT VRAM min 6 GB.
**HEAD**: `ca7a4e0e`.
**Bench-of-record**: ROADMAP.md `8a668eff` (R6a-stale-15, 2026-07-18) refreshed by the
FSR phase-7 matrix at `e153b50c` (2026-07-24) — both cited below, not re-derived.
**Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json …`
(29 open at sweep time) plus a supplementary `--state all --limit 1000` pull used by
Dimensions 4/5 for GitHub-issue-filing checks.
**Prior report**: `docs/audits/AUDIT_PERFORMANCE_2026-07-19.md` — every regression guard
that report verified was re-verified here; erosions and new findings are called out
explicitly below.

---

## Executive Summary

This sweep surfaces **two independent HIGH-severity temporal/motion-vector bugs**, on
top of one already-diagnosed HIGH shader-cost regression, plus a broad set of clean
guard re-verifications.

**The most consequential NEW finding** is **PERF-D9-NEW-01**: the `camera_cut`
heuristic added by `6c56e311` (2026-07-19) compares camera-*relative* view-projection
matrices, so it misfires on ordinary camera motion (any translation above ~0.55–0.75
world units/frame — i.e. normal walk/run speed) **and** on every 4096-unit render-origin
snap (a ~5562-unit apparent jump). Either trigger forces a full TAA history reset, an
SVGF recovery restart, an FSR reset, and zeroes both camera and per-object motion
vectors for that frame. In practice this means the engine is very often reconstructing
from a single spatial frame instead of accumulating temporally — while the player is
simply walking — and re-opens exactly the cell-boundary flash that `#1489` was closed
to fix. This is a **plausible additional contributor to `PERF-REGRESSION-6c56e311`**
that a `triangle.frag.spv`-only bisect would not isolate, because the heuristic lives
host-side and is present in both arms of that swap. No test covers it.

**The second HIGH finding**, `PERF-D4-01`, is a related but distinct bug in the same
commit family (`33d9a468`, 2026-07-22): a new per-frame rigid motion-history map keys
on `DrawCommand::entity_id`, and particle draws synthesize that field as `entity ^ i`
— a value that routinely collides with real ECS entity IDs, so a static mesh can
inherit a billboard's previous-frame transform and receive a bogus motion vector.

**The third HIGH finding**, `D5-01`, is **not new** — it is `PERF-REGRESSION-6c56e311`,
already tracked in ROADMAP.md Known Issues since 2026-07-24: a ~2.2× main-pass
fragment-shader cost regression from two deliberate ray-tracing quality features
(alpha-aware shadow transmittance, a bounded GI path tracer). Verified here line-by-line
against current code with no drift from the ROADMAP narrative. It is **not yet filed as
a GitHub issue** — recommended below. No fix is proposed: ROADMAP already evaluated and
rejected the one available no-quality-cost mitigation, and PERF-D9-NEW-01 above (found
independently in this sweep) may mean the *measured* 2.2× even understates the real-world
cost, since the bisect that measured it never triggered `camera_cut`'s zero-motion-vector
path in a way that would show up as a distinct number.

Beyond the three HIGH items, this session's commits also introduced one MEDIUM
regression (`D2-01`: the `#1804` two-sided-blend-split gate was narrowed back open,
re-paying a dead FRONT-cull pass on every particle batch) and one MEDIUM
telemetry-correctness bug (`PERF-D9-NEW-02`: an origin-crossing diagnostic log always
prints zero because it reads state after that state was overwritten — precisely the
diagnostic that would have caught PERF-D9-NEW-01 sooner).

Every other Session-46/47 and interim regression guard (18 named guards, 13 of them in
Dimension 9 alone) was individually re-verified against live code and test output.
**Two guards eroded** (the `#1804` blend-split limb and the `#1489` origin-correction
precedence), both dated to specific commits with git evidence, not speculation. Four
items flagged as open in the 2026-07-19 report (#2111, #2112, #2113, #2114, #2115) are
now confirmed **fixed and landed**, verified by re-reading the fix commits and, where
practical, re-running the tests.

### Findings by severity

| Severity | Count | IDs |
|----------|-------|-----|
| **CRITICAL** | 0 | — |
| **HIGH** | 3 | PERF-D9-NEW-01 (`camera_cut` false-positives defeat #1489, NEW), PERF-D4-01 (particle/entity-ID motion-history collision, NEW), D5-01 (`PERF-REGRESSION-6c56e311`, Existing/ROADMAP-tracked, unfiled) |
| **MEDIUM** | 6 | PERF-D1-01 (scheduler timing tracker always armed), D2-01 (two-sided blend split regression), D2-02 (opaque RT overdraw / no depth pre-pass, Existing), PERF-D4-02 (memory-budget.md 52% SSBO undercount), D6-01 (skinned-vertex output buffer 8.7× oversized), PERF-D9-NEW-02 (origin-delta diagnostic always logs zero) |
| **LOW** | 14 | see Findings table below |

No CRITICAL findings. No Vulkan spec violations, no AS/SSBO index corruption, no
missing barriers, no memory-safety issues found in this sweep.

### Observed-vs-ROADMAP bench delta

No benches were re-run in this session (headless Vulkan + on-disk game data is
smoke-test territory per project convention; per the "No Parallel Engine Launch" rule
this audit did not start a competing engine process). Citing ROADMAP.md as the
authority:

- **Bench-of-record** (`8a668eff`, R6a-stale-15, 2026-07-18): Prospector (FNV)
  145.1 FPS/6.90 ms, Whiterun (Skyrim SE) 335.0 FPS/3.00 ms, MedTek (FO4)
  74.4 FPS/13.49 ms — all vs. the prior record, see ROADMAP for the full delta table.
- **R6a-stale-16 refresh** (`e153b50c`, 2026-07-24) found the bench-of-record's TAA
  numbers had silently collapsed (Prospector 145.1 → 68.5 FPS) — this is
  `PERF-REGRESSION-6c56e311`, introduced 5 days and ~80 commits earlier by `6c56e311`
  and undetected until the refresh, exactly the failure mode the stale-bench tracker
  exists to catch.
- **FSR 3.1 Quality is now the shipped default** (`5c7acfe2`, closing R6a-stale-16),
  recovering +40–68% frame time across every measured scene by shading fewer pixels —
  a symptom mitigation, not a fix for the regression underneath (ROADMAP is explicit
  that the default flip should not read as lowering urgency). **PERF-D9-NEW-01** means
  this recovery number itself may be measured under a worse `camera_cut` reset rate
  than a fixed heuristic would show, since FSR receives `reset=true` on the same
  false-positive frames.
- This audit's own new findings (PERF-D9-NEW-01, PERF-D4-01, D2-01) are dated to
  commits after the `8a668eff`/`e153b50c` benches (`6c56e311`, `33d9a468`, `883f57cd`)
  and are not reflected in any existing bench-of-record number — they are
  visual-correctness/temporal-stability findings that a plain FPS counter would not
  directly surface, though PERF-D9-NEW-01's forced history resets would show up as
  *worse-than-expected* TAA/FSR quality-matrix SSIM scores if re-measured with per-frame
  reset logging.

---

## Hot Path Analysis

**Per-frame CPU (Dim 1, 9)**: all seven named Session-46 scratch/gating guards intact
(`drain_dirty_into`, `AnimScratch`, `last_cam` billboard gate, `build_debug_ui_snapshot`
gating, `SkinSlotPool` idle-slot contraction, `bone_world` no-clear reuse,
`emit_particles` dead-probe removal). One new MEDIUM finding: the #1647 scheduler
per-system timing gate is defeated because `boot.rs` inserts `SchedulerSystemTimings`
unconditionally rather than only when the debug UI is open — every system pays a
`String` alloc + mutex lock every frame for a ≤2 Hz consumer. Two LOW findings
(an un-scratched `collect()` in the GI-priority light sort; a stale draw-sort-threshold
calibration table after the 10→11-tuple sort-key widening).

**Draw & instancing (Dim 2)**: sort-key ordering, indirect-draw batch folding, and
per-draw state-change gating are all still correct and measured against the current
Prospector/Whiterun/MedTek draw counts. One MEDIUM regression: the #1804 two-sided
blend-split gate lost its `z_write` limb in `883f57cd`, re-broadening the FRONT/BACK
cull split to particle batches (its own guard tests were inverted to match, so
`cargo test` is silent). The opaque-overdraw-vs-depth-prepass architectural question
(Existing, unfiled) now carries more weight given `PERF-REGRESSION-6c56e311` made every
occluded fragment ~2.2× more expensive.

**GPU memory pressure (Dim 3)**: BLAS dynamic budget, mid-batch eviction gate (#1792),
LRU smoothness, all shrink floors, `MeshRegistry` caps, BGSM/BGEM half-eviction, and
`NifImportRegistry` LRU all intact. FSR 3.1's new GPU resources (upscaler outputs,
reactive/transparency masks, SDK working memory) are leak-free and FIF-correct — but
none of them are in `docs/engine/memory-budget.md` yet (LOW, doc rot, same class as
two previously-closed findings). One LOW error-path leak: `FrameUpscaler::create_outputs`
doesn't free its `gpu-allocator` sub-allocation if `bind_image_memory` fails.

**SSBO sizing & upload (Dim 4)**: `GpuInstance` (112 B) and PBR-resolved-once guards
both intact; every per-frame upload remains O(live data), content-hash gated, with
O(1) amortized material dedup. New this session: the HIGH particle/entity-ID collision
finding above, plus a MEDIUM doc-rot finding — the same `33d9a468` commit added a full
extra ~34 MB (previous-model) + ~25 MB (persistent bind-inverses) of resident SSBO that
`memory-budget.md`'s ≈140 MB total never accounted for; actual total is ≈213 MB, a 52%
undercount.

**GPU pipeline (Dim 5)**: `PERF-REGRESSION-6c56e311` is verified, line-by-line, still
present exactly as ROADMAP describes (`traceShadowTransmittance`'s two closest-hit
walks; the bounded GI path tracer's `MAX_PATH_SEGMENTS=6`/`MAX_DIFFUSE_BOUNCES=2`) — up
to ~336 nested ray queries per pixel in the worst case, entirely inside the fragment
shader, confirmed not to be a draw/instancing/sort issue (Dim 2 boundary). Both the
legacy-WRS compile-time gate (#1799) and the `inv_vp`-on-CPU / no-per-fragment-`inverse()`
guard remain intact; volumetrics/bloom stay strictly O(pixels)/O(froxels); the TLAS
build→read barrier is present and correctly scoped. Two LOW doc-rot findings on
`gbuffer.rs` attachment-count comments (5→7 attachments after FSR 3.1's two masks)
round out the dimension.

**Skinning & BLAS (Dim 6)**: all five named guards (compute-pass palette, dispatch-dirty
gate #1195, BLAS-refit gate #1196, descriptor-rewrite skip #1197, early-return rollback
#1791/#1796) verified intact with a clean `cargo test -p byroredux-renderer --lib skin`
run (34/34). One new MEDIUM finding: the skinned-vertex output buffer stores the full
104 B `Vertex` per vertex when the only consumer (BLAS build) reads 12 B — an 8.7×
VRAM/bandwidth over-allocation dating to the M29.5 design, now large enough (~216 MB on
a dense NPC crowd) to be worth narrowing. `#1797`'s shared-scratch serialization ceiling
remains correctly unmeasured/undecided, not re-filed.

**Streaming & cells (Dim 7)**: two-phase `pre_parse_cell` (#877), small-model
fast-path (#1262), process-lifetime NIF cache, leak-free shutdown drain, and
once-per-boot CDB parsing all intact (26/26 streaming tests pass). The prior audit's
one LOW finding (#2113, pending-request cancellation) is confirmed fixed. The
interior/sub-cell NPC spawn budget (#1798) remains the one open, already-tracked
architectural gap — no regression, no new finding.

**NIF parse (Dim 8)**: `read_pod_vec`, `#[must_use] allocate_vec`, split per-block
counters, and import-only particle-block parsing all intact; confirmed (again) that
`bytemuck` is not a workspace dependency, correcting a claim some earlier audits made.
Both prior open items (#2111 header re-parse, #2114 dhat geometry-bound gap) are
verified fixed with actual test runs, not just git log. Zero new findings.

**Telemetry & origin cost (Dim 9)**: this dimension carries the audit's most important
new finding. Twelve of thirteen checked guards pass cleanly: GPU timestamp readback
never stalls the current frame, is one batched call (#2041), gates unwritten queries,
extends the same non-blocking pattern to the new FSR `upscale`/`presentation` brackets,
`ScratchTelemetry` refresh is allocation-free, and the camera-relative-origin CPU cost
(`assemble_camera`'s one extra `look_at_rh`, the inline per-instance rebase) stays
negligible and inside the existing O(visible) loop exactly as designed. The thirteenth
guard — "a grid crossing does not drop TAA/SVGF/FSR history" (#1489) — **fails**: see
`PERF-D9-NEW-01`. A second finding (`PERF-D9-NEW-02`) shows the diagnostic trace added
for the open ghosting investigation always logs a zero origin-delta because it reads
state one write too late — actively hiding the frames `PERF-D9-NEW-01` corrupts. Two LOW
doc-rot findings round out the dimension (stale bracket counts in `gpu_timers.rs`
comments; `gpu_breakdown()` not yet reporting the new `upscale`/`presentation` timers).

---

## Findings

Findings are grouped CRITICAL → HIGH → MEDIUM → LOW. Cross-dimension duplicates
(two dimensions independently flagging the same code) are merged into one entry
with both citing dimensions noted.

### HIGH

#### PERF-D9-NEW-01 — `camera_cut` heuristic compares camera-relative matrices, misfires on ordinary motion and every origin crossing, defeating #1489
- **Severity**: HIGH
- **Dimension**: Telemetry & Camera-Relative Origin Cost (Dim 9) / temporal pipeline
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:910-948` (also consumed at
  `:1491`)
- **Status**: NEW (no match in `/tmp/audit/performance/issues.json`; not covered by the
  `PERF-REGRESSION-6c56e311` ROADMAP entry, which root-causes only `triangle.frag`)
- **Description**: Commit `6c56e311` (2026-07-19 — the same commit that caused
  `PERF-REGRESSION-6c56e311`) added an automatic camera-cut detector:
  ```rust
  let vp_max_abs_delta = vp.iter().zip(self.prev_view_proj.iter())
      .map(|(a, b)| (a - b).abs()).fold(0.0_f32, f32::max);
  let camera_cut = self.frame_counter > 0 && (camera_delta > 256.0 || vp_max_abs_delta > 0.75);
  ```
  `vp` and `self.prev_view_proj` are **camera-relative** matrices (built by
  `assemble_camera` as `proj * look_at_rh(cam_pos - render_origin, …)`,
  `byroredux/src/render/camera.rs:182-187`), and `self.prev_view_proj` is stored
  *un-corrected*, relative to the **previous** frame's origin. A raw element-wise
  comparison of two projection matrices is sensitive to (a) ordinary camera
  translation and (b) the 4096-unit render-origin snap — both make `camera_cut` true.
  When it fires, it calls `signal_temporal_discontinuity(8)` (SVGF recovery restart),
  `taa.signal_history_reset()` (zeroes `frames_since_creation`, forcing TAA back to
  first-frame mode), `fsr.signal_reset()`, and `previous_rigid_models.clear()`; it also
  sets `pvp = *vp` so the uploaded `prev_view_proj == view_proj` (zero camera motion
  vectors, bypassing `origin_corrected_prev_view_proj` entirely) and forces
  `previous_source = m` for every rigid instance (zero object motion vectors).
- **Evidence**: Reproducing the engine's default projection
  (`Camera::default().fov_y = FRAC_PI_4`, `perspective_rh` + Y-flip) at 16:9, the
  `0.75` threshold corresponds to:

  | camera delta this frame | `max|ΔVP|` | `camera_cut` |
  |---|---:|---|
  | lateral 0.25 u | 0.34 | no |
  | lateral 0.55 u | 0.75 | **at threshold** |
  | forward 0.75 u | 0.75 | **at threshold** |
  | forward 6.0 u (≈360 u/s @ 60 fps) | 6.00 | **yes** |
  | render-origin snap, 4096 u (cell crossing) | **5562** | **yes** |

  Bethesda-unit locomotion speeds (walk ≈100 u/s, run ≈350-400 u/s) give 1.7–6.7
  units/frame at 60 fps — 2× to 12× over the trip point. `camera_delta > 256.0`
  (absolute positions) is the limb that *would* correctly catch a teleport; the VP
  limb is what misfires. The `#1489 / REN2-04` comment block and the exactness proof
  in `origin_corrected_prev_view_proj` (unit-tested) are both still correct — but on
  the one frame class they exist for (a grid crossing), the `camera_cut` branch takes
  precedence and the correction never runs. `grep camera_cut` finds no test coverage
  anywhere in the tree.
- **Impact**: Re-opens exactly the failure #1489 closed (full-screen TAA flash + SVGF
  history drop on every 4096-unit cell-boundary crossing) — and is far worse in the
  general case: while the player is moving at all, TAA's `frames_since_creation` is
  re-zeroed every frame, so the temporal resolve never accumulates; SVGF sits
  permanently at its recovery alpha (0.5) instead of steady-state (0.2); and FSR 3.1
  — now the default upscaler — receives `reset=true` every frame, degrading a
  66%-render-resolution reconstruction to a single-frame spatial upscale. Motion
  vectors are identically zero on those frames, so any denoiser/upscaler that survives
  the reset still reprojects incorrectly. This is a **plausible additional contributor
  to `PERF-REGRESSION-6c56e311`** that the `triangle.frag.spv`-swap bisect would not
  have isolated, since swapping only the SPIR-V leaves this host-side heuristic in
  place in both arms of that comparison.
- **Related**: #1489 / REN2-04 (the fix this defeats); ROADMAP `PERF-REGRESSION-6c56e311`
  (same originating commit, possible compounding factor); memory note
  "Renderer Ghosting Investigation Open"; PERF-D9-NEW-02 (the diagnostic that should
  have caught this but reads stale state).
- **Suggested Fix**: Compare *origin-consistent* matrices — run
  `origin_corrected_prev_view_proj` first, then diff `vp` against the corrected `pvp`,
  which removes the crossing false-positive outright. For the motion false-positive,
  drop the raw-element test in favour of an angular one (compare view-basis vectors or
  a reprojected far-plane corner set in NDC), or restrict an element test to the
  rotational 3×3 and give the translation limb a threshold in world units (the existing
  `camera_delta > 256.0` already covers that). Add a unit test pinning "1 cell-grid
  crossing + 6 units/frame of walking ⇒ no cut".

---

#### PERF-D4-01 — Particle draws collide with real entity IDs in the new rigid motion-history map, corrupting motion vectors
- **Severity**: HIGH
- **Dimension**: SSBO Sizing & Per-Frame Upload (Dim 4)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1491-1501` (history read/write);
  producer `byroredux/src/render/particles.rs:190-194`
- **Status**: NEW (introduced by `33d9a468`, 2026-07-22 — post-dates the 2026-07-19 audit)
- **Description**: Commit `33d9a468` ("preserve rigid instance motion history") added a
  per-frame `previous_rigid_models`/`current_rigid_models_scratch` map keyed on
  `DrawCommand::entity_id`. Particle draws synthesize that field as `entity ^ (i as u32)`
  — the producer's own comment says this is only ever meant as a *sort tiebreaker*
  ("Deterministic tiebreaker for same-emitter particles sharing depth bucket and color"),
  not an identity. XOR-ing a small particle index into a dense sequential ECS entity ID
  routinely lands inside the live-entity ID range: emitter entity 500, particle `i = 1`
  produces key 501, which collides with any static-mesh entity 501 also drawn this frame
  (`static_meshes.rs:648` uses the raw, un-XORed `entity_id: entity`).
- **Evidence**:
  ```rust
  // draw.rs:1491-1501
  let previous_source = if draw_cmd.bone_offset == 0 && !camera_cut {
      self.previous_rigid_models.get(&draw_cmd.entity_id).unwrap_or(m)
  } else { m };
  previous_models.push(rebase_model_matrix(previous_source, render_origin));
  if draw_cmd.bone_offset == 0 {
      current_rigid_models.insert(draw_cmd.entity_id, *m);
  }
  ```
  ```rust
  // particles.rs:190-194
  // Deterministic tiebreaker for same-emitter particles sharing depth bucket and color.
  // XOR keeps the emitter grouping intact while giving each particle its own ordering slot.
  entity_id: entity ^ (i as u32),
  ```
  Particles set `bone_offset: 0` (`particles.rs:144`), so every particle draw takes the
  rigid branch unconditionally. Note this check is itself gated by `!camera_cut`
  (PERF-D9-NEW-01) — on a frame where `camera_cut` misfires, this particular bug is
  masked because the whole map is bypassed, but that is an accident of the other bug's
  presence, not a fix.
- **Impact**: A colliding static surface reads a billboard's previous-frame transform as
  its own, producing a large bogus screen-space motion vector for that surface. This
  vector feeds `GpuPreviousModel` → `triangle.vert` → the motion-vector G-buffer
  attachment → FSR 3.1 (the shipped default upscaler), TAA, and SVGF reprojection. Blast
  radius is any cell with particle emitters (torches, fires, steam, dust — i.e. nearly
  every FNV/FO3/Skyrim interior). Symptom is smearing/ghosting on scattered static
  geometry that shifts as particles are born and die. Per `_audit-severity.md`, "SVGF
  reprojection using wrong motion vectors" is a HIGH minimum. This is a plausible new
  contributor to the open ghosting investigation (memory: `renderer_ghosting_investigation_open`),
  though that investigation predates `33d9a468` and is not explained away by it.
- **Related**: introduced by `33d9a468`; `surface_id: draw_cmd.entity_id.wrapping_add(1)`
  (`draw.rs:1653`, introduced by `883f57cd`) inherits the same collision but is largely
  masked because particles carry `ALPHA_BLEND_NO_HISTORY`. PERF-D9-NEW-01 (same loop,
  different bug). No existing GitHub issue (checked `/tmp/audit/performance/issues.json`
  and a 1000-issue `--state all` pull).
- **Suggested Fix**: Stop overloading the sort tiebreaker as a temporal identity. Either
  give particles a reserved ID namespace that provably cannot alias a real ECS entity
  (e.g. `PARTICLE_ID_BASE | (emitter << 16) | i`), or — simpler — skip the
  history insert/lookup entirely for draws with `INSTANCE_FLAG_ALPHA_BLEND` set, since
  billboards get no temporal benefit from motion-history reuse anyway. Add a test pinning
  that no two `DrawCommand`s in one frame share an `entity_id`.

---

#### D5-01 — `PERF-REGRESSION-6c56e311`: main-pass fragment shader ~2.2× slower since 2026-07-19
- **Severity**: HIGH
- **Dimension**: GPU Pipeline & Pass Efficiency (Dim 5)
- **Location**: `crates/renderer/shaders/include/lighting.glsl:172-292`
  (`traceShadowTransmittance`); `crates/renderer/shaders/triangle.frag:2913-3022`
  (bounded GI path tracer); callers `triangle.frag:2657,2809`, `lighting.glsl:376,432`
- **Status**: **Existing** — tracked in ROADMAP.md Known Issues (`ROADMAP.md:755-842`)
  since 2026-07-24, but **not filed as a GitHub issue** (verified against a fresh
  `gh issue list --state all --limit 1000` pull — zero matches for `6c56e311`,
  `regression-`, `traceshadow`, `path trac`, `fps`, `frame time`). **Recommend filing.**
- **Description**: Commit `6c56e311` ("Refactor volumetric lighting and water shaders",
  2026-07-19) dropped Prospector from 149.6 FPS to 68.5 FPS. ROADMAP's own investigation
  (`git bisect` isolating `6c56e311`; a same-machine rebuild of the good parent ruling out
  environmental drift; a per-file SPIR-V swap isolating the cost to `triangle.frag`
  specifically, not the named volumetrics pass) is not re-derived here — this finding
  is a line-by-line verification that the current code still matches that narrative,
  performed independently against HEAD `ca7a4e0e`.
- **Evidence**: `traceShadowTransmittance` (`lighting.glsl:172-292`) replaced a single
  any-hit `TerminateOnFirstHitEXT` probe with two sequential closest-hit walks — an
  8-layer alpha-aware opaque walk (`MAX_OPAQUE_LAYERS = 8`, `lighting.glsl:179`) that
  unconditionally loads `GpuInstance` + `GpuMaterial` per hit before any early-out
  (`lighting.glsl:196-197`), plus a 4-interface glass walk (`MAX_GLASS_INTERFACES = 4`,
  `:233`) with per-interface Fresnel/Beer absorption. Worst case is 12 closest-hit
  queries per shadow ray, fired `SHADOW_RAYS = 4` times per light
  (`triangle.frag:2636,2657`) plus a pass-2 shadow-subtract (`:2809`). The GI ray became a
  bounded path tracer (`MAX_PATH_SEGMENTS = 6`, `MAX_DIFFUSE_BOUNCES = 2`,
  `triangle.frag:2913-2914`) where it was one `TerminateOnFirstHit` traversal; each
  diffuse-bounce hit re-invokes the same shadow-transmittance machinery
  (`giHitIrradiance`/`reflectionHitIrradiance`, up to 4 light candidates each), so the
  full GI path bounds at roughly 6 segments × 4 shadow calls × 12 queries ≈ 288 nested
  ray queries per pixel on top of the direct path's 48. Both features were introduced
  by `6c56e311` itself (`git log -S` on their defining constants returns only that
  commit).
- **Impact**: ~2.2× frame time on real glass-heavy interior content. Now partially masked
  by FSR 3.1 Quality (the shipped default) shading fewer pixels — a symptom reduction,
  not a fix; ROADMAP is explicit this should not be read as lowering urgency. Also
  amplifies the cost side of D2-02 (opaque RT overdraw with no depth pre-pass) and D5-03
  (missing `early_fragment_tests`) — every occluded fragment that still runs the full
  shader before the depth test now pays this ~2.2× higher per-fragment cost. See also
  PERF-D9-NEW-01: the same commit `6c56e311` also introduced the `camera_cut`
  false-positive, which may mean the measured 68.5 FPS already includes some frames
  shading under a forced-reset (single-frame, no-motion-vector) state rather than
  steady-state temporal accumulation — the two regressions were never isolated from
  each other in the ROADMAP bisect, which only swapped the fragment shader binary.
- **Related**: ROADMAP.md:755-842; R6a-stale-16 (the stale-bench tracker that surfaced
  it); commits `6c56e311`, `e414249f` (good parent), `8a668eff` (bench-of-record),
  `ca7a4e0e` (the new shader-artifact parity CI check that would have caught a
  source/binary drift, though this regression is a real source-level cost, not a build
  drift); PERF-D9-NEW-01 (same originating commit, independent host-side bug).
- **Suggested Fix**: **None proposed, deliberately** — both features are intentional
  visual work (glass tints light instead of casting black shadows; second-bounce colour
  bleeding) and ROADMAP already measured and evaluated the available trade-off points
  (`ROADMAP.md:786-790`), including a rejected `SHADOW_MASK_SOLID` TLAS-bucket mitigation
  that measured +6% but introduced an unexplained 0.336%-of-pixels visual delta against a
  0.000% noise floor — per the project's speculative-Vulkan rule, that needs RenderDoc
  evidence or a revert, not another attempt. The only open action is a **product/quality
  decision** (pick a point on the already-measured knob table, or accept the cost), not
  an engineering fix. File a GitHub issue so this decision is tracked outside ROADMAP
  prose — and consider re-measuring after PERF-D9-NEW-01 is fixed, since the fixed
  baseline may show a different (likely smaller, since real accumulation replaces
  forced-reset frames) regression magnitude.

---

### MEDIUM

#### PERF-D1-01 — Scheduler per-system timing tracker is always armed, defeating the #1647 gate
- **Severity**: MEDIUM
- **Dimension**: CPU Per-Frame Allocations & Hot Paths (Dim 1)
- **Location**: `byroredux/src/boot.rs:313`; `crates/core/src/ecs/scheduler.rs:62-84,453-503`
- **Status**: NEW
- **Description**: `Scheduler::run` is documented (per #1647) to allocate its per-system
  wall-time tracker only when the `SchedulerSystemTimings` resource is present — the
  stated intent being that the resource exists only when the debug UI is open. But
  `boot.rs` inserts `SchedulerSystemTimings::default()` unconditionally at world setup,
  so the "no resource" steady-state path the #1647 comment describes never occurs in the
  shipping binary. Every one of the 39 registered systems therefore pays, every frame: a
  `String::from(&'static str)` allocation, an `Instant::now()`, and a global `Mutex`
  lock/unlock — for a consumer (`byroredux/src/systems/metrics.rs`, the egui Metrics
  panel) that samples at ≤2 Hz.
- **Evidence**:
  ```rust
  // scheduler.rs:468-471
  let timings: Option<Mutex<Vec<(String, u64)>>> = world
      .try_resource::<SchedulerSystemTimings>().is_some()
      .then(|| Mutex::new(Vec::new()));
  // scheduler.rs:73-80 — runs for EVERY system, EVERY frame
  let name = self.system.name().to_string();
  let t0 = Instant::now();
  self.system.run(world, dt);
  timings.lock()....push((name, ns));
  // boot.rs:313 — unconditional, no debug-UI gate
  world.insert_resource(byroredux_core::ecs::SchedulerSystemTimings::default());
  ```
- **Impact**: ~2340 `String` allocations/s + ~2400 mutex acquisitions/s + ~360 `Vec`
  reallocs/s at 60 fps (39 systems). Absolute cost is low single-digit µs/frame; the
  sharper edge is a single shared `Mutex` touched by every rayon worker at every system
  completion in every stage — a scaling hazard as system count grows, and exactly the
  churn #1647 was filed to remove.
- **Related**: #1647 (the gate this defeats); same class as CLOSED #2115/D9-01
  (`format!` behind a rate gate).
- **Suggested Fix**: Gate the `insert_resource` call in `boot.rs` on the debug-UI /
  `BYRO_PROFILE` path (or insert lazily when the overlay first opens); additionally,
  store `&'static str` (already available from `SystemEntry::name()`) instead of
  allocating a `String`, and give `SchedulerSystemTimings` a persistent scratch `Vec`
  the scheduler clears and refills instead of a fresh `Mutex<Vec<_>>` every frame.

---

#### D2-01 — Two-sided alpha-blend split re-enabled for `z_write=false` batches — particles pay a dead FRONT-cull pass and drop out of indirect grouping
- **Severity**: MEDIUM
- **Dimension**: Draw-Call & Instancing Efficiency (Dim 2)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:325-328`;
  consumed at `crates/renderer/src/vulkan/context/geometry_pass.rs:323,394-407`
- **Status**: **Regression of #1804** ("two-sided glass split runs on additive particle
  batches — 2× draws + a fully-culled vertex pass with zero compositing benefit", CLOSED)
- **Description**: #1804 gated the two-pass FRONT-then-BACK cull split on `z_write`,
  since the split's purpose (back faces write depth before front faces blend) is
  meaningless when neither pass writes depth. Commit `883f57cd` (2026-07-20, "introduce
  stable surface ID") removed the `&& b.z_write` limb because FO4 BGEM glass is commonly
  authored `z_write == false` and a single `CULL_NONE` draw let TAA jitter pick a
  different blend winner per frame (a legitimate crawling-cross-hatch fix). But
  `z_write` was being used as a *proxy* for "order-dependent glass", and dropping the
  limb re-broadens the split to every two-sided blended batch — exactly the particle
  population #1804 excluded. The regression guard tests were **inverted rather than
  removed** (`draw.rs:3187-3192`, `splits_when_z_write_false`), so `cargo test` stays
  green through the regression.
- **Evidence**:
  ```rust
  // draw.rs:325-328 (current) — pre-883f57cd: `is_blend && b.two_sided && b.z_write`
  pub(super) fn needs_two_sided_blend_split(b: &DrawBatch) -> bool {
      let is_blend = matches!(b.pipeline_key, PipelineKey::Blended { .. });
      is_blend && b.two_sided
  }
  ```
  Particle draws qualify on every limb (`byroredux/src/render/particles.rs:130,133,210`:
  `alpha_blend: true`, `two_sided: true`, `z_write: false`). The consumer branch also
  exits the indirect path entirely (`geometry_pass.rs:394-407`): two direct
  `cmd_draw_indexed` calls plus two `cmd_set_cull_mode` instead of one
  `cmd_draw_indexed_indirect` group.
- **Impact**: Every two-sided blended particle batch costs 2 direct draws instead of
  participating in one indirect group, and the FRONT-cull pass runs the full instanced
  vertex walk to produce zero camera-facing fragments (billboards are front-facing by
  construction). Batch count is small (particles collapse to ~1 batch per distinct
  blend combo), so blast radius is bounded — wasted work, not a stall.
- **Related**: #1804 (closed, reverted); commit `883f57cd`;
  `docs/audits/AUDIT_PERFORMANCE_2026-07-19.md`.
- **Suggested Fix**: Stop using `z_write` as the glass proxy. Carry an explicit
  `two_sided_blend_split: bool` on `DrawCommand`/`DrawBatch`, set at emit time from
  `material_kind` (`MATERIAL_KIND_GLASS` or MultiLayerParallax) — preserves the FO4 BGEM
  fix exactly while restoring the particle fast path, and re-point the (currently
  inverted) unit tests at a predicate that actually distinguishes the two populations.

---

#### D2-02 — Opaque pass runs the RT fragment shader on occluded fragments (no depth pre-pass)
- **Severity**: MEDIUM
- **Dimension**: Draw-Call & Instancing Efficiency (Dim 2)
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs:179,260-265` (single
  subpass); `byroredux/src/render/mod.rs:260-274` (opaque sort: `mesh_handle` dominates
  `sort_depth`); `crates/renderer/src/vulkan/context/geometry_pass.rs:232-262` (opaque
  batches bind `triangle.frag`)
- **Status**: **Existing** — prior audit `docs/audits/AUDIT_PERFORMANCE_2026-07-19.md`
  DIM2-01 (never filed as a GitHub issue). Re-verified accurate at HEAD; line
  references updated for the `9a9a4c5d` `draw.rs` decomposition.
- **Description**: The main geometry render pass has exactly one `vk::SubpassDescription`
  — no depth-only pre-pass. The opaque sort key deliberately places `mesh_handle` ahead
  of `sort_depth` to maximize instancing, so across distinct meshes there is no
  front-to-back ordering at all. Occluded opaque fragments execute the full PBR + RT
  ray-query shader before the depth test rejects them.
- **Impact**: Fragment-bound overdraw multiplies RT ray-query work, not just ALU — and
  is now *more* costly than at the 2026-07-19 audit given D5-01
  (`PERF-REGRESSION-6c56e311`) made `triangle.frag` ~2.2× more expensive per fragment.
  Still gated behind a measurement.
- **Related**: #779 (OPEN, "`triangle.frag` missing `layout(early_fragment_tests)`" —
  confirmed still absent); D5-01/`PERF-REGRESSION-6c56e311` (amplifies the cost side).
- **Suggested Fix**: Measure first (`gpu_main_render` timer with a synthetic Z-prepass
  A/B on Prospector/MedTek) before committing to a depth pre-pass — per the
  speculative-Vulkan posture, this needs RenderDoc/GPU-timer evidence, not speculation.

---

#### PERF-D4-02 — `memory-budget.md` scene-buffer table omits ~73 MB of resident scene SSBOs (52% undercount)
- **Severity**: MEDIUM
- **Dimension**: SSBO Sizing & Per-Frame Upload (Dim 4)
- **Location**: `docs/engine/memory-budget.md:15-28`; actual allocations at
  `crates/renderer/src/vulkan/scene_buffer/buffers.rs:437-590`
- **Status**: NEW (same class as CLOSED #1814 / CLOSED #1872 — both "resource absent
  from memory-budget.md")
- **Description**: The authoritative scene-buffer table claims "≈140 MB across all
  copies". Three buffer families allocated by `33d9a468` are missing entirely, and the
  bone footnote undercounts by a factor of ~2.7×: the doc's footnote claims "3 × 12.6 MB
  ≈ 37.8 MB" for the bone family; the real bone family is **eight** 12.58 MB allocations
  ≈ 100.6 MB (palette ×2 FIF, host-visible staging ×2 FIF, device-copy ×2 FIF,
  persistent bind-inverses, bind-inverse upload staging).
- **Evidence**:

  | Buffer | Per-FIF | Total | In doc? |
  |---|---|---|---|
  | instance | 29.36 MB | 58.72 MB | yes |
  | **previous_model** (new, `33d9a468`) | **16.78 MB** | **33.55 MB** | **NO** |
  | indirect | 5.24 MB | 10.49 MB | yes |
  | material | 4.92 MB | 9.83 MB | yes |
  | bone_device + staging + device-copy (×2 FIF each) | 3 × 12.58 MB | 75.5 MB | folded into a wrong footnote |
  | **bind_inverses_persistent** | — | **12.58 MB** | **NO** |
  | **bind_inverse_upload_staging** | — | **12.59 MB** | **NO** |

  **Actual total ≈ 213.4 MB. Documented ≈ 140 MB.**
- **Impact**: `memory-budget.md` is the cited authority for VRAM planning against the
  6 GB RT-minimum target and is explicitly named in `_audit-common.md` as "prefer over
  re-deriving facts from source" — every downstream headroom calculation inherits the
  73 MB undercount. A future `MAX_INSTANCES` bump would silently cost 1.75× what the
  table implies (112 B + 64 B per slot, not 112 B), because the previous-model SSBO
  scales with the same constant invisibly.
- **Related**: CLOSED #1814 (ReSTIR reservoirs absent), CLOSED #1872 (denoiser images
  absent) — same failure mode, both fixed by adding rows. Also PERF-D3-01 (FSR 3.1
  resources similarly unledgered) and PERF-D3-02 (stale 100 B vertex stride) — all
  three are `memory-budget.md` doc-rot found independently by Dim 3 and Dim 4; not
  literal duplicates (different buffer families) but the same root cause (the doc
  hasn't tracked the last two sessions of SSBO additions) and should be fixed in one
  documentation pass.
- **Suggested Fix**: Add rows for `previous_model`, `bind_inverses_persistent`, and
  `bind_inverse_upload_staging`; split the single "Bone-palette SSBO" row into the three
  real per-FIF bone buffers; correct footnote ¹ and the ≈140 MB total to ≈213 MB.
  Consider a `scene_buffers_total_bytes()` accessor pinned by a test so the doc figure
  can't drift again.

---

#### D6-01 — Skinned-vertex output buffer stores the full 104-byte `Vertex` when only the 12-byte position lane is ever read
- **Severity**: MEDIUM
- **Dimension**: Skinning & BLAS Cost (Dim 6)
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs:402`,
  `crates/renderer/shaders/skin_vertices.comp:164-202`,
  `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:82,504`
- **Status**: NEW
- **Description**: Each `SkinSlot` allocates `vertex_count × 104 B` of DEVICE_LOCAL
  memory, and `skin_vertices.comp` writes all 26 floats per vertex (position, skinned
  normal, skinned tangent, plus 17 floats of verbatim pass-through: colour RGBA, UV,
  bone indices/weights, splats, bitangent sign). The only consumer in the codebase is
  the acceleration-structure build, which reads the buffer as `R32G32B32_SFLOAT` at
  `vertex_stride = size_of::<Vertex>()` — i.e. touches 12 of every 104 bytes. RT hit
  shading samples the bind-pose global vertex SSBO, not this slot output; nothing reads
  the skinned normal/tangent or the pass-through lanes.
- **Evidence**: Exhaustive grep for `output_buffer` across the renderer + binary crates
  yields exactly four non-doc uses — allocation, destruction, the descriptor write, and
  the two AS-build call sites (first-sight BUILD, refit UPDATE) — both going through
  `vertex_format(R32G32B32_SFLOAT)`. The shader's own comment states the pass-through
  exists because "Phase 3 (vertex shader reads pre-skinned) needs every field present" —
  but Phase 3 is explicitly deferred, and `create_slot` deliberately omits
  `VERTEX_BUFFER` usage for exactly that reason (#681 / MEM-2-6), so the buffer is
  provisioned for a consumer that does not exist and cannot be bound without a
  usage-flag change anyway.
- **Impact**: Two costs, both scaling with skinned-entity count: (1) VRAM — 8.7×
  over-allocation per slot; at a conservative 2K verts/sub-mesh and the ~1040 distinct
  `SkinnedMesh` allocation attempts/frame telemetered on FNV Atomic Wrangler peak, this
  is ~216 MB of slot output buffers where ~25 MB would serve, against the ~4 GB total
  budget target. (2) Bandwidth — every non-skipped dispatch writes 104 B/vertex instead
  of 12 B, the dominant traffic of the skin pass on a moving-crowd frame with the #1195
  dirty-gate open. Neither is a correctness risk.
- **Related**: #681 / MEM-2-6 (the paired decision to omit `VERTEX_BUFFER` usage);
  #1797 (the other unmeasured skin-pass throughput ceiling — both want the same
  moving-crowd bench); `docs/engine/memory-budget.md`.
- **Suggested Fix**: Narrow the slot output buffer to a positions-only layout (stride
  12–16 B), drop the pass-through writes from `skin_vertices.comp`, and pass the
  matching `vertex_stride` to both AS-build sites. Keep the change behind the same
  commit that would otherwise land Phase 3, or explicitly retire Phase 3 in the
  shader's header comment so the provisioning rationale doesn't silently outlive its
  plan. Quantify with the existing `skin.coverage`/`gpu_skin_dispatch_ms` hooks on the
  same moving-crowd bench #1797 needs.

---

#### PERF-D9-NEW-02 — Origin-crossing diagnostic trace logs `render_origin_delta` after the state it measures was already overwritten — always prints `(0,0,0)`
- **Severity**: MEDIUM
- **Dimension**: Telemetry (Dim 9)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1165-1187`
- **Status**: NEW
- **Description**: The trace added for the open ghosting investigation computes
  `render_origin.{x,y,z} - self.prev_render_origin[..]`, but `self.prev_render_origin`
  was assigned the *current* origin 16 lines earlier. The subtraction is therefore
  always exactly zero.
- **Evidence**:
  ```rust
  self.prev_view_proj = *vp;                                                    // :1165
  self.prev_camera_position = camera_pos;                                       // :1166
  self.prev_render_origin = [render_origin.x, render_origin.y, render_origin.z]; // :1167
  …
  log::trace!("… render_origin_delta=({:.3},{:.3},{:.3}) …",
      render_origin.x - self.prev_render_origin[0],   // :1183 — always 0.0
      render_origin.y - self.prev_render_origin[1],
      render_origin.z - self.prev_render_origin[2], …);
  ```
- **Impact**: The single diagnostic added specifically to identify origin-crossing
  frames in a live repro reports a constant zero, actively arguing "no crossing
  happened" on precisely the frames under investigation — including the frames
  PERF-D9-NEW-01 corrupts. (`vp_max_abs_delta` on the same log line *is* correct and
  would have shown the ~5562 spike from PERF-D9-NEW-01's evidence table — it was
  available but the origin-delta figure printed alongside it was actively misleading.)
  Cost is nil (trace level), but the diagnostic value is negative.
- **Related**: PERF-D9-NEW-01; memory note "Renderer Ghosting Investigation Open".
- **Suggested Fix**: Capture `let origin_delta = render_origin -
  Vec3::from_array(self.prev_render_origin);` **before** the overwrite, and log that
  local.

---

### LOW

| ID | Dimension | Title | Status |
|----|-----------|-------|--------|
| PERF-D1-02 | CPU Hot Paths | `collect_lights` builds a fresh decorate-sort `Vec` every frame instead of a caller-owned scratch | NEW |
| PERF-D1-03 | CPU Hot Paths | Draw-sort parallel threshold (2000) calibration predates the 10→11-tuple sort-key widening (`883f57cd`) | NEW |
| D2-03 / PERF-D4-03 | Draw & Instancing / SSBO Upload | New rigid motion-history maps (`33d9a468`) use `std::collections::HashMap` (SipHash) instead of the renderer's established `FxHashMap` — **merged, reported once below** | NEW (duplicate across Dim 2 + Dim 4, same site) |
| D2-04 | Draw & Instancing | Water pass rebinds pipeline + 3 descriptor sets once per water plane instead of once per pass | NEW |
| PERF-D3-01 | GPU Memory Pressure | `memory-budget.md` has no FSR 3.1 entry; existing screen-sized tables are keyed to the wrong resolution axis (render vs output) post-`5c7acfe2` | NEW |
| PERF-D3-02 | GPU Memory Pressure | `memory-budget.md` vertex stride stale at 100 B — actual is 104 B since `cd2b5fe4` | NEW |
| PERF-D3-03 | GPU Memory Pressure | `FrameUpscaler::create_outputs` leaks its `gpu-allocator` sub-allocation if `bind_image_memory` fails (unreachable except on driver OOM) | NEW |
| D5-02 | GPU Pipeline | Volumetrics "output discarded / pure GPU waste" stale comment block, now in `post_passes.rs` after the `9a9a4c5d` move | Existing: #1938 (OPEN) |
| D5-03 | GPU Pipeline | `triangle.frag` still has no `layout(early_fragment_tests)` — cost basis raised by D5-01 | Existing: #779 (OPEN) |
| D5-N1 | GPU Pipeline | `gbuffer.rs` leak-guard doc comment undercounts attachments (5/30 vs actual 7/42, post-FSR) | NEW |
| D5-N2 | GPU Pipeline | FSR reactive/transparency masks are unconditional main-pass attachments, written even under `--upscaler taa` fallback | NEW (low confidence, estimated impact) |
| D6-02 | Skinning & BLAS | `CLAUDE.md` still documents the pre-`cd2b5fe4` 100-byte vertex layout | NEW |
| PERF-D9-NEW-03 | Telemetry | `gpu_breakdown()` omits the new `upscale`/`presentation` GPU-timer brackets — the SLOW-FRAME / 1 Hz line no longer accounts for the whole frame | NEW |
| PERF-D9-NEW-04 | Telemetry | `gpu_timers.rs` doc comments still say "12 brackets / 24 queries"; actual is 14 / 28 since the FSR brackets landed | NEW |

#### D2-03 / PERF-D4-03 (merged) — New rigid motion-history maps use `std::collections::HashMap` (SipHash) on the per-draw hot path
- **Severity**: LOW
- **Dimension**: Draw-Call & Instancing Efficiency / SSBO Sizing & Per-Frame Upload
  (independently flagged by both Dim 2 and Dim 4 — same code, merged here)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs:38,1079,1081,2745-2746`;
  hot loop at `crates/renderer/src/vulkan/context/draw.rs:1491-1501`
- **Status**: NEW (regression-class of CLOSED #1368, "SipHash on render hot path" — a
  different site, same anti-pattern reintroduced)
- **Description**: `previous_rigid_models` and `current_rigid_models_scratch`
  (introduced by `33d9a468`) are `std::collections::HashMap<u32, [f32;16]>` — the
  default SipHash-1-3 hasher — hit once for `.get()` and once for `.insert()` per rigid
  draw, per frame. The renderer already standardizes on `rustc_hash::FxHashMap` for
  exactly this shape of per-frame hot map (`material.rs:929`, `context/mod.rs:510`,
  `scene_buffer/descriptors.rs:303`); `33d9a468` reintroduced SipHash at a new site
  after #1368 closed removing it elsewhere.
- **Impact**: ~2 SipHash probes per rigid draw per frame — ~2.4K on Prospector (1224
  draws), ~29K on MedTek (14535 draws). Estimated tens of µs/frame at MedTek scale; not
  a bottleneck, purely avoidable CPU. Allocation behaviour is already correct (the maps
  are `mem::take`n, cleared, and swapped — no per-frame heap churn), so hashing is the
  only remaining cost.
- **Related**: CLOSED #1368; the `FxHashMap` precedent at `material.rs:31,929,971`;
  PERF-D4-01 (same call site, different bug — the entity-ID collision is the identity
  problem, this is the hash-function-choice problem; fixing one doesn't fix the other).
- **Suggested Fix**: Change both fields to `rustc_hash::FxHashMap<u32, [f32; 16]>` and
  update the two `HashMap::new()` construction sites — the crate dependency and the
  in-crate precedent both already exist; one-line-per-site change.

---

## Eroded Guards

**Two confirmed erosions** (out of 18 named guards checked across all 9 dimensions):

- **#1804 two-sided blend split gate** (Dim 2) — the `&& b.z_write` limb was removed by
  `883f57cd`, re-broadening the split to particle batches. See **D2-01** above. The
  regression is invisible to `cargo test` because the guard's own unit tests were
  inverted to match the new (regressed) behavior rather than left to fail.
- **#1489 origin-corrected history preservation on cell-boundary crossing** (Dim 9) —
  the `camera_cut` heuristic added by `6c56e311` takes precedence over
  `origin_corrected_prev_view_proj` on exactly the frame class #1489 was fixed for, and
  additionally misfires on ordinary camera motion. See **PERF-D9-NEW-01** above. No test
  exists for `camera_cut`, so this erosion was invisible to `cargo test` as well.

All other 16 guards (7 in Dim 1, 1 in Dim 2 [GT-hoist], 1 in Dim 3, 2 in Dim 4, 2 in
Dim 5, 5 in Dim 6, 12 of 13 in Dim 9, plus Dim 7/8's architectural checks) were
individually re-verified against live code and, where applicable, live test runs — no
further erosion found.

**Four items open at the 2026-07-19 audit are now confirmed fixed and landed** (not
just present in git log — verified against the actual diff and, where practical,
re-run tests):
- #2111 (streaming worker NIF-header re-parse) — `f9ad6ca2`
- #2112 (`skin.coverage` staleness on bailed frame) — `21fe71af`
- #2113 (pending stream requests not cancelled on ring exit) — `1009e792`
- #2114 (dhat geometry bound never exercised the packed-vertex path) — `424ac4c0`
- #2115 (per-frame telemetry `format!` strings ungated) — `a48e037a`, verified in both
  Dim 1's and Dim 9's independent re-reads of `byroredux/src/systems/debug.rs`.

---

## Prioritized Fix Order

**Correctness-first (temporal/motion-vector bugs — these should land before the next
FSR/TAA/SVGF quality measurement, since they corrupt exactly the signal those systems
depend on):**
1. **PERF-D9-NEW-01** (HIGH) — fix `camera_cut` to diff origin-consistent matrices
   (run `origin_corrected_prev_view_proj` first) and/or replace the raw element-wise VP
   diff with an angular/rotational-only test. Add the "crossing + walking ⇒ no cut"
   regression test. This is the highest-leverage fix in the report: it is live on every
   frame the player moves, not just at cell boundaries, and its `reset=true` propagates
   into FSR (the shipped default), TAA and SVGF.
2. **PERF-D4-01** (HIGH) — stop overloading the particle sort-tiebreaker as motion-history
   identity; reserve a non-colliding ID namespace or skip the history map for
   alpha-blended draws. Add a same-frame-entity-ID-uniqueness test.
3. **PERF-D9-NEW-02** (MEDIUM) — fix the origin-delta diagnostic to read state before
   the overwrite; cheap, and needed to observe whether #1 above is actually fixed in a
   live repro.

**Quick wins (mechanical, low risk):**
4. **D2-03/PERF-D4-03** (LOW) — swap `HashMap` → `FxHashMap` on the two new rigid
   motion-history maps.
5. **PERF-D1-01** (MEDIUM) — gate `SchedulerSystemTimings` insertion in `boot.rs` behind
   the debug-UI path.
6. **D6-02**, **PERF-D3-02** (LOW) — fix the two stale 100 B vertex-stride references
   (`CLAUDE.md`, `memory-budget.md`).
7. **D5-N1**, **PERF-D9-NEW-04** (LOW) — update stale attachment/bracket-count doc
   comments (`gbuffer.rs`, `gpu_timers.rs`).
8. **D5-02** (LOW, #1938) — rewrite the stale volumetrics "pure GPU waste" comment block
   (now in `post_passes.rs`).
9. **PERF-D9-NEW-03** (LOW) — add `upscale`/`presentation` to `gpu_breakdown()`.
10. **PERF-D1-02** (LOW) — thread a caller-owned scratch `Vec` through `collect_lights`.

**Needs a fix, not just a comment (moderate risk, no measurement required):**
11. **D2-01** (MEDIUM) — replace the `z_write` glass proxy with an explicit
    `two_sided_blend_split` flag set from `material_kind`; un-invert the regression
    guard tests.
12. **D6-01** (MEDIUM) — narrow the skinned-vertex output buffer to positions-only;
    quantify with `skin.coverage` before/after on a moving-crowd bench.
13. **PERF-D4-02** + **PERF-D3-01** (MEDIUM/LOW) — one `memory-budget.md` documentation
    pass covering both the ~73 MB SSBO undercount and the FSR 3.1 resource gap; consider
    a test-pinned `scene_buffers_total_bytes()` accessor so it can't drift again.
14. **D2-04** (LOW) — hoist the water-pass pipeline/descriptor-set rebind out of the
    per-plane loop.
15. **PERF-D3-03** (LOW) — free the `gpu-allocator` sub-allocation on the
    `bind_image_memory` failure branch in `FrameUpscaler::create_outputs` (mirror
    `exposure.rs`'s pattern); same shape exists in `gbuffer.rs`, fix both.

**Measurement-gated (do not commit speculatively — RenderDoc/GPU-timer evidence first):**
16. **D2-02** (MEDIUM) — depth pre-pass for the opaque geometry pass. Real upside, now
    larger given D5-01, but needs a synthetic Z-prepass A/B on Prospector/MedTek first.
17. **D5-03** (LOW, #779) — `early_fragment_tests` on the non-`discard` shader
    permutation; interacts with alpha-test discard semantics, needs RenderDoc validation.
18. **D5-N2** (LOW) — leave the FSR mask attachments unconditional; a second render-pass
    permutation is likely a poor trade for ~7 MB/frame on the non-default TAA fallback
    path only.

**Decision-gated (not an engineering task):**
19. **D5-01 / `PERF-REGRESSION-6c56e311`** (HIGH) — file a GitHub issue to track the
    product decision (recover FPS at a stated visual-quality cost, per the already
    measured knob table in ROADMAP, or accept the cost). No code change proposed here.
    Consider re-measuring after item #1 lands, since the two `6c56e311` regressions
    were never isolated from each other.

**Deferred / architectural (already tracked, re-confirmed only):**
20. D7-02 (#1798, interior/sub-cell NPC spawn budget), #1793 (BLAS burst-aging edge
    cases), #1797 (shared skinned-BLAS scratch serialization) — all re-verified still
    accurate and correctly left open pending their own measurement/design work.

---

*Generated by `/audit-performance` (deep, all 9 dimensions), as one leg of a
`comprehensive` audit-suite sweep. Bench figures cite ROADMAP.md `8a668eff`/`e153b50c`;
no bench re-run this session. Dedup checked against a 200-issue `gh issue list` pull
(Dimensions 4/5 additionally checked a 1000-issue `--state all` pull before
recommending PERF-D4-01, D5-01 and PERF-D9-NEW-01 as unfiled). Suggested next step:
`/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-07-25.md`.*
