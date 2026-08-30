# #3635 — REN-2026-08-30-D23-05: the presentation pass's incoming `SUBPASS_EXTERNAL` dependency no longer describes all of the pass's consumers after #3426 (observation — needs validation run)

**Labels**: `low,renderer,sync,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3635 --json state`.

---

- **Severity**: LOW (latent; no live hazard found by source inspection)
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/src/vulkan/presentation.rs` (`create`, the `incoming` `vk::SubpassDependency`)
- **Status**: NEW — **OBSERVATION ONLY, NOT A PROPOSED EDIT**
- **Description**: The incoming dependency is
  `src = COMPUTE_SHADER|TRANSFER|COLOR_ATTACHMENT_OUTPUT / SHADER_WRITE|TRANSFER_WRITE|COLOR_ATTACHMENT_WRITE`
  → `dst = FRAGMENT_SHADER|COLOR_ATTACHMENT_OUTPUT / SHADER_READ|COLOR_ATTACHMENT_WRITE`.
  Both scopes, and the long `#2465` / `#2143` comment blocks around them, were written when
  this pass contained exactly one fragment-only fullscreen draw sampling the upscale output.
  #3426 added a second draw into the same subpass that additionally reads a vertex buffer
  and an index buffer (`VERTEX_INPUT` / `INDEX_READ` / `VERTEX_ATTRIBUTE_READ`) and the
  scene instance SSBO in the **vertex** stage (`ui.vert`, `set = 1, binding = 4`). Neither
  stage is in the dst scope, and the `#2143` block's own rule for the *outgoing* dependency
  — "the dst scope below names those two consumers rather than being maximally wide, so it
  stays a description of the frame graph. A third consumer means extending it" — was not
  applied to the incoming side when the third consumer arrived.
- **Evidence**:
  - `presentation.rs::record_overlay` records `cmd_bind_vertex_buffers`,
    `cmd_bind_index_buffer`, `cmd_draw_indexed` inside the open render pass; `ui.vert`
    declares `layout(std430, set = 1, binding = 4) readonly buffer InstanceBuffer`.
  - `post_passes.rs::record_presentation_pass` sources those handles from
    `self.mesh_registry.get(ui_quad)` and `self.scene_buffers.descriptor_set(frame)`.
  - **Why no live hazard is claimed**: the UI quad's vertex/index buffers are written once
    at `register_ui_quad` (`context/resources.rs:362`) through `mesh_registry.upload`, i.e.
    a separate fenced one-time submit, not into this command buffer. The instance SSBO is a
    host-mapped write (`scene_buffer/upload.rs::upload_instances` → `mapped_slice_mut`),
    made visible by `vkQueueSubmit`'s implicit host-write dependency, not by an in-command
    barrier. The UI *texture* read in `ui.frag` is a fragment-stage `SHADER_READ` behind a
    `TRANSFER_WRITE` producer, which the existing `TRANSFER → FRAGMENT_SHADER / SHADER_READ`
    limb already covers. So the gap is descriptive, not (today) a hazard.
  - Also noted and *not* acted on: the `#2465` "MEASURED, deliberately unchanged" block
    records a `BYRO_VALIDATION=1` run of 300 FNV-exterior frames dated **2026-08-14** — i.e.
    before #3426 landed. That measurement no longer covers this pass's current contents.
- **Impact**: The dependency is no longer a truthful description of the frame graph, which
  is the property its own comments claim make narrow scopes safe here. If a future change
  makes any of the three newly-read resources a same-command-buffer write (a GPU-side
  instance-buffer compaction, a per-frame UI mesh rebuild), the missing limbs become a real
  WAR/RAW with nothing in `cargo test` able to see it.
- **Needs RenderDoc**: **yes** — validation-layer run required before *any* scope change,
  and a fresh `BYRO_VALIDATION=1` (sync validation) pass over a frame with a menu open is
  needed to re-establish the `#2465` measurement for the post-#3426 pass. No barrier edit is
  proposed here.
- **Suggested Fix**: None proposed. Minimum action: re-run the `#2465` measurement with the
  overlay actually drawing (`--menu` route, `docs/smoke-tests/m48-menu-load.sh`) and record
  the result in the existing comment block with its new date, so the next reader is not
  relying on a pre-#3426 measurement.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D23-05

> **Policy note (publish-time):** per the project's standing rule, no speculative Vulkan render-pass / pipeline / barrier restructure is proposed here. The observation is filed with its evidence; any scope change needs a `BYRO_VALIDATION=1` sync-validation run or a RenderDoc capture first.


## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
