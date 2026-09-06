# #4020 — REN-2026-09-06-D2-05: the audit's own Dimension-2 anchors have rotted — `_audit-common.md`'s `context/` layout row lists 10 of 18 files, and the BC1 bullet points at `draw.rs` for a bit that now lives elsewhere

**Labels**: low, renderer, shaders, tech-debt, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D2-05), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

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

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
