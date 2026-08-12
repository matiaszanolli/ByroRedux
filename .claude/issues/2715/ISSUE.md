# #2715: UI overlay rewrites a bindless descriptor on the previous, still-pending frame slot

- **Severity**: HIGH (`_audit-severity.md`: "Vulkan spec violation → at least HIGH")
- **Dimension**: 7 — Worker Threads (UI host layer ↔ renderer boundary)
- **Location**: `byroredux/src/main.rs:662-695`, `crates/renderer/src/texture_registry.rs:1460-1488`,
  `crates/renderer/src/vulkan/context/draw.rs:1460`
- **Status**: NEW
- **Description**: `TextureRegistry::apply_descriptor_write` writes the new descriptor
  **immediately** into `bindless_sets[self.current_slot]` and defers the write for every *other*
  slot into `pending_set_writes`. Its SAFETY comment justifies the immediate write with
  "`self.current_slot` is being recorded by the CPU right now — no submitted command buffer can be
  reading `bindless_sets[self.current_slot]` concurrently". `current_slot` is set by
  `TextureRegistry::begin_frame`, whose **sole call site is inside `draw_frame`**
  (`draw.rs:1460`), after the both-slot `wait_for_fences`. The UI overlay path runs
  `ui.tick()` → `ui.render()` → `texture_registry.update_rgba(...)` in `main.rs` **before**
  `ctx.draw_frame(...)` is entered. At that moment `current_slot` still holds the *previous*
  iteration's frame index, whose command buffer was submitted at the end of the previous
  iteration and is in the pending state. `MAX_FRAMES_IN_FLIGHT` is 2
  (`crates/renderer/src/vulkan/sync.rs:6`), so `current_slot` is deterministically the slot the
  in-flight submission is using.
  The bindless layout is created with `PARTIALLY_BOUND | UPDATE_AFTER_BIND`
  (`texture_registry.rs:331-332`) — **not** `UPDATE_UNUSED_WHILE_PENDING`. `UPDATE_AFTER_BIND`
  covers the bind→submit window, not the pending window; the layout's own comment states the
  safety argument as *"safe because only previously-unbound array indices are written"*, which is
  exactly the premise `update_rgba` breaks: it rewrites an **already-bound, actively-sampled**
  array index. And even if `UPDATE_UNUSED_WHILE_PENDING` were added, its exemption is conditioned
  on the descriptor not being *dynamically used* by the pending command buffer — here it is:
  `draw.rs:2681-2691` appends a UI instance carrying `texture_index: ui_tex`, `geometry_pass.rs:62`
  binds `texture_registry.descriptor_set(frame)` (== `bindless_sets[frame]`), and `ui.frag` samples
  that index every frame the overlay is up.
- **Evidence**:
  - `texture_registry.rs:1476` — `self.bindless_sets[self.current_slot]` as the immediate write target.
  - `draw.rs:1460` — `self.texture_registry.begin_frame(&self.device, frame);` — the only writer of
    `current_slot`, inside `draw_frame`, after the fence wait.
  - `main.rs:686` — `.update_rgba(upload_ctx, handle, ui_w, ui_h, pixels)`, ~110 lines before the
    `ctx.draw_frame(FrameInputs { … ui_texture_handle: ui_tex, … })` call at `main.rs:796`.
- **Trigger Conditions**: any frame `N ≥ 2` in which the UI overlay is visible and frame `N-1`'s
  submission has not yet retired when the CPU reaches the UI block. This is the *common* case, not
  a narrow window — the CPU normally runs a frame ahead.
- **Impact**: host writes to a descriptor a live shader invocation is reading. Consequence is
  driver-dependent (torn descriptor read → sampling a destroyed-or-mismatched image view). Note the
  rendered *content* is still correct by construction — the current frame's set gets the same
  payload through the `pending_set_writes` flush — so this failure is invisible without validation,
  which is precisely why it has survived unexamined.
- **Verification Path**: **NOT observable in `cargo test`** (no headless device assertion covers
  descriptor-in-use). Cheapest confirmation: a **release** run with `BYRO_VALIDATION=1` and `--swf`.
  The expected signal is `VUID-vkUpdateDescriptorSets-None-03047` (descriptor set in use by a
  command buffer in the pending state) firing once per frame while the overlay is visible.
  Treat the finding as confirmed only once that message is captured; the source-order argument
  above is what makes it worth spending the run on.
- **Related**: #92 (the `pending_set_writes` deferral this path bypasses); #134 (the deferred image
  destruction that *does* cover the image, see §3.6); CONC-D7-UI-03 below shares the call site.
- **Suggested Fix**: make the immediate-write path refuse to run outside a recording window — e.g.
  have `apply_descriptor_write` queue for **all** slots (including `current_slot`) when a
  `recording: bool` latch set by `begin_frame` / cleared at submit is false. Cheaper interim fix:
  move the UI tick/render/upload block from `main.rs` to inside `draw_frame` after
  `texture_registry.begin_frame`, which restores the invariant the SAFETY comment asserts.

---

---
**Source**: `docs/audits/AUDIT_CONCURRENCY_UI_2026-08-12.md` (finding `CONC-D7-UI-01`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

