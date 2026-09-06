# #3993 — REN-2026-09-06-D5-02: the composite HDR pair and the depth / depth-history attachments — 40 B/px of unconditional render-extent VRAM — have no row anywhere in memory-budget.md

**Labels**: medium, memory, renderer, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D5-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM (authoritative-ledger gap on the page cited for the
  `< 4 GB` target; same class and severity as `REN-2026-08-30-D5-02`)
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md` — §"RT-Denoiser & Post-Process
  Screen-Sized Resources" (no subsection) and §"VRAM Rough Budget" (no row).
  Ground truth: `CompositePipeline` (`crates/renderer/src/vulkan/composite.rs`
  — the `hdr_images` and `scene_images` families and `HDR_FORMAT`), and
  `VulkanContext::depth_image` / `depth_history_image` allocated by
  `create_depth_resources` (`context/helpers.rs`) from `context/init.rs` and
  `context/resize.rs`.
- **Status**: **NEW.** `grep -in "hdr\|composite\|depth" docs/engine/memory-budget.md`
  returns no ledger row for any of them — the only hit that mentions them is
  the G-buffer roll-up row's own exclusion clause. Not in the 151 open
  issues; not in the 2026-08-30 or 2026-09-05 reports.
- **Description**: The VRAM roll-up's G-buffer row is scoped as
  "…22 B/px, × 2 FIF; **not** counting the separate HDR colour, depth, or
  depth-history attachments". Nothing else on the page counts them either, so
  the exclusion points at a row that does not exist.

  `CompositePipeline` owns **two** independent screen-sized image families,
  each `MAX_FRAMES_IN_FLIGHT` deep and each created at `extents.render`:
  `hdr_images` (the main HDR attachment) and `scene_images` (added by the FSR
  tail — "the single image that either FSR or the native bridge consumes";
  the field's own comment explains why the two must stay distinct). Both are
  `HDR_FORMAT = R16G16B16A16_SFLOAT`, 8 B/px, both `GpuOnly`, both
  unconditional. That is 4 images × 8 B/px = **32 B/px**.

  `depth_image` and `depth_history_image` add 4 B/px each at render extent
  (`find_depth_format` selects `D32_SFLOAT`; `depth_capture_record_copy`
  refuses anything else since #3570) — **8 B/px** more, single-buffered.

  Total **40 B/px** of unconditional render-extent VRAM with no row: 83.0 MB
  at 1080p, 331.8 MB at 4K native. For scale, that is more than the SVGF
  row (~83 MB) which has a whole subsection, and more than the TAA
  (~33 MB), Bloom (~11 MB) and SSAO (~4 MB) rows combined.
- **Evidence**:
  - `composite.rs`: `pub const HDR_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;`
    with the doc comment "RGBA16F = 8 bytes/pixel"; `pub hdr_images: Vec<vk::Image>`
    and `pub scene_images: Vec<vk::Image>`, both filled by
    `for i in 0..MAX_FRAMES_IN_FLIGHT` loops in `new_inner`, both
    `.extent(… width: extents.render.width, height: extents.render.height …)`,
    both `location: gpu_allocator::MemoryLocation::GpuOnly`.
  - `context/resize.rs` calls `create_depth_resources(…, self.frame_extents.render,
    self.depth_format, …)` twice — `"depth_buffer"` and `"depth_history"`.
  - `docs/engine/memory-budget.md`'s G-buffer row carries the exclusion text
    quoted above; no `### HDR`, `### Composite`, or `### Depth` heading exists
    (`grep -n "^### " docs/engine/memory-budget.md`).
- **Impact**: Doc-trust, but on the page every other subsystem's budgeting is
  derived from, and with a concrete downstream consumer: `REN-2026-09-06-D5-01`
  cannot reserve what the ledger does not name, so this omission propagates
  into the live BLAS-eviction threshold. The `Estimated total ~1.81 GB`
  roll-up understates by ~83 MB at 1080p and the 4K peak by ~332 MB.
  `scene_images` in particular is *newer* than the section around it — it
  arrived with the FSR tail, exactly the kind of growth the page's own
  §"Not yet ledgered" preamble ("a grep of this page for the owning subsystem
  name is the cheapest way to find a gap in it") is meant to surface.
- **Related**: `REN-2026-08-30-D5-02` (same class — a real allocation with no
  row; fixed for the staging pools), `REN-2026-09-06-D5-01`,
  `REN-2026-09-06-D5-06`.
- **Suggested Fix**: Add a `### Composite HDR intermediates + depth` subsection
  under "RT-Denoiser & Post-Process Screen-Sized Resources" with the 32 + 8
  B/px split, and a matching roll-up row; then delete the G-buffer row's
  exclusion clause or repoint it at the new section. Follow the
  `SVGF_BYTES_PER_PIXEL` / `CAUSTIC_BYTES_PER_PIXEL` / `FROXEL_BYTES_PER_SLOT`
  precedent — derive a `COMPOSITE_BYTES_PER_PIXEL` from `HDR_FORMAT` and the
  two families' arity and pin the doc against it, so the next image family
  added to `CompositePipeline` fails a test instead of drifting. Also worth
  a row while nearby: `ClusterCullPipeline`'s light-index buffers are
  `TOTAL_CLUSTERS (16×9×24) × MAX_LIGHTS_PER_CLUSTER (512) × 4 B ≈ 7.1 MB`
  per FIF — fixed-size, not resolution-scaled, and likewise unledgered.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
