# #3603 — REN-2026-08-30-D11-02: both `create_render_pass` call sites still describe a 7-attachment G-buffer with a reservoir attachment removed under #1583

**Labels**: `low,renderer,pipeline,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3603 --json state`.

---

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/context/init.rs:305-306`, `crates/renderer/src/vulkan/context/resize.rs:299-300`
- **Status**: OPEN
- **Description**: The two places that call `create_render_pass` both label it
  "Main render pass: 7 color attachments (HDR + G-buffer + raw_indirect + albedo +
  **reservoir**) + depth." The pass has **8** color attachments (+ depth as attachment 8),
  and the ReSTIR reservoir output at location 6 was deleted under #1583 — slots 6 and 7 are
  now the FSR reactive and transparency-and-composition masks. `helpers.rs`'s own
  `create_render_pass` header block (:148-190) is correct and enumerates all nine.
- **Evidence**:
  - `init.rs:305` `// 10. Main render pass: 7 color attachments (HDR + G-buffer +` / `:306` `// raw_indirect + albedo + reservoir) + depth.`
  - `resize.rs:299` `// Main render pass: 7 color (HDR + G-buffer + raw_indirect` / `:300` `// + albedo + reservoir) + depth.`
  - `helpers.rs:222-237` `color_refs` is 8 entries; `attachments` is 9 with `depth_attachment` last.
  - `reflect.rs:1118` `triangle_frag_declares_eight_color_outputs` (passing) is the live pin.
- **Impact**: Documentation only. The hazard is specific though: attachment-count drift is
  exactly the class of bug this dimension exists to catch (a blend-state array that does not
  match `attachmentCount` is `VUID-VkGraphicsPipelineCreateInfo-renderPass-07609`), and the
  two comments a reader hits *first* both state the wrong number and name a slot that is now
  something else entirely.
- **Needs RenderDoc**: no
- **Suggested Fix**: Replace both with "8 color attachments (HDR + normal + motion + mesh_id + raw_indirect + albedo + 2 FSR masks) + depth", or just point at `helpers::create_render_pass`'s header table so there is one copy.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D11-02

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
