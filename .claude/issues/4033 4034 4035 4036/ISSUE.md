# #4033: REN-2026-09-06-D4-03: the authoritative submission-order block enumerates two barrier-only steps but omits the frame's two most load-bearing barriers

Labels: documentation, renderer, low, sync, shaders, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D4-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix




# #4034: REN-2026-09-06-D4-04: `image_health_docs_no_longer_claim_fence_alone_proves_host_visibility` scans `draw.rs` for a call site #3282 moved to `sync_and_acquire_frame.rs`

Labels: bug, renderer, low, sync, test-gap

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D4-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix




# #4035: REN-2026-09-06-D4-05: `draw_frame_does_not_re_upload_bloom_params_every_frame` scans `draw.rs`, but the per-frame UBO section it guards moved to `build_and_upload_instances.rs`

Labels: bug, renderer, low, sync, test-gap

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D4-05), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix




# #4036: REN-2026-09-06-D4-06: `signal_temporal_discontinuity`'s `previous_rigid_models.clear()` is inert at all three of its in-`draw_frame` call sites

Labels: bug, renderer, low, sync

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D4-06), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix




