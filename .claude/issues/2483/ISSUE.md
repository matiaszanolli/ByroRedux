# REN-D3-2026-08-07-03_MAT-D7-2026-08-07-01: Load-bearing layout doc comments quote superseded byte sizes (gpu_types.rs / constants.rs)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2483
**Finding ID**: REN-D3-2026-08-07-03_MAT-D7-2026-08-07-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 3 — GPU-Struct Layout **+** 7 — Material Table (cross-dimension duplicate, merged in source report)
**Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:84` and `crates/renderer/src/vulkan/scene_buffer/constants.rs:168` (`MAX_MATERIALS`)
**Status**: NEW — **scoped**: this report's merged finding covers 3 sites; 2 of them are already tracked by open issues (see Related) and are NOT re-filed here to avoid duplicate content. This issue covers only the 2 sites neither existing issue reaches.

## Description
Three in-code comments state byte sizes the code contradicts — the prose is the primary reference a future field-adder reads before touching a struct whose whole risk profile is silent byte drift:
1. `gpu_types.rs:84` — "The `size_of::<GpuInstance>() == 112` test below asserts the invariant" — sits directly under a layout history whose last line is `112 → 128 (#2219, ...)`, and the test actually asserts **128**. **Not yet tracked by any open issue.**
2. `gpu_instance_layout_tests.rs:97` — "rely on the size assertion above (112 B)" — **already tracked by #2433** (TD9-002), whose suggested fix explicitly includes correcting this "112 B" → "128 B" mention. Not re-filed here.
3. `constants.rs:168` — `MAX_MATERIALS` doc: "16384 × 300 B ≈ 4.9 MB per frame ... ≈ 9.8 MB total". `GpuMaterial` is **348 B** (pinned by `gpu_material_size_is_348_bytes`), so the real figures are 5.7 MB / 11.4 MB — matching `docs/engine/memory-budget.md:21`. The same doc also cites "the 4 GB total VRAM budget" while the current baseline note is 6 GB RT-minimum. **Not yet tracked by any open issue** (the sibling `gpu_instance_layout_tests.rs:939,990` 300 B sites are tracked by #2415, but `constants.rs:168` is a distinct location #2415 does not cover).

(Benign, not counted: `gpu_types.rs:123/126` and `descriptors.rs:317` / `upload.rs:558` use 112 B as deliberate *historical* context.)

## Impact
No runtime effect. Misleads the next author of a `GpuInstance` field addition or a VRAM-budget recalculation; the memory-budget arithmetic in `constants.rs` understates material-SSBO VRAM by ~16%.

## Related
**#2415** (TD3-208, this session — covers `gpu_instance_layout_tests.rs:939,990`'s stale 300 B `GpuMaterial` size, a *different* file from this issue's `constants.rs:168`). **#2433** (TD9-002, this session — covers `gpu_instance_layout_tests.rs:97`'s stale 112 B mention, already includes fixing it as part of its own suggested fix). This issue, #2415, and #2433 together close the full "5 sites" the merged dimension-3/7 finding originally identified — fix all in one pass per the source report's own recommendation.

## Suggested Fix
`s/112/128/` in the `gpu_types.rs:84` comment; recompute the `MAX_MATERIALS` doc arithmetic at 348 B (5.70 MB per frame / 11.4 MB total) and align the budget figure with `feedback_vram_baseline.md`'s 6 GB RT-minimum note.

## Completeness Checks
- [ ] **SIBLING**: Fix alongside #2415 and #2433 in one pass since they're the same drift class in the same file cluster
- [ ] **TESTS**: N/A (comment-only change)
