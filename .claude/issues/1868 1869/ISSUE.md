# #1868: SAFE-2026-07-03-01: Residual ~222 renderer unsafe blocks lack a SAFETY comment (continuation of #1644)

**Severity**: MEDIUM
**Location**: `crates/renderer/src/` — worst by absolute count: `vulkan/composite.rs` 17, `vulkan/context/mod.rs` 16, `vulkan/context/helpers.rs` 16, `vulkan/texture.rs` 15, `vulkan/device.rs` 14, `vulkan/svgf.rs` 13, `vulkan/context/resize.rs` 13, `texture_registry.rs` 10, `vulkan/skin_compute.rs` 9, `vulkan/taa.rs` 9, `vulkan/compute.rs` 9, `vulkan/caustic.rs` 8, `vulkan/scene_buffer/upload.rs` 8, `vulkan/egui_pass.rs` 7.

## Summary
Independent recount across all of `crates/`: 545 non-test `unsafe {` block openers, 222
without a `SAFETY` comment on the same line or in the preceding 6 lines. Same rolling gap
tracked at #1644 (closed, fixed 124 of ~327 originally) — no open issue currently tracks
the remainder, so this continues that sweep.

## Suggested Fix
Resume the #1644 sweep starting with the small fully-uncommented files
(`texture_registry.rs`, `context/screenshot.rs`, `egui_pass.rs`, `compute.rs`,
`skin_compute.rs`), then the four large partially-commented files (`composite.rs`,
`context/mod.rs`, `context/helpers.rs`, `texture.rs`). Batch one SAFETY note per FFI
cluster rather than per call site.

---

# #1869: TD1-2026-07-03-01: crates/core/src/ecs/resources.rs crossed 2000 LOC (now 2077)

**Severity**: LOW
**Location**: `crates/core/src/ecs/resources.rs` (whole file, 2077 lines)

## Summary
File grew from 1867 to 2077 lines via commits `af6e4c9b` (#1791) and `e040231a` (#1796),
the pose-hash rollback feature for `SkinSlotPool`. Crosses the tech-debt audit's 2000-LOC
Dimension-1 complexity threshold for the first time.

Four natural domains, none currently separated:
- L1–228: `SystemList`, `SchedulerAccessReport`, `ScreenshotBridge`, `DeltaTime`/`TotalTime`/`EngineConfig`
- L229–479: `DebugStats`, `ScratchRow`, `ScratchTelemetry`
- L480–1537: `SkinCoverageStats`, `CpuFrameTimings`, **`SkinSlotPool`** (struct+impl+tests, ~1057 lines, over half the file)
- L1538–2077: `SelectedRef`, `ItemInstance` + `ItemInstancePool` + tests

## Suggested Fix
Extract `SkinSlotPool` (struct, impl, `Resource` impl, test module) into
`crates/core/src/ecs/resources/skin_slot_pool.rs`. No behavior change expected;
`skin_slot_pool_tests` should pass unmodified after the move.
