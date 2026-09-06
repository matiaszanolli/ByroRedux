# #4037 — REN-2026-09-06-D4-07: `WaterCausticAccum::clear_pre_render_pass`'s barrier comment claims it performs the `UNDEFINED → GENERAL` discard, contradicting `initialize_layouts` 60 lines above

**Labels**: low, renderer, sync, water, documentation, doc-rot

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
