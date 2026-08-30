# #3736 — TD1-2026-08-30-01: `VulkanContext` is a 728-line, 128-field struct — a God Object against CLAUDE.md Invariant 1, and the reason `context/mod.rs` will not stay split

**Labels**: bug, renderer, medium, tech-debt

---

- **Severity**: MEDIUM
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/renderer/src/vulkan/context/mod.rs` — `pub struct VulkanContext`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD1-2026-08-30-01`), HEAD `64f64480`

## Description

`pub struct VulkanContext` spans **728 lines and declares 128 fields** (re-verified at
HEAD: `awk '/^pub struct VulkanContext/,/^}/'` → 728 lines, 128 field declarations). It
is 30 % of the file's 2468 production LOC on its own, and it is the reason this file will
not stay split: the `context/` directory already has **13 siblings totalling 17 401 LOC**
(`init.rs` 1642, `draw.rs` 4959, `resize.rs` 1736, `post_passes.rs` 1190, …) — the
*behaviour* has been extracted repeatedly, but every extraction still reaches back into
the same 128-field struct, so the type stays a single mutable God Object and `mod.rs`
stays over threshold.

This is a different defect from "file is long", and it is why the previous splits did not
settle it. CLAUDE.md Architecture Invariant 1 is explicit: *"ECS over scene graph.
Components are data, systems are logic. **No God Objects.**"* `VulkanContext` is the one
place in the engine that invariant is not held.

## Suggested Fix — split by resource lifetime, not by line count

Group the 128 fields into sub-structs that each own one destroy-order group, which also
makes the reverse-order teardown in `teardown.rs` locally checkable instead of a
128-field manual sequence:

- `SwapchainResources` — swapchain, images, views, framebuffers, depth, render pass
  (everything `recreate_swapchain` rebuilds).
- `RtResources` — `accel_manager`, TLAS/BLAS handles, ray-query descriptor state.
- `PostChain` — SVGF, TAA, composite, bloom, volumetrics, FSR/upscaler.
- `OverlayResources` — UI quad, egui bridge, screenshot/depth-capture handles.
- `Telemetry` — the `fill_*` accessors' backing counters (`fill_upscaler_telemetry`,
  `fill_scratch_telemetry`, `fill_skin_coverage_stats`, `fill_rt_integrity_stats`,
  ~290 LOC, which can move wholesale with their data).

Two further pure-code moves are free and independent of the struct work:
`DrawCommand` + `to_gpu_material` + `material_hash` (~475 LOC) →
`context/draw_command.rs`; the telemetry fillers → `context/telemetry.rs`. Those two
alone drop `mod.rs` under threshold.

## Method constraints (project conventions — carry these into the fix)

- Per `feedback_safe_large_function_split`: **`sed`-extract the exact line ranges rather
  than retyping**, and diff-check before committing because `cargo fmt` reformats the
  **whole crate**.
- This is renderer-adjacent but **NOT** a render-pass or barrier change: moving a struct
  field into a sub-struct does not touch submission order, so
  `feedback_speculative_vulkan_fixes` does not gate it.
- **Do not reorder destroys while doing it.** A field regrouping must preserve the
  existing reverse-order teardown exactly.
- Effort: large — one commit per sub-struct.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the 13 `context/` siblings that reach into these fields)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
