# #4037: REN-2026-09-06-D4-07: `WaterCausticAccum::clear_pre_render_pass`'s barrier comment claims it performs the `UNDEFINED → GENERAL` discard, contradicting `initialize_layouts` 60 lines above

Labels: documentation, renderer, low, sync, water, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D4-07), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix




# #4038: REN-2026-09-06-D5-04: the three `pending_destroy_*` accessors have no caller anywhere in the workspace, and their doc comments name a `ctx.scratch` surface and a unit test that do not exist

Labels: bug, renderer, low, memory, tech-debt

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D5-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix




# #4039: REN-2026-09-06-D5-05: `destroy_screenshot_staging`'s SAFETY comment still carries the exact wrong caller claim `229306ce` just corrected in its depth-capture sibling

Labels: documentation, renderer, low, sync, memory, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D5-05), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix




# #4040: REN-2026-09-06-D5-06: memory-budget.md's `### Not yet ledgered` says "One is known" and then "Both are listed"

Labels: documentation, renderer, low, memory, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D5-06), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix




