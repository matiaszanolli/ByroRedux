# Renderer Audit — 2026-09-06 (full sweep, all 23 dimensions)

**Scope**: bare `/audit-renderer`, `--depth deep`, all 23 dimensions. HEAD at
audit time: `229306ce`.

The last **full** renderer sweep was `AUDIT_RENDERER_2026-08-30.md`. Since then
only scoped runs landed — `AUDIT_RENDERER_2026-09-04.md` (`water-deep`),
`AUDIT_RENDERER_2026-09-05.md` (`volumetrics-deep`, dims 1/2/5/16) and
`AUDIT_RENDERER_2026-09-05_DIM6_DIM7.md`. Dimensions 3, 4, 8–13, 17–23 had no
coverage in the last week, and the tree moved substantially in that window: 68
commits since 2026-09-04, touching every file in `crates/renderer/src/vulkan/`.

**Method**: 13 agents, one per dimension or per cohesive dimension group, each
working its SKILL checklist bullet by bullet and required to mark every bullet
either CLEAN with the naming symbol/test or → finding. Each report opens with a
Coverage section, so a silent bullet is itself a defect in the report. Every
CRITICAL and HIGH, and every MEDIUM whose premise concerned a recent commit, was
independently re-verified by the orchestrator against live source before
inclusion here.

**Verification discipline**: findings are anchored on symbols, not line numbers.
Every backticked path was validated against the live tree (`_audit-validate.sh`
green). No render-pass / pipeline / barrier edit is proposed on reasoning alone —
those observations are isolated in the **Needs-RenderDoc** section. No bench
FPS/ms figures are quoted.

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 1 |
| HIGH | 2 |
| MEDIUM | 17 |
| LOW | 56 |
| **Total** | **76** |

Plus one recorded regression-check note (`REN-2026-09-06-D3-07`, `DBG_*` u32
exhaustion) that is explicitly **not** a finding — #3563's guard is verified in
place and it is logged so the next sweep does not re-file it.

### What this sweep is actually about

Three of the four most severe findings are **incomplete recent fixes**, not old
rot. That is the headline. The engine's correctness posture is good — 871
renderer lib tests green, every layout pin holding, no stale SPIR-V, no
per-frame leak, no Rust↔GLSL size drift — but four commits from the last three
days each solved the half of their problem that a test could see and left the
half it could not.

| Finding | Fix it follows up | What the fix did | What it left |
|---|---|---|---|
| `D9-01` CRITICAL | `709de0e6` / #3569 | requeued the drained upload entries | the same frame still builds a BLAS from never-written memory |
| `D16-01` MEDIUM | `fa5c4191` / #3829 | corrected the `GpuBoundaryInstance` mirror | three sibling mirrors still outside every lockstep guard |
| `D5-01` `D23-01` `D16-02` MEDIUM | `fa5c4191` / #3839 | derived a VRAM reservation | it covers 3 of 11 screen-scaled passes and excludes the default upscaler |
| `D5-03` `D1-01` MEDIUM | `fa5c4191` / #3840 | split resident-vs-paper BLAS accounting | the consumer re-gates on the paper figure, so the swap is inert |

The last three are follow-ups to `fa5c4191`, which this session authored
yesterday. They are reported here at full severity with no discount.

### Cross-cutting: the audit instrument is itself a source of defects

Seven LOW findings (`D1-02`, `D2-05`, `D3-08`, `D6-01`, `D7-01`, `D8-03`,
`D17-04`) are stale premises inside `/audit-renderer`'s own SKILL text and the
shared `_audit-common.md`. This is not bookkeeping. `D1-02` documents a stale
claim — "no registered command reads the `missing_blas` counters", false since
`9c805cd7` (2026-08-14) — that produced a **false finding in both the 08-30 and
09-05 sweeps**. The 09-05 instance was published as #3833 and closed at
2026-09-06T01:34 as *"stale — not fixed, because there is nothing to fix"*. The
live chain is `integrity_snapshot` → `fill_rt_integrity_stats` →
`RtIntegrityStats` → the registered `rt.integrity` command.

`D3-08` is the same class with a sharper edge: the Dimension-3 instruction tells
the auditor to enumerate `GpuInstance` mirrors with an anchored
`^struct GpuInstance` grep returning 5 sites. `GpuBoundaryInstance` in
`volumetrics_inject.comp` is a sixth mirror of the same SSBO that the
instruction structurally cannot see — which is why it sat 32 B misaligned for
13 days until yesterday.

**Fix the instrument before the next sweep**, or it will keep manufacturing
false findings and hiding real ones.

---

## RT Pipeline Assessment

**BLAS/TLAS**: clean. All build-flag constants stable and test-pinned, the
`instance_custom_index` ↔ draw-index contract intact, deferred destruction
correct at every eviction and drop site, `restore_missing_static_blas_for_draws`
still running before `draw_frame` with `mark_static_blas_used` correctly ordered
ahead of the retain. 109 acceleration tests green. The one MEDIUM (`D1-01`) is
an inert accessor, not a correctness break.

**SSBO indexing and ray queries**: clean on every load-bearing contract — the
ReSTIR normal cone, the stable surface ID, the thin-glass gate, the BC1
punch-through pin, `sceneFlags.x` gating at every query site, and the
frame-counter noise seed all verified live. `caustic_splat.comp`, the disclosed
gap in the 09-05 report, was audited and its `GpuInstance` mirror is current.

**The RT correctness gap is coverage, not indexing.** Two findings say the same
thing from different ends: `rayHitHasCoverage` omits the decal-slot alpha
composite the raster path applies (`D2-01`), and off-frustum instances get
`flags = 0` while `include/ray_hit.glsl` now reads four of those bits
(`D12-01`, HIGH). Both make RT silhouettes a strict subset of the raster
silhouette — off-screen alpha-blended surfaces cast fully opaque shadows.

**Denoiser**: the SVGF α-recovery window is discarded exactly when it is needed
(`D8-01`), and the failure path that installs it is unreachable because
`TaaPipeline::dispatch` and `SvgfPipeline::dispatch` return `Result<()>` with no
error-producing construct in them (`D12-02`). `c43cb269` (#3605) is therefore
wired to code that cannot execute, and its source-scan guard passes regardless.

---

## GPU-Struct & Memory Assessment

**Layout: zero live drift.** All three headline sizes re-derived from passing
pins — `GpuInstance` 160 B, `GpuCamera` 368 B, `GpuMaterial` 432 B.
`GpuMaterial` is 108 scalar fields (79 f32 + 29 u32, zero `[f32;3]`, zero pads);
its single GLSL mirror matches all 108 by name, order *and* type; all 108 offsets
individually pinned; `hash_gpu_material_fields` walks exactly 108 with empty
symmetric difference. All five `^struct GpuInstance` mirrors match field-for-field
across 21 slots. Every capacity constant agrees three ways across code,
`memory-budget.md` and `shader-pipeline.md`.

**The layout risk has moved from size drift to name-divergent mirrors.**
`D16-01` is the finding to act on: #3829 closed one of four. `ClusterEntry` (3
GLSL copies, zero coverage), `FogClusterEntry` and `CombustionLightMoment` remain
outside every guard. `CombustionLightMoment` is the worst of them — the shader
writes by field name while `decode_combustion_light_moment` decodes purely
positionally (`word(0)`…`word(7)`), so a GLSL reorder keeps the struct at 32 B,
passes every size check, and silently swaps fields. A stride guard cannot catch
a reorder. The real fix is a completeness meta-guard, not three more one-off
tests.

**Memory: no leak, no lifecycle defect.** All 128 `VulkanContext` fields audited
field-by-field against `destroy_allocator_owned_resources` with zero gaps;
`AllocatorResource` correctly removed before context drop on both shutdown and
panic-unwind; zero Vulkan object creation anywhere in the per-frame draw path;
#1782 / #1390 / #418 / #1430 / #3298 / #3372 / #3443 all intact.

**The memory finding is the reservation's coverage.** Four agents reached
`screen_scaled_reservation_bytes` independently. It reserves 3 of the 11
screen-scaled passes `memory-budget.md` ledgers — 315.19 MB of ~720 MB at
1080p — omitting ReSTIR (the page's own "largest single VRAM addition"),
G-buffer, TAA, bloom, SSAO, the water half of the caustic row it half-imports,
and FSR entirely. FSR's exclusion is structural rather than an oversight: the
function takes only the *render* extent, while FSR's per-FIF outputs live at
*output* resolution. The error's sign is adversarial — the reserved terms shrink
quadratically with the FSR preset while FSR's own residency does not shrink at
all. The arithmetic it does perform reproduces every doc figure exactly; this is
coverage, not math. The existing pin asserts only `> 128 MiB` and monotonicity,
so it passes with two terms as easily as eleven.

---

## Verified clean (evidence, not silence)

Recorded so future sweeps can diff rather than re-derive:

- **Committed SPIR-V is current.** All 22 shaders recompile byte-identical to
  their committed `.spv`; 22 sources ↔ 22 blobs, no orphans, no compile
  failures. A commit-order heuristic flags 12 as suspect; all 12 are false
  positives (named-constant and comment-only edits). Confirmed independently by
  three agents. Recipe in the orchestrator section below — use it instead of the
  heuristic.
- **`#3601`** (`1ff9bc73`) audited closely: bound exact in both directions, skip
  is state-safe because the overlay is the last command before
  `cmd_end_render_pass`.
- **`#3607`** (`20f5f476`): all five `octDecode` bodies extracted and diffed —
  semantically identical, rename complete.
- **`#3834` / `#3835` / `#3836` / `#3837` / `#3838`**: none introduced a
  retained-forever buffer or a stale cached answer. The fog high-water mark and
  combustion dirty flag are correctly per-FIF; `SceneEffectSoftCache` invalidates
  on cell change.
- **`#3912`** (`86976f56`) pushed no default into the renderer: the direction is
  core → renderer/nif → generated GLSL macro. No value moved.
- **NIFAL boundary holds**: `translate_material`'s literal is compiler-forced
  exhaustive across all 63 `Material` fields; zero live per-game branches
  downstream (44 game-name hits, all comments).
- **FSR**: jitter genuinely single-sourced; 2/2 `unsafe fn` and 9/9 `unsafe`
  blocks documented; #3605's reset does reach FSR; dispatch-failure fallback
  renders on all three arms.
- **#3922 has no siblings.** Authored cubemaps do invert the basis
  (`vec3(R.x, -R.z, R.y)`); Skyrim DALC is remapped CPU-side. The `_msn` branch
  is the sole outlier — a negative result worth keeping.
- **Tangent conventions** checked against vendored `nifly`, not the project's own
  comments: Bethesda's `CalcTangentSpace` swap confirmed, all four import paths
  route the sign through one `bitangent_sign`, no path converts N without T.

---

## Prioritized Fix Order

Correctness → safety → coverage → documentation.

1. **`D9-01` (CRITICAL)** — gate the skin dispatch and first-sight BLAS build on
   `bind_inverse_upload_failed`. The requeue is correct; the same-frame consume
   is not.
2. **`D11-01` (HIGH)** — one-line-ish: either declare `water.frag`'s two missing
   outputs (delivering #3821's intent) or revert attachments 4/5 to `masked_off`.
   Add a shader-output limb to `attachment_doc_pin_tests`.
3. **`D12-01` (HIGH)** — assemble the four RT-read flag bits unconditionally;
   keep skipping only terrain-splat/render-layer. Replace the prose invariant
   with a source-scan test.
4. **`D4-01` (MEDIUM)** — move the skin chain's record-time commits after submit,
   adopting the #917 pattern already present 30 lines below.
5. **`D8-01` + `D12-02` (MEDIUM)** — fix together; they are one recovery path.
   Make the dispatch results actually fallible, then stop the camera-static flag
   zeroing the α floor.
6. **`D16-01` (MEDIUM)** — a completeness meta-guard over GLSL↔Rust struct
   mirrors. This is the finding most likely to prevent the next CRITICAL.
7. **`D5-01` / `D23-01` / `D16-02` (MEDIUM)** — widen the reservation and
   strengthen its pin beyond `> 128 MiB`.
8. **`D5-03` / `D1-01` (MEDIUM)** — make #3840's resident figure actually reach a
   decision, or remove it and document why the paper figure is correct.
9. **`D22-01` (MEDIUM)** — resolve the Starfield `DAT2` Flags contradiction; the
   evidence the premise says is missing is cited in the same tree.
10. **The seven SKILL-rot LOWs** — before the next sweep, not after.

Remaining MEDIUMs (`D2-01`, `D3-01`, `D3-02`, `D10-01`, `D17-01`, `D17-02`,
`D18-01`) and the LOW tail follow in the findings below.

---

## Findings

---

# CRITICAL

### REN-2026-09-06-D9-01: a failed first-sight `bind_inverses` upload still lets the SAME frame build that entity's skinned BLAS out of never-written device memory

- **Severity**: CRITICAL
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (`VulkanContext::dispatch_skin_and_cluster`, the `upload_pending_bind_inverses` error arm), `crates/renderer/src/vulkan/context/skinned_blas_refit.rs` (`VulkanContext::record_skinned_blas_refit`), `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`SceneBuffers::seed_persistent_bind_inverses_identity`)
- **Status**: NEW (the residual half of the just-closed #3569; no open issue matches — grepped `bind_inverse|first-sight|uninitial|garbage|palette` against the 151 open titles)
- **Description**: `709de0e6` fixed the *bookkeeping* half of #3569 — a failed
  `upload_pending_bind_inverses` now latches `bind_inverse_upload_failed`, and
  `app_frame.rs` requeues the drained entries so they retry next frame. It did **not**
  gate the rest of that same frame. `bind_inverse_upload_failed` is written in exactly
  two places (`= false` at the top of `draw_frame`, `= true` in the error arm) and read
  in exactly one (`app_frame.rs`'s rollback check) — no consumer inside
  `dispatch_skin_and_cluster` or `record_skinned_blas_refit` looks at it. So on the
  failing frame the engine goes on to (1) run `skin_palette.comp` over the *dense*
  `0..bone_count` range including the never-written slot, (2) dispatch
  `skin_vertices.comp` for the entity against that palette, and (3) issue the entity's
  first-sight skinned-BLAS **BUILD** over the resulting positions.
  `bind_inverses_persistent` is created by `GpuBuffer::create_device_local_uninit` and
  only **slot 0** is ever seeded (`seed_persistent_bind_inverses_identity`, #1191) — every
  other slot is raw uninitialised device memory (or, on a reused free-list slot, the
  previous tenant's matrices) until its own first-sight copy lands.
- **Evidence**:
  - Error arm returns `0`, so the `if pending_capped > 0 { … record_pending_bind_inverse_copies(…) }`
    block is skipped entirely — nothing writes the slot, and no `TRANSFER_WRITE → SHADER_READ`
    barrier is emitted for it.
  - The palette dispatch guard immediately below is
    `bone_count > 0 && !skip_skin_gpu_refresh && (bone_world_copy_recorded || pending_capped > 0)`.
    On a first-sight frame `skin_state_dirty` is true (the pending list is non-empty) so
    `skip_skin_gpu_refresh` is `false`, and the entity is in `pose_dirty` (first sight →
    `SkinSlotPool::try_mark_pose_dirty` has no prior hash → always dirty), so
    `upload_bone_worlds` writes its slot and `bone_input_upload_bytes(frame) > 0` makes
    `bone_world_copy_recorded` true. **The dispatch runs.**
  - `skin_palette.comp::main` is unconditional per slot: `palette[slot] = boneWorld[slot] * bindInverses[slot];`
  - `record_skinned_blas_refit` walks `dispatches` (built from `draw_commands` with
    `bone_offset != 0` — which the entity has, its slot was assigned pre-draw), takes the
    `needs_blas` branch, pushes into `first_sight_builds`, and the batch is recorded on
    this frame's command buffer via `build_skinned_blas_batched_on_cmd`. The only
    suppressors on that path are `failed_skin_slots` / `failed_skin_blas` (both empty on
    first sight) and `mesh.rt_capable`.
  - Recovery is *not* complete next frame: the requeue + `rollback_pending_pose_commits`
    do get the slot uploaded and the entity re-marked dirty, but `needs_blas` is now
    `false`, so the entity gets an **UPDATE-mode refit**, which by design preserves the
    BVH topology the poisoned BUILD produced. A fresh BUILD only happens after
    `should_rebuild_skinned_blas` trips `SKINNED_BLAS_REFIT_THRESHOLD` (600 frames, ~10 s)
    or the LRU sweep drops the entry.
- **Impact**: An acceleration structure built over undefined memory. `boneWorld × <arbitrary bits>`
  can yield NaN/Inf, so the BLAS' vertex positions — and therefore the AABB it contributes
  to the TLAS with an identity instance transform — are unbounded or non-finite. This is
  the `_audit-severity.md` "BLAS/TLAS build with wrong geometry" row: every ray query that
  frame (shadows, reflections, GI, water refraction, caustic splat) traverses it, and the
  degenerate topology survives ~600 frames of refits. It is exactly the #2467 failure
  class the `bones[bone_offset]` rigid fallback was introduced to close, re-entered
  through a different door. Trigger is rare — the staging buffer is a `GpuBuffer` from
  `GpuBuffer::create_host_visible`, so the `Err` comes from its `mapped_slice_mut`
  ("Buffer has no allocation" / "Buffer not mapped") or its `flush_if_needed`
  (`vkFlushMappedMemoryRanges` failure, i.e. device-lost or OOM) — but
  severity here is impact, not likelihood, and the engine's response to a transient map
  failure should not be to poison the TLAS.
- **Related**: #3569 (`709de0e6`, the bookkeeping half), #1191 / SAFE-D7-NEW-01 (the slot-0
  identity seed this needs the general case of), #2467 / REN-D9-NEW-01 (the identical
  "absolute-space skinned BLAS built from a wrong transform" class), #1796 / D6-02.
- **Suggested Fix**: Two independent options, either sufficient:
  (a) **Make the undefined case defined** — extend `seed_persistent_bind_inverses_identity`
  from slot 0 to the whole `bind_inverses_persistent` buffer (2 MB; needs a staging copy
  or `cmd_fill_buffer`-style path rather than `cmd_update_buffer`'s 64 KiB payload cap).
  A never-uploaded slot then yields `palette = boneWorld × identity`, a well-defined
  bind-pose-at-bone-world transform — visually wrong but finite and traversable, which is
  the same degrade the pool-overflow path already accepts.
  (b) **Gate the frame** — have `record_skinned_blas_refit` consult
  `bind_inverse_upload_failed` and skip the first-sight BUILD (leaving the entity to next
  frame's retry, which the #3569 requeue already guarantees). Cheaper, but only covers the
  upload-failure route, not a future one that leaves a slot unwritten.
  (a) is the structural fix; (b) is the one-line stopgap. Either should carry a source-scan
  guard in the same shape as `bind_inverse_upload_failure_latch_tests`.

---

---

# HIGH

### REN-2026-09-06-D11-01: the water pipeline coverage-blends two G-buffer attachments `water.frag` never writes

- **Severity**: HIGH
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/water.rs` (`create_water_pipeline`'s `attachments` array + the `masked_off` / `auxiliary_blend` locals; module doc), `crates/renderer/shaders/water.frag` (its fragment-output declarations), `crates/renderer/src/vulkan/pipeline.rs` (`auxiliary_blend_attachment`)
- **Status**: NEW
- **Description**: `d9e61ead` (bundling #3821 / REN-WD-D8-01) changed the water
  pipeline's blend table from `[hdr_blend, masked_off × 5, fsr_mask_max × 2]`
  to `[hdr_blend, masked_off, masked_off, masked_off, auxiliary_blend,
  auxiliary_blend, fsr_mask_max, fsr_mask_max]`, giving colour attachments 4
  (`raw_indirect`) and 5 (`albedo`) a `color_write_mask = RGBA` with
  `blend_enable = true` (`SRC_ALPHA` / `ONE_MINUS_SRC_ALPHA`). It did **not**
  add the matching fragment outputs. `water.frag` declares exactly three
  outputs — `layout(location = 0) out vec4 outColor`,
  `layout(location = 6) out float outFsrReactive`,
  `layout(location = 7) out float outFsrTransparency` — and nothing at 4 or 5.
  Per the Vulkan fragment-output-interface rules, an attachment enabled for
  writing that the fragment shader's interface does not include receives
  **undefined** values; with blending on, the undefined source colour *and*
  source alpha both feed the blend equation, so the destination (the opaque
  receiver's demodulated GI and albedo) is destroyed by an undefined amount
  rather than attenuated by water's coverage.
  The three attachments that stay `masked_off` (1 normal, 2 motion, 3 mesh_id)
  are fine — a zero write mask discards the undefined value.
  The previous shape was correct precisely *because* every non-declared
  attachment was masked off; the shared-state extraction inherited the
  ordinary blend pipeline's states without inheriting `triangle.frag`'s
  outputs, and `triangle.frag` *does* declare 4 and 5.
- **Evidence**:
  - `water.rs`: `let attachments = [hdr_blend, masked_off, masked_off, masked_off, auxiliary_blend, auxiliary_blend, fsr_mask_max, fsr_mask_max];`
  - `pipeline.rs::auxiliary_blend_attachment` → `.color_write_mask(vk::ColorComponentFlags::RGBA).blend_enable(true).src_color_blend_factor(SRC_ALPHA).dst_color_blend_factor(ONE_MINUS_SRC_ALPHA)`, pinned by `auxiliary_blend_attachment_is_a_src_alpha_coverage_blend`.
  - `grep -n ") out " crates/renderer/shaders/water.frag` → only locations 0, 6, 7.
  - `water.frag`'s own comment above those declarations still reads "the
    intermediate G-buffer attachments stay masked off as before" — written
    for the pre-#3821 table and now false for 4/5.
  - `attachment_doc_pin_tests::module_doc_matches_the_blend_table` pins the
    table string and the module doc against each other, but has no
    shader-side limb, so it passed through this change.
- **Impact**: Every pixel a water plane covers writes undefined values into
  the `raw_indirect` and `albedo` G-buffer attachments. `svgf_temporal.comp`
  consumes `raw_indirect` and `composite.frag` re-multiplies
  `indirect * albedo`, so the blast radius is every exterior/interior cell
  with water — lake and river beds, waterfalls, sewers, the FNV/FO4 water
  interiors. Symptoms would read as unstable colour/brightness in the water
  column, potentially with per-driver and per-frame variation (an unwritten
  output typically retains whatever the shader last left in that register —
  commonly the HDR colour — so it can look plausible-but-wrong rather than
  obviously broken). Not a crash; a correctness violation with a whole
  content-class blast radius. Note this is the *opposite* direction from what
  #3821 set out to fix: the receiver's GI is not "attenuated by water's
  coverage", it is replaced by an undefined value.
- **Related**: #3821 / REN-WD-D8-01 (`docs/audits/AUDIT_RENDERER_2026-09-04.md`,
  the finding this regressed out of), `d9e61ead` (the commit), #2745 (why
  refractive glass preserves attachment 3 but not 4/5), #3604 (the doc-pin
  test that has no shader limb).
- **Suggested Fix**: Two viable directions, both `cargo test`-pinnable.
  (a) Declare `outRawIndirect` (location 4) and `outAlbedo` (location 5) in
  `water.frag` and write the values #3821 actually wanted — the water
  surface's own demodulated indirect + albedo with `finalAlpha` in the alpha
  lane, mirroring `triangle.frag`'s `auxiliaryAlpha = isAlphaBlend ?
  finalAlpha : 1.0` convention (#883f57cd). This is the fix that delivers
  #3821's intent. (b) If (a) is more shader work than wanted right now,
  revert 4/5 to `masked_off` and reopen #3821 — that restores the previously
  correct (if un-attenuated) behaviour instead of an undefined one.
  Either way, add a limb to `attachment_doc_pin_tests` that scans
  `include_str!("../../shaders/water.frag")` and asserts *every* attachment
  index whose blend state is not `masked_off` has a matching
  `layout(location = N) out` — the class of bug the existing pin cannot see.
  `water.rs` and `presentation.rs` are also the only two graphics pipelines
  in the crate that do not run `reflect::validate_set_layout`; a
  fragment-output arm on that helper would generalise the guard.

---

### REN-2026-09-06-D12-01: #1260's off-frustum `flags = 0` skip rests on a premise `include/ray_hit.glsl` invalidated two months later

- **Severity**: HIGH
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (the `let flags = if skip_batch { 0u32 } else { … }` block and its #1260 rationale comment), `crates/renderer/shaders/include/ray_hit.glsl` (`getHitInterpolatedNormal`, `rayHitHasCoverage`), `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas_instances`)
- **Status**: NEW
- **Description**: #1260 (`26c60335`, 2026-05-24) skips per-instance flag
  assembly for draws that will not be rasterized —
  `skip_batch = !draw_cmd.in_raster || draw_cmd.is_water` — and ships
  `GpuInstance.flags = 0` for them. Its written justification is explicit:
  > "CAUSTIC_SOURCE is gated by the meshId G-buffer …, which only contains
  > pixels for in-frustum rasterized geometry. The RT hit paths read
  > `hitInst.vertexOffset / indexOffset / materialId / avgAlbedo* /
  > textureIndex` … **but NEVER `hitInst.flags`**."

  That is no longer true. `include/ray_hit.glsl` — `#include`d by
  `raytrace.glsl`, `shadow_transport.glsl`, `triangle.frag` and `water.frag`
  — now reads the **hit** instance's flags at four sites, all added after
  #1260 landed: `INSTANCE_FLAG_FLAT_SHADING` and
  `INSTANCE_FLAG_NON_UNIFORM_SCALE` in `getHitInterpolatedNormal`
  (`9ade7506`, 2026-07-30), and `INSTANCE_FLAG_DIFFUSE_ALPHA` and
  `INSTANCE_FLAG_ALPHA_BLEND` in `rayHitHasCoverage` (`5d8bb982`,
  2026-07-26).
  Off-frustum instances are exactly the population that stays in the TLAS —
  `build_tlas_instances`'s own comment: "frustum culling only gates
  rasterization (`in_raster`) and off-screen occluders stay in" — so they are
  precisely the instances rays land on while carrying `flags == 0`.
- **Evidence**:
  - `ray_hit.glsl`, `getHitInterpolatedNormal`:
    `if ((hitInst.flags & INSTANCE_FLAG_FLAT_SHADING) != 0u) { … } else if ((hitInst.flags & INSTANCE_FLAG_NON_UNIFORM_SCALE) != 0u) { … transpose(inverse(model3)) * localN … } else { worldN = model3 * localN; }`
  - `ray_hit.glsl`, `rayHitHasCoverage`:
    `if ((inst.flags & INSTANCE_FLAG_DIFFUSE_ALPHA) == 0u && mat.alphaThreshold == 0.0) { alpha = 1.0; }` and
    `if ((inst.flags & INSTANCE_FLAG_ALPHA_BLEND) != 0u && mat.materialKind != MATERIAL_KIND_GLASS) { return alpha >= (1.0 / 255.0); }`
  - Callers of `rayHitHasCoverage`: `raytrace.glsl` (reflection/GI traversal),
    `shadow_transport.glsl` (×2, shadow traversal), `triangle.frag`,
    `water.frag` — all pass a *hit* instance index, not the shading fragment's own.
  - Commit dates: `26c60335` 2026-05-24 (the skip) vs `5d8bb982` 2026-07-26
    and `9ade7506` 2026-07-30 (the flag reads).
- **Impact**: For every frustum-culled instance a ray hits:
  1. **`ALPHA_BLEND` clear** → the "pure blend geometry uses alpha as binary
     coverage for ray traversal" branch never runs, so an off-screen
     alpha-blended surface is a **fully opaque blocker** for shadow,
     reflection and GI rays. A glass pane, foliage card, curtain or FX card
     just outside the frustum casts a solid shadow that vanishes the moment
     it enters the frustum — a camera-dependent lighting change on geometry
     the camera cannot see, which is the exact class of artifact off-screen
     TLAS retention (#516) exists to avoid.
  2. **`DIFFUSE_ALPHA` clear** → `alpha` is forced to `1.0` for these hits
     whenever `mat.alphaThreshold == 0.0`, reinforcing (1) even for materials
     that *do* carry an authored alpha channel (BC3/BC7).
  3. **`NON_UNIFORM_SCALE` clear** → `getHitInterpolatedNormal` takes the
     plain `model3 * localN` branch for non-uniformly-scaled off-frustum
     instances, so the hit normal is skewed. Bethesda cells scale placed
     REFRs non-uniformly routinely; the wrong normal biases the shading and
     the ray-offset of every bounce off that surface.
  4. **`FLAT_SHADING` clear** → flat-shaded off-frustum geometry gets
     smooth-interpolated hit normals.
  The severity is set by the fact that (1) silently changes direct shadowing,
  which is the most visible RT output, and by the blast radius: every cell,
  every game, every frame, for whatever fraction of the loaded set is
  currently out of frustum (typically most of it).
- **Related**: #1260 / PERF-D3-NEW-04-05 (the optimization), #516 (why
  off-frustum entries exist in the SSBO/TLAS at all), #922 (the
  `CAUSTIC_SOURCE` half of the rationale, which *is* still sound — the
  caustic gate really is mesh-ID-driven), #ae285062 (the `DIFFUSE_ALPHA`
  BC1 contract this now half-applies).
- **Suggested Fix**: Narrow the skip to the flags that are provably
  rasterizer-only rather than zeroing the whole word: assemble
  `NON_UNIFORM_SCALE`, `FLAT_SHADING`, `ALPHA_BLEND` and `DIFFUSE_ALPHA`
  unconditionally (the first is three dot products, the last two are one
  branch plus a cached `handle_has_alpha` lookup already paid for every
  in-frustum blended draw), and keep skipping only `TERRAIN_SPLAT` + tile
  index and the `RENDER_LAYER` bits, which no `ray_hit.glsl` / `raytrace.glsl`
  / `shadow_transport.glsl` path reads. Then replace the prose invariant with
  a `shader_constants`-style source-scan test that greps the RT include set
  for `\.flags &` and asserts every constant it finds is in the
  unconditionally-assembled set — the premise rotted silently precisely
  because it was only a comment.

---

---

# MEDIUM

### REN-2026-09-06-D1-01: `#3840`'s `resident_static_blas_bytes` is wired into the one predicate where it cannot change any outcome — the "admission" its own docstring describes does not exist, so a BLAS batch still allocates against headroom the GPU has not released


- **Severity**: MEDIUM
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs`
  (`resident_static_blas_bytes`, and its single consumer inside
  `build_blas_batched`'s mid-batch check);
  `crates/renderer/src/vulkan/acceleration/predicates.rs`
  (`should_evict_mid_batch`, `blas_over_budget`)
- **Status**: NEW (fresh churn — `fa5c4191`, 2026-09-05; not in the 151-issue
  OPEN cache; `#3840` is the *fix* commit, not an open issue)
- **Description**: `fa5c4191` added `pending_destroy_static_bytes` and the
  derived `resident_static_blas_bytes()` to fix a real accounting gap — eviction
  credits `static_blas_bytes` the instant it queues an entry, but the allocator
  free happens `DEFAULT_COUNTDOWN` frames later inside `draw_frame`, which never
  runs during a streaming batch. The counter itself is correct and complete
  (verified: every static push credits it, both destroy paths release it, skinned
  entries are excluded via `counted_in_static_bytes`). The **wiring** is the
  problem. `resident_static_blas_bytes()` has exactly one non-test call site in
  the workspace — the first argument of `should_evict_mid_batch` — and at that
  site it is provably inert:
  - trigger: `(A + pending) * 10 >= budget * 9` (the 90 % line);
  - callee gate and loop break: `blas_over_budget(static_blas_bytes, pending, budget)`
    = `paper + pending > budget` (the 100 % line), deliberately on the **paper**
    figure.

  Because `budget > 0.9 · budget`, every state in which the callee can actually
  evict already satisfies the trigger *with the paper figure*. Substituting
  `resident` (= `paper + queued`) can only widen the trigger set into states
  where the callee immediately early-returns. So the change cannot cause one
  extra byte to be evicted, and — since nothing in `build_blas_batched` ever
  declines to allocate — it cannot prevent one extra byte from being allocated
  either. Its only effect is additional no-op calls to `evict_unused_blas` every
  `BATCH_EVICTION_CHECK_INTERVAL` iterations.

  Meanwhile `resident_static_blas_bytes`'s docstring asserts the opposite:
  *"This is the figure admission checks must use … letting a batch allocate
  against headroom that does not exist yet (#3840)."* There is no admission
  check. The hazard the docstring names is still open.
- **Evidence**:
  - `grep -rn "resident_static_blas_bytes" --include='*.rs' crates byroredux` →
    5 hits: the definition, one doc cross-reference in
    `acceleration/mod.rs`, one comment inside `evict_unused_blas`, the single
    call inside `build_blas_batched`'s mid-batch check, and the source-shape
    assertion in `acceleration/tests/blas_static_tests.rs`. **Zero** other
    consumers.
  - `evict_unused_blas`'s first statement after the `let _ = (device, allocator);`
    is `if !blas_over_budget(self.static_blas_bytes, pending_bytes, self.blas_budget_bytes) { return; }`
    — the paper figure, by design (`#3840` comment at the push site, pinned by
    `mid_batch_trigger_uses_resident_bytes_but_the_evict_loop_does_not`).
  - `build_blas_batched`'s Phase-1 loop calls
    `GpuBuffer::create_device_local_uninit(...)?` unconditionally on every
    iteration; the only budget interaction in the loop is the
    `should_evict_mid_batch` → `evict_unused_blas` pair. The compaction-phase
    call (`alloc_compact`, `pending = total_before + total_after`) is likewise
    only an eviction request, not an admission gate.
  - The reasoning is sound *within* the current design: the `#3840` comment is
    right that a resident-based **loop break** would never be satisfied (each
    iteration just moves bytes from `static_blas_bytes` into
    `pending_destroy_static_bytes`). Eviction genuinely cannot reclaim inside a
    batch. That is precisely why the missing piece has to be admission, not
    eviction.
- **Impact**: On the 6 GB RT-minimum target the doc calls out
  (`blas_static.rs`'s own *"this is a 6 GB-RT-minimum-target path"* note), a
  cell-load batch that evicts early and then keeps allocating can overshoot the
  real static-BLAS residency by up to the whole deferred-destroy backlog before
  anything pushes back. The blast radius is bounded, not catastrophic:
  `build_blas_batched` handles allocator failure with full rollback of
  `prepared` + `compact_accels` + the query pool (#1097 / #2926) and returns
  `Err`, which `restore_missing_static_blas_for_draws` / the cell loader turn
  into a `warn!` and a cell whose rigid geometry is missing from RT for that
  frame. So the failure mode is "RT loses the tail of a cell under VRAM
  pressure", not corruption or device loss. On the 12 GB dev card it is
  unreachable. Secondary impact: the docstring is a false statement about a
  memory-safety-adjacent invariant, and it is the kind a future reader will
  trust rather than re-derive.
- **Related**: `#3840` (the commit), `#1792` / `PERF-D3-NEW-01` (the previous
  round of the same "the gate and the trigger disagree" bug in this exact pair
  of predicates), `#1449` / `#1782` (why the deferral itself must stay),
  `#3540` / `plan_static_blas_restore` (the precedent policy: *decline the pass*
  rather than thrash).
- **Suggested Fix**: Give the counter an actual admission consumer. The
  cheapest correct shape mirrors `plan_static_blas_restore`: inside
  `build_blas_batched`'s Phase-1 loop, once
  `resident_static_blas_bytes() + pending_bytes` exceeds `blas_budget_bytes`
  **and** the preceding `evict_unused_blas` reclaimed nothing, stop admitting
  further meshes, finish the batch with what is already prepared, and return the
  partial count with a one-shot `warn!` — the same "decline rather than
  converge-never" policy #3540 installed for the recovery pass. Do **not**
  attempt to fix this by ticking `pending_destroy_blas` at a batch boundary:
  that is exactly the #1449 / #1782 use-after-free class, and the countdown's
  whole purpose is to stand in for a fence wait `build_blas_batched` does not
  have. This is CPU-side bookkeeping only — no barrier, render-pass or pipeline
  change is involved.

---

### REN-2026-09-06-D10-01: `weatherGroundNoise` hashes ABSOLUTE world XZ through `sin()`, whose argument reaches ~3×10⁷ rad in vanilla exterior worldspaces

- **Severity**: MEDIUM
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/renderer/shaders/triangle.frag` (`weatherGroundNoise`, and its three call sites in the `terrainSplatActive && jitter.w > 0.5` weather block)
- **Status**: NEW (landed in `01451d5e` "implement weather surface state management for rain and snow effects"; no open issue matches `weather|snow|puddle|noise|hash`)
- **Description**: The weather surface response keys its puddle and snow-patch fields on
  the **absolute** reconstructed position (`fragWorldPos.xz`), not on the render-origin-relative
  varying. Using absolute coordinates here is defensible on its own — a relative key would
  make the pattern jump every time `render_origin` snaps across a 4096-unit cell boundary.
  The problem is the *formulation*: `weatherGroundNoise` is the classic
  `fract(sin(dot(cell, vec2(127.1, 311.7))) * 43758.5453)` sine hash, which multiplies the
  coordinate up by ~440× before the precision-critical `sin`. That amplification is what
  `RT_ABSOLUTE_PRECISION_CEILING` cannot protect against: the ceiling bounds `|coord|` at
  2²⁰, but this consumer fails an order of magnitude *below* it.
- **Evidence**:
  - `float weatherGroundNoise(vec2 worldXZ, float cellSize) { vec2 cell = floor(worldXZ / max(cellSize, 0.001)); return fract(sin(dot(cell, vec2(127.1, 311.7))) * 43758.5453); }`
  - Call sites pass `fragWorldPos.xz` (absolute) with `cellSize` 7.0, 2.5 and 3.5.
  - Worked number at the smallest cell size, on shipped content: `docs/engine/shader-pipeline.md`
    gives Skyrim Tamriel ≈ ±233 000 u and Markarth at X ≈ −176 000. At `cellSize = 2.5`,
    `|cell|` reaches ~93 000; `|dot(cell, vec2(127.1, 311.7))|` reaches ~4.1×10⁷. f32 ULP at
    4×10⁷ (exponent 25) is **4.0 radians** — more than half a full period of `sin`.
  - The gate is `terrainSplatActive`, i.e. this code only ever runs on exterior LAND — the
    exact regime where the argument is largest. It is unreachable in the interiors and
    unit-scale harness scenes where it would be numerically well-behaved.
  - GLSL/SPIR-V places no useful accuracy requirement on `Sin` at arguments this far
    outside `[−π, π]`, so the field is additionally **driver-dependent**: the same cell can
    hash differently on two GPUs.
- **Impact**: Visual only, but real and not reproducible across hardware. The puddle/snow
  patch field stops being the intended per-cell white hash at exterior distances; what it
  actually produces there is unspecified. This also makes any future "does the weather
  system look right?" comparison between two machines meaningless.
- **Related**: `docs/engine/shader-pipeline.md` §"Coordinate Spaces & Precision" ("Any future
  absolute-space shader consumer inherits this same ceiling") — this is the first consumer
  found that needs a *tighter* bound than the ceiling provides. Adjacent but distinct:
  `triangle.frag`'s translucency turbulence proxy `sin(NdotV * 11.0 + fragWorldPos.x * 0.013)`
  scales *down* by 0.013 and stays well-conditioned; not a finding.
- **Suggested Fix**: Replace the sine hash with an integer-domain one that is exact at any
  magnitude — hash the `ivec2` cell index with a bit-mixing function (e.g. Wang/PCG-style
  `uint` mixing on `floatBitsToInt(cell)` or on `ivec2(cell)`), which keeps the absolute
  world key (so the pattern stays anchored across origin snaps) while removing the
  large-argument transcendental entirely. Alternatively keep `sin` but wrap the dot product
  into `[0, 2π)` first — cheaper to write, but it re-quantises rather than fixing the
  precision loss, so the integer hash is the better answer.

---

### REN-2026-09-06-D12-02: `taa_failed` / `svgf_failed` can never latch — `c43cb269`'s #3605 fix sits on an unreachable branch, and the one reachable TAA failure is warn-only

- **Severity**: MEDIUM
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/taa.rs` (`TaaPipeline::dispatch`), `crates/renderer/src/vulkan/svgf.rs` (`SvgfPipeline::dispatch`), `crates/renderer/src/vulkan/context/post_passes.rs` (`record_taa_pass`, `record_svgf_pass`), `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (the `taa.upload_params` call site)
- **Status**: NEW
- **Description**: `TaaPipeline::dispatch` and `SvgfPipeline::dispatch` are
  declared `-> Result<()>` but their bodies contain **no error-producing
  construct at all** — no `?`, no `return Err`, no `bail!`, no `.context(`,
  no `map_err` — and both end in an unconditional `Ok(())`. Every statement
  is an infallible `ash` command recording call. Therefore
  `if let Err(e) = taa.dispatch(…)` in `record_taa_pass` and the matching arm
  in `record_svgf_pass` are dead code, and `self.taa_failed` /
  `self.svgf_failed` can never become `true` at runtime (they are only ever
  set to `false`, at construction and on resize).
  Three consequences:
  1. `c43cb269` ("Fix #3605: signal a temporal discontinuity on TAA dispatch
     failure", 2026-09-05) adds `signal_temporal_discontinuity(
     TAA_DISPATCH_FAILURE_RECOVERY_FRAMES)` inside that dead arm. Its
     regression guard,
     `record_taa_pass_signals_temporal_discontinuity_on_dispatch_failure`,
     is a source-scan that asserts the *text* is present — so it passes while
     the behaviour is unreachable. The hazard #3605 describes is real; the
     fix as landed cannot fire.
  2. The `#1932` un-jitter gate
     (`assemble_camera_and_lights` gates Halton jitter on
     `taa.is_some() && !taa_failed`) and the `fall_back_to_raw_hdr` reroute
     are likewise unreachable.
  3. The TAA failure that **is** reachable is a different one:
     `taa.upload_params` (which does `param_buffers[frame].write_mapped(…)`
     — a mapped-slice/flush operation, the same fallible class #2504
     hardened for `upload_indirect_draws`) is handled with a bare
     `log::warn!("TAA upload_params failed: {e}")` and no latch. On that
     path the dispatch still runs, against whatever the params UBO held
     before (a previous frame's, or uninitialised on a slot's first use),
     while the geometry pass has already rendered jittered. That is exactly
     the "jittered but unresolved" state #3605 exists to protect against,
     and it is the case with no protection.
- **Evidence**: extracting each `dispatch` body by brace matching and
  grepping for `?;`, `return Err`, `bail!`, `Err(`, `.context(`, `map_err`
  yields zero hits for `taa.rs`, `svgf.rs` (and `bloom.rs`, whose arm is a
  harmless `warn!` with no latch); `ssao.rs`, `volumetrics.rs` and
  `caustic.rs` by contrast do contain `?`, so the fallible-dispatch shape is
  genuine elsewhere in the same file's call sequence — this is a
  three-pipeline anomaly, not a blanket convention.
  `crates/renderer/src/vulkan/context/mod.rs` documents `taa_failed` as
  "first `taa.dispatch` error in a session".
- **Impact**: A documented per-pass permanent-failure recovery tier (named as
  such in `record_post_passes`'s own doc: "the per-pass permanent-failure
  latches are preserved exactly") does not exist for SVGF or TAA. No runtime
  misbehaviour today — nothing fails, so nothing is mishandled — but two
  shipped fixes (#1932, #3605) and one reroute are unverifiable dead weight,
  and the real failure mode (a params-upload failure) silently degrades to a
  stale-parameter TAA resolve on a jittered frame. Also a maintenance trap:
  the source-scan guard gives false confidence that the path is exercised.
- **Related**: #3605 / REN-2026-08-30-D13-02 (`c43cb269`), #1932 / TAA-D13-01
  (the jitter gate), #917 / REN-D10-NEW-03 (`dispatched_this_frame`), #2504 /
  D12-2026-08-07-02 (the same fallible-upload class, correctly handled for
  indirect draws), #2146 / #917 (why `record_post_passes` is infallible).
- **Suggested Fix**: Route the reachable failure into the existing latch
  instead of inventing a new one: make the `taa.upload_params` /
  `svgf.upload_params` call sites set `taa_failed` / `svgf_failed` on `Err`
  (they already run in `build_and_upload_instances`, before
  `record_post_passes`, so the `!self.taa_failed` gate at the dispatch site
  picks it up in the same frame and the jitter gate picks it up the next).
  That makes #3605's `signal_temporal_discontinuity` live — but see
  **D12-03**, which must be fixed in the same change. Separately, either drop
  the `-> Result<()>` on the two dispatches or add a comment saying it is
  reserved; a `Result` no producer can populate is what made the dead arm
  look alive to three successive audits.

---

### REN-2026-09-06-D16-01: #3829's fix closed one of four name-diverging GLSL↔Rust struct mirrors; `ClusterEntry`, `FogClusterEntry` and `CombustionLightMoment` remain outside every lockstep guard


- **Severity**: MEDIUM (defense-in-depth gap on a class with a demonstrated CRITICAL outcome 32 hours ago; not itself a live drift — all three are in sync at HEAD)
- **Dimension**: Volumetrics (GPU-struct lockstep)
- **Location**:
  - `crates/renderer/shaders/volumetrics_inject.comp` — `struct FogClusterEntry`, `struct CombustionLightMoment`, `struct ClusterEntry`
  - `crates/renderer/shaders/cluster_cull.comp` — `struct ClusterEntry`
  - `crates/renderer/shaders/include/bindings.glsl` — `struct ClusterEntry`
  - Rust counterparts: `crates/renderer/src/vulkan/volumetrics.rs` (`GpuFogClusterEntry`, `GpuCombustionLightMoment`), `crates/renderer/src/vulkan/compute.rs` (`ClusterEntry`)
  - The guards that do **not** cover them: `assert_mirror_list_is_complete` / `shader_sources_declaring` and the four lockstep tests that call them (`gpu_instance_glsl_copies_stay_in_lockstep`, `gpu_light_glsl_copies_stay_in_lockstep`, `gpu_water_params_rust_and_glsl_copies_stay_in_lockstep`, `gpu_terrain_tile_glsl_and_rust_fields_stay_in_lockstep`), all in `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`
- **Status**: **NEW.** No matching open issue (`open_titles.txt` searched for `clusterentry`, `combustion`, `fogcluster`, `mirror`, `lockstep`, `stride` — zero hits). Not a finding in `AUDIT_RENDERER_2026-09-05.md`, which explicitly framed `GpuBoundaryInstance` as *"a sixth mirror sitting entirely outside the tracked set, **not a symptom of a wider pattern**"*. That framing was wrong: the pattern has three more members. `fa5c4191`'s own commit message records the sibling check — *"shader `FogClusterEntry` and `CombustionLightMoment` mirror Rust structs under different names too, and share #3829's discovery blind spot, but both are currently in sync (8 B and 32 B)"* — so the class was **seen and verified once, but not guarded**, and `ClusterEntry` (three GLSL copies) was not part of even that check.
- **Description**: The discovery half of the mirror guard, `shader_sources_declaring(decl)`, matches on a **literal declaration string** (`"struct GpuInstance"`, `"struct GpuLight"`, …). A GLSL struct that mirrors a Rust struct under a *different name* is therefore invisible to it, and the hand-written `SOURCES` tables only list what someone remembered. #3829 was exactly that failure — `GpuBoundaryInstance` went 13 days at a 128 B stride against a 160 B `GpuInstance`, with a green suite, because it wore its own name. The fix added `gpu_boundary_instance_stride_matches_gpu_instance` for that one struct and did not generalise.

  Three name-diverging mirrors remain, none of them covered by any test that reads the GLSL side:

  1. **`CombustionLightMoment` ↔ `GpuCombustionLightMoment`** — eight tightly packed `uint`s, 32 B. The Rust struct's own doc says *"Fixed-point ABI mirrored by `CombustionLightMoment` in `volumetrics_inject.comp`"*, i.e. a stated cross-language ABI. `combustion_light_moment_abi_is_eight_std430_words` asserts **only the Rust side** (`size_of == 32`, `align_of == 4`, `COMBUSTION_LIGHT_GRID_COUNT == 256`); it never `include_str!`s the shader. This one is **field-order sensitive, not just stride sensitive**: the shader writes by name (`atomicAdd(combustionLightMoments[binIndex].weighted_x, …)`, `.radiant_r`, `.luminous_volume`, …) while `decode_combustion_light_moment` decodes **positionally** by word index (`weight: word(0) … luminous_volume: word(7)`). A GLSL-side reorder compiles clean, keeps the size at 32 B, and silently swaps a luma-weighted centroid for a radiant channel.
  2. **`FogClusterEntry` ↔ `GpuFogClusterEntry`** — `{offset, count}`, 8 B. Only a Rust-side `assert_eq!(size_of::<GpuFogClusterEntry>(), 8)` exists. This buffer is now read under the **#3834 partial-upload** contract, where a wrong `count` decode is what makes a stale cluster live.
  3. **`ClusterEntry`** — `{offset, count}`, declared **three times in GLSL** (`cluster_cull.comp` writes it, `include/bindings.glsl` and `volumetrics_inject.comp` read it) against one Rust `#[repr(C)] struct ClusterEntry` in `compute.rs` that only ever appears as `size_of::<ClusterEntry>() * TOTAL_CLUSTERS`. Three GLSL copies with **zero** lockstep coverage — the same multi-copy shape `GpuInstance` and `GpuLight` each have a test for.
- **Evidence**:
  ```
  $ grep -rn "^struct " crates/renderer/shaders/*.{comp,frag,vert} crates/renderer/shaders/include/*.glsl
  → ClusterEntry ×3, FogClusterEntry ×1, CombustionLightMoment ×1, GpuBoundaryInstance ×1,
    GpuFogVolume ×1, GpuInstance ×5, GpuLight ×4, GpuMaterial ×1, GpuTerrainTile ×1, Reservoir ×1
  $ grep -rn "ClusterEntry" crates/renderer/src
  → definition (compute.rs:23) + two size_of uses. No test.
  ```
  `crates/renderer/src/vulkan/reflect.rs` cannot substitute: `uniform_block_size_by_name` reflects **uniform blocks** only. Every struct above lives in an SSBO element array, which the reflector does not size — that is precisely why #3829 needed a source-text test rather than a reflection one.
  Counter-check performed: `GpuFogVolume` **is** covered (`gpu_fog_volume_glsl_field_order_matches_rust_struct`, #2228), so this is a specific gap, not a blanket absence.
- **Impact**: No live corruption today — all three were confirmed field-for-field and size-for-size in sync at HEAD, and the workspace suite is green. The exposure is the next edit: any of these five structs can grow or reorder with a fully green `cargo test`, and the failure mode is the #3829 one — silently wrong data, no validation-layer diagnostic. Blast radius per struct: `CombustionLightMoment` → wrong fire/explosion surface lights (the field-order case is the nastiest, because size stays right); `FogClusterEntry` → wrong local-fog cluster walk under the new partial-upload contract; `ClusterEntry` → wrong clustered-light lists in **both** the fragment shader and the volumetrics inject pass simultaneously.
- **Related**: #3829 (the CRITICAL this class produced, and its one-struct fix), #3231 (the growth that triggered it), #2748 / #3564 (the `GpuInstance` mirror guard + its completeness half), #2228 (the `GpuFogVolume` precedent for a GLSL-reading field-order test), #3834 (the partial-upload contract now riding on `FogClusterEntry`), `feedback_shader_struct_sync.md`.
- **Suggested Fix**: Two steps, the second more valuable than the first.
  1. Add three narrow guards on the `gpu_fog_volume_glsl_field_order_matches_rust_struct` / `gpu_boundary_instance_stride_matches_gpu_instance` pattern — `include_str!` the shader, `parse_glsl_struct_fields_typed`, compare against `parse_rust_struct_fields` (name+order) and `std430_struct_size` (stride). `CombustionLightMoment` needs the **field-order** leg, not just the stride leg, because `decode_combustion_light_moment` is positional. `ClusterEntry` additionally needs its three GLSL copies compared against each other.
  2. Close the discovery gap so a *fifth* name-diverging mirror cannot appear unnoticed: extend `assert_mirror_list_is_complete`'s companion walk to enumerate **every** `^struct ` declaration across `crates/renderer/shaders/` and assert each name appears in a registry of "tracked mirror" or "shader-local, no Rust counterpart" (today's shader-local set: `LocalMedium`, `CombustionDifferential`, `DisneyDiffuseSplit`). That converts "someone remembered" into "someone had to classify it", which is the only version of this guard that survives the next struct.

---

### LOW

### REN-2026-09-06-D17-01: `specularAaRoughness`'s screen-space derivatives run inside per-invocation-divergent control flow at all five call sites, and are recomputed once per cluster light for a fragment-invariant value


- **Severity**: MEDIUM
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/include/pbr.glsl` (`specularAaRoughness`), `crates/renderer/shaders/include/lighting.glsl` (`shadowableLightRadiance`), `crates/renderer/shaders/triangle.frag` (five call sites)
- **Status**: NEW
- **Description**: `shadowableLightRadiance` opens with

  ```glsl
  float aaRoughness = ((dbgFlags & DBG_DISABLE_SPECULAR_AA) != 0u)
      ? roughness
      : specularAaRoughness(N, roughness);
  ```

  and `specularAaRoughness` evaluates `dFdx(N)` / `dFdy(N)`. Every one of the function's five call sites in `triangle.frag` is reached only under control flow that diverges *per invocation*, never per quad:

  1. the cluster light loop, past `if (contribution < 0.001) continue;` — a per-fragment N·L / attenuation gate, so lanes in a quad can be on different `i`, or have exited the loop entirely;
  2. the ReSTIR **temporal** reuse gate (`sameSurface && rpLightIndex < lightCount && … && rp.W > 0.0 && !isnan(rp.W)`) — reprojection validity is per pixel;
  3. the ReSTIR **spatial** reuse gate (`rnSurfaceId == surfaceId && spatialDepthCompatible && … && dot(geomN, nGeomN) >= SPATIAL_NORMAL_COS`);
  4. the selected-light block (`if (restirY != 0xFFFFFFFFu && restirW > 0.0 && visibilityMaskNeedsTrace(...))`);
  5. the legacy-WRS shadow subtraction, `if (any(lessThan(transmission, vec3(0.999))))` — gated on a **ray-query result**, the most divergent predicate in the shader.

  GLSL/SPIR-V leave derivative results undefined in non-uniform control flow. This is the same defect class #3622 fixed in `parallaxDisplaceUV`, whose comment states the rule explicitly ("Implicit derivatives are undefined per the GLSL/Vulkan spec when the sample sits inside non-uniform control flow"); the sibling in the BRDF assembly was not swept. It is also aggravated by the alpha-test `discard` earlier in `main()`: a quad that lost a lane to `OpKill` has undefined derivatives for the survivors from that point on, and hoisting above the discard removes that exposure too.

  Independently of the spec question, the call is **redundant**. `N` is final by the terrain-splat/weather block and `roughness` by the weather-puddle/snow block — both hundreds of lines before the first call site, and neither is written inside the light loop. A 16-light cluster therefore executes the `dFdx`+`dFdy`+clamp+two-`sqrt` chain sixteen times per fragment to produce sixteen identical values, and forces the quad into lockstep at each one.
- **Evidence**:
  - `pbr.glsl`: `float specularAaRoughness(vec3 N, float roughness) { vec3 dNdx = dFdx(N); vec3 dNdy = dFdy(N); … }`
  - `lighting.glsl`: `shadowableLightRadiance` calls it unconditionally (modulo the uniform `DBG_DISABLE_SPECULAR_AA` UBO bit).
  - Invariance: `roughness` is assigned only at the gloss-map mix and the two `weatherPuddles`/`weatherSnow` mixes; `N` only in the normal-map / model-space-normal / terrain-splat / snow / glass blocks — all strictly before the lighting section.
  - Precedent: `material_sampling.glsl`'s `parallaxLod` comment (#3622) and `ray_hit.glsl`'s always-explicit `textureLod`.
- **Impact**: Undefined specular-AA roughness on lit fragments — the visible signature would be inconsistent specular-lobe width at light-cluster and ReSTIR-reuse boundaries, and near alpha-tested geometry. Because `shadowableLightRadiance` is *also* the ReSTIR `pHat` scorer and the legacy shadow-subtrahend, a divergent value there desynchronises the "unshadowed accumulation cancels bit-for-bit against the shadowed subtraction" invariant #1369 depends on. All games, every lit fragment. The wasted work is unconditional.
- **Related**: #3622 (the fixed sibling), #1369 (the refactor that moved the BRDF — and this derivative — into the per-light function), #2471 / #2806 (`specularAaRoughness`'s clamp history).
- **Suggested Fix**: Hoist the filter to uniform control flow: compute `float aaRoughness = (dbgFlags & DBG_DISABLE_SPECULAR_AA) != 0u ? roughness : specularAaRoughness(N, roughness);` once in `main()` immediately after `N` and `roughness` are final (and ideally before the alpha-test `discard`), and pass `aaRoughness` into `shadowableLightRadiance` in place of `roughness` — keeping the raw `roughness` as a separate parameter for `disneyDiffuseSplit`, which deliberately uses the unfiltered value. Pin with a `shader_contract_tests.rs` assertion that `specularAaRoughness` does not appear inside `lighting.glsl`. **Needs RenderDoc / an A-B capture to demonstrate a visual delta**: because the inputs are branch-invariant, a compiler that hoists the derivative already produces the correct value, so the correctness half is a conformance-and-hardening claim, not an observed artefact. The redundant-work half is a source-level fact and needs no capture.

---

### REN-2026-09-06-D17-02: the anisotropic GGX branch is the one TBN builder in the shader tree without the #2815 post-Gram-Schmidt zero guard — `normalize()` on a zero vector poisons `Lo`, the ReSTIR reservoir and the EMA history with NaN


- **Severity**: MEDIUM
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/include/lighting.glsl` (`shadowableLightRadiance`, the `mat.anisotropic > 0.0` branch)
- **Status**: NEW
- **Description**: The anisotropic branch rebuilds a tangent frame from the interpolated vertex tangent:

  ```glsl
  vec3 T = normalize(fragTangent.xyz);
  T = normalize(T - dot(T, N) * N);
  ```

  The guard above it (`dot(fragTangent.xyz, fragTangent.xyz) > 1e-4`) proves only that the **raw** tangent is non-zero, not that it is non-parallel to the shading normal `N`. When `T ∥ N` the Gram-Schmidt projection is the zero vector and `normalize()` on it is `0/0` → NaN. This is precisely the hazard #2815 / REN-D19-04 fixed in `perturbNormal`, and `material_sampling.glsl`'s comment there enumerates the sibling builders that already carry the guard — `parallaxDisplaceUV`'s `if (dot(T, T) < 1e-8 || heightScale <= 0.0) return uv;` and `getRayHitTangentFrame`'s `if (dot(worldT, worldT) < 1e-8) return false;`. This fourth builder is not in that list and does not have the guard.

  `N` here is the *normal-mapped* shading normal (or `glassViewNormal`), not the geometric normal, so a strongly-perturbing normal map is enough to rotate `N` into the authored tangent's direction; `perturbNormal`'s own guard exists on exactly that reasoning.
- **Evidence**: The three guarded siblings vs the unguarded fourth, all in the same `#include` chain:
  - `material_sampling.glsl` / `perturbNormal`: `if (dot(Tproj, Tproj) < 1e-8) { return N; }`
  - `material_sampling.glsl` / `parallaxDisplaceUV`: `if (dot(T, T) < 1e-8 || heightScale <= 0.0) { return uv; }`
  - `ray_hit.glsl` / `getRayHitTangentFrame`: `worldT -= dot(worldT, N) * N; if (dot(worldT, worldT) < 1e-8) { return false; }`
  - `lighting.glsl` / `shadowableLightRadiance`: `T = normalize(T - dot(T, N) * N);` — no post-projection test.
- **Trigger Conditions**: Requires `mat.anisotropic > 0.0`. `translate_material` pins `anisotropic: 0.0` for every source format (guarded by `anisotropic_rationale_matches_what_the_source_formats_carry`), so **no shipped game content reaches this branch** — but it is deliberately reachable through the two producers built for it: `cornell::pbr_bsdf_lobes` (the `--cornell` Disney-lobe probe, `anisotropic = 0.1`) and the live `mat.set <id> anisotropic <v>` console arm (`commands/scene.rs`), both added by #2514 / REN-D21-2026-08-07-02 precisely so the anisotropic lobe could be exercised.
- **Impact**: A NaN emitted here does not stay local. `shadowableLightRadiance` is the shared BRDF for the pass-1 accumulation, the ReSTIR `pHat` score, and the legacy shadow subtraction, so a single bad fragment produces `restirWSum`/`restirPHat` NaN (→ `restirW` NaN, which the `!isnan(rp.W)` reuse gates then propagate to *neighbouring* pixels through spatial reuse) and a NaN `accum` in the EMA history, which the `mix(prevAccum, …)` recurrence can never clear. The result is a persistent, spreading black/white blot rather than a one-frame speck — and it lands first on the Cornell harness built to validate this exact lobe, which is where the false all-clear costs most.
- **Related**: #2815 / REN-D19-04 (the identical fix in `perturbNormal`), #2512 (the sibling `fragTangent.w` ±1 clamp this branch *does* have), #2514 (the constructors that make the branch reachable), #1250 (the anisotropic lobe itself).
- **Suggested Fix**: Mirror `perturbNormal` exactly — project first, test the projected length, and fall back to the isotropic `distributionGGX(NdotH, aaRoughness)` when it collapses:
  ```glsl
  vec3 Tproj = T - dot(T, N) * N;
  bool anisoUsable = dot(Tproj, Tproj) >= 1e-8;
  ```
  gating the existing `distributionGGXAniso` call on `anisoUsable`. Add it to the sibling list in `material_sampling.glsl`'s comment so the four builders stay enumerated, and pin with a `shader_contract_tests.rs` assertion that the branch contains a post-projection length test.

---

### REN-2026-09-06-D18-01: an authored-still cloud layer scrolls — `cloud_scroll_vectors` uses a zero vector as its "no data" sentinel, and its fallback ignores the authored wind direction the same record supplies


- **Severity**: MEDIUM
- **Dimension**: Sky/Weather
- **Location**: `byroredux/src/systems/weather.rs` (`cloud_scroll_vectors`, `cloud_scroll_rate_from_wind`, `weather_system`'s `CloudSimState` block); source: `crates/plugin/src/esm/records/weather.rs` (`b"ONAM"`, `b"RNAM"`, `b"QNAM"` arms, `WeatherRecord::cloud_layer_velocities`)
- **Status**: NEW
- **Description**: Two independent problems in one helper.

  **(a) Authored zero is indistinguishable from absent.** `cloud_scroll_vectors`
  decides per layer:

  ```rust
  if velocities[layer][0].abs() > 1.0e-5 || velocities[layer][1].abs() > 1.0e-5 {
      result[layer] = [velocities[layer][0] * 0.16, velocities[layer][1] * 0.16];
  }
  ```

  falling back otherwise to a wind-derived synthetic vector. `WeatherRecord::
  cloud_layer_velocities` defaults to `[[0; 2]; 4]` and is filled by the ONAM
  (FO3/FNV, one byte per layer) and RNAM/QNAM (Skyrim, the two components) arms.
  A record that authors `0` for a layer — a deliberately motionless deck — lands
  on the exact value that means "this record shipped no motion sub-record at
  all", and the engine substitutes motion the artist did not ask for. There is
  no presence flag on the struct to tell the two apart. This is the same
  presence-vs-value sentinel class as the `[1,1,1]` specular default that
  produced the chrome-flyer bug (#1873).

  **(b) The fallback drops the authored wind direction.** The fallback is a fixed
  sign/ratio table scaled only by speed:

  ```rust
  let fallback = [
      [fallback_rate, fallback_rate * 0.3],
      [-fallback_rate * 1.35, fallback_rate * 0.5],
      [fallback_rate * 0.85, fallback_rate * 0.45],
      [-fallback_rate * 1.15, fallback_rate * 0.6],
  ];
  ```

  `fallback_rate` is `cloud_scroll_rate_from_wind(wd.wind_speed)` — speed only.
  The authored WTHR wind *direction* (DATA `WTHR_WIND_DIRECTION_OFFSET`, turned
  into `[cos θ, sin θ]` by `byroredux/src/env_translate.rs`) never reaches this
  function, so the four textured decks always drift on the same hardcoded
  headings regardless of the weather's authored wind.
- **Evidence**:
  - `crates/renderer/shaders/composite.frag` proves the direction *is* plumbed and
    consumed elsewhere: `weather_procedural_cloud` computes
    `vec2 drift = wind * time * (0.0012 + wind_speed * 0.0065)` from
    `params.weather_wind.xz`, and the precipitation term uses the same vector for
    `rain_drift`. So in a single frame the procedural cloud body drifts along the
    authored wind while the four authored WTHR layers composited on top of it
    drift along `[+x, -x, +x, -x]` constants — visibly crossing on any weather
    whose wind is not roughly +X.
  - `crates/plugin/src/esm/records/weather.rs`, `b"ONAM"` arm:
    `record.cloud_layer_velocities[layer] = [sub.data[layer], 0]` — one authored
    byte, zero Y. A `0` byte is a legal authored value and reaches the sentinel
    branch.
  - `advance_cloud_scroll` is applied to all four layers in `weather_system`, so
    every layer is affected.
- **Impact**: Per-weather, exterior-only, visual. On Oblivion/FO3/FNV (ONAM,
  single scalar) any layer authored at rest drifts; on Skyrim (RNAM/QNAM) the
  same holds per component pair. Independently, every fallback layer on every
  game ignores the record's own wind heading, so cloud drift and both
  precipitation and the procedural cloud body disagree on wind direction.
- **Related**: #1033 (`WIND_TO_SCROLL_RATE` calibration), #529 (cloud tile
  scale), `feedback_chrome_means_missing_textures` / #1873 (the same
  authored-value-vs-struct-default sentinel class).
- **Suggested Fix**: Carry presence explicitly — e.g. make
  `WeatherRecord::cloud_layer_velocities` an `Option<[[u8; 2]; 4]>` set by the
  ONAM/RNAM/QNAM arms, so `cloud_scroll_vectors` branches on "the record
  authored motion data" rather than on the value. Separately, pass the authored
  `wind_direction` into the fallback and rotate the per-layer ratio table by it,
  keeping the magnitudes as the documented `WIND_TO_SCROLL_RATE` calibration.

---

### REN-2026-09-06-D2-01: `rayHitHasCoverage` omits the decal-slot alpha composite the raster path applies, so alpha-tested decal meshes cast a different silhouette than they present


- **Severity**: MEDIUM
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/include/ray_hit.glsl` —
  `rayHitHasCoverage` / `alphaComparePass`. Raster counterpart:
  `crates/renderer/shaders/triangle.frag`, the `materialDecals` overlay loop
  and the inline `mat.alphaTestFunc` block immediately after it. Data source:
  `crates/renderer/src/vulkan/material.rs` (`decal_map_0_index`, and
  `supplemental_texture_slot::DECAL_0`, imported as `slot` at the call site),
  populated from `byroredux/src/render/static_meshes.rs`
  (`supplemental_texture_indices[slot::DECAL_0] = texture_indices.decals[0]`).
  GLSL mirror: `mat.decalMap0Index` in `crates/renderer/shaders/include/bindings.glsl`.
- **Status**: **NEW.** No open issue matches (`decal`, `silhouette`,
  `coverage`, `alpha-test`, `rayHitHasCoverage`, `alphaComparePass` all return
  nothing across `/tmp/audit/renderer/open_titles.txt` and the JSON cache). No
  prior `docs/audits/` report names `rayHitHasCoverage` in this context.
  Sibling of the already-filed #3902, which covers the *shade* half of the same
  primary↔secondary divergence in `rayHitAlbedo`; this is the *coverage* half,
  in a different function.
- **Description**: `ray_hit.glsl`'s own `getHitVertexAlpha` docstring states the
  invariant: *"secondary rays must reconstruct the same barycentric value or
  alpha-tested leaves/grates cast a different silhouette from the visible
  surface."* The raster path builds its alpha-test input in this order — sample
  diffuse, apply the BC1 punch-through pin, multiply by
  `mat.materialAlpha * fragColor.a`, then run the four `mat.decalMap0..3Index`
  overlays as an alpha-over composite (`texColor.a = decalSample.a +
  texColor.a * (1.0 - decalSample.a)`), and only then evaluate the
  `alphaTestFunc` comparison. `rayHitHasCoverage` reproduces every step of that
  chain **except** the decal composite: it applies the BC1 pin, multiplies by
  `mat.materialAlpha * getHitVertexAlpha(...)`, and calls `alphaComparePass`
  directly.

  Because the composite is alpha-over, the raster alpha is always **≥** the
  un-composited alpha wherever a decal slot is bound. The RT silhouette is
  therefore a strict subset of the raster silhouette: shadow, reflection,
  refraction, GI and water rays punch through texels that the visible surface
  covers.

  Second, smaller divergence in the same pair: the seven-arm comparison table
  is hand-written twice with a **different EQUAL/NOTEQUAL epsilon** —
  `abs(a - aThresh) < 0.004` in `triangle.frag` versus
  `abs(alpha - threshold) < (1.0 / 255.0)` (0.0039216) in `alphaComparePass`.
  `alphaComparePass` already exists as the shared helper; the raster path does
  not call it. Two copies that have already drifted once will drift again.
- **Evidence**:
  - `triangle.frag`: `uint materialDecals[4] = uint[4](mat.decalMap0Index, …);
    for (int decalIndex = 0; decalIndex < 4; ++decalIndex) { … texColor.a =
    decalSample.a + texColor.a * (1.0 - decalSample.a); }` — unconditional, no
    material-kind gate — followed by `if (aThresh > 0.0) { … if (!pass) discard; }`.
  - `ray_hit.glsl::rayHitHasCoverage`: `alpha *= mat.materialAlpha *
    getHitVertexAlpha(instanceIdx, primitiveIdx, barycentrics); if
    (!alphaComparePass(alpha, mat.alphaThreshold, mat.alphaTestFunc)) return false;`
    — `mat.decalMap*Index` appears nowhere in the file.
  - The role is live, not dead: `decal_map_0_index` is written into
    `GpuMaterial` and hashed into the dedup key
    (`h.write_u32(mat.decal_map_0_index)` in `material.rs`), and fed from
    `texture_indices.decals[0]` in `static_meshes.rs`.
- **Impact**: Wrong RT silhouettes — holes in shadows, reflections and GI that
  the visible surface does not have — for any alpha-tested mesh whose
  `NiTexturingProperty` decal slots are populated with a partially-transparent
  overlay. That is Oblivion/FO3/FNV-era authoring, where the decal slots are
  the legacy multi-layer path. **The affected population is uncensused**: no
  archive was mounted this run, so the count of materials with
  (`alphaThreshold > 0` ∧ a bound decal slot ∧ `decalSample.a < 1`) is unknown.
  The mechanism is certain; the blast radius is not. Rated MEDIUM per the
  severity decision tree's "visual artifacts only" row rather than escalated,
  precisely because the population is unmeasured.
- **Related**: #3902 (the shade half of the same primary↔secondary divergence);
  #3911 (supplemental role↔slot correspondence is unpinned, which is how a
  decal slot could also be mis-routed); #1653 / #ae285062 (the BC1 pin that
  *is* mirrored correctly in both paths).
- **Suggested Fix**: Census first, then fix — count decal-slot materials with an
  active alpha test across the FO3/FNV/Oblivion archives before changing the
  hit path, since the composite costs up to four extra bindless fetches per RT
  hit. If the population is non-trivial, fold the four-slot alpha-over composite
  into `rayHitHasCoverage` behind an early-out on
  `mat.decalMap0Index == 0u && …`. Independently and cheaply: replace
  `triangle.frag`'s inline seven-arm block with a call to `alphaComparePass` so
  the table has one definition and the epsilon cannot diverge again.

---

### REN-2026-09-06-D22-01: the Starfield "no evidenced Flags field" premise that zeroes both light canonicalizers is contradicted by the DAT2 decoder's own verified layout comment


- **Severity**: MEDIUM
- **Dimension**: Light Animation
- **Location**: `byroredux/src/systems/light_anim.rs` (`canonical_light_animation_flags`, `canonical_light_shadow_flags`, `translate_light`) vs `crates/plugin/src/esm/cell/support.rs` (`build_static_object_from_subs`, `b"DAT2"` arm)
- **Status**: NEW
- **Description**: All three per-game boundary functions in `light_anim.rs` gate
  Starfield to a zero mask on one shared premise, stated verbatim in
  `canonical_light_animation_flags`'s doc: *"SF1Edit's live LIGH definition
  (`wbDefinitionsSF1.pas`) replaced the Skyrim/FO4/FO76 `DATA` subrecord with a
  restructured 76-byte `DAT2` whose only named fields are a handful of floats —
  the bytes a Flags field would occupy are an undifferentiated `wbUnknown`
  block."* `canonical_light_shadow_flags` and `translate_light` each restate it
  and cite the animation sibling as their authority.

  The `DAT2` decoder that actually produces the `flags` word says the opposite,
  citing the *same* reference file. `support.rs`'s arm carries an explicit,
  offset-by-offset layout table introduced as *"Byte layout verified against
  xEdit `wbDefinitionsSF1.pas` (`wbRecord(LIGH … wbStruct(DAT2, 'Data', [...]))`),
  NOT guessed"* — and its third row is `{12} UInt16 Flags (Skyrim DATA stores
  u32)`. The decoder then reads exactly that: `u16::from_le_bytes([sub.data[12],
  sub.data[13]]) as u32`.

  One of the two comments is wrong about what `wbDefinitionsSF1.pas` contains,
  and which one is right decides whether three `match` arms and five decoded
  fields are correct. (The two claims are only reconcilable if the *field* is
  named but its *bit meanings* are not — in which case `light_anim.rs`'s
  "undifferentiated `wbUnknown` block" phrasing is describing the wrong thing
  and should say so, because "no named field at all" is the argument the arms
  currently rest on.)
- **Evidence**:
  - `support.rs` (`b"DAT2" if is_ligh && sub.data.len() >= 11`) reads the flags
    word at offset 12 under a comment naming that offset `Flags`.
  - Downstream consequences of the zero masks, all on real decoded data:
    - `canonical_light_animation_flags` → `0` ⇒ `attach_light_flicker_if_needed`
      hits `if animation_flags == 0 { return; }`, so the three DAT2 flicker
      fields the same arm decodes — `period_secs` (+28), `intensity_amplitude`
      (+32), `movement_amplitude` (+36) — are structurally unreachable on
      Starfield.
    - `translate_light` → `is_spot` is `false` for every Starfield LIGH, so the
      function returns `LightKind::Point` before reading `fov_degrees`; the
      DAT2 FOV at +20 (decoded under `#2439 / NIFAL-D2-01 — same offset as the
      DATA arm above`) is likewise unreachable.
    - `canonical_light_shadow_flags` → `0` ⇒ `LightSource::from_legacy_world_units`
      computes `VisibilityMask::for_legacy_projection(false)` =
      `VisibilityMask::ARCHITECTURE` (`crates/core/src/lighting.rs`), so **every**
      placed Starfield light is invisible to `STATIC_PROP`, `DYNAMIC_ACTOR`,
      `FOLIAGE`, `GLASS` and `EFFECT` shadow rays.
  - That last consequence is the exact failure the shadow canonicalizer's own
    doc argues against: *"Shadow decode is permissive-by-default. Dropping a
    shadow bit that a game does name is the strictly worse error: the light
    silently stops casting RT shadows and the scene just looks flat, with
    nothing to trace it back to."* The Starfield arm applies the strict default
    to an entire game.
- **Impact**: Visual-only, but whole-game on Starfield: no flicker/pulse on any
  LIGH, no spot cones from ESM-placed lights, and props/actors/foliage cast no
  shadows from any placed light. Five decoded DAT2 fields are dead. The
  documentation conflict also means a future reviewer reading either comment
  gets an authoritative-sounding but contradicted answer.
- **Related**: #2251 (the arm's origin), `starfield_has_no_verified_flags_field_for_either_canonicalization`
  (the test that encodes the disputed premise), `crates/core/src/ecs/components/light.rs`
  (`LIGHT_FLAG_SHADOW_MASK`, `VisibilityMask::for_legacy_projection`).
- **Suggested Fix**: Settle the premise against `wbDefinitionsSF1.pas` once and
  make both comments agree. If the field is named but its bits are not, say
  exactly that in `light_anim.rs` and note the shadow-side consequence
  explicitly (all-`ARCHITECTURE` Starfield lights) so it is a recorded decision
  rather than a side effect. If the bit positions can be evidenced, give
  Starfield a real arm in both canonicalizers and let `translate_light` read the
  FOV it already decodes.

---

### REN-2026-09-06-D23-01: the BLAS-budget VRAM reservation excludes the default upscaler entirely — its signature takes only the render extent, so FSR's output-resolution images and SDK working set are structurally unrepresentable


- **Severity**: MEDIUM
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs`
  (`screen_scaled_reservation_bytes`, `blas_budget_for_heap`);
  `crates/renderer/src/vulkan/acceleration/memory.rs`
  (`AccelerationManager::recompute_blas_budget`);
  callers in `crates/renderer/src/vulkan/context/init.rs` and
  `crates/renderer/src/vulkan/context/resize.rs`
- **Status**: NEW (no open issue matches `budget`/`reservation`/`vram` in
  `/tmp/audit/renderer/open_titles.txt`; `AUDIT_TECH_DEBT_2026-09-05.md` reviewed
  this function only for constant-derivation hygiene and passed it — it did not
  question coverage)
- **Description**: `#3839` made the static-BLAS residency budget honest by
  subtracting what the resolution-scaled post-process passes hold:
  `blas_budget_for_heap(heap, reserved) = (heap − reserved) / 3`. The reservation
  covers the froxel grid, SVGF and the caustic accumulator. It does **not**
  cover either of the two allocations the **default** upscaler holds, and its
  signature cannot be made to:
  1. `FrameUpscaler::create_outputs` allocates `MAX_FRAMES_IN_FLIGHT`
     `HDR_FORMAT` (`R16G16B16A16_SFLOAT`, 8 B/px) images at
     `self.extents.**output**`. `screen_scaled_reservation_bytes` takes
     `render_extent: vk::Extent2D` and every caller passes
     `frame_extents.render`, so there is no output extent in scope to bill this
     against.
  2. The FSR SDK's own `VkDeviceMemory` is allocated by the vendored FFX Vulkan
     backend, outside `gpu-allocator`. `FrameUpscaler::build_summary`'s own doc
     says it "is invisible to `ctx.memory` unless reported here". It is invisible
     to the reservation too.

  This is materially different from the omissions the function's doc comment
  disclaims. That comment excuses "textures, geometry pools and the swapchain" —
  all demand-driven or allocator-visible. The FSR terms are neither: they are
  *fixed* for a swapchain generation, they scale with the **output** resolution
  the reservation never sees, and item 2 is accounted for nowhere in the engine.
- **Evidence**:
  ```rust
  // predicates.rs — only render-extent terms; no output-extent parameter exists
  pub(super) fn screen_scaled_reservation_bytes(
      render_extent: vk::Extent2D,
      volumetrics: VolumetricsConfig,
  ) -> vk::DeviceSize {
      let pixels = u64::from(render_extent.width) * u64::from(render_extent.height);
      ...
      froxel_bytes
          .saturating_add(pixels.saturating_mul(u64::from(SVGF_BYTES_PER_PIXEL)))
          .saturating_add(pixels.saturating_mul(u64::from(CAUSTIC_BYTES_PER_PIXEL)))
  }
  ```
  ```rust
  // memory.rs — the only production consumer
  let reserved = screen_scaled_reservation_bytes(render_extent, volumetrics);
  let updated = blas_budget_for_heap(self.blas_heap_bytes, reserved);
  ```
  Both call sites pass the render extent: `accel.recompute_blas_budget(render_extent, …)`
  (`init.rs`) and `accel.recompute_blas_budget(self.frame_extents.render, …)`
  (`resize.rs`).

  Magnitude, derived from named constants only (`FROXEL_BYTES_PER_SLOT = 44`,
  `SVGF_BYTES_PER_PIXEL = 40`, `CAUSTIC_BYTES_PER_PIXEL = 24`,
  `froxel_xy_divisor = 8`, `froxel_z_slices = 64`, `MAX_FRAMES_IN_FLIGHT = 2`),
  for the shipped default (1080p output, FSR Quality → 1280×720 render):

  | Term | Reserved? | Bytes |
  |---|---|---:|
  | froxel grid (160×90×64 × 44 × 2) | yes | 81.1 MB |
  | SVGF (921 600 px × 40) | yes | 36.9 MB |
  | caustic (921 600 px × 24) | yes | 22.1 MB |
  | **reservation total** | | **140.1 MB** |
  | FSR upscale outputs (1920×1080 × 8 × 2 FIF) | **no** | 33.2 MB |
  | FSR SDK working memory | **no** | reported at runtime; the `ctx.upscaler` sample in `fsr3-troubleshooting.md` reads 31.8 MB at a *1280×720* output |

  `docs/engine/memory-budget.md`'s own fixed-floor table already carries the row
  "FSR 3.1 upscaler output (2 FIF, output resolution) | ~33 MB (1080p) | ~133 MB
  (4K) — SDK working memory not separately tracked", so the doc knows about a
  cost the code's reservation cannot see.
- **Impact**: The BLAS budget over-states available VRAM by roughly a third of
  the omitted bytes (`/3`). The error's *sign is adversarial to the default
  path*: switching from `--upscaler taa` to `fsr3/quality` or `fsr3/performance`
  shrinks every reserved term quadratically with the render extent while the two
  FSR terms do not shrink at all (the outputs are output-resolution; the SDK
  context is created for `max_upscale_size`). So the reservation's *relative*
  under-count is largest exactly on the presets the engine ships by default —
  ~24 % under-counted at 1080p/Quality from the output images alone, before the
  SDK working set. The failure mode is the one #3839 was written to remove:
  eviction that starts too late on small-VRAM cards, ending in an allocator
  failure rather than a reclaim. Not corruption, not a crash on the 12 GB dev
  card — hence MEDIUM rather than HIGH.
- **Related**: #3839 (the reservation), #2158 / #2829 (FSR teardown, which
  already proves the SDK context owns memory nothing else tracks),
  `docs/engine/memory-budget.md` §"FSR 3.1 Upscaler"
- **Suggested Fix**: Widen the parameter to the whole `FrameExtentSet` (or add an
  `output_extent`) and add two terms: `MAX_FRAMES_IN_FLIGHT × output.width ×
  output.height × 8` for the upscale outputs, and the SDK figure — which is
  already in hand and costs nothing to plumb, since
  `FrameUpscaler::build_summary` calls `fsr3::Context::memory_usage()` once per
  swapchain generation and caches it. Pin the new terms the way
  `froxel_grid_cost_matches_the_memory_budget_doc` pins the froxel term. If the
  SDK term is judged too awkward to thread, adding the output-image term alone is
  strictly better than today and needs no new data.

---

### REN-2026-09-06-D3-01: `shader-pipeline.md` marks two live lanes "reserved", one of them the exact lane the code names as the next expansion hazard


- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout
- **Location**: `docs/engine/shader-pipeline.md` (§GPU Data Types — the `GpuCamera` table's `render_debug` row at offset 336, and the `material_flags` bit table's bit-10 row)
- **Status**: NEW
- **Description**: Two rows of the authoritative GPU-layout doc describe live lanes as free.
  1. `GpuCamera.render_debug` is documented as `"… z = diagnostic LOD-counter enable; w reserved"`. `w` is not reserved: `pack_weather_surface` (`crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs`) writes rain wetness into its low 16 bits and snow coverage into its high 16, and `triangle.frag` decodes both. All five GLSL `CameraUBO` mirrors carry the correct comment; only the doc is wrong.
  2. The `material_flags` table lists bit 10 as `*(unused/reserved)*`. Bit 10 is `material_flag::BGSM_AUTHORED` (`crates/renderer/src/vulkan/material.rs`) — live host-side provenance, deliberately *not* mirrored to GLSL (`crates/renderer/build.rs` and `crates/renderer/src/shader_constants_data.rs` both carry an explicit "intentionally NOT emitted" note). `docs/engine/renderer.md` documents it correctly; `shader-pipeline.md` does not.
- **Evidence**:
  - `assemble_camera_and_lights.rs` builds `render_debug: [mode, lod_scale_bits, lod_telemetry, pack_weather_surface(sky_params.weather.surface_wetness, sky_params.weather.surface_snow)]`.
  - `triangle.frag` decodes it as `float(renderDebug.w & 0xFFFFu)` / `float(renderDebug.w >> 16u)`.
  - `pack_weather_surface` is unit-tested by `packs_wetness_low_and_snow_high`.
  - `DBG_BITS`' own doc comment in `shader_constants_data.rs` says future debug expansion "must coordinate with the history-dependent weather-surface payload already carried in `GpuCamera.render_debug.w`; that lane is no longer an unused flag word."
  - `material_flag::BGSM_AUTHORED: u32 = 1 << 10;`
- **Impact**: The `DBG_*` mask is now fully exhausted (32/32 bits, see the Coverage table), so the *next* debug-flag expansion is precisely the change the code warns must not silently claim `render_debug.w`. An author following the audit skill's own instruction — treat `shader-pipeline.md` as authoritative, do not re-derive — reads "w reserved" and takes a lane that carries per-frame weather state consumed by `triangle.frag`, silently breaking wetness/snow response with no test failure. Bit 10 has the mirror-image risk: a new shader-visible `MAT_FLAG_*` allocated at the "unused" bit 10 would collide with the host-side provenance bit that `cell_loader.rs` already sets.
- **Related**: This is the third instance of the same class in this struct family — #1928 (`VolumetricsParams.render_origin.w` documented free while `volumetrics_inject.comp` read it) and #2750 / REN-D3-2026-08-12-02 (`GpuCamera.dof_params` documented `zw = reserved (0)` over two live consumers). The `render_origin` row *immediately above* the wrong one carries an explicit "Not a free slot — same trap as `VolumetricsParams.render_origin.w` (#1928)" warning, and the next row repeats the mistake the warning names. Distinct from the **closed** #3447 / `REN-2026-08-27-D3-01` ("shader-pipeline.md still documents GpuInstance at 128 B and GpuCamera at 352 B"), whose fix `03407ae3` corrected the *size* literals in this same document — both sizes verified correct today (160 B / 368 B). This finding is about field semantics in two rows that fix did not touch. Also distinct from #3846, which is the same class in `bindings.glsl` rather than the doc.
- **Suggested Fix**: Change the offset-336 row to `w = packed weather surface (low 16 bits rain wetness, high 16 bits snow coverage), consumed by triangle.frag — not a free slot`, and change the bit-10 row to `MAT_FLAG` `BGSM_AUTHORED` — host-side only, deliberately not mirrored to GLSL. Both should carry the same "not a free slot" phrasing the `render_origin` row already uses.

---

### REN-2026-09-06-D3-02: `upload_instances`' `unsafe` SAFETY argument states the wrong field types, omitting exactly the three fields that could introduce implicit padding


- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`SceneBuffers::upload_instances`), `crates/renderer/src/vulkan/scene_buffer/descriptors.rs` (`hash_instance_slice`)
- **Status**: NEW
- **Description**: `upload_instances` reinterprets the `&[GpuInstance]` slice as raw bytes via `std::ptr::copy_nonoverlapping`, justified by the comment *"SAFETY: GpuInstance is `#[repr(C)]` with plain f32/u32 fields."* That has been false since #2219: `GpuInstance` carries three `u64` fields (`skinned_vertex_address`, `morph_delta_address`, `morph_weight_address`), which raise the struct's alignment to 8 and are the *only* reason implicit padding could ever appear in it. The safety argument therefore asserts the absence of the one hazard it needs to rule out by describing a struct shape that no longer exists. `hash_instance_slice`'s companion doc has the same shape (`"f32 / u32 / packed-vec4 fields"`).

  Separately, this is the layer the `NoUninit` trait (#3761 / SAFE-2026-08-30-D4-01) was added to enforce rather than argue in prose — yet of the ten upload paths in `scene_buffer/upload.rs`, only the two UBO writers (`upload_camera`, `upload_dalc`) route through the `NoUninit`-bounded `write_mapped`. All eight SSBO paths (`upload_lights`, `upload_bone_worlds`, `upload_pending_bind_inverses`, `upload_instances`, `upload_previous_models`, `upload_materials`, `upload_indirect_draws`, `upload_terrain_tiles`) still hand-roll `copy_nonoverlapping` with prose. `GpuInstance` and `GpuMaterial` — the two most churn-prone structs in the workspace, five and one GLSL mirrors respectively — are among the eight.
- **Evidence**:
  - `gpu_types.rs`: `pub skinned_vertex_address: u64`, `pub morph_delta_address: u64`, `pub morph_weight_address: u64`.
  - `unsafe impl NoUninit for …` exists for `GpuCamera`, `GpuDalcCube`, `GpuSelectedRayProbe`, `GpuWaterParams`, `GpuFogVolume`, `Vertex`, `UiVertex` and others — but **not** for `GpuInstance` or `GpuMaterial`.
  - Verified no implicit padding exists today: `surface_id`@108 → `skinned_vertex_address`@112 is 8-aligned, and 160 % 8 == 0. **This finding is about the guard, not a live UB.**
- **Impact**: A future field insertion that lands a `u32` immediately before one of the `u64`s at an odd 4-byte offset introduces 4 bytes of implicit padding. `gpu_instance_is_160_bytes_std430_compatible` *would* catch the size change, but nothing would flag that the byte view now contains uninitialised bytes — which is UB in both the upload copy and `hash_instance_slice`'s dirty-gate hash, and reaches the GPU as garbage in whichever std430 lane the padding lands. The stale comment is what removes the reader's chance to notice; #3761 exists precisely because "`Copy` alone does not rule this out".
- **Related**: #3761 / SAFE-2026-08-30-D4-01 (introduced `NoUninit`); #2219, #3231 (added the `u64` fields the comment predates).
- **Suggested Fix**: Add `unsafe impl NoUninit for GpuInstance {}` and `for GpuMaterial {}` with a SAFETY note naming the `u64` fields and the 8-byte alignment argument, then convert `upload_instances` / `upload_materials` to `write_mapped`. At minimum, correct both comments to say "`#[repr(C)]` scalars plus three `u64`s, positioned so no implicit padding appears".

---

### REN-2026-09-06-D4-01: the skin/BLAS chain commits record-time state that `draw_frame`'s three tail `Err` sites never roll back — `skin_dispatch_ran` is a record-time latch used as a submit-time signal


- **Severity**: MEDIUM
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`
  (`VulkanContext::record_skinned_blas_refit` — the `self.skin_dispatch_ran = true`
  statement and its "#1796 / D6-02" justification comment),
  `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`
  (`AccelerationManager::build_skinned_blas_batched_on_cmd`, Phase 4),
  `crates/renderer/src/vulkan/scene_buffer/upload.rs`
  (`SceneBuffers::record_bone_world_copy`'s trailing
  `mark_bone_world_slot_written` loop),
  `byroredux/src/app_frame.rs` (the
  `if !ctx.skin_dispatch_ran || ctx.bind_inverse_upload_failed` rollback gate)
- **Status**: NEW
- **Description**: `#917` established this codebase's rule for the frame tail:
  temporal state advances **after** `queue_submit` returns success, never at
  recording time. `draw_frame` applies it to SVGF, TAA, volumetrics, FSR and the
  rigid-model history swap — all of which sit *below* the `queue_submit` call.
  The skin / skinned-BLAS chain never got the same treatment, and its guard flag
  is the wrong shape to give it one.

  `record_skinned_blas_refit` sets `self.skin_dispatch_ran = true`
  unconditionally at its top, with the rationale *"reaching this function at all
  proves `draw_frame` got past both early-return guards"*. That was true of the
  hazards it was written for (#1796) and of the ones #2522 widened it to (fence
  wait, command-buffer begin, FSR parameter build — all *above* this point). It
  is not true of `draw_frame`'s three **tail** `Err` sites, which sit *below* it:
  `end_command_buffer`, `reset_fences`, and `queue_submit`. `draw.rs`'s own
  `#3837` comment describes exactly those three as *"the same recovery paths
  #910 hardened, so reachable in practice on swapchain churn"*. On any of them
  the command buffer is discarded and nothing recorded this frame executes — yet
  `skin_dispatch_ran == true`, so `app_frame.rs`'s rollback does not fire.

  Four pieces of state have already been committed by then, all describing GPU
  work that will never run:

  1. **`SkinSlotPool` pose-hash commits** stay committed, so next frame's
     `pose_dirty` is empty for those entities and no re-upload is scheduled.
  2. **`bone_world_slot_states[slot]`** — `record_bone_world_copy` calls
     `mark_bone_world_slot_written(state, frame_index)` in a loop *after*
     recording the copy. Once the sibling FIF slot's bit lands on a later
     successful frame, `mark_bone_world_slot_written` resets the byte to `0`
     (fully clean) and `bone_world_device_buffers[frame_index]` is left holding
     the pre-update pose until something re-dirties the slot.
  3. **`SkinSlot::has_populated_output = true`**, set immediately after the
     `skin_vertices.comp` dispatch is *recorded*, which is what the next frame's
     `#1196` skip gate reads.
  4. **`AccelerationManager::skinned_blas`** — `build_skinned_blas_batched_on_cmd`
     Phase 4 inserts the `BlasEntry` (with `built_flags: SKINNED_BLAS_FLAGS`)
     after recording the BUILD. `has_skinned_blas(entity)` therefore returns
     `true` for an acceleration structure whose backing memory was never written.
     `refit_skinned_blas` then takes the `mode(UPDATE)` /
     `src_acceleration_structure(entry.accel) == dst_acceleration_structure(entry.accel)`
     path against it on every subsequent frame, and `build_tlas` publishes its
     device address into the TLAS for ray queries to traverse. Item 4 is the one
     with spec weight — see *Needs-RenderDoc N-3*.
- **Evidence**:
  - `record_skinned_blas_refit` — `self.skin_dispatch_ran = true;` precedes the
    `if let (Some(skin_pipeline), Some(ref mut accel))` gate; the comment above
    it enumerates only the two early-return guards.
  - `draw_frame` — after `self.dispatch_skin_and_cluster(...)` (which calls
    `record_skinned_blas_refit`) the function still contains three
    `return Err(e)` sites: the `end_command_buffer` arm inside the tail `unsafe`
    block, the `reset_fences` arm, and the `queue_submit` arm. Each calls
    `recreate_image_available_for_frame` (and the last also
    `recreate_in_flight_for_frame`) and returns — the sync objects are healed,
    the skin state is not.
  - `byroredux/src/app_frame.rs` — the rollback is
    `if !ctx.skin_dispatch_ran || ctx.bind_inverse_upload_failed { … }`;
    its `#2522` comment enumerates only Err sites that execute *before*
    `record_skinned_blas_refit`.
  - `app_frame.rs`'s `Err(e) =>` arm logs `"Draw failed"` and calls
    `event_loop.exit()` — **queued**, not immediate. `draw.rs`'s own `#1211`
    guard comment establishes that a `RedrawRequested` already in flight still
    reaches `draw_frame` after such an exit is queued, which is what makes the
    post-failure frames reachable at all.
  - The contrasting correct pattern is 30 lines below in `draw_frame`:
    `svgf.mark_frame_completed()` / `taa.mark_frame_completed()` /
    `volumetrics.mark_frame_completed()` / `mark_dispatch_completed()` /
    the `previous_rigid_models` swap, all gated on `queue_submit` having
    returned `Ok` (#917).
- **Impact**: Bounded to the frames between a tail `Err` and process teardown,
  but within that window: a skinned actor renders from a stale bone palette in
  one of the two FIF buffers (alternating-frame pose pop), its GPU-skinned
  vertex buffer is treated as populated when it is not, and its BLAS is
  UPDATE-refit and ray-traced from memory that was never built. Severity
  arbitrated to MEDIUM rather than the `/audit-severity` HIGH floor for a Vulkan
  spec violation because the whole window sits inside an already-fatal error
  path with exit queued; if N-3's validation-layer check confirms the
  UPDATE-against-never-built path fires, HIGH is the right reading.
- **Related**: #917 (the pattern this chain never adopted), #1796 / #2522 /
  #3569 (three successive widenings of the same rollback gate, none of which
  reached the tail sites), #910 / #952 (which hardened the sync objects on
  exactly these three sites), #1211 (queued-exit reachability), #3837.
- **Needs RenderDoc**: only for the item-4 half — see N-3. Items 1–3 are plain
  CPU state machines, fully decidable from source.
- **Suggested Fix**: **No barrier or pipeline change.** Split the record-time
  latch from the submit-time signal: keep `skin_dispatch_ran` as the
  "recording reached the skin section" flag it is, and either (a) widen
  `app_frame.rs`'s rollback to fire on `draw_result.is_err()` as well, or
  (b) mirror #917 — move the four commits (`mark_bone_world_slot_written`,
  `has_populated_output`, the `skinned_blas` insert, and the pool's pose-hash
  commit) behind a post-`queue_submit` promotion, as
  `VolumetricsPipeline`'s own `pending_simulation_time_seconds` →
  `last_simulation_time_seconds` promotion already does. Either is
  `cargo test`-pinnable in the existing style of
  `skin_dispatch_ran_rollback_scope_tests`.

---

### REN-2026-09-06-D5-01: `screen_scaled_reservation_bytes` counts 3 of the 11 screen-scaled passes memory-budget.md ledgers — ~44 % of the bytes — so #3839's own worked example still holds after the fix


- **Severity**: MEDIUM (inefficient / under-modelled GPU memory budgeting;
  no leak, no corruption)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs` —
  `screen_scaled_reservation_bytes`, consumed by
  `AccelerationManager::recompute_blas_budget`
  (`acceleration/memory.rs`) via `blas_budget_for_heap`. Ground truth:
  `docs/engine/memory-budget.md` §"RT-Denoiser & Post-Process Screen-Sized
  Resources" and §"ReSTIR Reservoirs". Guard that cannot see it:
  `blas_budget_subtracts_the_resolution_scaled_reservation`
  (`acceleration/tests/predicates_tests.rs`).
- **Status**: **NEW.** Landed yesterday in `fa5c4191`; not examined by
  yesterday's scoped run (its Dim 5 was volumetrics-only). No matching open
  issue — searched `reservation`, `3839`, `blas budget`, `screen.scaled`,
  `memory`, `budget` across all 151 open titles. #3866 and #3842 are the
  *rename* / *orphaned-doc-comment* half of the same commit, not this.
- **Description**: The function's own docstring says it "Covers the three
  passes that scale with render resolution and **dominate the fixed floor**
  documented in `docs/engine/memory-budget.md`", and explicitly lists what it
  deliberately excludes: "textures, geometry pools and the swapchain". Every
  omission below is a resolution-scaled *pass*, i.e. inside the stated scope,
  and every one of them has its own row or subsection on the page the
  docstring cites:

  - **ReSTIR reservoirs**, 64 B/px — the single largest omission, and the
    page's own words for it are "the largest single VRAM addition of the
    denoiser overhaul … at 4K it is over 13 % of the ~4 GB engine budget
    target". `RESERVOIR_STRIDE` (`vulkan/restir.rs`) is a `pub` constant the
    function could read directly.
  - **G-buffer**, 44 B/px — `gbuffer.rs`'s seven attachments.
  - **TAA history**, 16 B/px (`taa.rs`).
  - **Water-side caustic accumulator**, 8 B/px. This one is the sharpest:
    the function already imports `CAUSTIC_BYTES_PER_PIXEL` from
    `vulkan/caustic.rs`, which is the **glass half only** (24 B/px,
    `4 * CAUSTIC_COLOR_LAYERS * MAX_FRAMES_IN_FLIGHT`). The doc row it comes
    from is headed "Glass + Water Caustics" and publishes 32 B/px combined.
    The water half has no production constant at all — `WATER_BYTES_PER_PIXEL`
    exists only as a `const` inside `caustic.rs`'s own test module, so the
    reservation cannot reach it even in principle.
  - **Bloom pyramid** (~5.3 B/px effective) and **SSAO** (2 B/px).
  - **FSR upscaler output**, 16 B/px at *output* extent — legitimately
    awkward, since this function is handed the render extent only.

- **Evidence**: The function body is three terms:
  `froxel_bytes + pixels*SVGF_BYTES_PER_PIXEL + pixels*CAUSTIC_BYTES_PER_PIXEL`.
  Re-derived against the doc's own per-row figures (table above): counted
  315.19 MB, omitted ~405 MB at 1080p — the same 43.8 % ratio at 4K, since
  everything including the froxel grid scales with pixel count.

  The arithmetic that *is* there is exactly right: `SVGF_BYTES_PER_PIXEL` = 40
  and `CAUSTIC_BYTES_PER_PIXEL` = 24 are already FIF-inclusive by
  construction, and `FROXEL_BYTES_PER_SLOT` = 44 is per-slot and correctly
  multiplied by `MAX_FRAMES_IN_FLIGHT`. There is no double-count. This is a
  coverage finding, not a math finding.

  The existing pin cannot catch it: it asserts only
  `reserved_hd > 128 MiB`, `reserved_uhd > 2 × reserved_hd`, and monotonicity
  of the resulting budget. All three pass with three terms, and all three
  would still pass with two.
- **Impact**: `blas_budget_for_heap` is `(heap − reserved) / 3`, so a missing
  reservation byte costs one third of a budget byte. The commit's own
  justification —
  *"on a 6 GB card at 1080p the old math handed BLAS 2 GB while ~1.1 GB was
  already committed elsewhere, so nothing evicted until the allocator
  failed"* — is the scenario the fix was written for, and after the fix that
  card gets **1 947.8 MiB** instead of 2 048.0 MiB. The described failure mode
  is essentially unchanged. A complete reservation roughly doubles the
  correction (to −228.9 MiB), which is still modest but is the difference
  between "eviction engages before the allocator does" and "it does not".
  Blast radius is confined to LRU eviction aggressiveness on small-VRAM cards
  and at high resolutions — the 12 GB dev card never reaches the gate.
- **Related**: #3839 (the fix this audits), #3866 / #3842 (sibling doc-rot
  from the same commit, already open), `REN-2026-09-06-D5-02` (the two passes
  the doc itself never ledgered, which is *why* they are also missing here).
- **Suggested Fix**: Add the missing render-extent terms, each from the
  owning pass's own published constant so the number moves when the pass
  does — the discipline the docstring already states. `RESERVOIR_STRIDE`
  (`restir.rs`) and `SVGF_BYTES_PER_PIXEL` are the model. Two of them need a
  constant published first: promote `WATER_BYTES_PER_PIXEL` out of
  `caustic.rs`'s test module, and add a `GBUFFER_BYTES_PER_PIXEL` beside
  `gbuffer.rs`'s attachment table (the doc already publishes 22 B/px × 2 FIF).
  Then strengthen the pin from a magnitude floor to an enumeration — assert
  the reservation equals the sum of the named per-pass constants, so a pass
  added without a term fails the test rather than shrinking the coverage
  ratio silently. If FSR's output-extent term is judged out of scope, say so
  in the docstring rather than leaving it in the "covers the passes that
  dominate" claim.

---

### REN-2026-09-06-D5-02: the composite HDR pair and the depth / depth-history attachments — 40 B/px of unconditional render-extent VRAM — have no row anywhere in memory-budget.md


- **Severity**: MEDIUM (authoritative-ledger gap on the page cited for the
  `< 4 GB` target; same class and severity as `REN-2026-08-30-D5-02`)
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md` — §"RT-Denoiser & Post-Process
  Screen-Sized Resources" (no subsection) and §"VRAM Rough Budget" (no row).
  Ground truth: `CompositePipeline` (`crates/renderer/src/vulkan/composite.rs`
  — the `hdr_images` and `scene_images` families and `HDR_FORMAT`), and
  `VulkanContext::depth_image` / `depth_history_image` allocated by
  `create_depth_resources` (`context/helpers.rs`) from `context/init.rs` and
  `context/resize.rs`.
- **Status**: **NEW.** `grep -in "hdr\|composite\|depth" docs/engine/memory-budget.md`
  returns no ledger row for any of them — the only hit that mentions them is
  the G-buffer roll-up row's own exclusion clause. Not in the 151 open
  issues; not in the 2026-08-30 or 2026-09-05 reports.
- **Description**: The VRAM roll-up's G-buffer row is scoped as
  "…22 B/px, × 2 FIF; **not** counting the separate HDR colour, depth, or
  depth-history attachments". Nothing else on the page counts them either, so
  the exclusion points at a row that does not exist.

  `CompositePipeline` owns **two** independent screen-sized image families,
  each `MAX_FRAMES_IN_FLIGHT` deep and each created at `extents.render`:
  `hdr_images` (the main HDR attachment) and `scene_images` (added by the FSR
  tail — "the single image that either FSR or the native bridge consumes";
  the field's own comment explains why the two must stay distinct). Both are
  `HDR_FORMAT = R16G16B16A16_SFLOAT`, 8 B/px, both `GpuOnly`, both
  unconditional. That is 4 images × 8 B/px = **32 B/px**.

  `depth_image` and `depth_history_image` add 4 B/px each at render extent
  (`find_depth_format` selects `D32_SFLOAT`; `depth_capture_record_copy`
  refuses anything else since #3570) — **8 B/px** more, single-buffered.

  Total **40 B/px** of unconditional render-extent VRAM with no row: 83.0 MB
  at 1080p, 331.8 MB at 4K native. For scale, that is more than the SVGF
  row (~83 MB) which has a whole subsection, and more than the TAA
  (~33 MB), Bloom (~11 MB) and SSAO (~4 MB) rows combined.
- **Evidence**:
  - `composite.rs`: `pub const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;`
    with the doc comment "RGBA16F = 8 bytes/pixel"; `pub hdr_images: Vec<vk::Image>`
    and `pub scene_images: Vec<vk::Image>`, both filled by
    `for i in 0..MAX_FRAMES_IN_FLIGHT` loops in `new_inner`, both
    `.extent(… width: extents.render.width, height: extents.render.height …)`,
    both `location: gpu_allocator::MemoryLocation::GpuOnly`.
  - `context/resize.rs` calls `create_depth_resources(…, self.frame_extents.render,
    self.depth_format, …)` twice — `"depth_buffer"` and `"depth_history"`.
  - `docs/engine/memory-budget.md`'s G-buffer row carries the exclusion text
    quoted above; no `### HDR`, `### Composite`, or `### Depth` heading exists
    (`grep -n "^### " docs/engine/memory-budget.md`).
- **Impact**: Doc-trust, but on the page every other subsystem's budgeting is
  derived from, and with a concrete downstream consumer: `REN-2026-09-06-D5-01`
  cannot reserve what the ledger does not name, so this omission propagates
  into the live BLAS-eviction threshold. The `Estimated total ~1.81 GB`
  roll-up understates by ~83 MB at 1080p and the 4K peak by ~332 MB.
  `scene_images` in particular is *newer* than the section around it — it
  arrived with the FSR tail, exactly the kind of growth the page's own
  §"Not yet ledgered" preamble ("a grep of this page for the owning subsystem
  name is the cheapest way to find a gap in it") is meant to surface.
- **Related**: `REN-2026-08-30-D5-02` (same class — a real allocation with no
  row; fixed for the staging pools), `REN-2026-09-06-D5-01`,
  `REN-2026-09-06-D5-06`.
- **Suggested Fix**: Add a `### Composite HDR intermediates + depth` subsection
  under "RT-Denoiser & Post-Process Screen-Sized Resources" with the 32 + 8
  B/px split, and a matching roll-up row; then delete the G-buffer row's
  exclusion clause or repoint it at the new section. Follow the
  `SVGF_BYTES_PER_PIXEL` / `CAUSTIC_BYTES_PER_PIXEL` / `FROXEL_BYTES_PER_SLOT`
  precedent — derive a `COMPOSITE_BYTES_PER_PIXEL` from `HDR_FORMAT` and the
  two families' arity and pin the doc against it, so the next image family
  added to `CompositePipeline` fails a test instead of drifting. Also worth
  a row while nearby: `ClusterCullPipeline`'s light-index buffers are
  `TOTAL_CLUSTERS (16×9×24) × MAX_LIGHTS_PER_CLUSTER (512) × 4 B ≈ 7.1 MB`
  per FIF — fixed-size, not resolution-scaled, and likewise unledgered.

---

### REN-2026-09-06-D5-03: #3840's `resident_static_blas_bytes()` cannot change any behaviour — its only consumer is a trigger whose callee re-tests with the paper figure and returns


- **Severity**: MEDIUM (defence-in-depth gap: the machinery is carried, the
  hazard it names is not addressed)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs` —
  `AccelerationManager::resident_static_blas_bytes`,
  `pending_destroy_static_bytes`, and the mid-batch trigger inside
  `build_blas_batched`; `AccelerationManager::evict_unused_blas` (same file);
  predicates `should_evict_mid_batch` and `blas_over_budget`
  (`acceleration/predicates.rs`); `BlasEntry.counted_in_static_bytes`
  (`acceleration/types.rs`). Guard: the
  `pending_destroy_static_bytes_stays_balanced_tests` module
  (`acceleration/tests/blas_static_tests.rs`).
- **Status**: **NEW.** Landed yesterday in `fa5c4191`. No matching open issue
  (searched `3840`, `resident`, `pending_destroy`, `eviction`, `budget`).
- **Description**: #3840's stated hazard, in
  `resident_static_blas_bytes`'s own doc comment, is that
  `static_blas_bytes` is "the *paper* figure — eviction credits it the moment
  an entry is queued … letting a batch **allocate against headroom that does
  not exist yet**", and that the resident figure "is the figure admission
  checks must use".

  There is exactly one production call site
  (`grep -rn "resident_static_blas_bytes"` → the definition, one call, one
  doc reference, one source-scan assertion). It is the 90 % **trigger**:

  `should_evict_mid_batch(self.resident_static_blas_bytes(), pending_bytes, budget)`
  → `(static + pending_destroy + pending) * 10 >= budget * 9`

  and the trigger's only action is `self.evict_unused_blas(device, allocator,
  pending_bytes)`, whose first statement is the 100 % **reclaim gate** on the
  *paper* figure:

  `if !blas_over_budget(self.static_blas_bytes, pending_bytes, budget) { return; }`
  → `(static + pending) > budget`

  Because `resident >= paper` always, the resident trigger is a strict
  superset of the paper trigger. In the band it newly covers
  (`static + pending <= budget < 0.9⁻¹ · (static + pending_destroy + pending)`)
  the callee's gate rejects and returns immediately. Outside that band both
  spellings agree. So swapping in the resident figure adds trigger firings
  that do nothing and changes no eviction decision — the batch continues
  allocating against the same headroom #3840 says does not exist.

  This is the shape the sibling comment fifteen lines below already warns
  about, for `pending_bytes`, in the #1792 write-up: *"the trigger above
  fired, but the callee it called was structurally blind to the very bytes
  that triggered it."*

  Note the paper-figure choice inside `evict_unused_blas` is **correct and
  deliberate** — its own #3840 comment explains that a resident-based loop
  condition would never improve, because each eviction moves the same bytes
  from `static_blas_bytes` into `pending_destroy_static_bytes`, so the loop
  would evict every candidate. The gap is that nothing else consumes the
  resident figure: there is no admission check anywhere that can stop or
  throttle a batch, and `tick_deferred_destroy` (the only thing that actually
  reduces residency) cannot run mid-batch — its sole caller is inside
  `draw_frame`.
- **Evidence**:
  - `predicates.rs`: `should_evict_mid_batch(total_live, pending, budget)` =
    `projected.saturating_mul(10) >= budget_bytes.saturating_mul(9)`;
    `blas_over_budget(static, pending, budget)` =
    `static_blas_bytes.saturating_add(pending_bytes) > budget_bytes`.
  - `blas_static.rs`: the only `resident_static_blas_bytes()` call is the
    first argument of the `should_evict_mid_batch` in `build_blas_batched`.
  - `evict_unused_blas`'s early-return and its per-candidate `break` both use
    `self.static_blas_bytes`.
  - The pin is a **source-scan**:
    `BLAS_STATIC_RS.contains("self.resident_static_blas_bytes(),")` — it
    asserts the call is textually present, so it passes whether or not the
    call can affect anything.
- **Impact**: No correctness harm and no leak — the bookkeeping itself is
  balanced and correct (verified: `drop_blas` and `evict_unused_blas` both
  credit `pending_destroy_static_bytes`; `tick_deferred_destroy` debits
  exactly the entries whose `counted_in_static_bytes` is set;
  `drain_pending_destroys` zeroes it; `blas_skinned.rs` pushes
  `counted_in_static_bytes: false` so skinned entries never touch the static
  counter). The cost is that a closed issue's hazard is still live: on a
  cell transition, the outgoing cell's BLAS bytes sit in
  `pending_destroy_static_bytes` until the next `draw_frame`, while
  `build_blas_batched` for the incoming cell allocates as if they were
  already freed. On the 12 GB dev card this is unreachable; on a 6 GB card at
  a large streaming boundary it is the allocator-failure path #3840 named.
  It also means three new struct fields, an accessor, and a guard test are
  carried for no behavioural effect, which is the kind of thing that reads as
  "already handled" to the next reader.
- **Related**: #3840 (the issue this closed), #1792 (the identical
  trigger-fires-callee-blind shape, for `pending_bytes`), #1449 (deferred
  eviction, which created the paper-vs-resident divergence),
  `REN-2026-09-05-D1-02` (a sibling "computed and thrown away" observation in
  the same subsystem, still unfiled).
- **Suggested Fix**: Decide which of the two this is meant to be and make the
  code say so. Either (a) give the resident figure a consumer that can act —
  the natural one is an admission check in `build_blas_batched`'s Phase 1
  loop that stops adding meshes to *this* batch when
  `resident + pending + next_size > budget`, deferring the remainder to the
  next batch after a tick has run; or (b) if throttling is not wanted, revert
  the trigger to `self.static_blas_bytes` and keep
  `resident_static_blas_bytes` purely as telemetry, documented as such. Either
  way, replace the source-scan pin with a behavioural one: a pure-function
  test over `should_evict_mid_batch` + `blas_over_budget` showing that the
  band where they disagree produces a different outcome. Today no test can
  fail if the resident call is deleted outright.

---

### LOW

### REN-2026-09-06-D8-01: SVGF's camera-static progressive-accumulation flag zeroes the α floor, silently cancelling the `svgf_recovery_frames` window that `signal_temporal_discontinuity` exists to install


- **Severity**: MEDIUM
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp` (the `floorC` / `floorM` assignment inside the `hasHistory` branch); `crates/renderer/src/vulkan/svgf.rs` (`next_svgf_temporal_alpha`, `SvgfPipeline::upload_params`); `crates/renderer/src/vulkan/context/mod.rs` (`signal_temporal_discontinuity`)
- **Status**: NEW (no open issue matches `svgf` / `recovery` / `parked` / `static` in `open_titles.txt`; not present in the 2026-08-30 sweep's Dim 8/13 sections)
- **Description**: `signal_temporal_discontinuity`'s only effect on SVGF is
  `self.svgf_recovery_frames = self.svgf_recovery_frames.max(frames)`, which
  `next_svgf_temporal_alpha` turns into `SVGF_ALPHA_RECOVERY = 0.5` for both
  `alpha_color` and `alpha_moments`. Those land in `params.x` / `params.y`.
  The shader then does:

  ```glsl
  float invN   = 1.0 / (histAge + 1.0);
  float floorC = params.w > 0.5 ? 0.0 : params.x;   // params.w = camera_static
  float alphaC = max(floorC, invN);
  ```

  `params.w` is `camera_static`, computed in `assemble_camera_and_lights` as a
  pure element-wise `view_proj` vs `prev_view_proj` comparison. When the camera
  is parked, `floorC` is **0**, so the host-side recovery α is discarded and the
  blend falls back to `1/(histAge + 1)` — and `histAge` is exactly what a parked
  camera drives to its `min(histAge + 1.0, 255.0)` ceiling, because zero motion
  means every pixel passes the mesh-ID and normal-cone tests every frame.
  A recovery window that is supposed to weight the current frame at 0.5
  therefore weights it at 1/256 ≈ 0.0039 instead — a 128× weaker recovery, and
  the elevated window decrements to zero while having had no effect at all.
  SVGF has no other response to a discontinuity: `signal_temporal_discontinuity`
  does **not** touch `SvgfPipeline::frames_since_creation`, so the `params.z`
  hard-reset path is not an alternative route.
- **Evidence**:
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` passes
    both values to the same call:
    `svgf.upload_params(&self.device, frame, alpha_color, alpha_moments, camera_static)`.
  - `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs`:
    `let camera_static = vp.iter().zip(self.prev_view_proj.iter()).all(|(a, b)| (a - b).abs() < 1.0e-6);`
    — camera-only, no scene or lighting term.
  - Reachable callers that can fire with a parked camera: `save_io.rs` (live
    load-apply, twice), `debug_load.rs` (three sites), `streaming_helpers.rs`
    (three sites), `app_step.rs` (four sites), `context/resize.rs`
    (`RESIZE_RECOVERY_FRAMES` / `SWITCH_RECOVERY_FRAMES`), and now
    `context/post_passes.rs` (`TAA_DISPATCH_FAILURE_RECOVERY_FRAMES`,
    `FSR_DISPATCH_FAILURE_RECOVERY_FRAMES`).
  - **No test covers the interaction.** All four α tests
    (`steady_state_alpha_is_schied_floor`,
    `recovery_window_uses_elevated_alpha_and_decrements`,
    `last_recovery_frame_uses_elevated_alpha_then_reverts`,
    `streaming_recovery_window_runs_full_n_frames_then_reverts`) exercise
    `next_svgf_temporal_alpha` in isolation; every one of them passes while the
    shader throws the returned value away.
  - Side note on the same doc: `signal_temporal_discontinuity`'s comment names
    "cell load, weather flip, fast camera turn" as its triggers, but no weather
    system calls it — `byroredux/src/systems/weather.rs` cross-fades every WTHR
    field continuously (`lerp3` / `lerp1` per key), so no signal is *needed*;
    the trigger list is aspirational, not a missing call.
- **Impact**: Every discontinuity signalled while the camera happens to be
  stationary is a no-op for SVGF colour and moments on every pixel whose
  per-pixel history survived — which is precisely the geometry-unchanged,
  lighting-changed case (live save load onto the same cell, a scripted
  light/imagespace change, a debug reload, a resize/upscaler switch, and the new
  `#3605` path). The visible artefact is the stale bounce term persisting for
  seconds. The direct term is unaffected, which is why it reads as a soft
  "GI didn't notice" rather than a frozen image.
- **Related**: `#674` / `DEN-4` (the recovery window itself); `#3605`
  (`c43cb269`) — its SVGF limb is subject to exactly this cancellation;
  `REN-2026-09-06-D8-02` below (the same flag's other blind spot);
  `REN-2026-09-06-D13-01`.
- **Needs RenderDoc**: no — the whole state machine is host-side plus one
  shader `select`.
- **Suggested Fix**: Make the progressive-accumulation drop conditional on the
  recovery window being closed: pass the floor the host already computed and let
  the shader use `floorC = (params.w > 0.5 && recoveryClosed) ? 0.0 : params.x`,
  or (simpler, no new lane) have `next_svgf_temporal_alpha`'s caller force
  `camera_static = false` while `svgf_recovery_frames > 0`. Add a pure-fn test
  that pins "a live recovery window wins over the camera-static drop", since the
  existing four cannot see this.

---

---

# LOW

### REN-2026-09-06-D1-02: `audit-renderer/SKILL.md`'s Dimension-1 checklist carries three stale claims, and one of them has now manufactured a false finding in two consecutive sweeps — the most recent explicitly recommended for GitHub filing


- **Severity**: LOW
- **Dimension**: AS Correctness (audit-tooling doc rot)
- **Location**: `.claude/commands/audit-renderer/SKILL.md`, the Dimension-1
  "LRU/shrink wiring" bullet and the "Transform" bullet. Ground truth:
  `crates/renderer/src/vulkan/context/mod.rs` (`fill_rt_integrity_stats`),
  `byroredux/src/app_events.rs`, `byroredux/src/commands/world_info.rs`
  (`RtIntegrityCommand`), `byroredux/src/commands/mod.rs`,
  `crates/core/src/ecs/resources/mod.rs` (`RtIntegrityStats::verdict`,
  `RtIntegrityStats::machine_line`),
  `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas_instances`),
  `crates/renderer/src/vulkan/acceleration/memory.rs`
  (`shrink_tlas_scratch_to_fit`)
- **Status**: NEW (same class as `REN-2026-08-30-D1-02`, a different set of
  claims in the same paragraph; not in the OPEN cache)
- **Description**: Three independent inaccuracies in the Dimension-1 checklist
  the auditor is told to treat as the task list:

  1. **"the three `missing_blas` cause-counters … surface only through the
     rate-limited (once/sec) `log::warn!` … there is NO *mem.stats* command,
     and no registered command reads them (#1228 / REN-LOW L-1)."** The
     *mem.stats* half is still true. The "no registered command reads them" half
     has been false since `9c805cd7` (**2026-08-14**), which added the entire
     chain: `AccelerationManager::integrity_snapshot()` →
     `VulkanContext::fill_rt_integrity_stats` → the `RtIntegrityStats` ECS
     resource, refreshed **every frame** from `byroredux/src/app_events.rs` →
     the registered `rt.integrity` console command. `RtIntegrityStats::verdict`
     implements exactly the positive assertion `TlasIntegritySnapshot`'s own
     docstring promised: `PASS` requires `tlas_emitted == tlas_eligible` **and**
     `missing_skinned_blas == missing_rigid_blas == missing_ssbo_instance == 0`.
  2. **"`TRIANGLE_FACING_CULL_DISABLE` on all instances (two-sided meshes)."**
     The code deliberately does the opposite: `build_tlas_instances` sets the
     flag only when `draw_cmd.two_sided`, because pre-#416 disabling backface
     culling on every instance made shadow/GI rays hit the interior backfaces of
     closed single-sided architecture from outside (~2× ray cost on closed
     meshes). **Here the code is right and the SKILL is wrong**; an auditor
     following the checklist literally would file the correct behaviour as a bug.
  3. **"`shrink_tlas_scratch_to_fit` uses TLAS-calibrated slack matching
     `tlas_instance_should_shrink`."** It calls `tlas_scratch_should_shrink`
     (`TLAS_SCRATCH_SLACK_BYTES` = 256 KB). `tlas_instance_should_shrink`
     (`TLAS_REBUILD_SLACK_BYTES` = 1 MB) is the *instance-buffer* predicate used
     by `shrink_tlas_to_fit`. Both are TLAS-calibrated, so the spirit is right
     and the named symbol is wrong — but a symbol reference in a No-Guessing
     checklist is the one place that distinction has to hold.

  Minor, same paragraph: the `blas_static.rs:228-238` line anchor for the #1793
  `--grid` note has rotted off the text it points at (the note is intact; only
  the range moved). The SKILL's own discipline section says to anchor on
  symbols, not line numbers.
- **Evidence**:
  - `grep -rn "integrity_snapshot\|tlas_integrity\|TlasIntegritySnapshot" --include='*.rs' crates byroredux tools`
    → 7 hits, including `crates/renderer/src/vulkan/context/mod.rs`:
    `.map(super::acceleration::AccelerationManager::integrity_snapshot)`.
  - `git log -S "fill_rt_integrity_stats"` and
    `git log -S "RtIntegrityCommand"` both →
    `9c805cd7`, `2026-08-14 23:26:10 -0300`.
  - `grep -rn "RtIntegrityCommand" byroredux/src/` → registered in
    `byroredux/src/commands/mod.rs`, implemented in
    `byroredux/src/commands/world_info.rs`, exercised in
    `byroredux/src/commands_tests.rs`.
  - **The consequence, twice**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`
    `REN-2026-08-30-D1-01` claims *"`integrity_snapshot()` is a `pub` accessor
    with **zero call sites**"*; `docs/audits/AUDIT_RENDERER_2026-09-05.md`
    `REN-2026-09-05-D1-02` re-verified it and concluded *"Re-verified the premise
    still holds at HEAD … **Recommended as the one finding from this run that
    should actually be filed** — it has now survived two sweeps without an issue
    number."* Both are false, and both were false on the day they were written
    (the consumer predates the 08-30 sweep by 16 days). The mechanism is
    visible in the 09-05 report's own evidence line: it greps
    `tlas_integrity\|TlasIntegritySnapshot` — the *struct* name — which cannot
    match a call to the *method* `integrity_snapshot`. The stale SKILL sentence
    is what told both auditors the answer before they grepped.
- **Impact**: The `/audit-publish` step would have created a GitHub issue
  asserting a telemetry gap that was closed three weeks earlier, against code
  whose maintainer would then have to disprove it. This is the failure mode
  `_audit-common.md`'s "Verify the premise before writing a finding" rule and
  the user's `feedback_audit_findings` memory exist to prevent, and it is now
  measured at 2-for-2 on this one sentence. Claim 2 is the more dangerous of the
  three going forward: it points an auditor at correct code and tells them it is
  wrong.
- **Related**: `REN-2026-08-30-D1-02` (the same file, the previous pair of stale
  Dimension-1 claims), `#1228` (the original telemetry gap, now closed by
  `9c805cd7`), `#416` (the two-sided cull gate), `#1226` (the two TLAS shrink
  predicates), `_audit-common.md` §"Never write an instruction to not look".
- **Suggested Fix**: In `SKILL.md`'s Dimension-1 bullet: (a) replace the "no
  registered command reads them" clause with a regression guard — *"verify the
  `integrity_snapshot()` → `fill_rt_integrity_stats` → `RtIntegrityStats` →
  `rt.integrity` chain still runs per-frame and that `verdict()` still requires
  `emitted == eligible` with all three counters zero"* — and keep only the
  (still-true) "*mem.stats* does not exist" half; (b) change "on all instances"
  to "gated on `draw_cmd.two_sided` (#416) — a blanket enable is the
  regression"; (c) swap `tlas_instance_should_shrink` for
  `tlas_scratch_should_shrink` in the `shrink_tlas_scratch_to_fit` clause;
  (d) drop the `blas_static.rs:228-238` line range. Additionally, add a
  corrective note to `docs/audits/AUDIT_RENDERER_2026-09-05.md` and
  `docs/audits/AUDIT_RENDERER_2026-08-30.md` so the two false findings are not
  picked up by a future `/audit-publish` run over the backlog.

---

### REN-2026-09-06-D1-03: `memory-budget.md`'s Acceleration-Structures section names a file the per-frame eviction call left, and its eviction-site census is one site short


- **Severity**: LOW
- **Dimension**: AS Correctness (doc-rot)
- **Location**: `docs/engine/memory-budget.md` §"LRU eviction". Ground truth:
  `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (the
  per-frame `accel.evict_unused_blas(&self.device, alloc, 0)` at the tail of the
  TLAS-build block), `crates/renderer/src/vulkan/acceleration/blas_static.rs`
  (`build_blas_batched`'s three internal calls: pre-batch, mid-batch, and the
  compaction-phase call inside `alloc_compact`)
- **Status**: NEW (not covered by #3866, which is scoped to the budget *formula*
  and the dead *compute_blas_budget* name; not in the OPEN cache)
- **Description**: Two divergences in the section this audit is instructed to
  treat as authoritative:
  1. The doc places the per-frame eviction call *"at the end of `draw_frame`'s
     TLAS-build block"* and links `draw.rs`. `7463204e` ("split `draw_frame`
     into phase helpers") moved that block into
     `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs`; `draw.rs`
     no longer contains an `evict_unused_blas` call at all. The behaviour is
     unchanged (it is still the tail of the TLAS-build block, still with
     `pending_bytes = 0`) — only the file is wrong. **The doc's sibling claim in
     the same section is still correct**: `shrink_tlas_to_fit` and
     `shrink_tlas_scratch_to_fit` really do remain in `draw.rs`, so the two
     statements now point at different files for what the doc describes as
     adjacent end-of-frame work, which is exactly the shape that misleads.
  2. The doc says eviction *"runs pre-batch and mid-batch"*. There is a third
     internal site: the pre-emptive call at the head of `alloc_compact`
     (#2927 / `PERF-D3-03`), which passes the exact
     `total_before + total_after` peak — the only site that sees the real
     residency peak of a batch, since the compaction destinations are allocated
     while every Phase-1 original is still live. The doc's `evict_unused_blas`
     doc-comment in `blas_static.rs` does name it ("`build_blas_batched`'s three
     internal call sites"), so the code and the doc disagree with each other.
- **Evidence**:
  - `grep -rn "evict_unused_blas" --include='*.rs' crates byroredux` → the only
    non-`blas_static.rs` production call is
    `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs`, guarded by
    `if !tlas_build_failed` immediately after `write_tlas` + the `rt_flag` patch.
  - `grep -rn "shrink_tlas_to_fit\|shrink_tlas_scratch_to_fit" --include='*.rs' crates byroredux`
    → both still in `crates/renderer/src/vulkan/context/draw.rs`.
  - `blas_static.rs`'s `evict_unused_blas` doc: *"The params are retained so the
    call sites (`build_blas_batched`'s **three** internal call sites plus
    `dispatch_skin_and_cluster.rs`) keep a stable signature."*
- **Impact**: Documentation only, but this doc is the authority an auditor is
  told to check the code against, so a wrong file name here converts into a
  wasted or wrong finding on the next sweep — the same mechanism as
  `REN-2026-09-06-D1-02`.
- **Related**: `7463204e` (the split), `#2927` / `PERF-D3-03` (the third site),
  `#1911` / `REN-D1-01`, `#1792`, `#3866` / `#3842` / `#3841` (the three other
  open doc-rot issues on this same subsystem).
- **Suggested Fix**: In the "LRU eviction" section, re-link the per-frame call to
  `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (noting it is
  still `draw_frame`'s TLAS-build tail, reached through a phase helper), and
  change "pre-batch and mid-batch" to "pre-batch, mid-batch, and once at the head
  of the compaction phase with the exact `total_before + total_after` peak
  (#2927)".

---

### REN-2026-09-06-D1-04: `memory-budget.md`'s AS constants ledger is one value wrong and one constant short — the skinned-refit rebuild threshold is no longer flat 600, and `MAX_STATIC_BLAS_RESTORES_PER_FRAME` has no row


- **Severity**: LOW
- **Dimension**: AS Correctness (doc-rot)
- **Location**: `docs/engine/memory-budget.md` §"Acceleration Structures (BLAS /
  TLAS)" (the closing `SKINNED_BLAS_REFIT_THRESHOLD` paragraph, and the
  "Reserve floors" / "Scratch buffers" tables). Ground truth:
  `crates/renderer/src/vulkan/acceleration/constants.rs`
  (`SKINNED_BLAS_REFIT_JITTER`, `MAX_STATIC_BLAS_RESTORES_PER_FRAME`),
  `crates/renderer/src/vulkan/acceleration/predicates.rs`
  (`skinned_blas_refit_limit`, `should_rebuild_skinned_blas_after`,
  `plan_static_blas_restore`)
- **Status**: NEW (not in the OPEN cache; `#3866` covers only the budget formula
  row)
- **Description**: The section opens by linking `acceleration/constants.rs` and
  presents itself as that file's ledger. Two entries are now wrong or missing:
  1. *"BLAS refit count before a forced rebuild: `SKINNED_BLAS_REFIT_THRESHOLD` =
     600 frames (~10 seconds at 60 FPS). **After 600 refits** the BLAS is rebuilt
     from scratch."* Since `931241a7` (#3669, 2026-09-03) the effective limit is
     `skinned_blas_refit_limit(entity_id) = 600 + (entity_id % SKINNED_BLAS_REFIT_JITTER)`
     with `SKINNED_BLAS_REFIT_JITTER = 60` — i.e. a per-entity value in
     **600..=659**, deliberately staggered so a continuously animated cohort does
     not all drop and rebuild in the same frame. `931241a7` touched six files,
     none of them under `docs/`. This is a numeric value in an authoritative
     tuning table, not prose: an operator reading "600" and measuring a rebuild
     at frame 641 has no way to tell an expected stagger from a bug.
  2. `MAX_STATIC_BLAS_RESTORES_PER_FRAME = 256` (#3540, `0c45e779`, 2026-08-30)
     is a live per-frame bound on a GPU-memory-driven pass — it is what stops
     Starfield's `citycydoniamainlevel` sitting single-threaded on frame 0 for
     ten minutes with RSS oscillating 12→20.6 GB — and it has no row anywhere in
     the doc. `0c45e779` likewise touched no `docs/` file. Its companion policy
     function `plan_static_blas_restore` (the fit projection that declines the
     pass entirely when the visible set cannot fit the budget) is also
     undocumented, which means the doc gives no account of the one code path
     that can silently drop RT geometry on an over-budget cell.
- **Evidence**:
  - `predicates.rs`:
    `SKINNED_BLAS_REFIT_THRESHOLD.saturating_add(if SKINNED_BLAS_REFIT_JITTER == 0 { 0 } else { entity_id % SKINNED_BLAS_REFIT_JITTER })`,
    consumed by `should_rebuild_skinned_blas_after` → `should_rebuild_skinned_blas`.
    Pinned by `skinned_blas_rebuild_jitter_repeats_only_after_one_full_window`.
  - `git show --stat 931241a7` → 6 files, all under
    `crates/renderer/src/vulkan/acceleration/`; no `docs/`.
  - `git show --stat 0c45e779 | grep docs/` → empty.
  - `grep -n "MAX_STATIC_BLAS_RESTORES_PER_FRAME\|SKINNED_BLAS_REFIT_JITTER" docs/engine/memory-budget.md`
    → no matches.
- **Impact**: Documentation only, but of the class `_audit-common.md` singles out
  — *"a wrong number in a GPU layout contract, not a typo"*. The refit-threshold
  figure is the one an operator would use to reason about a skinned-BLAS rebuild
  spike, and the missing restore cap is the one that explains an
  RT-geometry-missing-on-a-huge-cell report.
- **Related**: `#3669` / `931241a7` (the jitter), `#3540` / `0c45e779` (the
  restore cap), `#679` / `AS-8-9` (the original threshold), `#3866` (the sibling
  budget-formula rot in the same section).
- **Suggested Fix**: Change the closing paragraph to
  `SKINNED_BLAS_REFIT_THRESHOLD` = 600 **plus** a stable per-entity
  `SKINNED_BLAS_REFIT_JITTER` (60) offset, effective 600–659, with the
  cohort-stagger rationale (#3669). Add a row for
  `MAX_STATIC_BLAS_RESTORES_PER_FRAME` = 256 and a sentence on
  `plan_static_blas_restore`'s two bounds (fit projection, then per-frame cap) to
  the "LRU eviction" or a new "Per-frame BLAS recovery" subsection.

---

### REN-2026-09-06-D1-05: four `pub` accessors on `AccelerationManager` have zero call sites, and three of them name a consumer that does not exist — so the deferred-destroy backlog `#3840` introduced is unobservable at runtime


- **Severity**: LOW
- **Dimension**: AS Correctness (observability / dead API)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs`
  (`pending_destroy_blas_count`, `pending_destroy_static_bytes`,
  `pending_destroy_scratch_count`),
  `crates/renderer/src/vulkan/acceleration/memory.rs` (`total_blas_bytes`).
  Ground truth for the named consumers:
  `byroredux/src/commands/mod.rs` (the registry),
  `crates/renderer/src/vulkan/context/mod.rs` (`fill_scratch_telemetry`, what
  `ctx.scratch` actually prints)
- **Status**: NEW (not in the OPEN cache). **Distinct from — and NOT the same
  claim as — the false `REN-2026-08-30-D1-01` / `REN-2026-09-05-D1-02`:
  `integrity_snapshot()` *does* have a live consumer** (see
  `REN-2026-09-06-D1-02`). These four do not.
- **Description**: A `grep` for each accessor across `crates`, `byroredux` and
  `tools` returns only its own definition:
  | Accessor | Docstring claims | Reality |
  |---|---|---|
  | `total_blas_bytes()` | *"reported by `total_blas_bytes()` for telemetry / *tex.stats* console output"* | No caller. There is **no *tex.stats* command** in the registry. Total BLAS VRAM is not surfaced anywhere. |
  | `pending_destroy_blas_count()` | *"Surfaced for `drain_pending_destroys`'s unit test and shutdown telemetry — the count must reach zero after a drain. See #732."* | No caller, and no test names it. |
  | `pending_destroy_scratch_count()` | *"Surfaced for the deferred-destroy regression test and shutdown telemetry … See #1782."* | No caller, and no test names it. |
  | `pending_destroy_static_bytes()` | *"Companion to `pending_destroy_blas_count` for `ctx.scratch` telemetry — the count alone can't show how much VRAM the queue is holding. See #3840."* | No caller. `ctx.scratch` exists, but `fill_scratch_telemetry` emits host-side `Vec`/`HashMap` `(len, capacity)` rows only — no BLAS byte counters. |

  The last row is the sharp one: `fa5c4191` added the accessor **yesterday**
  specifically so an operator could see how much VRAM the deferred-destroy queue
  is holding, and then did not connect it. `live_static_blas_count()` /
  `live_skinned_blas_count()` are the counter-example that shows the wiring
  pattern is available and cheap — they *do* have a consumer
  (`byroredux/src/ownership_sample.rs`).
- **Evidence**:
  - `grep -rn "\btotal_blas_bytes\b" --include='*.rs' crates byroredux tools` →
    definition, field, and doc/comment mentions only; no `total_blas_bytes()`
    call expression anywhere.
  - Same for `pending_destroy_blas_count`, `pending_destroy_scratch_count`,
    `pending_destroy_static_bytes` (the last has field-level reads inside
    `blas_static.rs` itself, but zero calls to the `pub` accessor).
  - `grep -rn '"tex\.stats"' byroredux/src/commands/*.rs` → no match; the
    registered commands in that family are `ctx.scratch` and the memory-frag
    command `mem` + `.frag` (spelled out to avoid reading as a shader path).
  - `crates/renderer/src/vulkan/context/mod.rs`'s `fill_scratch_telemetry` pushes
    `ScratchRow { name, len, capacity, elem_size_bytes }` for
    `gpu_instances_scratch`, `frame_lights_scratch`, `previous_models_scratch`,
    `batches_scratch`, the two rigid-motion maps, `indirect_draws_scratch`,
    `terrain_tile_scratch`, the skin-path sets, and (per #3693) the three
    `tlas_*_scratch` Vecs — all host-side capacities, no device bytes.
- **Impact**: BLAS device residency and the deferred-destroy backlog cannot be
  read from a running engine, so the exact failure `REN-2026-09-06-D1-01`
  describes (a batch overshooting the real budget by the queued amount) is not
  diagnosable in the field even after `#3840` computed the number. Secondary:
  four `pub` items with docstrings that assert consumers which do not exist —
  the same "documented surface that isn't there" pattern that produced the
  *mem.stats* / *tex.stats* family of `REN-LOW L-1` / `L-6` findings.
- **Related**: `#3840` (added the newest of the four), `#732` / `#1782` (the two
  older ones), `REN-LOW L-1` / `L-6` (the *mem.stats* precedent), and
  `REN-2026-09-06-D1-01` (the accounting gap these would have made visible).
- **Suggested Fix**: Add four rows to `fill_rt_integrity_stats`'s neighbour
  (`ScratchTelemetry` is host-only; a `mem.frag`- or `ctx.scratch`-adjacent
  device-side block, or extra `RtIntegrityStats` fields, all work) surfacing
  `total_blas_bytes`, `static_blas_bytes`, `pending_destroy_static_bytes` and
  the two pending counts — and correct the three docstrings so no accessor names
  a command that does not exist. If a consumer is genuinely not wanted for the
  count accessors, delete them rather than leave `pub` items whose docs describe
  a test that was never written.

---

### REN-2026-09-06-D1-06: `AccelerationManager::skinned_blas` is the last `std::collections::HashMap` on the per-frame skinned path — `#3061` swept every sibling but could not see this one, because the guard test lives in a different file


- **Severity**: LOW
- **Dimension**: AS Correctness (hot-path hashing residual)
- **Location**: `crates/renderer/src/vulkan/acceleration/mod.rs`
  (`skinned_blas` field declaration and its `HashMap::new()` in
  `AccelerationManager::new`). Probe sites:
  `crates/renderer/src/vulkan/acceleration/tlas.rs` (`build_tlas_instances`,
  `self.skinned_blas.get_mut(&draw_cmd.entity_id)` once per skinned draw),
  `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`
  (`has_skinned_blas`, `skinned_blas_entry`, `should_rebuild_skinned_blas`,
  `refit_skinned_blas`, `drop_skinned_blas`)
- **Status**: **Residual of `PERF-D6-01`** (`docs/audits/AUDIT_PERFORMANCE_2026-08-16.md`,
  LOW) — that finding tabulated seven fields; `#3061` converted six of them, all
  of which live in `crates/renderer/src/vulkan/context/mod.rs`. `skinned_blas` is
  the seventh, and the only one outside that file. Not in the 151-issue OPEN
  cache; verified against the code rather than against GitHub, per
  `_audit-common.md`.
- **Description**: `_audit-common.md`'s hot-path rule states the per-frame
  render/skinning path is `FxHashMap`/`FxHashSet` end-to-end *and must stay that
  way across the crate boundary*. `context/mod.rs` now holds that line: eight
  fields are declared `FxHashMap`/`FxHashSet` with `#2923` / `#3045` / `#3061`
  citations, pinned by the `"{what} must stay \`FxHashSet\` (#2923)"` assertions
  and their `FxHashMap` siblings. `skinned_blas` is declared
  `std::collections::HashMap<EntityId, BlasEntry>` and is probed on the same
  per-frame per-entity keyspace: `build_tlas_instances` does one `get_mut` per
  skinned draw command, and the refit path adds `has_skinned_blas` +
  `skinned_blas_entry` + `should_rebuild_skinned_blas` per dirty entity. The
  guard tests cannot catch it because they read `context/mod.rs`'s own source
  text.
- **Evidence**:
  - `crates/renderer/src/vulkan/acceleration/mod.rs`:
    `pub(super) skinned_blas: std::collections::HashMap<EntityId, BlasEntry>,`
    and `skinned_blas: std::collections::HashMap::new(),`.
  - `grep -n "FxHashMap\|FxHashSet" crates/renderer/src/vulkan/context/mod.rs` →
    `rustc_hash` is already imported and used for `skin_slots`, `morph_slots`,
    `morph_delta_cache`, `failed_skin_slots`, `failed_skin_blas`,
    `skin_dispatch_seen_scratch`, `skin_built_this_frame_scratch`,
    `blend_seen_scratch`, `blend_pipeline_cache`, and the two rigid-motion maps.
    `rustc-hash` is therefore already a `crates/renderer` dependency — the change
    is a type substitution with no new dep.
  - `docs/audits/AUDIT_RENDERER_2026-08-30.md` records the sweep as complete:
    *"`pose_dirty_crosses_the_crate_boundary_without_siphash` … covers
    `FrameInputs.pose_dirty`, `record_skinned_blas_refit`'s parameter,
    `skin_slots`, `morph_slots`, `failed_skin_slots`, `failed_skin_blas` and the
    two scratch sets … All green."* `skinned_blas` is absent from that list.
- **Impact**: Small in absolute terms (SipHash-1-3 over a `u32` key, on the order
  of the live skinned-entity count per frame — ~120 on the FO4 baseline). The
  real cost is the one `PERF-D6-01` named: the path now *reads* as Fx-hashed
  end-to-end, and the guard tests say so, while one collection on it is not — so
  the next reader trusts a property that does not hold across the module
  boundary.
- **Related**: `PERF-D6-01` (`docs/audits/AUDIT_PERFORMANCE_2026-08-16.md`),
  `#2923`, `#3045`, `#3061`, `_audit-common.md` §"Hot-path hashing".
- **Suggested Fix**: Change the field to
  `rustc_hash::FxHashMap<EntityId, BlasEntry>` (construct with
  `FxHashMap::default()`), and add a source-shape assertion in
  `crates/renderer/src/vulkan/acceleration/tests/blas_static_tests.rs` mirroring
  the `context/mod.rs` guards so the acceleration module carries its own pin —
  the cross-file blind spot is what let this one survive `#3061`.

---

### REN-2026-09-06-D1-07: `drop_skinned_blas` is the one deferred-destroy push site that bypasses `DEFAULT_COUNTDOWN`


- **Severity**: LOW
- **Dimension**: AS Correctness (code quality)
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_skinned.rs`
  (`drop_skinned_blas`); contract in `crates/renderer/src/deferred_destroy.rs`
  (`DEFAULT_COUNTDOWN`, `DeferredDestroyQueue::push`)
- **Status**: NEW (not in the OPEN cache)
- **Description**: `DeferredDestroyQueue::push`'s doc says *"Production callers
  pass [`DEFAULT_COUNTDOWN`] so the item survives at least
  `MAX_FRAMES_IN_FLIGHT` frames"*, and `DEFAULT_COUNTDOWN` exists precisely so a
  future `MAX_FRAMES_IN_FLIGHT` bump propagates in one place. Three of the four
  push sites into `pending_destroy_blas` / `pending_destroy_scratch` pass
  `DEFAULT_COUNTDOWN`; `drop_skinned_blas` passes `MAX_FRAMES_IN_FLIGHT as u32`
  directly.
- **Evidence**: `blas_skinned.rs`:
  `self.pending_destroy_blas.push(entry, MAX_FRAMES_IN_FLIGHT as u32);` against
  `blas_static.rs`'s `self.pending_destroy_blas.push(entry, DEFAULT_COUNTDOWN);`
  (twice) and `self.pending_destroy_scratch.push(old, DEFAULT_COUNTDOWN);`.
  `crates/renderer/src/deferred_destroy.rs`:
  `pub(crate) const DEFAULT_COUNTDOWN: u32 = crate::vulkan::sync::MAX_FRAMES_IN_FLIGHT as u32;`
- **Impact**: **None behaviourally, today or after any `MAX_FRAMES_IN_FLIGHT`
  bump** — `DEFAULT_COUNTDOWN` *is* `MAX_FRAMES_IN_FLIGHT as u32`, so the two
  expressions are identical by construction and cannot diverge. Reported only
  because the module doc states a convention this site does not follow, and
  because a reader auditing the deferred-destroy contract has to re-derive the
  equivalence at this one site. Filed as the lowest-priority item in this run;
  drop it if the fix budget is tight.
- **Related**: `#372`, `#1449`, `#1782`, `#2481`.
- **Suggested Fix**: One-line substitution to `DEFAULT_COUNTDOWN` (already
  imported in the sibling module; add the `use` in `blas_skinned.rs`).

---

### REN-2026-09-06-D10-02: `dof_effective_view_proj` applies the lens jitter in ABSOLUTE space, so the aperture offset is quantised away at exterior magnitudes

- **Severity**: LOW (the DoF path has no production enabler today)
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`dof_effective_view_proj`)
- **Status**: NEW
- **Description**: The function correctly returns a **render-origin-relative** matrix — its
  own doc says so and `look_at_rh(jittered_eye − render_origin, focal_pt − render_origin, up)`
  delivers it. But the two points it subtracts from are *composed* at absolute magnitude
  first: `jittered_eye = pos + lens_u * right + lens_v * up` and
  `focal_pt = pos + focus_dist * fwd`, with `pos` the raw absolute camera position. The
  aperture term is a sub-unit lens offset being added to a value whose f32 ULP, at
  Markarth's X ≈ −176 000, is 0.015625 — so the jitter is quantised (and, below ~0.008 u,
  discarded outright) *before* the rebase that was supposed to protect it. Doing the
  rebase first — `let rel = pos − render_origin;` then composing from `rel` — is exact and
  costs nothing.
- **Evidence**: `jittered_eye` / `focal_pt` are built from `Vec3::from_array(camera_pos)`
  (absolute) and only rebased inside the `look_at_rh` call. The subtraction itself is exact
  (`render_origin` is a multiple of 4096, and the difference is < 4096, hence exactly
  representable) — the loss happens strictly in the addition that precedes it.
- **Impact**: **Currently dormant, and I could not find a way to reach it.** `Camera::aperture`
  defaults to `0.0`, no console command or production code path sets it (the only non-zero
  values in the tree are `2.5` inside `draw.rs`'s own tests), and `fsr_gated_dof` forces
  `aperture = 0.0` whenever FSR — the engine-default upscaler — is active. If DoF is ever
  wired up, a subtle aperture (≲ 0.05 u) would produce ~3 distinct sample offsets instead
  of a smooth disk at exterior worldspaces, i.e. banding rather than bokeh; a large one
  (2.5 u) would still work, coarsely. Reported because it is the one place in the
  render-origin machinery where the rebase happens later than it needs to, and the fix is
  a two-line reorder that removes the question permanently.
- **Related**: #1525 (the degenerate-`focus_dist` guard, intact), #2197 (`fsr_gated_dof`),
  `docs/engine/shader-pipeline.md` §"Render-origin-relative (raster path)".
- **Suggested Fix**: Hoist the rebase: `let rel = Vec3::from_array(camera_pos) - render_origin;`
  then build `jittered_eye_rel = rel + lens_u * right + lens_v * up` and
  `focal_pt_rel = rel + focus_dist * fwd`, passing those to `look_at_rh` directly. Keep
  returning the **absolute** `jittered_eye` (`rel + render_origin`) since the shader's
  view-dir math wants it, as the current doc comment already specifies.

---

### REN-2026-09-06-D10-03: on a non-`D32_SFLOAT` device `depth.stats` becomes an unbreakable "armed — come back in a frame or two" loop with no console-visible reason

- **Severity**: LOW
- **Dimension**: Camera-Relative Precision
- **Location**: `byroredux/src/commands/depth.rs` (`DepthStatsCommand::execute`), `crates/renderer/src/vulkan/context/depth_capture.rs` (`VulkanContext::depth_capture_record_copy`)
- **Status**: NEW (the console-facing consequence of the just-landed #3570; the refusal
  itself is correct — see Coverage)
- **Description**: `#3570` correctly made `depth_capture_record_copy` refuse rather than
  misdecode when `find_depth_format` selected `D16_UNORM`, and it consumes the request flag
  before returning, so the refusal is a clean no-op with no layout or leak consequence.
  What it has no path back to is the caller. `DepthCaptureBridge::take_result()` will
  return `None` forever, so `depth.stats` takes its "nothing landed yet" branch on every
  invocation: it re-arms and prints "depth capture armed — run `depth.stats` again in a
  frame or two to read it". The only signal that the capture will *never* land is a
  `log::warn!` in the renderer's log stream, which a `byro-dbg` console session does not
  see.
- **Evidence**: `execute` has exactly two exits for a present bridge — the `take_result()`
  `None` arm (arm + "come back") and the report arm — and no third arm for "capture
  refused". The refusal in `depth_capture_record_copy` writes nothing to
  `depth_capture_result` and never arms `depth_capture_pending_readback`.
- **Impact**: `depth.stats` is the *live* half of #3308's before/after comparison gate
  (the analytic half being `Camera::depth_resolution_at` / `_reversed`). On any device
  whose `find_depth_format` falls through to `D16_UNORM` — Vulkan mandates `D16_UNORM`
  depth-attachment support but not `D32_SFLOAT`, which is the whole premise of #3570 — the
  gate silently has no live half, and the operator's evidence is an instruction to keep
  waiting. Zero impact on the dev RTX 4070 Ti. Note this is *distinct* from the already-filed
  #3571 (`REN-2026-08-30-D10-02`), which is about the gate being runnable only
  pre-conversion; this is about the gate being unrunnable at all on some hardware, with no
  way to tell.
- **Related**: #3570 (`26f9ddf4`), #3308, #3571 (distinct — do not merge), #3630 (the
  degenerate-camera rejection line, which is the precedent for surfacing a refusal to the
  console instead of leaving it ambiguous).
- **Suggested Fix**: Give the refusal a return channel — e.g. have `depth_capture_record_copy`
  store a `DepthCapture`-shaped `Err`/unsupported marker on the bridge (or expose the
  selected `depth_format` on the bridge at init), and add a third arm to
  `DepthStatsCommand::execute` printing "depth capture unsupported: device selected
  {format}, not D32_SFLOAT (#3570)". Mirrors the shape #3630 already established for the
  degenerate-camera case, and stops the command from arming a request that can never
  complete.

### REN-2026-09-06-D11-02: the pipeline-cache header gate accepts a `headerSize` larger than the file it validated

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs` (`validate_pipeline_cache_header`)
- **Status**: NEW
- **Description**: The SAFE-11 / #91 gate exists so "a bad header never
  reaches the driver" — an explicit defence-in-depth argument about a cache
  file "dropped next to the binary by a process with filesystem write
  access". It rejects `len < 32`, `headerSize < 32`, `headerVersion != 1`,
  and vendor/device/UUID mismatch, but deliberately does not upper-bound
  `headerSize` ("a future version might legitimately grow the prefix"). The
  cheap and version-agnostic bound is missing: `headerSize` must not exceed
  the length of the file it describes. A 32-byte file claiming
  `headerSize = 0xFFFF_FFFF` passes every check and is handed to
  `vkCreatePipelineCache` with the correct vendor/device/UUID, which is the
  one shape the gate's own threat model names.
- **Evidence**: the `if header_size < 32 { return false; }` check with no
  companion `header_size as usize > initial_data.len()` arm; the six
  `pipeline_cache_header_tests` cases cover short files, bad version, and the
  three identity fields, not an over-large `headerSize`.
- **Impact**: Defence-in-depth only — the driver re-validates independently
  and a well-behaved one rejects it. The pre-condition (write access to the
  executable's directory) is already a strong position for an attacker.
  Filed because the gate's stated purpose is exactly to catch this, and the
  fix is one comparison plus one test.
- **Related**: SAFE-11 / #91.
- **Suggested Fix**: Add `if header_size as usize > initial_data.len() { return false; }`
  alongside the `< 32` check, and a `pipeline_cache_header_tests` case for it.
  This is compatible with the "future version might grow the prefix" comment
  — a grown prefix still fits inside its own file.

---

### REN-2026-09-06-D11-03: three stale prose statements on the pipeline / render-pass surface

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs` (`create_render_pass`'s attachment-3 comment), `crates/renderer/src/vulkan/pipeline.rs` (`create_ui_pipeline`'s doc comment), `docs/engine/shader-pipeline.md` (§"G-Buffer Layout", the sentence after the table)
- **Status**: NEW
- **Description**: Three independent prose claims on this surface no longer
  match the code they describe. Each is small; they are grouped because they
  are one edit's worth of work and all three would mislead the next reader of
  this exact dimension.
  1. `create_render_pass`'s mesh-ID attachment comment says overflow "is
     handled by the warn-once `log::error!` + clamp in `draw.rs::draw_frame` +
     `upload.rs`". The clamp is still in `scene_buffer/upload.rs`, but the
     warn-once "RP-1" `log::error!` moved out of `draw_frame` into
     `context/build_and_upload_instances.rs` under the #3282 phase split.
     `draw.rs`'s only surviving "RP-1" mention is the *indirect-draw* ceiling
     policy in `should_use_indirect_draws`'s doc — a different overflow, on a
     different buffer; the instance-overflow `log::error!` this comment sends
     the reader to `draw_frame` for is not there. Same class of stale pointer
     #3881 just fixed one
     comment above it (the "search `0x80000000u` in `triangle.frag`"
     instruction).
  2. `create_ui_pipeline`'s doc says "water uses its own 128-byte
     push-constant layout on a separate pipeline layout". The separate layout
     is still true; the size is not — `WaterPush` is **16 bytes**, held there
     by `const _: () = assert!(size_of::<WaterPush>() == 16)`, since the
     per-draw payload moved into the `GpuWaterParams[]` SSBO and the push
     block became a compact `{ uint waterIndex; uvec3 _reserved; }` index.
  3. `docs/engine/shader-pipeline.md` §"G-Buffer Layout" ends with "After
     `vkCmdEndRenderPass` all attachments transition to
     `SHADER_READ_ONLY_OPTIMAL`." The depth attachment's `final_layout` is
     `DEPTH_STENCIL_READ_ONLY_OPTIMAL`, not `SHADER_READ_ONLY_OPTIMAL` — and
     that distinction is load-bearing three paragraphs later, where the same
     doc correctly names `DEPTH_STENCIL_READ_ONLY_OPTIMAL` as
     `copy_depth_to_history`'s precondition, and again in
     `depth_capture_record_copy`'s contract (#3628). The doc contradicts
     itself; the code is right.
- **Evidence**: as cited per item above.
- **Impact**: Documentation only. Item 3 is the one worth prioritising — it
  sits in the file the audit skill designates authoritative for G-buffer
  layout, and it disagrees with a layout precondition two other subsystems
  now depend on by name.
- **Related**: #3881 (`bf8ded3d`, which fixed the sibling stale pointer in the
  same comment block), #3282 (the split that moved RP-1), #3628 (the depth
  layout contract), #2757 (the "line numbers rot, name the symbol" rule this
  keeps re-proving).
- **Suggested Fix**: Point item 1 at `build_and_upload_instances`; change item
  2's "128-byte" to "16-byte" (or drop the size and name `WaterPush`); qualify
  item 3 to "all eight colour attachments transition to
  `SHADER_READ_ONLY_OPTIMAL`; depth transitions to
  `DEPTH_STENCIL_READ_ONLY_OPTIMAL`".

---

### REN-2026-09-06-D12-03: the TAA-failure recovery would rewrite a possibly-pending descriptor set (latent behind D12-02)

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs` (`record_taa_pass`'s error arm), `crates/renderer/src/vulkan/composite.rs` (`CompositePipeline::fall_back_to_raw_hdr` → `rebind_hdr_views`)
- **Status**: NEW
- **Description**: `record_taa_pass`'s failure arm calls
  `composite.fall_back_to_raw_hdr(&self.device)` from **inside** the frame's
  command-buffer recording. That delegates to `rebind_hdr_views`, which loops
  `for (i, &hdr_view) in hdr_views.iter().enumerate()` over all
  `MAX_FRAMES_IN_FLIGHT` slots and issues
  `device.update_descriptor_sets(&[write_combined_image_sampler(self.descriptor_sets[i], 0, …)], &[])`
  for each. At that point `draw_frame` has waited only `in_flight[frame]`;
  the other slot's command buffer — which bound
  `composite.descriptor_sets[1 - frame]` in `CompositePipeline::dispatch`
  (`cmd_bind_descriptor_sets(… &[self.descriptor_sets[frame], bindless_set] …)`)
  — may still be in the pending state. Updating it violates
  VUID-vkUpdateDescriptorSets-None-03047, and the composite descriptor set
  layout is created with a plain
  `vk::DescriptorSetLayoutCreateInfo::default().bindings(&ds_bindings)` — no
  `VK_DESCRIPTOR_BINDING_UPDATE_AFTER_BIND_BIT` or
  `UPDATE_UNUSED_WHILE_PENDING_BIT` — so neither exemption applies.
  This is **not currently reachable**: per **D12-02** the arm is dead. It is
  filed separately because fixing D12-02 (making a TAA failure latch) makes
  this live, and the two must land together.
- **Evidence**: `rebind_hdr_views`'s `debug_assert_eq!(hdr_views.len(),
  MAX_FRAMES_IN_FLIGHT)` and its per-`i` `update_descriptor_sets` call;
  `composite.rs`'s `create_descriptor_set_layout` call takes no binding-flags
  `p_next`; the only other `rebind_hdr_views` caller is `context/init.rs`
  (construction time, safe).
- **Impact**: Undefined behaviour on a descriptor set read by executing GPU
  work — validation error at minimum, potential device-lost. Zero impact
  today (unreachable); the finding exists so a D12-02 fix does not silently
  open it.
- **Related**: #3605 (`c43cb269`), #2519 (the FSR sibling recovery), D12-02.
- **Suggested Fix**: Defer the rebind rather than doing it mid-recording:
  latch a `composite_needs_raw_hdr_rebind` flag and perform the
  `rebind_hdr_views` at the top of the *next* `draw_frame`, after
  `sync_and_acquire_frame`'s fence wait proves both slots retired — or wrap
  it in a `device_wait_idle()` on this once-per-session path, which is
  acceptable given it fires at most once and already implies a degraded
  session. Prefer the former; it needs no idle stall and matches the
  deferred-destroy convention the rest of the context uses.

---

### REN-2026-09-06-D13-01: `signal_temporal_discontinuity` has phase-dependent semantics — two of its five limbs are inert from the two `record_post_passes` call sites, including `#3605`'s new one, and nothing documents or guards the phase requirement


- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/mod.rs` (`signal_temporal_discontinuity`); call sites `crates/renderer/src/vulkan/context/post_passes.rs` (`record_taa_pass`, `record_upscale_pass`); the end-of-frame swap in `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`, the `std::mem::swap(&mut self.previous_rigid_models, &mut current_rigid_models)` immediately after `mark_dispatch_completed`)
- **Status**: NEW — a gap in the fix `c43cb269` shipped, not a regression of it
- **Description**: `signal_temporal_discontinuity` has five limbs:
  `svgf_recovery_frames.max(frames)`, `taa.signal_history_reset()`,
  `fsr_temporal.signal_reset()`, `volumetrics.signal_history_reset()`, and
  `previous_rigid_models.clear()`. Its documented contract includes *"The first
  frame after a discontinuity must not encode object motion against transforms
  from the retired scene/camera history."* That contract holds only for callers
  that run **before** `build_and_upload_instances` — i.e. outside `draw_frame`
  (streaming / save / debug-load / app-step / resize) or at the `camera_cut`
  site inside `assemble_camera_and_lights`. Both `record_post_passes` callers
  run *after* it, and:

  1. **`previous_rigid_models.clear()` is unconditionally undone.** `draw_frame`
     ends with `std::mem::swap(&mut self.previous_rigid_models, &mut current_rigid_models)`,
     refilling the map with this frame's models. The clear performed at
     `record_taa_pass` / `record_upscale_pass` time is discarded before the next
     frame's `uses_rigid_motion_history` lookup ever reads it.
  2. **`fsr_temporal.signal_reset()` is phase-fragile.** `reset_pending` is read
     into `fsr_frame` back in `assemble_camera_and_lights`; a reset raised
     afterwards would be cleared by this same frame's `mark_dispatch_completed()`
     (reached via `take_submitted_dispatch()` at the tail of `draw_frame`)
     before the next frame reads it. Today this is saved only by an accident of
     which states can coexist — `#2519` only signals when the dispatch *failed*
     (so `dispatched_this_frame` is false and the reset survives), and `#3605`
     only fires in `UpscalerMode::Taa`, where `fsr_temporal` is `None`.

  Evaluating `#3605`'s call limb by limb: `taa.signal_history_reset()` is inert
  by construction (`taa_failed` has just latched, so `upload_params` and the
  dispatch are both gated off for the rest of the session);
  `fsr_temporal` is `None`; `previous_rigid_models.clear()` is undone per (1);
  `volumetrics.signal_history_reset()` **works** (it runs after
  `record_volumetrics_pass`, so clearing `dispatched_this_frame` correctly stops
  `mark_frame_completed` from validating the history); and the SVGF limb works
  only when the camera is moving (`REN-2026-09-06-D8-01`). Net delivered effect
  of `c43cb269` is one volumetrics history reset plus a conditional SVGF α bump.
- **Evidence**:
  - `signal_temporal_discontinuity`'s own comment: *"The first frame after a
    discontinuity must not encode object motion against transforms from the
    retired scene/camera history."*
  - Call order inside `record_post_passes`: `record_svgf_pass`,
    `record_caustic_splat_pass`, `record_volumetrics_pass`, `record_taa_pass`,
    `record_ssao_pass`, `record_composite_pass`, `record_bloom_pass`,
    `record_upscale_pass`, `record_presentation_pass`.
  - `previous_rigid_models` is read only at `build_and_upload_instances`
    (`self.previous_rigid_models.get(&draw_cmd.entity_id)`) and written only by
    the end-of-frame swap plus the `clear()` in question.
  - `record_taa_pass_signals_temporal_discontinuity_on_dispatch_failure`
    (`post_passes.rs`) asserts the *call* is present; nothing asserts which
    limbs of it survive to the next frame.
- **Impact**: No live visual defect today — both in-frame callers signal on a
  frame where the scene geometry did **not** change, which is exactly the case
  where correct (non-zeroed) motion vectors are wanted anyway. The defect is
  that a documented, load-bearing invariant is silently unenforceable from
  inside `record_post_passes`, and `#3605` has just established that call site
  as a normal place to signal from. The next in-frame caller that signals a
  *real* scene discontinuity gets a partial reset with no diagnostic.
- **Related**: `#3605` / `c43cb269`; `#2519` (the FSR sibling);
  `REN-2026-09-06-D8-01`.
- **Needs RenderDoc**: no.
- **Suggested Fix**: Either (a) make the in-frame limbs order-independent —
  have `previous_rigid_models.clear()` set a `suppress_rigid_history_next_frame`
  flag the next `build_and_upload_instances` consumes and clears, mirroring the
  existing `!camera_cut` guard in that same loop; or (b) document the phase
  requirement on `signal_temporal_discontinuity` and add a source-scan test in
  the style of `record_taa_pass_signals_temporal_discontinuity_on_dispatch_failure`
  pinning that no limb depends on being called pre-upload. (a) is preferable —
  a doc-only fix leaves the trap armed.

---

### REN-2026-09-06-D13-02: `#3607` closed the discoverability half of the five-copy `octDecode` duplication but not the drift half — every guard is a name/count pin, and the shared copy in `include/math_common.glsl` is declared non-standalone


- **Severity**: LOW
- **Dimension**: TAA
- **Location**: `crates/renderer/shaders/taa.comp`, `crates/renderer/shaders/svgf_temporal.comp`, `crates/renderer/shaders/svgf_atrous.comp`, `crates/renderer/shaders/caustic_splat.comp`, `crates/renderer/shaders/include/math_common.glsl`; guard `taa_comp_octahedral_decoder_is_named_octdecode` (`crates/renderer/src/vulkan/taa.rs`)
- **Status**: NEW (residual of closed `#3607`; the rename itself verified complete — see Coverage)
- **Description**: The rename landed correctly and the maintenance comments in
  all four `.comp` copies now enumerate each other. But `taa.comp`'s own comment
  states the residual hazard verbatim: *"a divergence here is a silent, per-pixel
  difference in a history-rejection predicate, invisible to every existing test
  since all of them are source-scan pins."* That is still true. The two guards
  are `taa_comp_octahedral_decoder_is_named_octdecode` (asserts the string
  `vec3 octDecode(vec2 e)` is present, that `oct_decode` is absent, and that
  `octDecode(` occurs exactly 3×) and
  `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces` (asserts the
  reject-list expression). Neither compares the *bodies*. A one-line edit to
  three of the four copies would still pass everything.
  A fifth copy exists that the enumerations do not name:
  `include/math_common.glsl` already defines `octDecode` next to `octEncode`.
  Note the obvious fix is **not** a drop-in: that header opens with *"NON-STANDALONE
  shader fragment … it references symbols (structs, SSBO/UBO bindings, helper
  functions, constants) defined in `shader_constants.glsl` and in earlier
  includes"* (`sampleDalcCube` reads `dalcPosX` &c.), so the compute shaders
  cannot `#include` it as-is; the codec would have to be split into its own
  standalone header first.
- **Evidence**: I extracted the five bodies and compared them — identical apart
  from where the `vec2(...)` argument list wraps. `grep -rn "math_common.glsl" crates/renderer/shaders/`
  returns four prose mentions plus exactly one real `#include`, from
  `triangle.frag`.
- **Impact**: Drift risk only, on the predicate that gates TAA history
  acceptance (`dot(currNormal, prevNormal) < 0.85`) and SVGF's bilinear
  consistency loop (`dot(currN, prevN) < 0.9`). A future correction to the codec
  (precision, dropping the `normalize`, an snorm-range change) applied to some
  copies leaves TAA rejecting history differently from SVGF and from the
  `octEncode` producer, with no test failing.
- **Related**: `#3607` (`20f5f476`); the 2026-08-30 `D13-04`.
- **Needs RenderDoc**: no.
- **Suggested Fix**: Cheapest closure is a body-equality source scan next to the
  existing pin: extract the `vec3 octDecode(vec2 e) { … }` span from all five
  files, strip whitespace, and assert all five are equal. Real fix is to split a
  standalone `include/oct_codec.glsl` out of `math_common.glsl` and have all
  five `#include` it.

---

### REN-2026-09-06-D14-01: six rotted `file:NN` cross-references inside the caustic / water / volumetrics sources point at unrelated code


- **Severity**: LOW (comment accuracy; no runtime effect)
- **Dimension**: Caustics / Water / Volumetrics (in-code doc-rot)
- **Location**:
  - `crates/renderer/src/vulkan/water_caustic.rs` — module docstring, *"the caustic pipeline's pre-clear barrier at `caustic.rs:720-735`"*
  - `crates/renderer/shaders/caustic_splat.comp` — *"see draw.rs:268-273"*, *"`INSTANCE_FLAG_CAUSTIC_SOURCE` in `shader_constants_data.rs:86`"*, *"the Rust ↔ define lockstep assertion at shader_constants.rs:313-320"*
  - `crates/renderer/src/vulkan/volumetrics.rs` — two sites, both *"Mirrors `CausticPipeline::write_tlas` (caustic.rs:627)"*
  - `crates/renderer/shaders/water.frag` — *"the same Nperturbed already used by the primary refraction ray above (line ~547)"*
- **Status**: **NEW.** No open issue matches (searched `line number`, `line-number`, `doc-rot` + the file names). The adjacent `crates/renderer/src/vulkan/context/resize.rs` *"matches init behaviour at mod.rs:1422-1426"* is the same class in a Dim-16-adjacent file and is included in the fix scope below.
- **Description**: Each of these was correct when written and now names a different construct. Verified individually against HEAD:

  | Cited | Claimed to be | What is actually there |
  |---|---|---|
  | `caustic.rs:720-735` | the pre-clear barrier | the `write_combined_image_sampler` / `write_storage_buffer` block inside `write_descriptor_sets` |
  | `draw.rs:268-273` | the `sceneFlags.x` RT gate | a `morph_slot_backs_mesh` unit-test assertion |
  | `shader_constants_data.rs:86` | `INSTANCE_FLAG_CAUSTIC_SOURCE` | `VERTEX_STRIDE_FLOATS` (the real definition is ~380 lines further down) |
  | `shader_constants.rs:313-320` | the Rust↔`#define` lockstep assertion | the GLSL tokenizer's `while index < bytes.len()` loop |
  | `caustic.rs:627` (×2) | `CausticPipeline::write_tlas` | the image-view-creation error arm inside `create_slot` (`write_tlas` is ~166 lines later) |
  | `water.frag` "line ~547" | the primary refraction ray | `foamShoreline`'s `sceneFlags.x` early-out |
  | `context/mod.rs:1422-1426` | the bloom-init hard-fail | the `skin_first_sight_builds_scratch` field declaration |

  All seven still resolve to *plausible-looking* code, which is what makes them costly: a reader who follows one lands somewhere real and draws the wrong conclusion rather than noticing the reference is dead.
- **Evidence**: `sed -n` at each cited range, reproduced in the table above. `grep -n "pub fn write_tlas" crates/renderer/src/vulkan/caustic.rs` → line 793, not 627. `grep -n "INSTANCE_FLAG_CAUSTIC_SOURCE" crates/renderer/src/shader_constants_data.rs` → line 469, not 86.
- **Impact**: Auditor and maintainer time only — but this is the exact class the audit skill's *"Symbols, not line numbers — line anchors rot on every refactor"* rule exists to prevent, and the rule is currently enforced only on audit skill files (`_audit-validate.sh`) and not on production comments. Two of the seven sit in `caustic_splat.comp`, a file that changed three times in two days.
- **Related**: #1114 (the path-reference convention), the `_audit-validate.sh` gate, #3866 / #3842 / #3846 (the same doc-rot family in the acceleration and bindings docs).
- **Suggested Fix**: Replace each with the symbol it means — `CausticPipeline::clear_for_skip` / the moving-camera arm of `CausticPipeline::dispatch`; the `patch_camera_rt_flag` site in `draw_frame`; the bare constant name `INSTANCE_FLAG_CAUSTIC_SOURCE` plus the two tests that actually pin it (`caustic_splat_comp_uses_named_instance_flag_constant` and `instance_flag_bits_match_scene_buffer_consts`, both in `crates/renderer/src/shader_constants.rs`); `CausticPipeline::write_tlas`; `traceWaterRay`'s refraction call; `VulkanContext::new`'s bloom-init arm. Cheap, and it is the same rule the audit tooling already enforces one directory over.

### REN-2026-09-06-D15-01: `water.frag`'s caustic refraction hardcodes `1.0 / 1.33` while its own primary refraction ray uses the authored `WaterMaterial::ior`


- **Severity**: LOW (dormant today — nothing in the cell loader currently overrides the 1.33 default; becomes a visible divergence the moment a WATR record or a tuning pass sets one)
- **Dimension**: Water (water-side caustics)
- **Location**: `crates/renderer/shaders/water.frag` — the caustic block's `refract(-sunDir, causticNormal, 1.0 / 1.33)`, versus the primary refraction's `float eta = viewFromPositiveSide ? (1.0 / max(ior, 1.0)) : max(ior, 1.0);` where `float ior = push.timing.w;` (`push` is the `#define push waterParams.params[drawPush.waterIndex]` alias for one `WaterParams` SSBO record, **not** a Vulkan push constant). CPU side: `WaterMaterial::ior` (`crates/core/src/ecs/components/water.rs`, default `1.33`) → `GpuWaterParams::timing[3]` (`crates/renderer/src/vulkan/water.rs`), filled from `mat.ior` in `byroredux/src/render/water.rs`. Sibling writer: `crates/renderer/shaders/caustic_splat.comp`.
- **Status**: **NEW.** No open issue (searched `ior`, `1.33`, `caustic`). Not raised by the 2026-09-04 `water-deep` run, which examined this block for its bounds guard (`REN-WD-D2-01`, now fixed as #3820) rather than its eta.
- **Description**: The two caustic writers were deliberately aligned on everything else — the `CAUSTIC_FIXED_SCALE` fixed-point basis, the normalised 5×5 footprint, the `sunDirection` points-to-the-sun convention (`#1635`/`#1459`), `offsetRayOriginForDirection`'s zero-`tMin` origin contract, and (as of #3820) the `imageSize`-based bounds rule. They are **not** aligned on where the refractive index comes from, and the glass side is the one that does it correctly:

  ```glsl
  // caustic_splat.comp — reads the per-draw value, falls back to the pipeline default
  float instanceIor = instances[instIdx].ior;
  float ior = instanceIor > 1.0 ? instanceIor : causticTune.y;

  // water.frag — primary refraction ray, authored value
  float ior = push.timing.w;
  float eta = viewFromPositiveSide ? (1.0 / max(ior, 1.0)) : max(ior, 1.0);

  // water.frag — caustic refraction ray, literal
  vec3 refractDir = refract(-sunDir, causticNormal, 1.0 / 1.33);
  ```

  `WaterMaterial::ior` is canonical, authorable WATAL state whose own doc says *"1.33 = clean water; bumping up to 1.5 for stylised reads or thick visc fluid"* — i.e. the field exists precisely to be varied. The block's comment (*"η = 1.0/1.33 (air → water)"*) reads as a restatement of the default, not as a deliberate decision to ignore the authored value; nothing nearby argues for independence, and the same block already reuses `sunVisibility` computed further up rather than recomputing it.
- **Evidence**: `grep -n "ior\|1\.33" crates/renderer/shaders/water.frag` → `float ior = push.timing.w;` and `1.0 / max(ior, 1.0)` in the refraction block, `1.0 / 1.33` in the caustic block. `grep -n "ior" crates/core/src/ecs/components/water.rs` → `pub ior: f32` with `ior: 1.33` in `Default`. `grep -rn "ior" byroredux/src/render/water.rs` → `mat.ior` into `timing`. No cell-loader or EXAL site currently writes `WaterMaterial::ior`, so the two agree at runtime today. The Rust↔GLSL agreement of the record itself is separately pinned by `gpu_water_params_rust_and_glsl_copies_stay_in_lockstep` — the *record* is guarded, only its consumer diverges.
- **Impact**: Latent. Any authored or tuned water IOR ≠ 1.33 makes the caustic pattern on the lake bed refract at a different angle than the visible refraction of the same surface — the caustic focus and the seen-through geometry disagree, which reads as the caustic being registered to the wrong place rather than as a colour/intensity error. This is also exactly the failure mode `86976f56` (#3912) swept for the glass defaults one day earlier: a canonical constant plumbed to one consumer and hardcoded at another.
- **Related**: #3912 / `86976f56` (the named-default doctrine this violates), #3745 (`887c5d18`, which consolidated `water.frag`'s three RT reach budgets into `shader_constants_data.rs` — the precedent for removing literals from this shader), #1210 / #1255 (the water caustic phases), #3820 (the last time the two writers were brought into agreement).
- **Suggested Fix**: One-line change — `refract(-sunDir, causticNormal, 1.0 / max(ior, 1.0))`, reusing the `ior` local already in scope from line ~624, and drop the `1.33` from the comment. Pinnable by the `water.rs` source-assertion test style already used for the `#3820` bound (`crates/renderer/src/vulkan/water.rs` has a matching test for the `imageSize` rule); a negative assertion that `water.frag` contains no `1.0 / 1.33` would be the direct guard. Requires a `.spv` recompile.

---

### REN-2026-09-06-D16-02: `screen_scaled_reservation_bytes` claims to cover the caustics pass but reserves only its glass half — the water accumulator has no published per-pixel constant at all


- **Severity**: LOW (VRAM-accounting accuracy; effect is a BLAS eviction threshold set ~1/3 of the omitted bytes too high, never a correctness or leak issue)
- **Dimension**: Volumetrics / Memory (the #3839 reservation)
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs` — `screen_scaled_reservation_bytes` and its doc comment; consumed by `AccelerationManager::recompute_blas_budget` (`acceleration/memory.rs`) and `blas_budget_for_heap`. Constants: `CAUSTIC_BYTES_PER_PIXEL` (`crates/renderer/src/vulkan/caustic.rs`), and the **absent** counterpart in `crates/renderer/src/vulkan/water_caustic.rs`.
- **Status**: **NEW.** Distinct from **#3866** and **#3842**, which are doc-rot findings about the *old* `heap / 3` wording and the orphaned/renamed `compute_blas_budget` doc block — both filed 2026-09-05 and both verified still open and still accurate at HEAD (`memory-budget.md`'s Reserve-floors row and `MIN_BLAS_BUDGET_BYTES`'s docstring both still say `heap / 3`; **not re-filed here**). This finding is about the reservation's *arithmetic input set*, not its documentation.
- **Description**: `fa5c4191` (#3839) replaced the fixed `heap / 3` BLAS budget with `(heap − screen_scaled_reservation_bytes(render_extent, volumetrics)) / 3`, re-derived on every swapchain recreate. Its doc states the design rule: *"Derived from each pass's own published per-unit constant and the shared `froxel_extent` helper — never a hand-copied figure, so a pass that re-sizes itself moves this with it. Covers the three passes that scale with render resolution and dominate the fixed floor."*

  Two of the three constants hold up exactly. The caustics one does not, in a way the naming actively hides: `CAUSTIC_BYTES_PER_PIXEL = 4 * CAUSTIC_COLOR_LAYERS * MAX_FRAMES_IN_FLIGHT` = **24 B/px**, and its own docstring scopes it to *"bytes per pixel **this pipeline** allocates"* — the glass accumulator only. The water-side accumulator (`WaterCausticAccum`, a second full-render-extent `R32_UINT` image per FIF, `.array_layers(1)` → **8 B/px**) is a fourth screen-scaled resident allocation and contributes nothing to the reservation. `memory-budget.md` publishes the pair as **32 B/px combined**, and `caustic.rs`'s own `caustic_bytes_per_pixel_matches_documented_memory_budget` asserts `CAUSTIC_BYTES_PER_PIXEL + WATER_BYTES_PER_PIXEL == 32` — but `WATER_BYTES_PER_PIXEL` is declared **inside that test function**, so the value the reservation would need does not exist outside it.

  TAA (16 B/px), SSAO (2 B/px) and bloom (~5.3 B/px) are likewise absent. Those are defensible under the fn's "deliberately a floor, not a complete VRAM census" caveat; the caustics half is not, because the fn reads a constant that *looks* like it covers the pass and does not.
- **Evidence**:
  ```rust
  // predicates.rs — screen_scaled_reservation_bytes
  froxel_bytes
      .saturating_add(pixels.saturating_mul(u64::from(SVGF_BYTES_PER_PIXEL)))
      .saturating_add(pixels.saturating_mul(u64::from(CAUSTIC_BYTES_PER_PIXEL)))
  ```
  ```
  $ grep -rn "WATER_BYTES_PER_PIXEL" crates/renderer/src
  crates/renderer/src/vulkan/caustic.rs:1431:  const WATER_BYTES_PER_PIXEL: u32 = 4 * super::MAX_FRAMES_IN_FLIGHT as u32;   // test-local
  ```
  Independently recomputed against the live allocators: froxel `FROXEL_BYTES_PER_SLOT = 44` ✓ (`5×RGBA16F + 1×R32F`, and `assert_eq!(FROXEL_BYTES_PER_SLOT, 5 * 8 + 4)` holds); `SVGF_BYTES_PER_PIXEL = 40` ✓ (`4 + 8 + 4 + 4`, ×2 FIF); `CAUSTIC_BYTES_PER_PIXEL = 24` ✓ **for `caustic.rs` alone**.
- **Impact**: The reservation under-counts by 8 B/px (water caustics) — ~16.6 MB at 1080p render extent, ~66 MB at native 4K — plus ~23 B/px of TAA/SSAO/bloom. After the `/3`, the BLAS budget lands ~7 MB too high at 1080p and ~27 MB too high at native 4K from the water half alone (~21 MB / ~85 MB including the other three). Against `MIN_BLAS_BUDGET_BYTES = 256 MB` and a 12 GB dev card this is noise; on the 6 GB minimum-spec card the issue's own worked example targets, eviction fires that much later than intended. No correctness or leak consequence either way.
- **Related**: #3839 (the reservation), #2679 / PERF-D3-03 (which established `CAUSTIC_BYTES_PER_PIXEL` and its doc pin — and made the glass/water split explicit), #3866 / #3842 (the stale `heap / 3` doc sites — already filed, not re-reported).
- **Suggested Fix**: Promote the test-local `WATER_BYTES_PER_PIXEL` into a published `pub(crate) const WATER_CAUSTIC_BYTES_PER_PIXEL` on `water_caustic.rs` (derived from `CAUSTIC_FORMAT`'s 4 B and `MAX_FRAMES_IN_FLIGHT`, mirroring how `caustic.rs` derives its own), have `caustic_bytes_per_pixel_matches_documented_memory_budget` import it instead of redeclaring it, and add it to `screen_scaled_reservation_bytes`. If TAA/SSAO/bloom are to stay out, reword the doc from *"Covers the three passes that scale with render resolution"* to name exactly what is in and what is deliberately out — the current wording is what makes the caustics half look complete.

### REN-2026-09-06-D17-03: `disneyDiffuseSplit`'s doc block still describes two call sites; the fallback-directional arm it names was deleted


- **Severity**: LOW
- **Dimension**: Disney BSDF
- **Location**: `crates/renderer/shaders/include/pbr.glsl` (the `disneyDiffuseSplit` doc block)
- **Status**: NEW
- **Description**: The split-return rationale reads "The two call sites (fallback-directional and per-light loop) need to compose them with different PI scales because the per-light loop carries a `kD * albedo` (no /PI) legacy convention." There is now exactly **one** call site. `lighting.glsl`'s own else-arm comment records the change ("Directional, point and spot sources all arrive through this function; `triangle.frag` no longer carries a duplicate synthetic no-light sun/BRDF arm"), and `disney_sheen_keeps_its_relative_weight_in_canonical_direct_path` actively asserts the second site's *absence* (`!frag.contains("diffuseBrdf = (dd.diffuse + dd.sheen)")`). The struct return is still the right shape — `disneyDiffuseSplit`'s consumer does need diffuse and sheen separable, and the doc is the only place the π-scaling contract is written down — so the fix is to restate the rationale against the surviving single site, not to collapse the struct.
- **Evidence**: `grep -rn "disneyDiffuseSplit(" crates/renderer/shaders/` returns one call (`lighting.glsl`) plus the definition.
- **Impact**: Documentation only. It misleads a reader into thinking a second composition convention exists that must be kept in step — which is how the `* PI` vs no-`* PI` asymmetry #2243 fixed got introduced in the first place. The `/audit-renderer` skill's Dimension 17 text has already absorbed the stale claim verbatim ("the normalized direct-sun path (`triangle.frag`, which keeps `(dd.diffuse + dd.sheen) * (1.0 - metalness)`)"), so it is propagating.
- **Related**: #1252 (the split return), #2243 (the π-scaling fix), #3868 (`triangle.frag`'s own present-tense doc rot). Adjacent instance worth folding into the same edit: `triangle.frag`'s RT-glass block still says "Glass has IOR ≈ 1.5 (soda-lime, window glass, drinking glass)" thirty lines after the correct note that "Canonical glass carries `mat.ior = 1.45` (`GLASS_SURFACE_BEHAVIOR`)".
- **Suggested Fix**: Rewrite the paragraph to name the single surviving call site and state why the struct is still the right return shape (the caller must apply `* PI` to the *sum*, so the two lobes must arrive separable). Update the skill's Dimension 17 bullet in the same pass.

---

### REN-2026-09-06-D17-04: both #2243/#2244 regression guards named in the Dimension 17 checklist point at the wrong test file


- **Severity**: LOW
- **Dimension**: Disney BSDF
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 17 checklist)
- **Status**: NEW
- **Description**: The checklist cites `disney_sheen_keeps_its_relative_weight_in_canonical_direct_path` and `bounded_path_converts_dalc_irradiance_to_environment_radiance` as living in `gpu_instance_layout_tests.rs`. Both are in `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`. The path gate cannot catch this: `gpu_instance_layout_tests.rs` **does** exist, so the backtick resolves; only the symbol→file association is wrong. `.claude/issues/2472/ISSUE.md` carries the same wrong anchor with a line number attached (`gpu_instance_layout_tests.rs:1148`).
- **Evidence**: `grep -rn "disney_sheen_keeps_its_relative_weight_in_canonical_direct_path\|bounded_path_converts_dalc_irradiance_to_environment_radiance" .` returns only `shader_contract_tests.rs` (plus the skill, the prior audit report, and the issue file).
- **Impact**: An auditor working the Dimension 17 checklist opens the named file, does not find the guard, and must either conclude the guard was deleted (a false regression report) or re-derive the location. This is the exact TD7-* stale-anchor class `_audit-common.md`'s path-reference convention exists to prevent, in its one form the validate script's advisory cannot flag.
- **Related**: `.claude/commands/_audit-common.md` "Path-Reference Convention (post-#1114)", `.claude/commands/_audit-validate.sh`, #3047 (the same drift in the shader-includes list).
- **Suggested Fix**: Repoint both citations to `shader_contract_tests.rs` in `SKILL.md`, and drop the line number from `.claude/issues/2472/ISSUE.md`. Consider extending `_audit-validate.sh`'s backticked-symbol advisory to also check that a symbol named in the same sentence as a `*_tests.rs` path is actually defined in that file.

---

### REN-2026-09-06-D18-02: `sample_dalc_cube`'s TOD-fold justification cites a line range that holds unrelated content and a parser rule that does not exist


- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `byroredux/src/systems/weather.rs` (`sample_dalc_cube`); cited target `crates/plugin/src/esm/records/weather.rs`
- **Status**: NEW
- **Description**: `sample_dalc_cube`'s doc explains why it folds the six sky
  TOD slots onto DALC's four: *"fold high_noon→day and midnight→night per the
  WTHR parser's on-disk padding rule (`crates/plugin/src/esm/records/weather.rs:312-314`)"*.
  Both halves of that citation fail:

  1. **The line range is stale.** `weather.rs:310-320` today is the
     `cloud_textures` / `skyrim_cloud_textures` field documentation
     (*"Cloud texture paths. FNV/FO3 ship 4 layers (DNAM/CNAM/ANAM/BNAM …)"*) —
     nothing about DALC or TOD padding.
  2. **The named rule does not exist.** The parser has no high_noon→day /
     midnight→night mapping anywhere. The only DALC slot logic is the
     truncated-record backfill at the end of `parse_weather` (*"Fill missing
     slots with the most recent one … defensive against truncated mod
     records"*), which is about absent sub-records, not about mapping six
     slots onto four. The `SkyrimAmbientCube` doc and the
     `skyrim_dalc_per_tod` field doc both just state "4 entries: sunrise / day /
     sunset / night".

  The fold is a *consumer-side* decision made in `sample_dalc_cube` itself and is
  perfectly defensible — it just has no upstream authority, and the comment
  claims one.
- **Evidence**: `sed -n '305,320p' crates/plugin/src/esm/records/weather.rs`
  returns cloud-texture field docs; `grep -n "DALC" crates/plugin/src/esm/records/weather.rs`
  finds no TOD-fold logic, only `SKYRIM_DALC_SIZE`, `parse_skyrim_dalc`, and the
  backfill loop.
- **Impact**: Doc-rot in a load-bearing spot — a reader checking whether the
  fold is correct is sent to the wrong file region and told a non-existent
  parser rule sanctions it. This is precisely the line-anchor rot the audit
  discipline warns about, sitting in production code rather than an audit report.
- **Related**: #993 (DALC TOD interpolation), #2816 (the cross-fade fix that
  extracted this helper), `flat_dalc_cube`.
- **Suggested Fix**: Point the comment at `SkyrimAmbientCube`'s own doc (the
  "4 entries: sunrise / day / sunset / night" statement) by symbol, not line
  number, and reword the justification as a consumer-side mapping decision
  rather than a parser rule.

---

### REN-2026-09-06-D18-03: WTHR `DATA` bytes 1–2 are the only bytes #3883's naming pass left unnamed, and the one in-tree claim about them cites a decoder that never reads them


- **Severity**: LOW
- **Dimension**: Sky/Weather
- **Location**: `crates/plugin/src/esm/records/weather.rs` (`parse_weather_data`, the `WTHR_*_OFFSET` block, `SKYRIM_DATA_SIZE`); claim site `byroredux/src/scene/cloud_tile_scale_tests.rs` (module doc)
- **Status**: NEW
- **Description**: `e6d2811e` (#3883) replaced `parse_weather_data`'s bare
  literals with ten named `WTHR_<FIELD>_OFFSET` constants covering offsets
  3, 4, 5, 6, 8, 10, 11, 12, 15, 17, 18, plus byte 0 via `data.first()`. Bytes 1
  and 2 are skipped entirely — neither named, nor read, nor documented as
  reserved, in either the constant block or the function's doc comment. The
  offset block's own stated purpose (*"the byte layout … is stated once instead
  of scattered across twenty ungreppable integers"*) leaves a two-byte hole with
  no statement at all.

  The tree does contain a claim about them, but it is unverifiable in place:
  `cloud_tile_scale_tests.rs`'s module doc asserts *"DATA bytes 1-2 are
  cloud_speed_lower / cloud_speed_upper, NOT scales — see `weather.rs` DATA
  arm"*. The DATA arm it points to has never decoded those bytes, and
  `WeatherRecord` has no `cloud_speed_*` field (the old `cloud_speeds: [u8; 4]`
  was a *DNAM* mis-decode removed by #535 — `parse_wthr_dnam_is_texture_path_not_speeds`
  pins that it is gone). So the citation resolves to nothing either way.
- **Evidence**:
  - `parse_weather_data` goes from `data.first()` (offset 0) straight to
    `WTHR_TRANSITION_DELTA_OFFSET = 3`.
  - `grep -rn "cloud_speed" --include="*.rs"` finds exactly one live hit outside
    tests/issue archives: the `cloud_tile_scale_tests.rs` module doc.
  - Per-layer cloud motion for FO3/FNV comes from `ONAM` and for Skyrim from
    `RNAM`/`QNAM` (`WeatherRecord::cloud_layer_velocities`), so if bytes 1–2 do
    hold cloud speeds they are a second, unread source; if they do not, the
    comment is wrong. Either way the tree currently answers "unknown".
- **Impact**: Low today — no consumer depends on those bytes. But it is a
  two-byte gap in the one place the record's layout is now supposed to be
  stated authoritatively, propped up by a cross-reference that cannot be
  followed. Given that the same function's doc records one prior layout
  correction (*"byte 10 is thunder/lightning frequency and byte 11 is the
  classification bitmask (not byte 14)"*), an unstated hole is the shape a
  future correction gets wrong.
- **Related**: #3883 (`e6d2811e`), #535 (the DNAM `cloud_speeds` mis-decode),
  #529 (`cloud_tile_scale_for_dds`), REN-2026-09-06-D18-01.
- **Suggested Fix**: Resolve the two bytes against xEdit's shared WTHR `DATA`
  definition and either name them (`WTHR_UNKNOWN_1_OFFSET` / the real field
  names) with a one-line doc, or state explicitly in the offset block that
  bytes 1–2 are unread and why. Then correct or delete the
  `cloud_tile_scale_tests.rs` claim so it stops citing a decoder that does not
  contain the answer.

---

### REN-2026-09-06-D19-01: the terrain-splat normal-map loop takes its implicit-LOD sample under a per-fragment `continue` — the last unswept instance of #3622's class


- **Severity**: LOW
- **Dimension**: Tangent-Space
- **Location**: `crates/renderer/shaders/triangle.frag` (the `terrainSplatActive` normal-perturbation loop), `crates/renderer/shaders/include/material_sampling.glsl` (`perturbNormal`)
- **Status**: NEW
- **Description**: The LAND TX01 loop skips layers per fragment and then calls `perturbNormal`, whose normal-map fetch uses the implicit-derivative form:

  ```glsl
  for (uint i = 0u; i < 8u; ++i) {
      float w = terrainSplat[i / 4u][i & 3u];
      uint layerNormalIdx = terrainTile.layerNormalIndex[i];
      if (w <= 0.0 || layerNormalIdx == 0u) continue;
      vec3 layerNormal = perturbNormal(terrainGeometryNormal, fragWorldPosRel, sampleUV, layerNormalIdx, fragTangent);
      …
  }
  ```
  `perturbNormal` opens with `texture(textures[nonuniformEXT(normalMapIdx)], uv)`. `layerNormalIdx` is tile-uniform, but `w` is a per-fragment interpolated splat weight, so at any splat boundary one lane of a quad executes the fetch on iteration *k* while its neighbour `continue`s — implicit-LOD sampling in non-uniform control flow, the class `parallaxDisplaceUV` was converted away from under #3622 and that `ray_hit.glsl`'s `resolveRayHitUV` has always avoided.
- **Evidence**: `#3622`'s own comment in `material_sampling.glsl` states the rule and names the two paths that were fixed; this third path was not swept. Terrain vertices carry `tangent: [1.0, 0.0, 0.0, -1.0]` (`crates/renderer/src/vertex.rs`, pinned by `terrain_vertex_carries_a_nonzero_tangent`), so `perturbNormal` takes Path 1 and the `dFdx(worldPos)` fallback is *not* additionally exposed here — the implicit texture LOD is the sole exposure.
- **Impact**: Spec-undefined mip selection on exterior LAND normal maps at layer boundaries; would read as inconsistent terrain-relief sharpness along splat seams. **Practically inert today** and reported as hardening, not as an observed artefact: unlike the POM marcher — whose `currentUV` genuinely changed per divergent iteration — `sampleUV` here is computed once in quad-uniform flow and is unchanged in every lane at the fetch, so the derivative real hardware computes is the correct one. It is filed because the project has already decided this class is worth closing, the fix is one hoisted line, and leaving one instance behind makes the rule look optional.
- **Related**: #3622 (REN-2026-08-30-D19-03), #3902 (the sibling secondary-ray role gap).
- **Suggested Fix**: Capture the mip level once before the loop — the enclosing `if (terrainSplatActive && (dbgFlags & DBG_BYPASS_NORMAL_MAP) == 0u)` is quad-uniform, so a `textureQueryLod(…, sampleUV).x` there is well defined — and give `perturbNormal` an explicit-LOD sibling (or an optional `lod` parameter defaulting to the implicit path) exactly as `sampleParallaxHeight` took one under #3622.

---

### REN-2026-09-06-D2-02: `giHitIrradiance` has been dead shader code since 2026-07-29, and yesterday's `GI_VISIBLE_LIGHT_CAP` promotion pinned a CPU-side contract constant to its only (unreachable) consumer


- **Severity**: LOW
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/shaders/include/lighting.glsl` —
  `giHitIrradiance` (definition) and its `GI_VISIBLE_LIGHT_CAP` loop bound.
  Live siblings: `pathHitRadiance` (same file, `visibleLightLimit` parameter)
  and `reflectionHitIrradiance`. Caller: `crates/renderer/shaders/triangle.frag`,
  the `pathHitRadiance` call in the GI path. Constant:
  `crates/renderer/src/shader_constants_data.rs` (`GI_VISIBLE_LIGHT_CAP`),
  emitted into `crates/renderer/shaders/include/shader_constants.glsl`. Stale
  comments: `crates/renderer/shaders/include/raytrace.glsl` (the
  `reflectionHitIrradiance` prototype block) and `triangle.frag`'s
  refraction-terminus lighting comment.
- **Status**: **NEW.** Verified against code, not against GitHub (the issue
  cache is open-only). `grep -rn giHitIrradiance crates/renderer/shaders`
  returns exactly three hits: the definition and two comments. No call site.
- **Description**: `f8efde63` (2026-07-29, "bounded material-aware path-traced
  GI") replaced `triangle.frag`'s last `giHitIrradiance` call with
  `pathHitRadiance`; `6c56e311` (2026-07-19) had already moved the refraction
  terminus onto `reflectionHitIrradiance`. `giHitIrradiance` — a 52-line
  candidate-selection + visibility-ray function inside the main fragment
  shader's include chain — has had **no caller for ~5½ weeks**.

  It is also the **sole consumer** of `GI_VISIBLE_LIGHT_CAP`. Yesterday's
  `78cc7a41` (#3879/#3880) identified that constant as "the one genuine
  duplicate" its widened declaration gate exposed and promoted it into
  `shader_constants_data.rs` → the generated header, i.e. into the Rust↔GLSL
  contract. The promotion is therefore anchored to unreachable code, while the
  **live** instance of the same "first two visible contributors" number —
  `uint visibleLightLimit = shadedHits == 0 ? 2u : 1u;` at the `pathHitRadiance`
  call site in `triangle.frag` — remains a bare literal. The gate #3880 widened
  matches *declarations* (`const T` and object-like `#define`); a value inlined
  into an expression is invisible to it by construction, so it cannot close
  this last copy.

  Two comments assert the retired split and are now false: `raytrace.glsl`'s
  prototype block says *"Diffuse GI and refraction termini retain the wider
  locally-selected light set in `giHitIrradiance`"* — the refraction terminus
  calls the deliberately-narrow `reflectionHitIrradiance` (which returns after
  the **first** visible light) and the GI bounce calls `pathHitRadiance`; and
  `triangle.frag`'s refraction-terminus comment names `giHitIrradiance` as the
  function it shares its evaluation with.
- **Evidence**: `grep -rn "giHitIrradiance\|pathHitRadiance(\|reflectionHitIrradiance("
  crates/renderer/shaders` → `pathHitRadiance` one call site (`triangle.frag`,
  GI path), `reflectionHitIrradiance` two (`triangle.frag` refraction terminus +
  `raytrace.glsl`'s own hit shading, plus the prototype), `giHitIrradiance`
  **zero**. `git log -S "giHitIrradiance(" -- crates/renderer/shaders/triangle.frag`
  ends at `f8efde63` (2026-07-29). `GI_VISIBLE_LIGHT_CAP` appears in
  `lighting.glsl` once (`giHitIrradiance`'s `visibleCount >=` bound), in the
  generated header, and in `shader_constants_data.rs`.
- **Impact**: No runtime effect — dead code plus three misleading contract
  statements. The cost is auditing and maintenance: a reader tracing the GI
  light budget lands on the wrong function, and a maintainer retuning
  `GI_VISIBLE_LIGHT_CAP` in `shader_constants_data.rs` will change nothing on
  screen while the live cap sits in `triangle.frag` as `2u`. That is the exact
  silent-no-op failure mode #3879 was filed to remove.
- **Related**: #3879/#3880 (`78cc7a41`, the promotion); `f8efde63` (the commit
  that orphaned the function); #3868 (`triangle.frag` present-tense comments
  describing retired pipeline stages — same class).
- **Suggested Fix**: Delete `giHitIrradiance`, and either delete
  `GI_VISIBLE_LIGHT_CAP` with it or — better — repoint it at the live copy by
  replacing `triangle.frag`'s `shadedHits == 0 ? 2u : 1u` with
  `shadedHits == 0 ? GI_VISIBLE_LIGHT_CAP : 1u`, which makes the promoted
  constant load-bearing instead of decorative. Correct the two comments to name
  `pathHitRadiance` / `reflectionHitIrradiance`.

---

### REN-2026-09-06-D2-03: the unsafe-vertex-lane guard covers 2 of the 4 shaders that read a raw-float vertex SSBO, with no completeness half — the same shape as #3829


- **Severity**: LOW
- **Dimension**: SSBO/Indexing (test gap)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`
  — `rt_hit_shaders_have_no_unsafe_vertex_data_reads` and its `sources` array.
  Uncovered readers: `crates/renderer/shaders/caustic_splat.comp`
  (`getCausticHitTriWorldPositions`, its own `GlobalVertices` at set 0 binding 9)
  and `crates/renderer/shaders/volumetrics_inject.comp` (`boundaryVertexData`,
  binding 20). The invariant it enforces is documented on the `GlobalVertices`
  block in `crates/renderer/shaders/include/bindings.glsl` (#575 / SH-1).
- **Status**: **NEW.** `docs/audits/AUDIT_RENDERER_2026-08-14.md` names the test
  as present but does not examine its source list. No open issue matches.
- **Description**: The `Vertex` layout has six float lanes that are **not**
  IEEE-754 floats — bone indices (12–15, `u32` bits) and splat weights (20–21,
  packed 4× `u8` unorm) — and reading them as `vertexData[base + N]` yields
  NaN/denormal garbage. `rt_hit_shaders_have_no_unsafe_vertex_data_reads` is the
  static guard for that, and its `sources` array is a hardcoded two-entry list:
  `triangle.frag` and `include/ray_hit.glsl`. Two other shaders index a raw
  `float …[]` vertex SSBO with `VERTEX_STRIDE_FLOATS`-style arithmetic and are
  outside it. Unlike its sibling `gpu_instance_glsl_copies_stay_in_lockstep`,
  which pairs its hardcoded `SOURCES` with `assert_mirror_list_is_complete`,
  this test has **no discovery half** — nothing fails when a new shader starts
  reading the vertex buffer.

  **No live unsafe read exists today.** `caustic_splat.comp` reads only lanes
  0–2 (position) plus its skinned `buffer_reference` output;
  `volumetrics_inject.comp` reads only `base0..base0+2`. `triangle.vert` and
  `skin_vertices.comp` reach bone indices through `floatBitsToUint`, correctly.
  This is a coverage gap, not a defect — but it is structurally identical to
  #3829, where a hardcoded shader list with no completeness half let
  `volumetrics_inject.comp` (the same file) drift outside a GPU-layout contract
  for 13 days and cost a CRITICAL.
- **Evidence**: The test body's `let sources = [("triangle.frag", …),
  ("ray_hit.glsl", …)];` — two entries, and no `include_dir`/discovery
  assertion anywhere in the function. `grep -rln "vertexData\[" crates/renderer/shaders`
  → `caustic_splat.comp`, `include/ray_hit.glsl`, `include/bindings.glsl`;
  `grep -rn "float vertexData\[\]\|float boundaryVertexData\[\]" ` adds
  `volumetrics_inject.comp`.
- **Impact**: A future hit-fetch site added to `caustic_splat.comp` or
  `volumetrics_inject.comp` — e.g. reading a per-vertex splat weight for a
  terrain-aware caustic or boundary material — would silently reinterpret a
  packed `u32` as a float and produce NaN, with the guard green. The failure
  mode is exactly the one the `bindings.glsl` WARNING block calls "the
  pit-of-failure guardrail".
- **Related**: #575 / SH-1 (the invariant); #3829 /
  `REN-2026-09-05-D2-01` (the same hardcoded-list-without-a-completeness-half
  shape, same file, CRITICAL outcome); `gpu_boundary_instance_stride_matches_gpu_instance`
  and `assert_mirror_list_is_complete` (the two patterns that do it right).
- **Suggested Fix**: Replace the hardcoded pair with a recursive walk of
  `crates/renderer/shaders/` (`.frag`/`.vert`/`.comp`/`.glsl`) that scans every
  file declaring or indexing a raw-float vertex SSBO — `78cc7a41` already built
  exactly that walker for the #3880 constant gate, so the traversal can be
  reused rather than rewritten. Failing that, add
  `caustic_splat.comp` + `volumetrics_inject.comp` to `sources` **and** a
  completeness assertion that every file matching `float \w*[Vv]ertex\w*\[\]` is
  in the list.

---

### REN-2026-09-06-D2-04: `shader-pipeline.md`'s descriptor table credits `caustic_splat` and `volumetrics` with Set-1 (and bindless Set-0) bindings that their pipeline layouts do not contain — and self-contradicts its own following paragraph


- **Severity**: LOW
- **Dimension**: SSBO/Indexing (authoritative-doc divergence)
- **Location**: `docs/engine/shader-pipeline.md`, the `## Descriptor Sets`
  table — the "Used by" column of rows `0|0`, `0|1`, `1|0`, `1|1`, `1|2`, `1|4`.
  Ground truth: `crates/renderer/src/vulkan/caustic.rs` (the
  `PipelineLayoutCreateInfo` for the caustic compute pipeline) and
  `crates/renderer/shaders/caustic_splat.comp`'s own `layout(set = 0, …)`
  declarations.
- **Status**: **NEW.** Distinct from #3830, which is the *volumetrics binding
  table* further down the same page; this is the Set-0/Set-1 table above it.
  Precedent for the class: `REN-2026-08-30-D2-03` (same table, since fixed).
- **Description**: `CausticPipeline`'s pipeline layout is built with
  `set_layouts(std::slice::from_ref(&partial.descriptor_set_layout))` — **one**
  descriptor set layout, its own private set 0 with bindings 0–10
  (`depthTex`, `normalTex`, `meshIdTex`, its own `LightBuffer`, its own
  `CameraUBO`, its own `InstanceBuffer`, TLAS, `causticAccum`, `CausticParams`,
  and its own `GlobalVertices`/`GlobalIndices`). It binds neither the global
  bindless set 0 nor the scene set 1. The table nonetheless lists
  `caustic_splat` under Set 1 bindings 0, 1 and 4, and `caustic` under Set 0
  bindings 0 and 1.

  The volumetrics entries are worse than merely wrong — they are contradicted
  three paragraphs later by the page's own prose: *"Volumetrics uses its own
  private `set = 0` layout … neither binds any Set-1 resource above."* Yet the
  table credits `volumetrics` with Set-1 bindings 1 and 2 and Set-0 binding 0.
- **Evidence**: `grep -n "set_layouts(" crates/renderer/src/vulkan/caustic.rs`
  → a single-element slice at the pipeline-layout site.
  `grep -n "set = 1, binding" crates/renderer/shaders/caustic_splat.comp` →
  nothing; every declaration in that file is `set = 0`. `caustic_splat.comp`
  declares its own `struct GpuInstance` and its own `GlobalVertices`/
  `GlobalIndices` precisely *because* it cannot see set 1 — which is what its
  own header comment ("caustic uses its own descriptor set") says.
- **Impact**: Audit-methodology and onboarding only, no runtime effect — but
  this is the table every audit is instructed to prefer over re-deriving
  descriptor facts from source, and it is the table that answers "which
  pipelines must be re-bound when Set 1 changes". An engineer widening a Set-1
  binding would look here and conclude that `caustic_splat` and `volumetrics`
  need updating (they don't), or — the more dangerous direction — that
  `caustic_splat`'s instance reads are covered by the Set-1 lockstep guards
  (they aren't; that is a separate private mirror, and it is the class of
  mistake that produced #3829).
- **Related**: #3830 (`volumetrics_inject.comp` binding table on the same
  page); `REN-2026-08-30-D2-03` (the previous divergence in this same table);
  #3829 (the CRITICAL that turned on exactly this "which pipelines actually see
  Set 1" question).
- **Suggested Fix**: Correct the six "Used by" cells to name only the pipelines
  whose `VkPipelineLayout` actually includes that set, and add a one-line note
  to the caustic row equivalent to the volumetrics one ("`caustic_splat.comp`
  uses its own private `set = 0` layout with its own `GpuInstance` mirror and
  vertex/index SSBOs"). The `shader_contract_tests.rs` `include_str!`-the-doc
  pattern (`froxel_grid_cost_matches_the_memory_budget_doc`) could pin the
  caustic row against `caustic.rs`'s set count.

---

### REN-2026-09-06-D2-05: the audit's own Dimension-2 anchors have rotted — `_audit-common.md`'s `context/` layout row lists 10 of 18 files, and the BC1 bullet points at `draw.rs` for a bit that now lives elsewhere


- **Severity**: LOW
- **Dimension**: SSBO/Indexing (audit infrastructure / doc-rot)
- **Location**: `.claude/commands/_audit-common.md`, the
  `VulkanContext: crates/renderer/src/vulkan/context/` layout row;
  `.claude/commands/audit-renderer/SKILL.md`, the Dimension-2 BC1
  punch-through bullet ("The CPU bit is set in `draw.rs` from `format_has_alpha`").
  Ground truth: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`
  (`INSTANCE_FLAG_DIFFUSE_ALPHA`), and the 18-file `context/` directory.
- **Status**: **NEW.** Same class as, but a different row from, the open
  **#3847** (`_audit-common.md`'s `crates/sdk` layout row understates the crate
  ~50×) and **#3828 / DOC-ROT-1** (`extensions.rs` landed without a layout row
  and was invisible to audit-suite routing).
- **Description**: Two anchors this dimension is instructed to use no longer
  resolve to the code they name.

  1. `_audit-common.md`'s `context/` row enumerates *mod.rs, draw.rs, resize.rs,
     resources.rs, helpers.rs, screenshot.rs, geometry_pass.rs, post_passes.rs,
     skinned_blas_refit.rs, depth_capture.rs* — 10 files. The directory holds
     **18**. Missing: `init.rs` (1661 LOC), `build_and_upload_instances.rs`
     (1058), `assemble_camera_and_lights.rs` (614),
     `dispatch_skin_and_cluster.rs` (451), `teardown.rs` (423),
     `sync_and_acquire_frame.rs` (274), `begin_frame_recording.rs` (163),
     `render_debug.rs` (41). `init.rs`/`teardown.rs` landed 2026-08-23
     (`6fad32ac`, #1749); the other five landed 2026-09-02 (`7463204e`, #3282).
  2. Consequently the Dimension-2 checklist's BC1 bullet points at `draw.rs` for
     the CPU side of `INSTANCE_FLAG_DIFFUSE_ALPHA`. `grep -rn
     INSTANCE_FLAG_DIFFUSE_ALPHA crates/renderer/src` returns **no hit in
     `draw.rs`**; the flag is assembled in `build_and_upload_instances.rs`,
     one of the eight unlisted files. An auditor following the anchor finds
     nothing and must either re-derive it or record the bullet as
     unconfirmable.

  `build_and_upload_instances.rs` is the single largest Dimension-2-relevant
  CPU file — it is where `GpuInstance.flags`, `surface_id` and the instance
  upload that every `instance_custom_index` read depends on are assembled — and
  it is invisible to the routing table.
- **Evidence**: `ls crates/renderer/src/vulkan/context/` → 18 `.rs` files;
  `wc -l` as above. `git log --diff-filter=A` dates each addition to
  `6fad32ac` (2026-08-23) or `7463204e` (2026-09-02).
  `grep -rn "INSTANCE_FLAG_DIFFUSE_ALPHA" crates/renderer/src` → `shader_constants*.rs`,
  `scene_buffer/constants.rs`, `dds.rs`, and
  `context/build_and_upload_instances.rs`; `draw.rs` does not appear.
- **Impact**: Audit routing and verification only. But `_audit-common.md`'s own
  header calls this class out — *"Do not report full coverage without saying
  which of these you skipped"* — and #3828 is the precedent for what it costs: a
  10k-LOC file sat outside audit routing for weeks because no layout row named
  it. Eight files totalling ~4.7k LOC in the renderer's hottest directory are in
  that state now.
- **Related**: #3847 (same file, `crates/sdk` row); #3828 / DOC-ROT-1
  (`extensions.rs`); #3858 (the #3282 split's single-function files — tracks the
  *shape* of the split, not the missing layout row).
- **Suggested Fix**: Extend the `context/` layout row to all 18 files with a
  one-clause role for each, and repoint the Dimension-2 BC1 bullet at
  `build_and_upload_instances.rs`. `.claude/commands/_audit-validate.sh` cannot
  catch this — it checks that backticked paths *exist*, not that a directory
  listing is complete (and #3439 already records that it skips bare basenames
  entirely), so a directory-listing completeness check would be the structural
  fix.

---

### REN-2026-09-06-D20-01: #3628's second pin anchors on a conditionally-executed call and its hazard scan is blind to the file's own barrier idiom


- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/context/depth_capture.rs` (`capture_ordering_tests::record_copy_runs_immediately_after_the_depth_history_copy`); subject: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`'s frame tail), `crates/renderer/src/vulkan/context/post_passes.rs` (`copy_depth_to_history`)
- **Status**: NEW
- **Description**: Pin (a) of `229306ce` genuinely pins what it names (verified
  above). Pin (b) does not — it asserts two adjacent facts rather than the
  ordering invariant it describes.

  1. **Its stated rationale is false.** The assertion message reads
     *"depth_capture_record_copy must come AFTER copy_depth_to_history — it
     documents DEPTH_STENCIL_READ_ONLY_OPTIMAL as its precondition, and that
     layout is only guaranteed once the history copy's own barriers have run."*
     The history copy is wrapped in `if has_effect_soft_material { … }`, so it
     does not run at all on the common path. The actual guarantor is the main
     render pass's own `.final_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)`
     in `create_render_pass` (`crates/renderer/src/vulkan/context/helpers.rs`) —
     which `draw.rs`'s own comment at the call site states correctly
     (*"the render pass leaves the depth image in DEPTH_STENCIL_READ_ONLY_OPTIMAL …
     when the copy is skipped, the layout is already the precondition"*) and
     `copy_depth_to_history`'s doc in `post_passes.rs` also states
     (*"`DEPTH_STENCIL_READ_ONLY_OPTIMAL` (the render pass's final layout)"*).
     Three sites agree; the pin's message is the outlier.
  2. **The guarded window is the wrong one.** The hazard scan covers
     `src[history_copy_pos..record_copy_pos]` — roughly the four lines from
     inside the `if` block to the capture call. The real hazard region is from
     the render pass end to `depth_capture_record_copy`; anything added before
     the `if has_effect_soft_material` block is outside the scan. A `memory_barrier(…)`
     call already sits there today (harmless — it is a global `VkMemoryBarrier`,
     no image layout), which shows the region is actively edited.
  3. **The hazard list misses the codebase's own barrier idiom.** It greps for
     the literal `cmd_pipeline_barrier`, `cmd_copy_image(`,
     `cmd_copy_image_to_buffer(`, `cmd_blit_image(`. `crates/renderer/src/vulkan/descriptors.rs`
     exports `memory_barrier(...)` plus eight `image_barrier_*` builders, and
     `draw.rs` uses `memory_barrier(...)` fifteen lines above the scanned window.
     A layout transition introduced through any of those helpers — the file's
     dominant style — passes the scan unchanged. `cmd_pipeline_barrier2` and
     `cmd_clear_depth_stencil_image` are likewise unlisted.
- **Evidence**: The `if has_effect_soft_material {` wrapper around
  `self.copy_depth_to_history(cmd);` in `draw.rs`; the `final_layout` call in
  `helpers.rs::create_render_pass`; the `memory_barrier` / `image_barrier_*`
  exports in `descriptors.rs` versus the four-string hazard array in the test.
- **Impact**: Documentation-and-test only. The invariant holds at HEAD. But the
  pin is the sole guard on a precondition whose violation surfaces as a
  validation-layer error or garbage `depth.stats` output rather than a build or
  `cargo test` failure — exactly what #3628 was filed to prevent — and it would
  not fire on the most likely way to break it.
- **Related**: #3628, #3308, #2484, #3570; `depth_format_guard_tests` (the
  sibling source-scan in the same file, which *is* tight).
- **Suggested Fix**: Anchor the scan on the render pass end
  (`cmd_end_render_pass`) rather than on the conditional history copy, widen
  the hazard list to include `memory_barrier(`, `image_barrier_`,
  `cmd_pipeline_barrier2`, and `cmd_clear_depth_stencil_image`, and rewrite the
  assertion message to name the render pass's `final_layout` as the guarantor.

---

### REN-2026-09-06-D20-02: #3570's D16 refusal has no channel back to its only consumer — `depth.stats` reports "armed, run again" forever on a device without `D32_SFLOAT`


- **Severity**: LOW
- **Dimension**: Debug/Telemetry
- **Location**: `crates/renderer/src/vulkan/context/depth_capture.rs` (`depth_capture_record_copy`), `crates/core/src/ecs/resources/mod.rs` (`DepthCaptureBridge`), `byroredux/src/commands/depth.rs` (`DepthStatsCommand`)
- **Status**: NEW
- **Description**: `26f9ddf4` correctly refuses to record a capture when
  `self.depth_format != vk::Format::D32_SFLOAT`, and does so *before* arming
  `depth_capture_pending_readback` — so the readback decode can never
  misinterpret a `D16_UNORM` buffer as `f32` samples. That half is sound.

  The refusal is reported only through `log::warn!`, and it happens *after*
  `self.depth_capture_requested.swap(false, Ordering::AcqRel)` has already
  consumed the request. `DepthCaptureBridge` carries exactly two channels —
  `requested: Arc<AtomicBool>` and `result: Arc<Mutex<Option<DepthCapture>>>` —
  with no way to express "refused". `DepthStatsCommand::execute` therefore takes
  the `None` result, re-arms, and returns *"depth capture armed — run
  `depth.stats` again in a frame or two to read it"*. On a device that selected
  `D16_UNORM`, every invocation forever returns that same line, with the only
  explanation buried in the renderer's log stream.
- **Evidence**:
  - `depth_capture_record_copy` swaps the request flag first, then
    `if self.depth_format != vk::Format::D32_SFLOAT { log::warn!(…); return; }`.
  - `find_depth_format` (`crates/renderer/src/vulkan/context/helpers.rs`)
    iterates `[vk::Format::D32_SFLOAT, vk::Format::D16_UNORM]` — the D16 arm is
    reachable, which is the whole premise of #3570 (Vulkan mandates D16 depth
    attachments; D32_SFLOAT is not guaranteed).
  - `DepthCaptureBridge` (`crates/core/src/ecs/resources/mod.rs`) has no error
    or refusal field, and `take_result()` cannot distinguish "not ready yet"
    from "will never be ready".
- **Impact**: Diagnostic UX only, and only on hardware without D32_SFLOAT depth
  attachments (not the dev RTX 4070 Ti). But #3308's step-2 comparison gate is
  exactly the sort of thing run on an unfamiliar machine, and the failure mode
  is an unbounded "come back later" with no visible cause — the same class of
  silent-dead-end #3570 was closing on the decode side.
- **Related**: #3570, #3308, `record_copy_refuses_non_d32_sfloat_before_arming_the_pending_readback`;
  `crates/renderer/src/vulkan/context/screenshot.rs` (the sibling bridge, whose
  owner tag gives it a place to report a claim outcome).
- **Suggested Fix**: Add a one-shot refusal channel to `DepthCaptureBridge`
  (e.g. `unsupported: Arc<AtomicBool>` set once by `depth_capture_record_copy`
  alongside the warn) and have `DepthStatsCommand` print the actual reason —
  "depth capture unsupported on this device (depth format is not D32_SFLOAT)" —
  instead of re-arming. Cheaper alternative: refuse *before* the
  `swap`, so the request stays pending and the state is at least inspectable.

---

### REN-2026-09-06-D21-01: the four `glass_*` `GpuMaterial` scalars are the harness's only shader-consumed material lanes with no `mat.set` arm


- **Severity**: LOW
- **Dimension**: Cornell Harness
- **Location**: `byroredux/src/commands/scene.rs` (`MatSetCommand::execute`), `byroredux/src/cornell.rs` (`glass`), `crates/renderer/src/vulkan/context/mod.rs` (`to_gpu_material`)
- **Status**: NEW
- **Description**: `mat.set`'s field table has been extended three times
  specifically to close "the Cornell harness cannot reach this live" gaps —
  #2477 (`material_flags`), #2514 (`subsurface`/`sheen`/`sheen_tint`/
  `anisotropic`), #2823 (the three translucency lanes). Four shader-consumed
  scalars remain unreachable, and they are precisely the ones that define the
  glass appearance the harness exists to bisect:

  - `glass_fresnel_color` → `GpuMaterial.glass_fresnel_{r,g,b}`
  - `glass_refraction_scale` → `GpuMaterial.glass_refraction_scale`
  - `glass_blur_scale` → `GpuMaterial.glass_blur_scale`
  - `glass_blur_scale_factor` → `GpuMaterial.glass_blur_scale_factor`

  All four are assigned verbatim in `to_gpu_material` and read by the shader
  (`DEFAULT_GLASS_REFRACTION_SCALE` / `DEFAULT_GLASS_BLUR_SCALE` in
  `crates/core/src/ecs/components/material.rs` are emitted as GLSL macros the
  shader divides by — see #3459). `cornell.rs`'s `glass()` constructor sets only
  `diffuse_color`, `material_kind`, `alpha`, and `GLASS_SURFACE_BEHAVIOR`
  (`roughness 0.10 / metalness 0.0 / ior 1.45`), leaving all four at their
  `Material::default()` values with no console path to sweep them.

  Note this is *not* a re-report of the glass-stipple / IGN refraction jitter
  observation — it is the reason that observation cannot be A/B'd from the
  harness in the first place.
- **Evidence**: `MatSetCommand::execute`'s `match field.to_ascii_lowercase()`
  arms cover metalness, roughness, alpha, glossiness, emissive_mult,
  specular_strength, env_map_scale, ior, subsurface, sheen, sheen_tint,
  anisotropic, the two translucency scalars, four colour lanes, `material_kind`
  and `material_flags` — no `glass_*` arm, and none of the four appears in the
  `USAGE` string. `to_gpu_material` assigns all four.
- **Impact**: Harness capability gap. Any glass-path bisect requires editing
  `cornell.rs` and rebuilding, which is the workflow #2477/#2514/#2823 each
  concluded was too slow to be used in practice.
- **Related**: #2477, #2514, #2823, #3459 (the shader/host constant sync for two
  of these four), `docs/engine/nifal.md` (BGEM v21+ glass authoring).
- **Suggested Fix**: Add `glass_refraction_scale`, `glass_blur_scale`,
  `glass_blur_scale_factor` (scalar) and `glass_fresnel_color` (vec3) arms to
  `MatSetCommand`, following the existing `ior` precedent of no range clamp with
  the `floats` finite check as the guard, and extend `USAGE`.

---

### REN-2026-09-06-D23-02: ROADMAP's bench tracker still asserts the FSR harness is "byte-stable since `34074b93`" — three commits have touched the two harness files, one of them changing the reporter's arithmetic


- **Severity**: LOW
- **Dimension**: FSR/Presentation (doc-rot, measurement integrity)
- **Location**: `ROADMAP.md`, the `R6a-stale-20` tracker entry — the phrase
  "**Harness still confirmed byte-stable**: no commit against
  `scripts/fsr-bench-matrix.sh` or `scripts/fsr_bench_report.py` since
  `34074b93`" in its **2026-09-01 (Session 77, HEAD `f9dd52b4`)** and
  **2026-09-03 (Session 79, HEAD `4d78dce6`)** fold paragraphs
- **Status**: NEW
- **Description**: The claim was true when first written (2026-08-19) and stayed
  true through the 2026-08-28 fold. It became false on 2026-08-28 and was then
  repeated twice. `git log 34074b93..HEAD` on the two files returns three
  commits, all ancestors of both `f9dd52b4` and `4d78dce6`:

  | Commit | Date | Files | Effect |
  |---|---|---|---|
  | `ff177576` | 2026-08-28 | `fsr-bench-matrix.sh` (+94/−2) | bench sanity gates (entity floor, state-hash rejection) |
  | `0e91fc5e` | 2026-08-28 | **both** (+130/−13) | adds the `gpu_inactive` TSV column and changes `fsr_bench_report.py`'s `render_sum` so brackets flagged inactive are **excluded** rather than summed as `0.000` |
  | `1293dfc0` | 2026-08-29 | `fsr-bench-matrix.sh` (+29) | adds the `gridcross` exterior scene definition (deliberately outside the default `SCENES`) |

  The 2026-09-01 paragraph names `0e91fc5e`'s own #2830 in its body ("Session 76
  changes an over-limit FSR render-extent from clamped to rejected") and then
  asserts the harness untouched since `34074b93` — the same commit did both.
- **Evidence**: The harness's own provenance stamp contradicts the claim
  directly. The two archived records:
  ```
  docs/audits/BENCH_stepped-camera_34074b93.tsv
    # harness=4de5e78e engine=34074b93 …          (23 columns, ends state_hash)
  docs/audits/BENCH_stepped-camera_2da754e7.tsv
    # harness=1293dfc0 engine=2da754e7 …          (24 columns, ends gpu_inactive)
  ```
  `git merge-base --is-ancestor` confirms all three harness commits precede
  `2da754e7`, `f9dd52b4` and `4d78dce6`.
- **Impact**: The tracker is the only place in the repo that records whether two
  bench records are comparable, and it currently licenses an apples-to-apples
  read of the 2026-08-14 and 2026-09-03 matrices that is not valid: the column
  set differs, the acceptance gates differ, and `render_sum` — the input to the
  "render rec." column — is computed differently. The practical damage is bounded
  because the **live** bench-of-record section (2026-09-03, `2da754e7`) does the
  right thing independently: it declines old-vs-new attribution outright ("The
  1059-commit gap is too large for an uncontrolled old-vs-new attribution").
  Hence LOW.
- **Related**: #2835 (the harness provenance stamp that makes this checkable),
  `0e91fc5e` (#2821, the `gpu_inactive` change), REN-2026-09-06-D23-03
- **Suggested Fix**: Replace the assertion in the last two fold paragraphs with
  the measured fact — three harness commits, what each changed, and that the two
  archived records therefore carry different `harness=` stamps and are not
  directly comparable. Going forward, derive the sentence from
  `git log <record>..HEAD -- scripts/fsr-bench-matrix.sh scripts/fsr_bench_report.py`
  at fold time rather than carrying it forward verbatim; the fold ritual copied
  this line through five updates unverified.

---

### REN-2026-09-06-D23-03: `fsr_bench_report.py` discards the provenance line the harness writes for exactly this purpose, and stamps nothing of its own


- **Severity**: LOW
- **Dimension**: FSR/Presentation (debug/telemetry)
- **Location**: `scripts/fsr_bench_report.py` (`main` — the
  `not line.startswith("#")` filter, and the per-scene `print` block);
  `scripts/fsr-bench-matrix.sh` (the `# harness=%s engine=%s …` `printf`)
- **Status**: NEW
- **Description**: #2835 added the `# harness=… engine=… mode=… camera=… runs=…
  frames=…` header to the TSV because "nothing in a committed table said which
  harness produced it". The tool that turns the TSV into the table people
  actually quote drops that line as metadata and never re-emits it, so the
  human-readable output still says nothing about harness or engine commit. The
  reporter also records no version of its own — and it is not a pure formatter:
  `0e91fc5e` changed `render_sum` so brackets named in `gpu_inactive` are
  excluded from the render-resolution sum instead of summed as measured zeros.
  The same TSV therefore yields different "render rec." figures before and after
  that commit, with nothing in the output distinguishing them.
- **Evidence**:
  ```python
  # main(): the provenance line is filtered out and never referenced again
  lines = [line for line in handle if line.strip() and not line.startswith("#")]
  ```
  The only per-scene header printed is
  `f"\n=== {scene} — {mode}/{camera}, {entities} entities, {runs} runs, median (min–max)"`
  — scene state, no provenance. Contrast the harness, which went to the trouble
  of computing `HARNESS_COMMIT` from
  `git log -1 --format=%h -- scripts/fsr-bench-matrix.sh`.
- **Impact**: Bench tables are pasted into `ROADMAP.md` and audit reports. A
  pasted table carries no way to tell which harness/engine produced it or which
  reporter computed its recovery columns — which is precisely the gap that let
  D23-02's stale byte-stability claim survive two folds unchallenged. Purely a
  measurement-hygiene issue, no runtime effect.
- **Related**: #2835, `0e91fc5e` (#2821), REN-2026-09-06-D23-02
- **Suggested Fix**: Echo the `#` provenance line(s) verbatim at the top of the
  report, and add a `report=<git log -1 --format=%h -- scripts/fsr_bench_report.py>`
  token next to them so both halves of the harness pair are stamped. The
  self-test already fixtures the `# harness=deadbeef engine=cafef00d` header, so
  the assertion is a one-line addition to the existing loop.

---

### REN-2026-09-06-D23-04: the FSR plan's phase-3 status line still says exposure is "consumed by the composite tonemap" — contradicted five lines later in the same header


- **Severity**: LOW
- **Dimension**: FSR/Presentation (doc-rot)
- **Location**: `docs/engine/fsr3-upscaler-integration-plan.md`, the status
  header's phase-3 paragraph ("…the 1×1 `R32_SFLOAT` exposure producer consumed
  by the composite tonemap…")
- **Status**: NEW
- **Description**: Phase 4 moved exposure and ACES out of composite into the
  output-resolution presentation pass, and the very next paragraph of the same
  header says so ("an output-resolution presentation pass that owns exposure +
  ACES"). The phase-3 sentence was never updated. It is checkable and wrong:
  `composite.frag` contains no `exposure` uniform and no `aces()` — verified by
  grep — and `composite.rs`'s single mention of exposure is a comment pointing
  the reader at `frame_upscaler.rs` / `exposure.rs`. The live consumer is
  `presentation.frag`'s `vec3 presented = aces(graded * params.exposure)`.
- **Evidence**: `grep -i "exposure\|aces" crates/renderer/shaders/composite.frag`
  returns only prose comments about pre-ACES linear space (composite's *output*
  is pre-tone-map by design); the sole `params.exposure` reader in the tree is
  `presentation.frag`.
- **Impact**: This is the authoritative FSR document, and exposure agreement
  between the upscaler and the tone-mapper is exactly the invariant #2833 was
  filed about (`NO_EXPOSURE_RESOURCE_FALLBACK`). A reader chasing an exposure
  mismatch is sent to the wrong shader. Documentation only.
- **Related**: #2833, `docs/engine/shader-pipeline.md` (which the SKILL's Dim 8
  bullet already records correctly: "ACES tone-map is NOT in `composite.frag` —
  it lives in `presentation.frag`")
- **Suggested Fix**: Change "consumed by the composite tonemap" to "consumed by
  the presentation tone-map (`presentation.frag`, since phase 4)". One clause.

---

### REN-2026-09-06-D3-03: the terrain-tile shift/mask is the last `GpuInstance.flags` bitfield hand-written shader-side, with no generated `#define` and no lockstep pin


- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs` (`INSTANCE_TERRAIN_TILE_SHIFT`, `INSTANCE_TERRAIN_TILE_MASK`), `crates/renderer/shaders/triangle.frag`, `crates/renderer/src/shader_constants_data.rs`
- **Status**: NEW
- **Description**: Every other packed field in `GpuInstance.flags` reaches GLSL through the generated `include/shader_constants.glsl` header: `INSTANCE_FLAG_NON_UNIFORM_SCALE`/`ALPHA_BLEND`/`CAUSTIC_SOURCE`/`TERRAIN_SPLAT`/`FLAT_SHADING`/`DIFFUSE_ALPHA` plus `INSTANCE_RENDER_LAYER_SHIFT`/`_MASK`. The terrain-tile window does not. The CPU packs it with the named constants (`f |= (tile_idx & INSTANCE_TERRAIN_TILE_MASK) << INSTANCE_TERRAIN_TILE_SHIFT` in `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`), while `triangle.frag` unpacks it with the literals `(inst.flags >> 16) & 0xFFFFu`. No `#define` is emitted and no test pins the two halves equal.
- **Evidence**:
  - `grep -n "TERRAIN" crates/renderer/shaders/include/shader_constants.glsl` returns only `#define INSTANCE_FLAG_TERRAIN_SPLAT 8u` — no shift, no mask.
  - `shader_constants_data.rs`'s own header comment *names* the two constants in prose ("the upper 16 bits pack the terrain-tile slot per `INSTANCE_TERRAIN_TILE_SHIFT/MASK`") while not mirroring them.
  - The identical defect for the render-layer bits was fixed by #2045 / TD7-101, whose comment reads: *"Previously hand-written as `INST_RENDER_LAYER_SHIFT`/`_MASK` directly in `triangle.frag` with no lockstep test, unlike every other `INSTANCE_FLAG_*` bit"*.
- **Impact**: No live drift — the values are 16 and `0xFFFF` on both sides today, and `instance_flag_bits_unique_and_outside_packed_windows` guards the CPU side against collisions. The gap is one-directional: a future widening of the tile window (`MAX_TERRAIN_TILES` is capped at 65535 *by this encoding*) would move the Rust constants and leave `triangle.frag` reading a stale window, indexing `terrainTiles[nonuniformEXT(…)]` with a truncated slot — wrong diffuse/normal/specular layers on every exterior cell, no test failure, no validation error. Same failure mode `gpu_terrain_tile_is_96_bytes`' doc describes for the sibling stride hazard.
- **Related**: #2045 / TD7-101 (the same fix for the render-layer bits); #470 (the encoding).
- **Suggested Fix**: Mirror `INSTANCE_TERRAIN_TILE_SHIFT` / `INSTANCE_TERRAIN_TILE_MASK` into `shader_constants_data.rs`, add them to `build.rs`'s emit and to `generated_header_contains_all_defines`, add an *instance_terrain_tile_bits_match_scene_buffer_consts* pin alongside the existing render-layer one, and replace the two literals in `triangle.frag`.

---

### REN-2026-09-06-D3-04: `CameraUBO` is the only mirrored GPU struct without a mirror-discovery guard — the exact hole `fa5c4191` just closed for `GpuInstance`


- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` (`camera_ubo_glsl_copies_stay_in_lockstep`, `assert_mirror_list_is_complete`, `shader_sources_declaring`)
- **Status**: NEW — residual scope gap of the **closed** #3564 (`0d4a2e70`, "discover GLSL mirror sites instead of hardcoding each guard's list"), not a re-file of it
- **Description**: #3564 replaced every mirror guard's hardcoded `SOURCES` list with `assert_mirror_list_is_complete`, which walks the shader tree and fails if any source declares the struct without being listed. It reached four of the five hand-mirrored GPU structs: `struct GpuInstance` (#2748 / #3564), `struct GpuLight` (#1916 / #3564), `struct WaterParams` (#3564), `struct GpuTerrainTile` (#2463 / #3564). `camera_ubo_glsl_copies_stay_in_lockstep` (#3684, landed after #3564) deliberately does not call it, because `shader_sources_declaring` requires `decl` to start the trimmed line and `CameraUBO` is always preceded by a `layout(...)` qualifier. Its own doc comment records the residual gap. `CameraUBO` is therefore the one mirrored GPU struct that #3564's mechanism still does not cover.
- **Evidence**:
  - `grep -rn "uniform CameraUBO" crates/renderer/shaders/` returns exactly the 5 sites in the test's `SOURCES` — **no live drift today**.
  - The test's doc: *"A sixth shader adding `CameraUBO` without joining this SOURCES list is a real but narrower gap … and isn't in the issue's own suggested fix."*
- **Impact**: Regression-guard gap, not a live defect. `fa5c4191` (#3829) landed yesterday after exactly this class cost 13 days of silently misaligned reads: `volumetrics_inject.comp`'s `GpuBoundaryInstance` was a 6th `GpuInstance` mirror outside both the `SOURCES` list and the discovery grep, kept a 128-byte stride through #3231's growth to 160, and misaligned every boundary-geometry read past index 0 with a fully green suite. `GpuCamera` is 368 B across 5 mirrors and grew twice in three weeks (352 → 368 for `exterior_sky_tint`, #3323); it is the most likely next struct to grow a sixth reader.
- **Related**: #3684 (the field-lockstep test); #3829 / `fa5c4191` (the realized cost of an untracked mirror); #3564 (introduced `assert_mirror_list_is_complete`).
- **Suggested Fix**: Give `shader_sources_declaring` an optional "declaration may be preceded by a `layout(...)` qualifier" mode (or a second helper that strips a leading `layout(...)` before the prefix test) and call `assert_mirror_list_is_complete("uniform CameraUBO {", SOURCES, "#3684")`. The existing strict behaviour must stay the default so `skin_vertices.comp`'s *comment* mentioning `struct GpuInstance` keeps not matching.

---

### REN-2026-09-06-D3-05: `GpuInstance`'s per-frame PCIe accounting still quotes 128 B per instance, two sizes behind


- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/descriptors.rs` (`hash_instance_slice` doc comment), `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`SceneBuffers::upload_instances` dirty-gate comment)
- **Status**: NEW
- **Description**: Both comments size the instance dirty-gate's benefit at *"7359 draws at 128 B per `GpuInstance` ≈ 920 KB/frame … ~54 MB/s sustained PCIe at 60 fps"*, and both carry a parenthetical recording the previous correction (`"#2692 — was stated as 112 B / 805 KB / 48 MB/s, pre-#2219"`). #3231 then grew the struct 128 → 160 B and neither figure moved. The correct numbers are ≈ 1.12 MiB/frame and ≈ 67 MB/s.
- **Evidence**: `size_of::<GpuInstance>()` is pinned at 160 by `gpu_instance_is_160_bytes_std430_compatible`; 7359 × 160 = 1 177 440 B, × 60 = ~70.6 MB/s.
- **Impact**: Documentation only — the code reads `std::mem::size_of::<GpuInstance>()`, so no behaviour depends on the figure. It understates the dirty-gate's value by ~25 % in the two comments that justify its existence, and it sits directly adjacent to the `unsafe` safety argument in D3-02, where a reader checking one number and finding it stale has cause to distrust the other.
- **Related**: #2692 (the previous correction of the same figures); #3231 (the growth that stranded them). Same class as #3846.
- **Suggested Fix**: Restate as 160 B / ~1.12 MiB per frame / ~67 MB/s, and consider deriving the byte figure in the comment from `size_of::<GpuInstance>()` prose-side (i.e. state draws × `size_of`) so the next growth cannot strand it a third time.

---

### REN-2026-09-06-D3-06: `MESH_ID_ENCODING_CEILING` is a residual literal copy of the mask `bf8ded3d` consolidated a day earlier


- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` (`max_instances_stays_within_mesh_id_encoding_ceiling`)
- **Status**: NEW
- **Description**: `bf8ded3d` (#3881, 2026-09-06) gave the mesh-ID attachment ABI a Rust-side definition — `MESH_ID_NO_HISTORY_BIT` and `MESH_ID_STABLE_MASK` in `shader_constants_data.rs`, the latter expressed as `!MESH_ID_NO_HISTORY_BIT` explicitly *"rather than a fourth copy of `0x7FFFFFFF`"* — and replaced the GLSL literals. One executable copy survives: the ceiling test still declares `const MESH_ID_ENCODING_CEILING: usize = 0x7FFF_FFFF;` locally.
- **Evidence**: `grep -rn "0x7FFF_FFFF\|0x7FFFFFFF" crates/renderer/src/` returns eight hits; seven are doc comments or negative source-scan assertions, and this local `const` is the only remaining code literal.
- **Impact**: The test asserts `MAX_INSTANCES <= MESH_ID_ENCODING_CEILING`. If the no-history bit ever moved, the mask constant would move and this test would keep asserting against the old ceiling — a silently-green guard on the encoding contract it exists to protect. Very low likelihood; the value is a genuine hardening loss rather than a live risk.
- **Related**: #3881 / `bf8ded3d`; #992 (the `R16_UINT` → `R32_UINT` change this test guards).
- **Suggested Fix**: Replace the local `const` with `crate::shader_constants::MESH_ID_STABLE_MASK as usize`.

---

### REN-2026-09-06-D3-08: this dimension's own SKILL instruction now under-counts the `GpuInstance` mirrors, and the one it misses is the one that went wrong


- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 3, the `struct GpuInstance` lockstep bullet)
- **Status**: NEW
- **Description**: The Dimension 3 checklist instructs the auditor to enumerate mirrors with `grep -rlE '^struct GpuInstance' crates/renderer/shaders/`, states the result is **5** declaration sites, and adds that *"the unanchored form returns 6, also matching `skin_vertices.comp`'s comment noting that shader has no `struct GpuInstance`."* Measured today: the anchored grep still returns 5, but the **unanchored form returns 7** — `skin_vertices.comp` plus `volumetrics_inject.comp`, which acquired three `GpuInstance`-naming comments in `fa5c4191` (2026-09-05). More importantly, there are now **6** real mirrors of the struct, not 5: `volumetrics_inject.comp` declares `struct GpuBoundaryInstance`, which reads the very same per-frame `GpuInstance` SSBO — `VolumetricsPipeline::write_boundary_geometry` writes the scene `instance_buffer` into the volumetrics-private descriptor set at binding 19 — and the SKILL's grep recipe cannot see it by construction.
- **Evidence**:
  - `grep -rlE '^struct GpuInstance' crates/renderer/shaders/` → 5 files; unanchored → 7 files.
  - `grep -rhoE "^struct [A-Za-z_]+" crates/renderer/shaders/**` → `struct GpuBoundaryInstance` ×1.
  - `fa5c4191` ("Fix #3829 …"): *"It survived because it is a sixth mirror under its own struct name: outside `gpu_instance_glsl_copies_stay_in_lockstep`'s hardcoded SOURCES list, and invisible to the companion discovery guard, which greps for the literal `struct GpuInstance`."*
  - `.claude/commands/audit-renderer/SKILL.md` was last touched by `a5881e02` and has no mention of `GpuBoundaryInstance`.
- **Impact**: The instruction is a blindfold over exactly the mirror that most recently broke. #3829's `GpuBoundaryInstance` held a 128-byte stride against a 160-byte struct for ~13 days — silently wrong fire/smoke-vs-geometry collision normals, fully green suite — and an auditor following this bullet verbatim would run the anchored grep, get 5, confirm all 5 match, and file "no findings" without ever reaching it. The code side is now guarded (`gpu_boundary_instance_stride_matches_gpu_instance`), so this is a documentation defect rather than a live exposure — but the SKILL is the artifact that decides whether the *next* differently-named mirror gets looked at.
- **Related**: #3829 / `fa5c4191`; #3564 (mirror discovery); the `_audit-common.md` rule "Never write an instruction to not look" (#3199) — the same failure shape, arrived at by rot rather than by authoring.
- **Suggested Fix**: In the Dimension 3 bullet, correct the unanchored count to 7, and replace the "5 declaration sites" framing with "5 sites named `struct GpuInstance` **plus `struct GpuBoundaryInstance` in `volumetrics_inject.comp`**, which reads the same SSBO under a different name and is pinned separately by `gpu_boundary_instance_stride_matches_gpu_instance` (#3829)". Add the struct-name-agnostic recipe (`grep -rhoE '^struct [A-Za-z_]+' crates/renderer/shaders/` and inspect anything binding set 1 / binding 4 or 19) so the count cannot rot the same way again.

---

### Existing — verified still open, do not re-file

| ID | Title | Verification this run |
|---|---|---|
| **#3846** | `bindings.glsl` documents `GpuMaterial` as 396 B and points struct-sync at a nonexistent *gpu_material_size_is_396_bytes* | **STILL UNFIXED.** The header comment above `struct GpuMaterial` in `crates/renderer/shaders/include/bindings.glsl` says *"Mirrors the Rust `GpuMaterial` (396 B std430)"* and *"the size of this struct (396 B) is pinned by `gpu_material_size_is_396_bytes`"*. The real size is 432 B and the real test is `gpu_material_size_is_432_bytes`. `b10a7b7e` / `2853464f` did not reach this file. Report as **Existing**, not new |
| #3909 | `GpuMaterial.texture_index` is an undocumented unsampled lane in the dedup key | Not re-examined (Dim 7 scope); left as-is |
| #3910 / #3911 | supplemental-lane test gaps | Not re-examined (Dim 7 scope); left as-is |

### Checked and clean — no finding

- **All five `struct GpuInstance` GLSL mirrors match the Rust struct field-for-field**, in order, including types (`uvec2 _reserved` ↔ `[u32; 2]`, three scalar `uint`s ↔ three scalar `u32`s — the deliberate anti-`uvec3` shape from #3231 is intact in all five). The `ui.vert` / `water.vert` trap (#785 / #1498) has not recurred.
- **`GpuMaterial`'s single GLSL mirror matches all 108 fields** by name, order and type; all 108 offsets are individually pinned; the dedup hash walks all 108 in declaration order; **no field is `[f32; 3]`** and the struct has no pad fields at all.
- **`GpuInstance`'s pads are explicitly zeroed at both construction sites** (`build_and_upload_instances.rs` sets `_reserved: [0; 2]`, `_reserved2a/b/c: 0`; the UI-quad instance uses `..GpuInstance::default()`), and **no shader reads any pad lane** — the #2164 "live data wearing a padding name" trap has not recurred.
- **All five `uniform CameraUBO` sites are enumerated and lockstep-tested**; `render_debug` remains the appended tail (`uvec4`, not `vec4` — #2688's byte-lethal type-flip class is guarded by `camera_ubo_glsl_copies_stay_in_lockstep`'s typed leg).
- **Capacity constants are in three-way agreement** across code, `memory-budget.md` and `shader-pipeline.md` (table above); the `MAX_INSTANCES < 1 << 24` const-assert guarding the 24-bit `instance_custom_index` is present.
- **Over-cap material intern is safe**: `MaterialTable::intern_by_hash` returns id 0 with a `Once`-gated `warn!` naming `ctx.scratch`; `upload_materials` additionally hard-`assert!`s `len() <= MAX_MATERIALS` in release.
- **The generated-constants path is healthy**: `build.rs` regenerates `shaders/include/shader_constants.glsl` in-tree only on content change; `generated_header_contains_all_defines`, `dbg_bits_catalog_covers_every_dbg_constant`, `instance_flag_bits_match_scene_buffer_consts`, `instance_render_layer_bits_match_scene_buffer_consts`, `material_flag_bits_match_material_consts` and `material_kind_constants_stay_in_lockstep_across_rust_and_glsl` all pass. `bf8ded3d`'s `MESH_ID_*` consolidation left zero hardcoded literals in GLSL.
- **`GpuRayBudget` (`78cc7a41`)**: 17 × `u32` = 68 B Rust-side, 17 fields in the same order in `include/bindings.glsl`, matching `shader-pipeline.md`'s binding-11 row; `RAY_BUDGET_STRIDE = 256` ≥ 68.

### Scope note

This dimension examined `crates/renderer` only — `scene_buffer/{gpu_types,constants,upload,descriptors,ray_budget,material_hash_tests,instance_hash_tests,gpu_instance_layout_tests,shader_contract_tests}.rs`, `vulkan/{material,material_tests,restir,water,volumetrics,gbuffer}.rs`, `shader_constants{,_data}.rs`, `build.rs`, and all GLSL under `crates/renderer/shaders/`. It did not evaluate whether the fields are *used* correctly (Dims 2, 6, 7) or their VRAM cost (Dim 5).

### REN-2026-09-06-D4-02: `copy_depth_to_history` became conditional under #3667, and neither the authoritative doc nor #3628's ordering pin (landed three days later) says so


- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `docs/engine/shader-pipeline.md` (§"Per-Frame Submission Order",
  steps 7 and 7b) and `crates/renderer/src/vulkan/context/depth_capture.rs`
  (`capture_ordering_tests::record_copy_runs_immediately_after_the_depth_history_copy`
  — its doc block and its first `assert!` message)
- **Status**: NEW — doc + test-rationale wrong, code right
- **Description**: `1bf64187` (#3667, 2026-09-03) put `copy_depth_to_history`
  behind `if has_effect_soft_material`, a per-frame `FrameInputs` boolean. It is
  therefore skipped on most frames of most cells. Two places still describe it
  as unconditional, and both use it as the *source* of the depth image's layout:
  1. `shader-pipeline.md` step 7 lists it as an unconditional pass, and step 7b
     says `depth_capture_record_copy` is *"recorded immediately after step 7,
     **which leaves the depth image back in `DEPTH_STENCIL_READ_ONLY_OPTIMAL`**"*.
  2. #3628's pin asserts *"that layout is only guaranteed once the history
     copy's own barriers have run"*.

  Both are inverted for the common path. The layout comes from
  `create_render_pass`'s depth-attachment `final_layout`
  (`DEPTH_STENCIL_READ_ONLY_OPTIMAL`, `context/helpers.rs`); the history copy
  merely *restores* it when it runs. `draw_frame`'s own inline comment gets this
  right (*"when the copy is skipped, the layout is already the precondition
  `depth_capture_record_copy` requires and restores"*) — the doc and the test
  message do not.

  This is not only cosmetic: the pin's hazard scan
  (`for hazard in ["cmd_pipeline_barrier", "cmd_copy_image(", …]`) is applied to
  `src[history_copy_pos..record_copy_pos]`, a window that *starts at a call that
  usually does not execute*. The window that actually protects
  `depth_capture_record_copy`'s stated precondition starts at the render pass
  end. A depth-image layout transition inserted between `record_geometry_pass`
  and the `if has_effect_soft_material` block — or inside that block ahead of the
  copy — would satisfy the pin and still break the precondition.
- **Evidence**:
  - `crates/renderer/src/vulkan/context/draw.rs` — `self.copy_depth_to_history(cmd);`
    is inside `if has_effect_soft_material { … }`; `self.depth_capture_record_copy(cmd);`
    is outside it. `has_effect_soft_material` is destructured from `FrameInputs`
    at the top of `draw_frame`.
  - `git log -1 --format=%ad --date=short 1bf64187` → `2026-09-03`;
    `229306ce` (#3628) → `2026-09-06`.
  - `grep -n "has_effect_soft_material" docs/engine/shader-pipeline.md` → no hits.
  - `context/helpers.rs::create_render_pass` — depth attachment
    `.final_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)`.
- **Impact**: The one doc `/audit-renderer` designates authoritative for frame
  ordering, and the one test that pins this ordering, both attribute a layout
  guarantee to a pass that usually does not run. A future reader narrowing or
  removing the render pass's depth `final_layout` would find nothing objecting.
  No runtime misbehaviour.
- **Related**: #3667 (the gating change), #3628 (the pin), #2484 (the barrier
  whose src scope the pin's rationale describes), *REN-2026-08-30-D4-01*
  (the earlier, now-fixed gap in the same doc block).
- **Needs RenderDoc**: no.
- **Suggested Fix**: Mark step 7 conditional in `shader-pipeline.md` and move
  the layout attribution in step 7b to the render pass's depth `final_layout`.
  In `capture_ordering_tests`, restate the assert message the same way and
  consider widening the hazard scan's start anchor from
  `self.copy_depth_to_history(cmd);` to the `record_geometry_pass` call, so the
  window matches the precondition it claims to guard. No code change.

---

### REN-2026-09-06-D4-03: the authoritative submission-order block enumerates two barrier-only steps but omits the frame's two most load-bearing barriers


- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `docs/engine/shader-pipeline.md` (§"Per-Frame Submission Order",
  the fenced 24-step block — steps 8 and 9 are the only `[Barrier]` rows)
- **Status**: NEW — doc gap, code right
- **Description**: The block deliberately gives barrier-only work its own
  numbered rows (step 8 *"`SHADER_READ_ONLY_OPTIMAL` on all G-buffer
  attachments"*, step 9 *"caustic accum atomic-add → `SHADER_READ`"*), which
  establishes that barriers are in this doc's scope. Two barriers with far more
  weight than either of those appear nowhere in the document:

  1. **The bulk host-visibility barrier** — `memory_barrier(HOST/HOST_WRITE →
     VERTEX_SHADER | FRAGMENT_SHADER | COMPUTE_SHADER | DRAW_INDIRECT /
     SHADER_READ | SHADER_WRITE | UNIFORM_READ | INDIRECT_COMMAND_READ)` at the
     tail of `build_and_upload_instances`. It is the single publication point
     for the instance SSBO **and** the composite, SVGF, TAA and water parameter
     UBOs, which were deliberately folded onto it across #909, #961 and #1397
     precisely so those passes need no per-dispatch HOST barrier. A reader
     reasoning about why `record_composite_pass` (step 17) has no HOST barrier
     of its own cannot learn it from this doc.
  2. **The frame's only `AS_WRITE → AS_READ` barrier** —
     `ACCELERATION_STRUCTURE_BUILD_KHR`/`ACCELERATION_STRUCTURE_WRITE_KHR` →
     `FRAGMENT_SHADER | COMPUTE_SHADER`/`ACCELERATION_STRUCTURE_READ_KHR` in
     `dispatch_skin_and_cluster`, which publishes both the TLAS build and every
     skinned-BLAS refit and is emitted on both the success and the failure arm
     (#2931). `/audit-severity` puts *"Missing AS barrier (build → shader
     read)"* at a HIGH floor, making it the one frame-graph edge a doc most
     needs to describe. `docs/engine/renderer.md` describes it correctly (after
     `4ea40bd7`, "Fix doc rot: … nonexistent HOST->AS_BUILD barrier", closed
     *REN-2026-08-30-D4-06*); `shader-pipeline.md`
     — the doc the skill designates authoritative for submission order — does
     not mention it at all.

  Also absent, and cheaper to add: `flush_pending_morph_weights` (a host write
  to a mapped buffer that `sync.rs`'s #870 block names as item 5 on the
  both-slots-wait dependency list) and the
  `FRAGMENT_SHADER/SHADER_WRITE → HOST/HOST_READ` selected-ray-probe publish
  barrier between steps 6 and 7.
- **Evidence**:
  - `grep -n "HOST_WRITE\|ACCELERATION_STRUCTURE\|AS_BUILD" docs/engine/shader-pipeline.md`
    → only the descriptor-binding tables (lines with `ACCELERATION_STRUCTURE` as
    a *descriptor type*), nothing in the order block.
  - The barriers themselves: `build_and_upload_instances` (its comment block
    records the #909 / #961 / #1397 fold history) and
    `dispatch_skin_and_cluster` (its comment records #415 / #2931).
  - The probe publish barrier is already source-pinned by
    `selected_ray_probe_is_bounded_and_captures_the_detailed_shadow_query`
    (`scene_buffer/shader_contract_tests.rs`), which asserts its four masks
    against `draw.rs` — so the code side is guarded; only the doc is silent.
- **Impact**: An auditor or maintainer told to trust this block for ordering
  gets a list that names two minor barriers and omits the two that hold the
  frame together. This is the mechanism that produced *REN-2026-08-30-D4-06*
  (a sibling doc describing the AS barrier with the wrong source stage) in the
  first place.
- **Related**: `4ea40bd7` / *REN-2026-08-30-D4-06* (`renderer.md`'s version of
  the same edge), #3830 (another `shader-pipeline.md` accuracy gap, open),
  #3447, #909, #961, #1397, #2931.
- **Needs RenderDoc**: no.
- **Suggested Fix**: Add two rows to the fenced block — one before step 6 for
  the bulk `HOST_WRITE` fold (naming its four dst stages and the four UBOs
  folded onto it) and one inside step 4 for the `AS_WRITE → AS_READ` publish
  (naming that it covers the skinned refits as well as the TLAS, and that it
  runs on both arms). No code change.

---

### REN-2026-09-06-D4-04: `image_health_docs_no_longer_claim_fence_alone_proves_host_visibility` scans `draw.rs` for a call site #3282 moved to `sync_and_acquire_frame.rs`


- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/context/resources.rs`
  (`image_health_docs_no_longer_claim_fence_alone_proves_host_visibility`, the
  `("draw.rs (collect_image_health call site)", draw_src)` entry in its
  three-way loop)
- **Status**: NEW — sibling of the open #3442, different pin and different file
- **Description**: #2740 corrected three comments that claimed a fence wait
  alone makes a device write host-visible (it does not — a fence's access scope
  is device-side only), and pinned the correction with a negative source scan
  over three files. One of the three is `draw.rs`, labelled *"collect_image_health
  call site"*. The #3282 split moved that call site — and the corrected comment
  attached to it — into `sync_and_acquire_frame.rs`. `draw.rs` no longer
  contains the string `collect_image_health` at all, so that third of the pin is
  vacuously green while the comment it was written to guard is unscanned.

  The live comment in `sync_and_acquire_frame.rs` is currently **correct**
  (*"The fence wait above proves submission completed (device-side access scope
  only) — it does NOT by itself prove the GPU write is host-visible"*), so there
  is no live defect — only a guard that has quietly stopped guarding.
- **Evidence**:
  - `grep -rn "collect_image_health" crates/renderer/src/` → definition and
    tests in `resources.rs`, the field doc in `context/mod.rs`, the init comment
    in `context/init.rs`, and the **call site in
    `context/sync_and_acquire_frame.rs`**. No hit in `draw.rs`.
  - The test builds its needles at runtime (`["provably", "idle"].join(" ")`)
    specifically so its own source cannot satisfy them — the technique is sound;
    only the file list is stale.
- **Impact**: Reintroducing the retired claim at the live call site passes
  `cargo test`. The same class as #3442, which is filed against the `(f + 1) %
  MAX_FRAMES_IN_FLIGHT` pin for the same reason.
- **Related**: #2740, #2793, #3282, #3442 (open, same class).
- **Needs RenderDoc**: no.
- **Suggested Fix**: Replace the `draw.rs` entry with
  `include_str!("sync_and_acquire_frame.rs")` (keeping the `draw.rs` entry costs
  nothing and guards against the comment migrating back). No production change.

---

### REN-2026-09-06-D4-05: `draw_frame_does_not_re_upload_bloom_params_every_frame` scans `draw.rs`, but the per-frame UBO section it guards moved to `build_and_upload_instances.rs`


- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/bloom.rs`
  (`draw_frame_does_not_re_upload_bloom_params_every_frame`)
- **Status**: NEW — sibling of the open #3442
- **Description**: The test's own doc says *"`draw_frame`'s per-frame UBO section
  (composite/SVGF/TAA) must NOT call `bloom.upload_params`"*, and enforces it
  with `assert!(!include_str!("context/draw.rs").contains("bloom.upload_params"))`.
  That per-frame UBO section — the `composite.upload_params` /
  `svgf.upload_params` / `taa.upload_params` / `water.upload_params` block whose
  host writes the bulk barrier folds — now lives in
  `build_and_upload_instances.rs`. A re-added `bloom.upload_params` would
  naturally land there, where the scan cannot see it, and would additionally be
  a per-frame host write folded onto that same bulk barrier — i.e. exactly the
  redundant rewrite #2037 removed, silently reinstated.
- **Evidence**:
  - `grep -n "upload_params" crates/renderer/src/vulkan/context/build_and_upload_instances.rs`
    → `composite.upload_params`, `svgf.upload_params`, `taa.upload_params`,
    `water.upload_params`, plus the "#2037 / GPU-D5-01 — no per-frame upload
    needed here" comment that marks bloom's absence. All four are in that file;
    none is in `draw.rs`.
  - The bloom test still reads `include_str!("context/draw.rs")`.
- **Impact**: A negative pin pointed at the wrong file. Guard-coverage only; no
  live defect (bloom's UBOs are still written once in `BloomPipeline::new_inner`).
- **Related**: #2037 / GPU-D5-01, #3282, #3442 (open, same class), D4-04 above.
- **Needs RenderDoc**: no.
- **Suggested Fix**: Point the scan at
  `include_str!("context/build_and_upload_instances.rs")` (or scan both files)
  and update the doc comment's "draw_frame's per-frame UBO section" wording. No
  production change.

---

### REN-2026-09-06-D4-06: `signal_temporal_discontinuity`'s `previous_rigid_models.clear()` is inert at all three of its in-`draw_frame` call sites


- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/context/mod.rs`
  (`VulkanContext::signal_temporal_discontinuity`, the trailing
  `self.previous_rigid_models.clear();` and its comment), against its three
  in-frame callers: `context/post_passes.rs` (`record_taa_pass`'s Err arm, #3605,
  and `record_upscale_pass`'s, #2519) and
  `context/assemble_camera_and_lights.rs` (the `camera_cut` arm)
- **Status**: NEW
- **Description**: The clear carries an explicit contract — *"The first frame
  after a discontinuity must not encode object motion against transforms from
  the retired scene/camera history."* That contract is delivered for the fifteen
  out-of-frame callers (`streaming_helpers.rs`, `debug_load.rs`, `save_io.rs`,
  `app_step.rs`, `resize.rs`), which run between frames. None of the three
  in-`draw_frame` callers gets it:
  - The two `post_passes.rs` sites run during the post-pass tail, *before*
    `draw_frame`'s unconditional
    `std::mem::swap(&mut self.previous_rigid_models, &mut current_rigid_models);`
    — which immediately refills the map with this frame's transforms. The clear
    is overwritten within the same function.
  - The `assemble_camera_and_lights.rs` site runs early enough to take effect,
    but is redundant: `build_and_upload_instances`'s `previous_source` selection
    is already gated `if uses_rigid_history && !camera_cut`, so on a cut every
    instance falls back to `m` regardless of the map's contents.

  No live defect is claimed. For both `post_passes.rs` sites the transforms are
  *not* stale (the hazard #3605/#2519 address is jitter, and motion vectors are
  reconstructed from the un-jittered projection), and the four other effects of
  `signal_temporal_discontinuity` — `svgf_recovery_frames`,
  `taa.signal_history_reset()`, `fsr.signal_reset()`,
  `volumetrics.signal_history_reset()` — all persist correctly and are what
  actually protect the recovery frame.
- **Evidence**:
  - `crates/renderer/src/vulkan/context/mod.rs::signal_temporal_discontinuity`
    ends with `self.previous_rigid_models.clear();`.
  - `draw_frame` performs the swap unconditionally on the success path, after
    `record_post_passes` and after `queue_submit`; `record_taa_pass` and
    `record_upscale_pass` are both reached from `record_post_passes`.
  - `build_and_upload_instances` — `let previous_source = if uses_rigid_history
    && !camera_cut { … } else { m };`.
- **Impact**: A five-line API where one line silently does nothing at three of
  its eighteen call sites — precisely the three that run inside `draw_frame`. The risk is a future in-frame caller added on the
  belief the clear is effective — e.g. one added below the swap, or one where
  the transforms genuinely *are* retired.
- **Related**: #3605 (`c43cb269`, the newest of the three in-frame callers),
  #2519, #917 (which established that this frame's history advances only on
  submit success — the swap the clear collides with).
- **Needs RenderDoc**: no.
- **Suggested Fix**: Document on `signal_temporal_discontinuity` that the
  `previous_rigid_models` clear is only meaningful to callers running outside
  `draw_frame`, and that in-frame callers must additionally set the `camera_cut`
  path (or move the clear to a flag the tail swap honours). No behavioural change
  needed today.

---

### REN-2026-09-06-D4-07: `WaterCausticAccum::clear_pre_render_pass`'s barrier comment claims it performs the `UNDEFINED → GENERAL` discard, contradicting `initialize_layouts` 60 lines above


- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/water_caustic.rs`
  (`WaterCausticAccum::clear_pre_render_pass` — the comment on the `pre_clear`
  barrier's `.old_layout(...)`, vs the doc on
  `WaterCausticAccum::initialize_layouts`)
- **Status**: NEW — comment wrong, code right. Distinct from the open #3844,
  which is about the sandwich existing in four copies while its pin enumerates
  three; this is a single stale rationale inside one of them.
- **Description**: The `pre_clear` `VkImageMemoryBarrier` declares
  `.old_layout(GENERAL).new_layout(GENERAL)`, and its comment reads *"First use
  of this slot is `UNDEFINED → GENERAL` via a discarding layout transition.
  Subsequent frames go `GENERAL → GENERAL`."* That describes a barrier whose
  `old_layout` is `UNDEFINED` on the first frame — which this one is not, and
  could not be, since `old_layout` is a compile-time constant here.

  The code is correct because `WaterCausticAccum::initialize_layouts` walks every
  per-FIF slot `UNDEFINED → GENERAL` once (`image_barrier_undef_to_general` on a
  `with_one_time_commands` fenced submit), before any frame — and its own doc
  says exactly why it exists: *"so the first `clear_pre_render_pass` (which uses
  `oldLayout = GENERAL`) doesn't trip VUID-vkCmdDraw-None-09600"*. The two
  comments in the same file assert opposite things about the same barrier; the
  clear-sandwich one is a leftover from before `initialize_layouts` landed.
- **Evidence**:
  - `water_caustic.rs` — `initialize_layouts`'s doc block ("One-time
    `UNDEFINED → GENERAL` transition on every per-FIF slot … so the … clear
    (`oldLayout = GENERAL`) doesn't trip …") sits ~60 lines above
    `clear_pre_render_pass`'s contradicting comment.
  - The five sibling `initialize_layouts` owners phrase it correctly and carry
    no such claim: `caustic.rs` (`CausticPipeline::initialize_layouts`),
    `bloom.rs`, `taa.rs`, `svgf.rs`, `volumetrics.rs`.
    `grep -rn "First use of this slot" crates/renderer/src/vulkan/` returns
    this one site only.
- **Impact**: Someone auditing whether the clear sandwich is validation-clean on
  frame 0 reads a comment saying the barrier itself discards, concludes
  `initialize_layouts` is redundant, and removes it — reinstating exactly the
  first-frame layout violation that function's own doc says it exists to
  prevent. Nothing misbehaves today.
- **Related**: #3844 (open — the same sandwich's copy-count/pin mismatch),
  #3646 / #3647.
- **Needs RenderDoc**: no.
- **Suggested Fix**: Replace the comment with the `initialize_layouts` reference
  the other three accumulators use. No code change.

---

### Existing: #3442 — its stated location has drifted under #3282

- **Status**: **Existing: #3442** (open). Not re-filed; recorded here because
  the issue text is now unactionable as written.
- The issue is titled *"#2771's source-scan pin cannot see **draw.rs**'s
  `(f + 1) % MAX_FRAMES_IN_FLIGHT`"*. That expression no longer exists in
  `draw.rs` — the #3282 split moved it to
  `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs`
  (`let prev = (frame + 1) % super::super::sync::MAX_FRAMES_IN_FLIGHT;`, inside
  `sync_and_acquire_frame`'s both-slots `wait_for_fences`). The pin itself,
  `temporal_history_indexing_uses_the_general_previous_slot_form`
  (`crates/renderer/src/shader_constants.rs`), still covers only `taa.rs`,
  `svgf.rs`, `restir.rs` and `volumetrics.rs`, so the gap is unchanged — only
  the file to add to that list has changed. Worth a one-line correction on the
  issue before anyone acts on it.

---

### REN-2026-09-06-D5-04: the three `pending_destroy_*` accessors have no caller anywhere in the workspace, and their doc comments name a `ctx.scratch` surface and a unit test that do not exist


- **Severity**: LOW (observability / dead API)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs` —
  `AccelerationManager::pending_destroy_blas_count`,
  `pending_destroy_scratch_count`, `pending_destroy_static_bytes`.
  Claimed surface: `CtxScratchCommand` (`byroredux/src/commands/world_info.rs`).
- **Status**: **NEW.** Same class as `REN-2026-09-05-D1-02`
  (`TlasIntegritySnapshot`), which has now survived two sweeps without an
  issue number — worth filing together.
- **Description**: All three are `pub`, all three are documented as telemetry
  surfaces, and `grep -rn` across the whole workspace returns for each only
  its own definition plus doc references — zero call sites, zero test uses.
  The doc comments are specific and wrong:
  - `pending_destroy_static_bytes`: *"Companion to
    [`Self::pending_destroy_blas_count`] for `ctx.scratch` telemetry — the
    count alone can't show how much VRAM the queue is holding."*
    `CtxScratchCommand::execute` reads only `ScratchTelemetry.rows` (CPU-side
    `Vec` len/capacity rows produced by `VulkanContext::fill_scratch_telemetry`
    and `build_render_data`) plus the material dedup ratio. There is no
    deferred-destroy row, and `fill_scratch_telemetry` cannot be adding one —
    it would be a call site, and there are none.
  - `pending_destroy_blas_count`: *"Surfaced for [`drain_pending_destroys`]'s
    unit test and shutdown telemetry — the count must reach zero after a
    drain."* No such test exists (`AccelerationManager` needs a live device),
    and nothing logs it at shutdown.
  - `pending_destroy_scratch_count`: *"Surfaced for the deferred-destroy
    regression test and shutdown telemetry."* Same.
- **Evidence**: `grep -rn "pending_destroy_static_bytes()\|pending_destroy_blas_count\|pending_destroy_scratch_count" --include='*.rs' .`
  → definitions and doc-links only. `CtxScratchCommand::execute`'s body
  reads `tlm.rows`, `tlm.renderer_row_count`, `tlm.materials_*` and nothing
  else.
- **Impact**: A deferred-destroy queue that stops draining — the failure mode
  the countdown exists to make impossible, and the one a shortened countdown
  or a missed tick would produce — is unobservable at runtime. There is no
  console surface, no log, and no assertion. Secondarily, three doc comments
  assert a telemetry integration that a reader can reasonably act on
  ("`ctx.scratch` will tell me how much the queue holds") and will not find.
- **Related**: `REN-2026-09-05-D1-02` / `REN-2026-08-30-D1-01`
  (`TlasIntegritySnapshot`, the same "computed, `pub`, no reader" pattern in
  the same subsystem), #1228 (the underlying AS-telemetry gap),
  `REN-2026-09-06-D5-03` (the fourth #3840 symbol with no effective consumer).
- **Suggested Fix**: Add three rows to `ScratchTelemetry` from
  `VulkanContext::fill_scratch_telemetry` — `pending_destroy_blas`,
  `pending_destroy_scratch` (counts) and `pending_destroy_static_bytes`
  (bytes) — which makes all three doc comments true and gives `ctx.scratch`
  the queue-depth view it already claims to have. If that is not wanted,
  delete the accessors and the sentences that promise them; a `pub` accessor
  with no reader in a binary-only workspace is the #3884 class the project
  just spent a commit removing.

---

### REN-2026-09-06-D5-05: `destroy_screenshot_staging`'s SAFETY comment still carries the exact wrong caller claim `229306ce` just corrected in its depth-capture sibling


- **Severity**: LOW (an `unsafe` free justified by a property that does not
  hold; the free itself is sound for a different, unstated reason)
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/screenshot.rs` —
  the SAFETY block inside `VulkanContext::destroy_screenshot_staging`.
  Fixed sibling: `destroy_depth_capture_staging`
  (`context/depth_capture.rs`).
- **Status**: **NEW** — the unfixed half of `REN-2026-08-30-D5-06`'s class.
  That finding named `depth_capture.rs` only; `229306ce` ("Fix #3628: pin the
  depth-capture path's two ordering invariants") corrected it there and left
  the original the copy was made from.
- **Description**: The comment reads *"callers are the resize path in
  `ensure_screenshot_staging` (only reached between frames, before any copy is
  recorded against the new-sized buffer) and shutdown teardown (after
  `device_wait_idle`)"*. `ensure_screenshot_staging`'s sole caller is
  `screenshot_record_copy`, which runs **during** command-buffer recording —
  its own doc says "Called in `draw_frame()` at the tail of the `unsafe`
  block, after both the presentation pass and … `EguiPass` have written the
  swapchain, before `end_command_buffer`" — and `grep -n screenshot
  crates/renderer/src/vulkan/context/resize.rs` is empty, so there is no
  resize call site at all.

  The destroy *is* sound, for the reason the depth-capture sibling now states:
  `draw_frame` waits **both** frames-in-flight fences before any recording, so
  no submitted copy can still target the buffer being freed. That is the same
  both-slot wait #3442 flags as pinned by nothing that can see `draw.rs`'s
  `(f + 1) % MAX_FRAMES_IN_FLIGHT` — so here too the one correct reason is the
  one currently unguarded, and the comment points away from it.
- **Evidence**: `grep -rn "ensure_screenshot_staging\|destroy_screenshot_staging"
  crates/renderer/src/` → four hits total: the `screenshot_record_copy` call,
  the grow-branch destroy inside `ensure_screenshot_staging` itself, the
  definition, and `context/teardown.rs`'s shutdown call. The now-correct
  sibling comment in `depth_capture.rs` reads *"which runs DURING
  command-buffer recording (`draw.rs`), not between frames — there is no
  resize call site for depth-capture staging."*
- **Impact**: Documentation of an `unsafe` free. No runtime effect today. The
  risk is a future reader relocating `screenshot_record_copy` on the strength
  of a "between frames" guarantee it never had.
- **Related**: `REN-2026-08-30-D5-06`, #3628 (the sibling fix), #3442 (the
  unpinned both-slot fence wait that is the real invariant).
- **Suggested Fix**: Copy the corrected sibling comment across, adjusting the
  names — one caller during recording (`screenshot_record_copy` via
  `ensure_screenshot_staging`'s grow branch), one at shutdown after
  `device_wait_idle`, sound because `draw_frame` waits both FIF fences before
  recording. Both functions are now near-identical; a shared helper would stop
  the two comments diverging a third time.

---

### REN-2026-09-06-D5-06: memory-budget.md's `### Not yet ledgered` says "One is known" and then "Both are listed"


- **Severity**: LOW (doc-rot in the authoritative ledger)
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md` — the `### Not yet ledgered`
  subsection.
- **Status**: **NEW.** Not in the 151 open issues; not in the 2026-08-30 or
  2026-09-05 reports.
- **Description**: The subsection opens "A grep of this page for the owning
  subsystem name is the cheapest way to find a gap in it. **One** is known and
  unquantified:", lists a single bullet (`StagingPool` retained capacity), and
  closes "**Both** are listed rather than estimated on purpose: a fabricated
  number on this page is worse than an acknowledged hole".

  `git show 6cdb598c -- docs/engine/memory-budget.md` shows the section
  landed with two bullets — per-entity morph slots and the staging pool. The
  morph bullet was correctly removed when #3661 gave morph slots their own
  `## Morph-target GPU resources` section and the count was updated to "One",
  but the closing sentence was not.
- **Evidence**: The three quoted strings are adjacent in the current file.
  `git log -S "Not yet ledgered" -- docs/engine/memory-budget.md` →
  `6cdb598c`, whose diff carries both bullets.
- **Impact**: None at runtime. It matters only because this is the one
  subsection whose entire purpose is to be an accurate inventory of the page's
  own gaps, and a reader counting bullets against the prose will conclude one
  is missing from the render rather than from the sentence.
- **Related**: `REN-2026-09-05-D5-01` (the sibling stale-preamble fix in the
  same file, fixed by `b10a7b7e`), `REN-2026-09-06-D5-02` (a gap that belongs
  in this subsection, or better, in a real row).
- **Suggested Fix**: Change "Both are listed" to "It is listed", or restore a
  second bullet if `REN-2026-09-06-D5-02` is resolved by acknowledgement
  rather than by a row. Prefer the row.

---

### REN-2026-09-06-D6-01: yesterday's fix to the Dimension 6 caller bullet replaced two right facts with two wrong ones


- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 6, the
  "Single boundary" bullet), introduced by `2853464f`
- **Status**: NEW (the incorrect text is new; #3904, the issue whose fix
  introduced it, is closed)
- **Description**: `2853464f` ("Fix #3904: correct two stale NIFAL facts in
  the shared audit skill files") reworded the bullet to read:

  > `translate_material` has three production callers — `byroredux/src/scene/nif_loader.rs`
  > (loose NIF), `byroredux/src/cell_loader/spawn.rs` (REFR placement) and
  > `byroredux/src/cornell.rs` (the Cornell RT test harness).

  The count is right and the set is wrong in three ways. The reworded
  invariant sentence that follows it — "no `Material {…}` literal is
  constructed *outside* `translate_material` … an independently-built
  `Material` downstream is [a finding]" — then contradicts the file it just
  named.
- **Evidence**: `rg -n "translate_material" --type rust` gives four call
  sites, and `rg -n "^#\[cfg\(test\)\]" byroredux/src/cornell.rs` gives one
  hit, at line 1904:
  1. `byroredux/src/scene/nif_loader.rs` — production. ✔ named.
  2. `byroredux/src/cell_loader/spawn/mesh_instance.rs` — production. The
     bullet names `byroredux/src/cell_loader/spawn.rs`, which exists (so the
     path gate passes) but contains **no** `translate_material` call; the
     caller moved into the subdirectory. Yesterday's report said so
     explicitly and the fix wrote the pre-move path back.
  3. `byroredux/src/cell_loader/placement_lod.rs` — production (the exterior
     placement-LOD spawner, #2444). **Omitted entirely.** This is the caller
     `docs/engine/nifal.md` §3 singles out as the one exempt from the two
     Phase-2 resolvers, so it is the caller an auditor most needs to know
     about.
  4. `byroredux/src/cornell.rs` — the call is at line 2073, inside the
     `#[cfg(test)] mod tests` that opens at 1904. **Not a production
     caller**, and the bullet says so in its own parenthetical ("the Cornell
     RT *test* harness") while listing it as production.

  The self-contradiction: `cornell.rs`'s **production** half constructs seven
  `Material` literals directly — `matte`, `pbr`, `pbr_bsdf`, `pbr_bsdf_lobes`,
  `glass`, `emissive`, `fire_refraction` — called from ~15 sites across the
  harness. By the bullet's absolute phrasing those are the finding; in fact
  they are legitimate (an RT reference scene has no `Imported*` tier) and each
  carries a documented rationale (#2477, #2514). `crates/save/src/driver.rs`'s
  `restore_world` is a further documented non-literal producer (#2687).
- **Impact**: An auditor applying this bullet literally reaches one of two
  false conclusions: `placement_lod.rs` is invisible to them, or `cornell.rs`'s
  seven constructors are reported as a boundary violation. Both are exactly
  the failure the bullet's *own* closing clause was rewritten to prevent. The
  bullet has now been wrong in three successive states (two callers → three
  wrong callers), which is what a hand-maintained list does; the structural
  fix yesterday's report asked for — point at the guard test instead — was not
  applied.
- **Related**: #3904 (closed; this is its incomplete half), #2444,
  #3733 (the directory-scan rewrite of the sibling guard — the pattern to
  copy), #1114 (path/symbol convention).
- **Suggested Fix**: Replace the caller enumeration with the invariant plus
  its guard: no `Material` literal outside `translate_material` /
  `translate_texture_only_material` on a *content* path, enforced by
  `every_exterior_spawner_inserts_a_boundary_material`
  (`byroredux/src/material_translate.rs`), with the Cornell harness named as
  the one documented exemption. If a caller list is kept at all, derive it the
  way `documented_texture_role_list_matches_the_struct` derives the role
  count — that test already scans `.claude/commands/` files and could scan one
  more.

---

### REN-2026-09-06-D6-02: `cd8691be`'s new tree-wide `classify_pbr` gate reads `.rs` only, and `triangle.frag` — the render-side file the rule is about — still frames the deleted symbol as live


- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/workspace_hygiene_tests.rs`
  (`no_source_file_frames_the_deleted_classify_pbr_as_live`,
  `collect_live_classify_pbr_claims`) against
  `crates/renderer/shaders/triangle.frag`
- **Status**: NEW
- **Description**: `cd8691be` closed the fourth recurrence of "a doc names the
  deleted render-time `Material::classify_pbr` as live" (#1321 → #1522 →
  #1624 → #3869) and, correctly, replaced the single-file edit with a
  workspace-wide gate. The gate filters to `.rs` files:
  `if path.extension().and_then(|e| e.to_str()) != Some("rs") { continue; }`.
  One live claim survives, in the one file where the claim is most damaging.
- **Evidence**: `rg -n --glob '!target' -w 'classify_pbr' --glob '!*.rs'
  --glob '!*.md' .` returns exactly one hit,
  `crates/renderer/shaders/triangle.frag`:

  > `*   Legacy NIF (Oblivion / FO3 / FNV) — `classify_pbr` keyword`
  > `    fallback fills the same fields from texture-path tokens.`

  Present tense, no historic marker — the gate's `HISTORIC_MARKERS` list
  would reject this line verbatim if it could see it. The comment is wrong on
  two counts: the render-time `Material::classify_pbr` was deleted, and the
  live producer for legacy NIF content is `classify_legacy_pbr`
  (`crates/nif/src/import/mesh/`) at import time, not the core backstop
  `classify_pbr_keyword`. The sentence sits four lines below the block that
  declares "the shader is FORMAT-AGNOSTIC … Per-format branches in the shader
  were a smell we explicitly factored OUT", so the paragraph asserting the
  no-render-time-fallback rule is the paragraph breaking it.

  Two secondary reach gaps, noted for completeness rather than as separate
  findings: the gate also skips `.md`, and the recurrence history includes
  documentation (`ROADMAP.md` is discussed in `cd8691be`'s own message); and
  the directory-skip comment says `target/` and `.claude/issues/` are excluded
  while the code excludes `target` and `.git` — inert today because
  `.claude/issues/` holds no `.rs` files, but the comment does not describe
  the code.
- **Impact**: The gate's name is `no_source_file_frames_the_deleted_classify_pbr_as_live`,
  and GLSL is source in this workspace — a reader who sees the test green
  concludes the sweep is complete when the render-side instance is precisely
  the one still standing. This is the fifth instance of a class the project has
  now spent four fixes on; the fix that was supposed to end it does not reach
  the shader.
- **Related**: #3869 (closed, this is its reach gap), #1321, #1522, #1624,
  #3868 (a sibling open issue about *other* stale present-tense comments in
  `triangle.frag`), #2984 (`affected_shaders_include_constants_header` — the
  precedent for a Rust test that scans shader sources).
- **Suggested Fix**: Extend `collect_live_classify_pbr_claims`'s extension
  filter to `rs | vert | frag | comp | glsl | md`, then fix the one line it
  finds (name `classify_legacy_pbr` as the legacy producer and say the
  per-draw classifier was removed). The scan already walks the whole workspace
  tree, so this is a one-line predicate change plus the exclusions the
  directory comment already claims.

---

### REN-2026-09-06-D6-03: `translate_texture_only_material`'s contract prose is falsified by its own body and by its sibling guard


- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/material_translate.rs`
  (`translate_texture_only_material`'s doc block, against its own literal and
  against `every_exterior_spawner_inserts_a_boundary_material` in the same
  file)
- **Status**: NEW
- **Description**: This is the boundary's **second** production `Material`
  producer — the one for drawn surfaces with no source material record. Its
  rustdoc makes two load-bearing claims, and both are false against the live
  code.

  1. *"Three exterior draw populations are in this shape: LAND terrain
     (`cell_loader/terrain.rs`), distant terrain LOD (`terrain_lod.rs`), and
     object-LOD imposters (`object_lod.rs`)."* There are **five caller files
     and six call sites**.
  2. *"This is deliberately not a fourth ad hoc materialization site: it owns
     no scalar literals of its own. Every canonical value it produces comes
     from `Material::default()` or from `resolve_pbr`'s classifier."* It owns
     one — and it is the one that matters.
- **Evidence**: `rg -n "translate_texture_only_material" --type rust`,
  production sites only: `cell_loader/terrain.rs`, `cell_loader/object_lod.rs`,
  `cell_loader/terrain_lod.rs`, `cell_loader/terrain_lod_btr.rs` (#3336), and
  `cell_loader/water.rs` **twice** (#3733). The guard test 70 lines below the
  doc already knows this — its own failure message enumerates "the 6 known
  spawners (terrain, terrain_lod, object_lod, placement_lod, terrain_lod_btr,
  water)". Two statements about the same set, one file apart, disagreeing.

  For claim 2: the literal is
  `Material { texture_path, metalness: f32::NAN, roughness: f32::NAN,
  env_map_scale: 0.0, ..Material::default() }`. `env_map_scale: 0.0` is a
  scalar literal that **deliberately deviates** from `Material::default()`'s
  `1.0`, and it carries a 20-line comment explaining that `Material::default()`'s
  value "is the raw on-disk `BSShaderPPLighting` field value" and that
  inheriting it "would have switched distant terrain and LOD imposters into
  full-strength environment reflections as a side effect of a PBR-scalar fix".
  It is pinned by an assertion (`assert_eq!(m.env_map_scale, 0.0)` over three
  fixtures). The deviation is correct; the sentence saying it does not exist
  is not.
- **Impact**: Claim 2 is the *entire* argument for why this second producer is
  not a NIFAL boundary violation, so an auditor who checks it finds it false
  and has to re-derive the real argument ("it owns one documented deviation")
  from scratch. It also matters forward: the `..Material::default()` tail
  means a **newly added canonical `Material` field silently reaches all six
  exterior draw populations at its `Default` value**, with no compile error —
  the opposite of `translate_material`, whose exhaustive literal makes that
  impossible. `Material::default()` is demonstrably *not* a neutral-value
  struct (the `env_map_scale` comment says so outright), so the class of bug
  this permits has already happened once and was caught by a human, not a
  gate. Claim 1 additionally means mesh/ESM water — a population with
  different optical expectations from terrain — is absent from the
  documentation of the function that materializes it.
- **Related**: #2444 (the finding that created this function), #3336, #3733
  (the two spawners that arrived after the prose was written), #3073 (the
  named-default doctrine), #3912 (the same doctrine applied yesterday).
- **Suggested Fix**: Rewrite both sentences: name the five caller files (or
  better, point at `every_exterior_spawner_inserts_a_boundary_material`, which
  already owns the set), and state the real invariant — "one deviation from
  `Material::default()`, `env_map_scale = 0.0`, documented below". Consider
  pinning the deviation count: a source-scan of this function's literal
  asserting exactly one non-`Default` scalar assignment would make a second
  one a deliberate act rather than an accident.

---

### REN-2026-09-06-D6-04: the greyscale-palette LUT is still hand-copied at both particle spawn sites — the sibling half of the divergence #3589 closed one day earlier


- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/systems/particle.rs`
  (`apply_emitter_overlays`), against the two blocks assigning
  `preset.greyscale_lut_index` in `byroredux/src/scene/nif_loader.rs` and
  `byroredux/src/cell_loader/spawn.rs`
- **Status**: NEW
- **Description**: `84c4a1df` (#3589) routed the BGEM effect-shader payload
  through `apply_emitter_overlays` because packing it with a hand-copied line
  at each spawn site was "the same divergence-risk class #1513 closed this
  helper to prevent". The **texture those flags index** — resolved by #3590 in
  the same delta — was left outside the boundary and is still hand-copied at
  both sites today, immediately after the boundary call. So `effect_shader_flags`
  (the palette *enable* bits) now goes through the single boundary while
  `greyscale_lut_index` (the palette *LUT* those bits select) does not, even
  though they are the two halves of one authored feature.
- **Evidence**: A scan of every `preset.<field> = …` assignment at both spawn
  sites finds exactly one remaining outside the boundary, and it is the same
  expression twice:

  ```rust
  preset.greyscale_lut_index = em            // cell_loader/spawn.rs
      .greyscale_lut_map
      .as_deref()
      .map(|path| resolve_texture(ctx, tex_provider, Some(path)))
      .unwrap_or(0);
  ```
  ```rust
  preset.greyscale_lut_index = emitter       // scene/nif_loader.rs
      .greyscale_lut_map
      .as_deref()
      .map(|path| resolve_texture(ctx, tex_provider, Some(path)))
      .unwrap_or(0);
  ```

  Each carries a "Mirrored in the sibling site" comment — the literal marker of
  the pattern. Meanwhile `apply_emitter_overlays`'s own rustdoc calls it "the
  **single overlay boundary** that folds **every** authored emitter override",
  and its field-by-field paragraph enumerates seven overlays without
  mentioning this one. The load-bearing semantic — `.unwrap_or(0)` so an
  emitter with no authored LUT keeps bindless slot 0 (the shader's "no LUT"
  sentinel) rather than `resolve_texture`'s neutral-fallback handle for an
  absent path — is stated in prose at both sites and enforced by neither.
- **Impact**: Latent, not live: the two copies are byte-identical today, so
  **nothing renders wrong** — which is why this is LOW and not MEDIUM. The
  cost is that the boundary's stated guarantee is false, and the next change
  to LUT resolution has two places to land instead of one. Divergence here
  drops the FO4 greyscale-to-palette remap on one load path only — the remap
  #3897/#3898 measured across 30,166 FO4 shader properties — which is the
  invisible-on-one-path failure this boundary exists to make impossible.
- **Related**: #1513 (the boundary), #2610 / #3589 (the sibling field, fixed
  `84c4a1df`), #3590 (the LUT resolution, landed hand-copied), #3897/#3898
  (the population it affects). Not covered by #3927/#3928/#3929, which are
  about the palette *shader semantics*, not the overlay boundary.
- **Suggested Fix**: Add an eleventh parameter `greyscale_lut: Option<u32>`
  to `apply_emitter_overlays` and move `preset.greyscale_lut_index =
  greyscale_lut.unwrap_or(0)` inside it, leaving only the
  `resolve_texture` call (which needs `&mut VulkanContext`) at the call
  sites — the same shape `84c4a1df` used for `effect_shader`. Extend
  `apply_emitter_overlays_applies_color_rate_size_and_force_fields` and
  `apply_emitter_overlays_none_inputs_keep_preset_defaults` with the new
  field, again exactly as #3589 did, so the `unwrap_or(0)` sentinel is pinned
  once instead of restated twice in prose.

---

### REN-2026-09-06-D7-01: two premises in the Dimension 7 checklist name real symbols with the wrong meanings


- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 7,
  bullets 1 and 2), against `crates/renderer/src/vulkan/scene_buffer/upload.rs`
  (`upload_materials`) and `crates/renderer/src/vulkan/material.rs`
  (`GpuMaterial::as_bytes`, `hash_gpu_material_fields`)
- **Status**: NEW
- **Description**: Both are checkable-and-wrong, and both name a live symbol —
  which is what makes them worse than vague prose: an auditor can look the
  symbol up, find it, and conclude the bullet is verified.

  1. *"Per-frame SSBO sized to `min(intern_count, MAX_MATERIALS)`."*
     `MaterialTable::interned_count()` is a real accessor and it is the
     **wrong quantity** — its own doc reads "Total `intern()` calls so far
     this frame (hits + misses)", i.e. the *denominator of the dedup ratio*,
     which on a real cell is one to two orders of magnitude larger than the
     unique count. The SSBO is sized by the unique count.
  2. *"Hash/Eq treat `GpuMaterial` as raw bytes."* `GpuMaterial` has **no
     `Hash` impl** — derived or manual — and the dedup key has not been the
     byte string since #781. Only `PartialEq`/`Eq` are byte-level.
- **Evidence**: (1) `upload_materials` computes
  `let count = materials.len().min(MAX_MATERIALS);` behind a release
  `assert!(materials.len() <= MAX_MATERIALS, …)`, and dirty-gates the copy on
  `hash_material_slice(&materials[..count])`. `interned_count` appears
  nowhere in `scene_buffer/`. (2) `GpuMaterial::as_bytes`'s own doc states it:
  "`GpuMaterial` has no `Hash` impl; dedup is keyed on the field-walking
  `hash_gpu_material_fields` instead (#781 moved the index key off the struct
  itself)." `rg 'impl.*Hash for GpuMaterial|derive\(.*Hash'` over
  `material.rs` → no match.

  For the record, since the bullet's *intent* is the real invariant and it was
  checked properly: a field-name diff shows 108 declared `GpuMaterial` fields
  and 108 walked by `hash_gpu_material_fields`, symmetric difference empty;
  `DrawCommand::material_hash` reaches the same 108 through 97 explicit
  `write_*` calls plus a `supplemental_texture_indices[..12]` loop, with slots
  12–15 (`GLASS_ROUGHNESS_SCRATCH`, `GLASS_DIRT_OVERLAY`, `LIGHTING_MASK`,
  `BACK_LIGHTING`) written individually where their `GpuMaterial` fields sit;
  and `cargo test -p byroredux-renderer --lib
  material_hash_matches_gpu_material_field_hash` → **1 passed**. The
  invariant holds; only its description does not.
- **Impact**: (2) is the more consequential. It sends an auditor of the dedup
  key to look for a `Hash` impl and a padding-zeroing invariant that no longer
  exist, instead of at `hash_gpu_material_fields` — the hand-maintained
  108-field walk that is the *actual* single point of failure, and the one
  place a newly added `GpuMaterial` field can be silently omitted (the struct
  literal in `to_gpu_material` is exhaustive; the hash walk is not). The
  bullet's parenthetical "(depends on the Dim-3 scalar-fields + zeroed-pad
  invariant)" compounds it: `GpuMaterial` has zero pad fields today — all 108
  members are live named scalars (108 × 4 == 432 == `size_of`). (1) is
  milder but points an auditor at the wrong accessor when checking the one
  cap whose overflow path is live and counted.
- **Related**: #3846 (open — the sibling stale `GpuMaterial` size claim in
  `include/bindings.glsl`), #797 / #807 (the over-cap route), #781 (the move
  off the struct hash), #1368, and `AUDIT_SAFETY_2026-08-30.md`'s finding on
  the same "byte-level Hash / zeroed pads" claim in three `unsafe`-adjacent
  doc comments — this is the audit-skill copy of that same stale statement.
- **Suggested Fix**: Reword bullet 1 to `min(table.len(), MAX_MATERIALS)` (or
  simply "the unique-material count"), and bullet 2 to: "`Eq` is byte-level
  (`GpuMaterial::as_bytes`); the dedup **key** is the field-walking
  `hash_gpu_material_fields`, and `DrawCommand::material_hash` must stay in
  lockstep with it — pinned by
  `material_hash_matches_gpu_material_field_hash`." Naming the pin is the part
  that makes the bullet self-checking.

---

### REN-2026-09-06-D8-02: SVGF's progressive-accumulation flag ignores the light rig, although the engine already computes exactly that signal one scope away and spends it only on the caustic accumulator


- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs` (`camera_static`); `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (`caustic_scene_key`, `caustic_history_valid`); `crates/renderer/shaders/svgf_temporal.comp` (the `params.w` branch)
- **Status**: NEW
- **Description**: `#2468` built a per-frame "the scene that determines splat
  landing points is unchanged" signal: `caustic_scene_key` folds every
  caustic-source model matrix **and** every visible light's
  `position_radius` / `color_type` / `direction_angle` / `params`, and
  `caustic_scene_static` additionally requires `!rigid_instance_moved &&
  pose_dirty.is_empty()`. That composite signal is spent entirely on
  `caustic_history_valid`. SVGF is handed the camera-only half. The stated
  reason (`record_post_passes`' parameter comment) is that *"SVGF and TAA reject
  stale history per pixel"* — but the per-pixel rejection is purely geometric
  (`stableMeshIdsMatch` plus a normal cone). It cannot see a light that changed
  colour, intensity, radius, or position while the surface stayed put, which is
  precisely what the light half of `caustic_scene_key` was built to detect.
- **Evidence**:
  - `caustic_scene_key = fold_caustic_key_f32(caustic_scene_key, lights.len() as f32)`
    then a fold over each light's four `vec4`s, with the comment *"a lantern
    being carried, a light being coloured / dimmed by a weather or script
    change, or a light entering or leaving the visible set all move the pool."*
  - `let caustic_history_valid = camera_static && caustic_scene_static;` — the
    only consumer.
  - `svgf.upload_params(..., camera_static)` — SVGF gets the bare camera flag.
  - With `floorC = 0` the blend is `1/(histAge + 1)` and `histAge` saturates at
    255, i.e. a ~256-frame (~4.3 s at 60 FPS) time constant on the GI response.
  - Live animated-light producers exist: `byroredux/src/systems/light_anim.rs`
    and `append_combustion_surface_lights`
    (`crates/renderer/src/vulkan/volumetrics.rs`), whose sole caller is
    `crates/renderer/src/vulkan/context/assemble_camera_and_lights.rs` — the
    same function that computes `camera_static`, so both signals are already in
    scope together.
- **Impact**: Standing still in a torch-lit Bethesda interior, the *direct*
  flicker is correct (it is not denoised) but the bounce/GI response lags by up
  to ~4 s. The code comment calls the trade-off *"acceptable for the quality
  win"*, which is a considered decision — the finding is that it was taken
  without noticing the discriminating signal already exists and is free.
- **Related**: `#2468`; `REN-2026-09-06-D8-01` (same flag, sharper consequence).
- **Needs RenderDoc**: no.
- **Suggested Fix**: Thread the existing `caustic_scene_static` (or a
  lights-only sub-key) into the SVGF `params.w` decision so progressive
  accumulation only engages when the camera *and* the light rig are both
  unchanged; the value is already computed in the same function that calls
  `svgf.upload_params`.

---

### REN-2026-09-06-D8-03: two stale words in `/audit-renderer`'s own Dimension 8 checklist — "fog applied to direct only" and "caustic sampled via `usampler2D`" — both describe shapes the composite no longer has


- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 8, the "Composite reassembly" and "Caustic accumulator" bullets)
- **Status**: NEW — audit-infrastructure doc-rot; **the code is correct on both counts**
- **Description**: Two Dim-8 checklist assertions no longer match
  `crates/renderer/shaders/composite.frag`:
  1. *"Fog applied to direct only, not indirect."* The live shader attenuates
     the fully reassembled term: `combined = combined * vol.a + vol.rgb` for the
     froxel integral and `combined = combined * transmittance + aerial` for the
     beyond-grid tail, where `combined` is `direct + indirect * albedo + caustic`.
     Attenuating only the direct half would be physically wrong (transmittance
     applies to all radiance leaving the surface), so the **checklist is the
     stale side**, not the code.
  2. *"Caustic accumulator (`R32_UINT`) sampled via `usampler2D`."* The
     glass/MLP accumulator is `layout(set = 0, binding = 5) uniform usampler2DArray causticTex`
     — RGB across three `R32_UINT` array layers, fetched with
     `texelFetch(causticTex, ivec3(causticPixel, c), 0)`. Only the *water*-side
     accumulator (`binding = 8`, `waterCausticTex`) is a plain `usampler2D`.
- **Evidence**: `composite.frag`'s `glassCausticRaw` block and the two
  `combined = combined * …` fog lines; `docs/engine/shader-pipeline.md` does not
  cover either point, so the SKILL is the only place carrying them.
- **Impact**: An auditor working the Dim 8 checklist literally will either file
  a false positive against correct fog handling, or spend the bullet confirming
  a `usampler2D` that only half exists. This is the class the audit-common
  "verify the premise against current code" rule exists for.
- **Related**: `feedback_audit_findings.md` (stale-premise hygiene).
- **Needs RenderDoc**: no.
- **Suggested Fix**: Reword to *"Fog/volumetric transmittance applied to the
  reassembled `combined` (direct + indirect·albedo + caustics), not to `direct`
  alone"* and *"Caustic accumulators: glass/MLP is a three-layer
  `usampler2DArray`, water-side is a `usampler2D`; both divided by
  `CAUSTIC_FIXED_SCALE`."* Also correct the Dim 13 entry-point path — the
  `(jx, jy)` block now lives in `context/assemble_camera_and_lights.rs`, not
  `context/draw.rs`. Run `.claude/commands/_audit-validate.sh` after.

---

### REN-2026-09-06-D9-02: `SkinPushConstants`'s own doc comment still describes the pre-#3231 12-byte / three-`u32` block

- **Severity**: LOW (doc-rot)
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs` (the doc comment on `pub struct SkinPushConstants`)
- **Status**: NEW — sibling site of the now-fixed `REN-2026-08-30-D9-02` (that finding named
  the SAFETY comment on the `from_raw_parts` inside `SkinComputePipeline::dispatch`, which
  *has* been corrected to "six fields (u64, u64, u32, u32, u32, u32), 32 bytes"; the struct's
  own header was not updated in the same pass). Verified against code, not GitHub — no open
  issue matches `push_const|SkinPushConstants`.
- **Description**: The doc block immediately above the struct reads "**12 bytes (3 × u32).**
  std430 doesn't require 16-B block alignment when no vec4 follows, so we ship the tight
  layout." The struct beneath it has had six fields since #3231 and measures 32 B. The
  next paragraph *within the same struct*, on the `morph_delta_address` field, correctly
  explains the u64-first ordering and cites #3231 — so the header contradicts its own
  field docs three lines later.
- **Evidence**: `PUSH_CONSTANTS_SIZE` = `size_of::<SkinPushConstants>()`; both are pinned at
  32 by `push_constants_size_is_32_bytes`, whose body itself narrates the 12 → 32 growth.
  The GLSL `PushConstants` block in `skin_vertices.comp` ends with the comment "32 B total,
  matches Rust `SkinPushConstants` exactly".
- **Impact**: None at runtime — every consumer takes `PUSH_CONSTANTS_SIZE`, never the
  literal. The cost is that the one comment a reader hits *first* when opening this struct
  states a wrong number in a CPU/GPU layout contract, which is the failure mode the
  audit-common symbol-advisory rule exists to catch.
- **Related**: `REN-2026-08-30-D9-02` (the sibling comment, fixed), #3231.
- **Suggested Fix**: Replace "12 bytes (3 × u32)" with "32 bytes (2 × u64 at offsets 0/8,
  4 × u32 at 16/20/24/28; no interior or trailing padding)" and keep the existing
  128 B-minimum sentence. One line; already covered by `push_constants_size_is_32_bytes`.

---

### REN-2026-09-06-D9-03: the `bind_inverses` upload-failure path retries unboundedly with an un-gated per-frame `warn!`, against this subsystem's own convention

- **Severity**: LOW
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (the `unwrap_or_else` on `upload_pending_bind_inverses`)
- **Status**: NEW
- **Description**: The #3569 requeue turns a failed `bind_inverses` upload into an
  indefinite per-frame retry: the entries go back into `SkinSlotPool::pending_uploads`
  (prepended, so they are drained first next frame), get re-attempted, and — if the
  underlying host-visible map/flush failure is persistent rather than transient — fail
  again, log again, and requeue again, forever. `log::warn!("Failed to upload pending
  bind_inverses: {e}")` has no `Once` gate, no rate limit, and no attempt counter. Every
  sibling failure path in this exact file and its callee already has one:
  `failed_skin_slots` (#900, added because a retried `create_slot` logged 58 WARNs per 300
  frames), `failed_skin_blas` (#2802, the BLAS sibling of the same fix), and
  `SkinSlotPool::overflow_warned` (one-shot, with a silent `overflow_attempt_count` for the
  magnitude). This path is the odd one out, and #3569 is what made it retry at all.
- **Evidence**: `bind_inverse_upload_failed` is set unconditionally in the error arm with no
  counter alongside it; `requeue_pending` unconditionally re-inserts. `grep -rn
  "bind_inverse_upload_failed"` returns one write-true site, one reset site, and one reader
  (`app_frame.rs`) — no suppression state anywhere.
- **Impact**: On a persistently failing device (OOM on the upload heap, device-lost
  in progress), one WARN per frame per failing batch until the cell unloads — the same
  log-flooding #900 and #2802 were filed to stop, plus a per-frame staging write + flush
  attempt that will not succeed. It also masks the *first* failure in the flood, which is
  the diagnostically useful one.
- **Related**: #3569, #900, #2802, `SkinSlotPool::overflow_warned`. Pairs with `D9-01`
  (same error arm) and `D9-04` (same requeue).
- **Suggested Fix**: Add a bounded-retry counter (or a `Once`-gated warn plus a silent
  cumulative count surfaced through `SkinCoverageStats` / `skin.coverage`, mirroring
  `overflow_attempt_count`), and after N consecutive failures stop requeuing and instead
  route the affected slots through the `D9-01` "defined fallback" so they render bind-pose
  rather than retrying forever.

---

### REN-2026-09-06-D9-04: `SkinSlotPool::sweep` does not purge `pending_uploads`, so a requeued entry can outlive its slot's ownership

- **Severity**: LOW (latent — needs a persistent upload failure to reach)
- **Dimension**: Skinning
- **Location**: `crates/core/src/ecs/resources/skin_slot_pool.rs` (`SkinSlotPool::sweep`, `SkinSlotPool::requeue_pending`), consumed by `crates/renderer/src/vulkan/scene_buffer/upload.rs` (`SceneBuffers::record_pending_bind_inverse_copies`)
- **Status**: NEW
- **Description**: `sweep` is careful about eviction hygiene — it drops the doomed entity
  from `entity_to_slot`, `last_seen_frame`, `last_pose_hash`, `pose_dirty` and
  `rollback_pose_hash` (each with its own comment explaining why) — but leaves
  `pending_uploads` untouched. Before #3569 that was nearly unreachable, because
  `drain_pending`'s cap (1366) exceeds the pool's own capacity (1364), so a pending entry
  never survived the frame it was created in. The new requeue path is the first thing that
  can hold an entry across frames. If the failure persists ≥ `min_idle` (3) frames while the
  entity leaves the draw list, `sweep` returns its slot to the free list, a different entity
  can `allocate` that slot, and the stale `(slot, old_entity)` entry is still queued —
  `app_frame.rs`'s filter only drops it if the *old* entity's `SkinnedMesh` component is
  gone, which an off-screen-but-alive NPC's is not.
- **Evidence**: The two entries then land in the same drain (cap ≥ capacity), so
  `record_pending_bind_inverse_copies` builds two `vk::BufferCopy` regions with the **same**
  `dst_offset` and issues them in a single `cmd_copy_buffer`. The Vulkan spec does not
  specify the order in which a copy command's regions are applied, so which entity's
  bind-inverse matrices survive in that slot is unspecified — a coin flip, not the
  list-order last-write-wins one might assume from reading the loop.
- **Impact**: One entity renders with another's inverse-bind matrices — a scrambled skin,
  and (through `skin_vertices.comp` → the skinned BLAS) a wrong-geometry AS entry for it.
  Bounded to the two entities sharing the slot, and self-heals on the next successful
  upload for the *live* tenant. Not reachable without `D9-03`'s persistent-failure
  precondition, which is why this is LOW rather than a sibling of `D9-01`.
- **Related**: #3569, #1192 / SAFE-D7-NEW-02 (the cap that used to make this unreachable),
  #1791 / D6-01. Same requeue as `D9-03`.
- **Suggested Fix**: In `sweep`'s per-doomed-entity block, add
  `self.pending_uploads.retain(|(_, e)| *e != entity);` alongside the five maps it already
  cleans — the slot is being handed back to the free list, so any queued upload for it is
  by definition stale. One line, and it makes the "eviction hygiene" comment block
  complete rather than five-sixths complete. A unit test in the existing
  `drain_pending_*` / `requeue_pending_*` family covers it with no Vulkan device.

---

### REN-2026-09-06-ORCH-01: the emitted shader-recompile instruction does not run


- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (documentation / doc-rot)
- **Location**: `crates/renderer/build.rs` — module doc, and the `writeln!`
  that emits the "Then recompile shaders" line; the emitted result lands in
  `crates/renderer/shaders/include/shader_constants.glsl`.
- **Status**: NEW
- **Description**: Both documented invocations write the include path as
  `-I crates/renderer/shaders` — with a space. `glslangValidator` rejects that
  form outright. The emitted copy has a second defect: it drops `-o <output>`,
  so even with the spacing corrected it writes glslang's default output name
  rather than `<shader>.spv`.
- **Evidence**: reproduced against glslang `11:16.2.0`:
  ```
  $ glslangValidator -V -I . triangle.frag -o /tmp/t1.spv
  -I<dir> include path must immediately follow option (no spaces)
  ```
  The working forms both produce a correct 368 324-byte blob: `-I.` (no space),
  or CLAUDE.md's `glslangValidator -V triangle.frag -o triangle.frag.spv` run
  from inside `crates/renderer/shaders/` (relative `#include`s resolve against
  the including file's directory, so no `-I` is needed there at all).
  **CLAUDE.md's documented command is correct and was verified working** — only
  the two `build.rs` copies are broken.
- **Impact**: This is the instruction a contributor reads at the moment of
  maximum relevance — immediately after `cargo build` regenerates
  `shader_constants.glsl` following a constant or GPU-struct field change. That
  is precisely the lockstep chokepoint `feedback_shader_struct_sync.md` names as
  the project's #1 source of silent Rust↔GLSL desync. Severity stays LOW because
  the failure is loud and self-healing: glslang prints a specific diagnostic, and
  the obvious recovery (drop the `-I`) happens to work from inside the shaders
  directory. It is a papercut on a load-bearing path, not a correctness defect —
  and the sweep above proves no `.spv` has actually gone stale in practice.
- **Related**: `feedback_shader_struct_sync.md`; #3846 (`bindings.glsl` citing a
  nonexistent `gpu_material_size_is_396_bytes` pin) — the same class of defect
  in the same lockstep chokepoint, one file over.
- **Suggested Fix**: In `crates/renderer/build.rs`, change the module-doc line
  to `glslangValidator -V -I. <shader> -o <shader>.spv` (run from
  `crates/renderer/shaders/`), and give the emitted line the same treatment plus
  the missing `-o`. Optionally emit the whole-set loop above instead of a
  single-shader template, since a constants change is exactly the case where
  *every* dependent shader needs recompiling, not one.

### Completeness checks
- [ ] **SIBLING**: `docs/engine/shader-pipeline.md` and `CLAUDE.md` both carry
      shader-compile guidance — CLAUDE.md verified correct, shader-pipeline.md
      states only the SPIR-V target version (also verified correct). No other
      copies found.
- [ ] **TESTS**: no test executes a documented command line. A cheap guard is
      the byte-compare loop above as an `#[ignore]`d test, which would pin
      SPIR-V currency and the command line at once.

---

# Recorded regression checks (NOT findings)

Logged so the next sweep does not re-file them. Each was checked and its
guard verified in place.

### REN-2026-09-06-D3-07: (regression check, NOT a new finding): `DBG_*` `u32` exhaustion — #3563's guard verified in place


- **Severity**: n/a — informational
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/shader_constants_data.rs` (`DBG_BITS`), `crates/renderer/src/shader_constants.rs` (`dbg_bits_are_single_bit_and_pairwise_disjoint`, `dbg_bits_catalog_covers_every_dbg_constant`)
- **Status**: **Closed-issue verification — #3563 (`REN-2026-08-30-D3-01`), fix confirmed present and passing. Do NOT re-file.**
- **Description**: The dimension brief asks that an exhausted `DBG_*` mask "is itself a finding". Verifying the premise before writing one: the mask **is** exhausted, but the condition was filed as `REN-2026-08-30-D3-01`, fixed as #3563 (`d64f2fe3`, "guard the fully-allocated DBG_* bitfield against silent aliasing"), and the guard is live. Re-derived independently of any quoted figure: `DBG_BITS` holds **35** entries — 32 single-bit constants plus 3 compound unions; the union of the 32 singles is exactly `0xFFFFFFFF`, **zero** free bits, zero duplicates. `dbg_bits_are_single_bit_and_pairwise_disjoint` fails a 33rd bit rather than letting it land as a duplicate, and the catalog's doc states the exhaustion plainly. Remaining headroom is the two recyclable slots `DBG_RESERVED_20` (bit 5) and `DBG_RESERVED_200` (bit 9). **No regression.**
- **Evidence**: parsed `DBG_BITS`, resolved each name including the compound unions, folded the 32 singles → `0xFFFFFFFF`.
- **Impact**: The next two debug views must rename a reserved slot in place. The third has nowhere to go, and the catalog's own doc points that author at `GpuCamera.render_debug.w` — which `shader-pipeline.md` currently advertises as free. That is the entire reason **D3-01** is filed at MEDIUM rather than as cosmetic doc rot: the two are the same trap from opposite ends, and correcting the doc is what stops #3563's managed exhaustion from turning into a live defect.
- **Related**: #3563 (closed, verified); D3-01 above.
- **Suggested Fix**: None. Recorded so the measurement is on file and so a future sweep does not re-file the exhaustion as new.

---

---

## Needs-RenderDoc / Needs-Validation

Barrier, layout and GPU-visible-behaviour observations deliberately left as
observations. Per the project's standing guidance no edit is proposed for any of
these — each needs a RenderDoc capture or a `BYRO_VALIDATION=1` run.

### From `dim_1.md`

**None.** No finding in this run proposes a render-pass, pipeline or barrier
change. `REN-2026-09-06-D1-01`'s fix is pure CPU budget bookkeeping, and its
Suggested Fix explicitly rules out the one direction (early-ticking the
deferred-destroy queue) whose failure mode would be invisible to `cargo test`.

### From `dim_2.md`

**Nothing.** No finding proposes a render-pass, pipeline or barrier change; all
five are shader-source, test-source, or documentation edits verifiable by
`cargo test` and `grep`.

---

### From `dim_4.md`

Nothing in this section is a proposed change. Each is an observation whose
resolution requires a live device, per `feedback_speculative_vulkan_fixes.md`.

### From `dim_5.md`

**Nothing.** No render-pass, pipeline, or barrier edit is proposed anywhere in
this dimension. Every finding is either arithmetic over published constants or
documentation.

### From `dim_6_7.md`

None. Neither dimension touches render-pass, pipeline, barrier or
descriptor-lifetime state, and every finding here is decidable by source
inspection or by a test that already runs.

---

*Report generated by `/audit-renderer --focus 6,7` as a delta over
`docs/audits/AUDIT_RENDERER_2026-09-05_DIM6_DIM7.md`. Dimensions 1–5 and 8–23
were not run.*

### From `dim_8_13.md`

Both items below are pre-existing, deliberately deferred, and unchanged by this
window's churn. Recorded so the next RenderDoc session has them together; no
action recommended from a source-only audit, per the standing
no-speculative-Vulkan-fixes rule.

1. **`SvgfPipeline::dispatch`'s over-specified `src_access` / `src_stage`.**
   The pre-dispatch `GENERAL → GENERAL` barrier on `indirect_history[frame]` /
   `moments_history[frame]` names `SHADER_READ | SHADER_WRITE` with a
   `FRAGMENT_SHADER` source bit that `#962` / `REN-D10-NEW-05` already
   identified as redundant under the both-slots fence wait in
   `sync_and_acquire_frame`. Narrowing it is explicitly deferred to a
   RenderDoc-validated session; the sibling passes (`taa.rs`, `caustic.rs`,
   `volumetrics.rs`) share the pattern and would each need re-auditing with it.

2. **`TaaPipeline::dispatch`'s asymmetric pre/post barrier stage masks.** The
   pre-barrier uses `COMPUTE_SHADER → COMPUTE_SHADER` (the `FRAGMENT_SHADER`
   source bit was dropped under `REN-D11-NEW-05` on the argument that the
   previous frame's composite read is serialised structurally by the fence),
   while the post-barrier still names `FRAGMENT_SHADER | COMPUTE_SHADER` as
   destination (`#653`, deliberately covering a future fence relaxation). The
   asymmetry is intentional and documented at both sites; it is listed here only
   because any relaxation of the both-slots fence wait invalidates the
   pre-barrier's argument and not the post-barrier's.

### From `dim_11_12.md`

Observations only — no pipeline / render-pass / barrier edit is proposed
from these, per the standing no-speculative-Vulkan-changes rule.

1. **D11-01's visible magnitude.** What an unwritten-but-write-enabled
   fragment output actually produces is implementation-defined, and on the
   dev RTX 4070 Ti it is likely to be whatever the shader last left in that
   output register — plausibly a copy of `outColor`. A RenderDoc capture of a
   water-covered region should show the `raw_indirect` (attachment 4) and
   `albedo` (attachment 5) targets under the water surface and compare them
   against the same pixels one frame with water off. The *fix* does not need
   this capture — the shader-output/attachment mismatch is static and
   `cargo test`-pinnable — but the visual triage of "how bad is it today"
   does. Suggested scene: the same above/below-waterline captures
   `docs/smoke-tests/m-exteriors.sh` gained for #3821.
2. **D12-01's visible signature.** Two captures worth taking to size the
   impact before/after: (a) a Skyrim or FO4 exterior with an alpha-blended
   surface (glass pane, foliage card, hanging cloth) just outside the frustum
   casting a shadow into view — the shadow should soften/disappear as the
   camera pans it into frustum today, and stop changing after the fix; (b) a
   non-uniformly-scaled placed REFR (common on Bethesda architecture kits)
   visible only in a reflection — its reflected normals should change when
   the flags are assembled unconditionally.
3. **`taa.comp` / `svgf_temporal.comp` behaviour under a stale params UBO**
   (the reachable half of D12-02). Simulating an `upload_params` failure needs
   fault injection (`BYRO_VALIDATION` is not enough); the observable question
   is whether a one-frame-stale TAA param set is visually benign, which
   decides whether the D12-02 fix should latch the session off or just skip
   one frame's dispatch.
4. **Water pipeline / presentation pipeline have no SPIR-V reflection
   validation.** Every other pipeline built from this repo's own GLSL
   (`composite.rs`, `ssao.rs`, `compute.rs`/cluster_cull, `svgf.rs`,
   `taa.rs`, `bloom.rs`, `skin_compute.rs`, `volumetrics.rs`, `caustic.rs`,
   `texture_registry.rs`) calls `reflect::validate_set_layout` and `.expect`s
   on drift. `crates/renderer/src/vulkan/water.rs` and
   `crates/renderer/src/vulkan/presentation.rs` do not (`egui_pass.rs` and
   `frame_upscaler.rs` are excluded — their shaders are third-party).
   That is a test-gap observation rather than a
   defect — neither has drifted today — but it is the structural reason
   D11-01 could land: those two pipelines have no automated
   shader↔pipeline-declaration cross-check of any kind.

### From `dim_14_15_16.md`

**Nothing in this report requires a GPU capture to act on.** No render-pass, pipeline,
barrier, or descriptor change is proposed anywhere. Two observations are recorded as
capture-only if they are ever re-questioned:

- **The `caustic_accum` clear/decay sandwich's over-specified wait stages.** Both
  `CausticPipeline::dispatch` arms and `clear_for_skip` wait on
  `COMPUTE_SHADER | FRAGMENT_SHADER | TRANSFER` in the source scope. The in-code
  rationale (steady-state splat + composite read, or `clear_for_skip`'s TRANSFER, or
  `initialize_layouts`-fresh) is sound on paper and the #3646 both-ends closure is
  present; confirming the scopes are neither too narrow nor wasteful needs
  `BYRO_VALIDATION=1` sync-validation, not reasoning. **This is the four-copy sandwich
  already filed as #3844 — not re-reported.**
- **The `WaterCausticAccum` degraded path.** When `WaterCausticAccum::new` /
  `recreate_on_resize` fails, `context/resize.rs` binds the 1×1
  `placeholder_caustic_sink` to set 2 binding 0. Post-#3820 the shader bounds on
  `imageSize(waterCausticAccum)`, so every splat is now correctly rejected — but
  `composite.frag`'s `params.caustic_flags.x > 0.5` gate is what keeps the *read* from
  double-counting glass, and the two guards are on opposite sides of the frame. Proving
  they cannot disagree under a real allocation failure needs an induced-failure capture,
  not source reading. Recorded as an observation; no change proposed.

---

### From `dim_23.md`

Per `feedback_speculative_vulkan_fixes.md` and the SKILL: these are observations
about layout/barrier behaviour whose failure modes are invisible to `cargo test`.
**No edit is proposed for any of them.** They are recorded so a capture session
knows what to look at.

1. **The presentation render pass's incoming `SUBPASS_EXTERNAL` dependency has
   not been re-measured since #3426 added a second draw to its subpass.**
   `presentation.rs`'s `create` carries the measurement inline: 300 frames under
   `BYRO_VALIDATION=1` on a live FNV exterior, **2026-08-14**, zero SYNC-HAZARD
   reports. #3426 (2026-08-29) then moved the Scaleform overlay into this
   subpass, and `record_overlay` reads a vertex buffer, an index buffer and the
   scene instance SSBO in the vertex stage — neither `VERTEX_INPUT` nor
   `VERTEX_SHADER` is in this dependency's `dst_stage_mask`
   (`FRAGMENT_SHADER | COLOR_ATTACHMENT_OUTPUT`). The code already records this
   as `REN-2026-08-30-D23-05`, observation-only, with the correct re-run recipe:
   `BYRO_VALIDATION=1` via `docs/smoke-tests/m48-menu-load.sh` **with the overlay
   actually drawing**. That re-run has not happened. Carried forward, not
   re-filed.

2. **`record_fsr_barriers_after`'s `old_layout` values are an assertion about the
   vendored FFX backend's internal resource-state tracking, backed by evidence
   rather than proof.** The doc records: FSR 3.1.4, 2026-07-25, 900 frames under
   `BYRO_VALIDATION=1` across `quality` / `performance` / `native-aa` (three
   render→output ratios, both FIF slots), zero
   `VUID-VkImageMemoryBarrier-oldLayout-01197` and zero `SYNC-HAZARD-*` naming
   the upscale output or depth image. The doc's own instruction — **"Re-run that
   check when the SDK is upgraded"** — is the standing action item; the vendored
   SDK is unchanged since `c4b070a7`, so nothing is due yet. Flagged so an SDK
   bump does not slip past it.

3. **The dispatch-failure recovery arm's barrier sequence has no automated
   coverage.** Its correctness rests on "a rejected dispatch recorded nothing",
   which `byroredux_fsr3_sys::vendored_sdk_contract_tests` pins against the
   vendored sources statically — but the actual `GENERAL → TRANSFER_DST_OPTIMAL`
   acquire on the output image and the depth restore can only be exercised at
   runtime, through `BYRO_FSR_FORCE_DISPATCH_FAIL=1`. That toggle works in
   `--release` by design. A validation run of the recovery arm is cheap and, as
   far as this audit can tell, has not been repeated since #2140.

4. **The FP32 shader permutation is unexercised** — carried scope per the SKILL,
   not a defect. It needs a GPU without `shaderFloat16`; the dev box (RTX 4070
   Ti) has one. The permutation *is* built
   (`generate-fsr3-vulkan-shaders.sh` emits `-DFFX_HALF=0` variants), and the
   selection logic was verified correct against `GetDeviceCapabilitiesVK` this
   run (Coverage §9) — what is untested is the FP32 shaders' behaviour, not the
   branch that selects them.

5. **Not an FSR regression**: the exterior `CopyBufferToImage` VUID seen under
   `BYRO_VALIDATION=1` is pre-existing (project memory,
   `fsr_validation_and_fault_injection`). Recorded so it is not re-attributed.

---

---

## Dimension coverage index

| Dimensions | Agent report | Findings |
|---|---|---|
| 1 — Acceleration structures | `dim_1.md` | 7 |
| 2 — SSBO / ray queries | `dim_2.md` | 5 |
| 3 — GPU-struct layout | `dim_3.md` | 8 |
| 4 — Sync & barriers | `dim_4.md` | 7 |
| 5 — GPU memory & lifecycle | `dim_5.md` | 6 |
| 6, 7 — NIFAL material, material table | `dim_6_7.md` | 5 |
| 8, 13 — Denoiser/composite, TAA | `dim_8_13.md` | 5 |
| 9, 10 — Skinning, camera-relative precision | `dim_9_10.md` | 7 |
| 11, 12 — Pipeline/render pass, command recording | `dim_11_12.md` | 6 |
| 14, 15, 16 — Caustics, water, volumetrics & bloom | `dim_14_15_16.md` | 4 |
| 17, 19 — Disney BSDF & shadows, tangent space | `dim_17_19.md` | 5 |
| 18, 20, 21, 22 — Sky/weather, telemetry, Cornell, light animation | `dim_18_20_21_22.md` | 7 |
| 23 — FSR 3.1 upscaler & presentation | `dim_23.md` | 4 |
| Cross-cutting (orchestrator) | `dim_orch.md` | 1 |

**All 23 dimensions covered.** Every checklist bullet in every dimension is marked CLEAN with a naming symbol/test, or routed to a finding, in the per-dimension reports.
