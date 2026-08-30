# #3584 — REN-2026-08-30-D4-05: `presentation.rs`'s "#2465 — MEASURED, deliberately unchanged" justification predates #3426, which added three new access types to that pass

**Labels**: `low,renderer,sync,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3584 --json state`.

---

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/presentation.rs:292–330` (the `#2465 (REN-D4-2026-08-07-01)` comment block sitting between the `incoming` and `outgoing` `vk::SubpassDependency` declarations in `PresentationPipeline::create`)
- **Status**: NEW — stale in-code justification. **Observation only; no edit proposed.**
- **Description**: The presentation render pass declares
  `incoming` with
  `dst_stage_mask = FRAGMENT_SHADER | COLOR_ATTACHMENT_OUTPUT` and
  `dst_access_mask = SHADER_READ | COLOR_ATTACHMENT_WRITE`. Immediately below
  it, a long comment closes a prior audit finding with: "Verified 2026-08-14,
  release build, `BYRO_VALIDATION=1` … 300 frames on a live FNV exterior …
  **zero SYNC-HAZARD reports** … So this stays as-is. … Revisit only with an
  actual sync-val hazard or a driver-observed artifact; **a repeat of the
  static reading alone is not new evidence**."

  On 2026-08-29 (`b28acb0c`, #3426) the pass's *contents* changed. Its single
  subpass previously held exactly one non-blending fullscreen triangle. It now
  additionally holds `record_overlay`, which introduces three access types the
  measured pass did not have:
  - `VERTEX_INPUT` — `cmd_bind_vertex_buffers` / `cmd_bind_index_buffer` on the
    UI quad;
  - `VERTEX_SHADER` `SHADER_READ` — `ui.vert` reads
    `instances[gl_InstanceIndex]` from the instance SSBO (set 1, binding 4);
  - `COLOR_ATTACHMENT_READ` — the overlay pipeline blends against the
    attachment.

  None of the three appears in the `incoming` dependency's dst scope. Reading
  the source, each looks benign: the UI quad's vertex/index buffers are
  uploaded once at registration on a separate fence-waited submit; the
  instance SSBO's host write is published by the global
  `HOST_WRITE → VERTEX_SHADER | FRAGMENT_SHADER | COMPUTE_SHADER |
  DRAW_INDIRECT` `memory_barrier` recorded before the geometry pass
  (`draw.rs:3612`), which is a plain pre-render-pass barrier and therefore
  applies without needing to be restated as a subpass dependency; and the
  blend reads only what the tone-map triangle wrote in the same subpass, which
  rasterization order covers. So this is **not** reported as a defect.

  What *is* a defect is the standing justification: the comment's own escape
  clause ("revisit only with an actual sync-val hazard") is now unsatisfiable,
  because the pass that was measured on 2026-08-14 is not the pass that ships
  today, and nobody can produce a sync-val hazard for the new contents without
  re-running the measurement.
- **Evidence**:
  - `crates/renderer/src/vulkan/presentation.rs` — `incoming` masks as quoted;
    `#2465` comment at `:292`, "Verified 2026-08-14" at `:311`.
  - `PresentationPipeline::record_overlay` — `cmd_bind_vertex_buffers`,
    `cmd_bind_index_buffer`, `cmd_draw_indexed`, and the two-set rebind
    (`overlay.texture_set`, `overlay.scene_set`) against
    `self.overlay_pipeline_layout`. (The checklist's "both descriptor sets are
    rebound because the tone-map draw binds a layout-incompatible set 0" is
    **confirmed present** — see the verified-clean list below.)
  - `crates/renderer/shaders/ui.vert` — `GpuInstance inst =
    instances[gl_InstanceIndex];` in `main()`, i.e. a **vertex-stage** SSBO read.
  - `crates/renderer/src/vulkan/pipeline.rs::create_ui_pipeline` — blending
    enabled, one colour-blend attachment.
  - `git log -1 --format=%ad --date=short b28acb0c` → `2026-08-29`, four days
    after the recorded measurement.
- **Impact**: A future auditor reading this file is told the dependency scopes
  were empirically validated and should not be re-examined. That statement is
  now scoped to a superseded version of the pass. No runtime misbehaviour is
  claimed or implied.
- **Needs RenderDoc**: **yes** — settling whether the three new access types
  want naming in `incoming` requires a `BYRO_VALIDATION=1` run
  (`SYNCHRONIZATION_VALIDATION`) with the Scaleform overlay actually on screen
  (e.g. `--menu` on FO4/Skyrim per `docs/smoke-tests/m48-menu-load.sh`, plus
  `--bench-hold`). No such device exists in this session.
- **Suggested Fix**: **No barrier edit.** Annotate the `#2465` block to record
  that the measurement predates #3426 and covers only the tone-map triangle,
  and that a re-run with the overlay live is the outstanding evidence. If and
  only if that re-run reports a hazard should the masks be touched.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D4-05

> **Policy note (publish-time):** per the project's standing rule, no speculative Vulkan render-pass / pipeline / barrier restructure is proposed here. The observation is filed with its evidence; any scope change needs a `BYRO_VALIDATION=1` sync-validation run or a RenderDoc capture first.


## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
