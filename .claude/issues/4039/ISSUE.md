# #4039 — REN-2026-09-06-D5-05: `destroy_screenshot_staging`'s SAFETY comment still carries the exact wrong caller claim `229306ce` just corrected in its depth-capture sibling

**Labels**: low, memory, renderer, sync, documentation, doc-rot

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
