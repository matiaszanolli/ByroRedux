===== ISSUE 3995 =====
REN-2026-09-06-D8-01: SVGF's camera-static progressive-accumulation flag zeroes the α floor, silently cancelling the `svgf_recovery_frames` window that `signal_temporal_discontinuity` exists to install
STATE: OPEN
LABELS: bug renderer medium shaders 

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D8-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix

===== ISSUE 3996 =====
REN-2026-09-06-D1-02: `audit-renderer/SKILL.md`'s Dimension-1 checklist carries three stale claims, and one of them has now manufactured a false finding in two consecutive sweeps — the most recent explicitly recommended for GitHub filing
STATE: OPEN
LABELS: documentation renderer low tech-debt doc-rot 

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

===== ISSUE 3997 =====
REN-2026-09-06-D1-03: `memory-budget.md`'s Acceleration-Structures section names a file the per-frame eviction call left, and its eviction-site census is one site short
STATE: OPEN
LABELS: documentation renderer low memory doc-rot 

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

===== ISSUE 3998 =====
REN-2026-09-06-D1-04: `memory-budget.md`'s AS constants ledger is one value wrong and one constant short — the skinned-refit rebuild threshold is no longer flat 600, and `MAX_STATIC_BLAS_RESTORES_PER_FRAME` has no row
STATE: OPEN
LABELS: documentation renderer low memory tech-debt doc-rot 

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D1-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

