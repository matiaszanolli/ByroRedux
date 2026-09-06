# #3988 — REN-2026-09-06-D23-01: the BLAS-budget VRAM reservation excludes the default upscaler entirely — its signature takes only the render extent, so FSR's output-resolution images and SDK working set are structurally unrepresentable

**Labels**: medium, memory, renderer, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D23-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs`
  (`screen_scaled_reservation_bytes`, `blas_budget_for_heap`);
  `crates/renderer/src/vulkan/acceleration/memory.rs`
  (`AccelerationManager::recompute_blas_budget`);
  callers in `crates/renderer/src/vulkan/context/init.rs` and
  `crates/renderer/src/vulkan/context/resize.rs`
- **Status**: NEW (no open issue matches `budget`/`reservation`/`vram` in
  `/tmp/audit/renderer/open_titles.txt`; `AUDIT_TECH_DEBT_2026-09-05.md` reviewed
  this function only for constant-derivation hygiene and passed it — it did not
  question coverage)
- **Description**: `#3839` made the static-BLAS residency budget honest by
  subtracting what the resolution-scaled post-process passes hold:
  `blas_budget_for_heap(heap, reserved) = (heap − reserved) / 3`. The reservation
  covers the froxel grid, SVGF and the caustic accumulator. It does **not**
  cover either of the two allocations the **default** upscaler holds, and its
  signature cannot be made to:
  1. `FrameUpscaler::create_outputs` allocates `MAX_FRAMES_IN_FLIGHT`
     `HDR_FORMAT` (`R16G16B16A16_SFLOAT`, 8 B/px) images at
     `self.extents.**output**`. `screen_scaled_reservation_bytes` takes
     `render_extent: vk::Extent2D` and every caller passes
     `frame_extents.render`, so there is no output extent in scope to bill this
     against.
  2. The FSR SDK's own `VkDeviceMemory` is allocated by the vendored FFX Vulkan
     backend, outside `gpu-allocator`. `FrameUpscaler::build_summary`'s own doc
     says it "is invisible to `ctx.memory` unless reported here". It is invisible
     to the reservation too.

  This is materially different from the omissions the function's doc comment
  disclaims. That comment excuses "textures, geometry pools and the swapchain" —
  all demand-driven or allocator-visible. The FSR terms are neither: they are
  *fixed* for a swapchain generation, they scale with the **output** resolution
  the reservation never sees, and item 2 is accounted for nowhere in the engine.
- **Evidence**:
  ```rust
  // predicates.rs — only render-extent terms; no output-extent parameter exists
  pub(super) fn screen_scaled_reservation_bytes(
      render_extent: vk::Extent2D,
      volumetrics: VolumetricsConfig,
  ) -> vk::DeviceSize {
      let pixels = u64::from(render_extent.width) * u64::from(render_extent.height);
      ...
      froxel_bytes
          .saturating_add(pixels.saturating_mul(u64::from(SVGF_BYTES_PER_PIXEL)))
          .saturating_add(pixels.saturating_mul(u64::from(CAUSTIC_BYTES_PER_PIXEL)))
  }
  ```
  ```rust
  // memory.rs — the only production consumer
  let reserved = screen_scaled_reservation_bytes(render_extent, volumetrics);
  let updated = blas_budget_for_heap(self.blas_heap_bytes, reserved);
  ```
  Both call sites pass the render extent: `accel.recompute_blas_budget(render_extent, …)`
  (`init.rs`) and `accel.recompute_blas_budget(self.frame_extents.render, …)`
  (`resize.rs`).

  Magnitude, derived from named constants only (`FROXEL_BYTES_PER_SLOT = 44`,
  `SVGF_BYTES_PER_PIXEL = 40`, `CAUSTIC_BYTES_PER_PIXEL = 24`,
  `froxel_xy_divisor = 8`, `froxel_z_slices = 64`, `MAX_FRAMES_IN_FLIGHT = 2`),
  for the shipped default (1080p output, FSR Quality → 1280×720 render):

  | Term | Reserved? | Bytes |
  |---|---|---:|
  | froxel grid (160×90×64 × 44 × 2) | yes | 81.1 MB |
  | SVGF (921 600 px × 40) | yes | 36.9 MB |
  | caustic (921 600 px × 24) | yes | 22.1 MB |
  | **reservation total** | | **140.1 MB** |
  | FSR upscale outputs (1920×1080 × 8 × 2 FIF) | **no** | 33.2 MB |
  | FSR SDK working memory | **no** | reported at runtime; the `ctx.upscaler` sample in `fsr3-troubleshooting.md` reads 31.8 MB at a *1280×720* output |

  `docs/engine/memory-budget.md`'s own fixed-floor table already carries the row
  "FSR 3.1 upscaler output (2 FIF, output resolution) | ~33 MB (1080p) | ~133 MB
  (4K) — SDK working memory not separately tracked", so the doc knows about a
  cost the code's reservation cannot see.
- **Impact**: The BLAS budget over-states available VRAM by roughly a third of
  the omitted bytes (`/3`). The error's *sign is adversarial to the default
  path*: switching from `--upscaler taa` to `fsr3/quality` or `fsr3/performance`
  shrinks every reserved term quadratically with the render extent while the two
  FSR terms do not shrink at all (the outputs are output-resolution; the SDK
  context is created for `max_upscale_size`). So the reservation's *relative*
  under-count is largest exactly on the presets the engine ships by default —
  ~24 % under-counted at 1080p/Quality from the output images alone, before the
  SDK working set. The failure mode is the one #3839 was written to remove:
  eviction that starts too late on small-VRAM cards, ending in an allocator
  failure rather than a reclaim. Not corruption, not a crash on the 12 GB dev
  card — hence MEDIUM rather than HIGH.
- **Related**: #3839 (the reservation), #2158 / #2829 (FSR teardown, which
  already proves the SDK context owns memory nothing else tracks),
  `docs/engine/memory-budget.md` §"FSR 3.1 Upscaler"
- **Suggested Fix**: Widen the parameter to the whole `FrameExtentSet` (or add an
  `output_extent`) and add two terms: `MAX_FRAMES_IN_FLIGHT × output.width ×
  output.height × 8` for the upscale outputs, and the SDK figure — which is
  already in hand and costs nothing to plumb, since
  `FrameUpscaler::build_summary` calls `fsr3::Context::memory_usage()` once per
  swapchain generation and caches it. Pin the new terms the way
  `froxel_grid_cost_matches_the_memory_budget_doc` pins the froxel term. If the
  SDK term is judged too awkward to thread, adding the output-image term alone is
  strictly better than today and needs no new data.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
