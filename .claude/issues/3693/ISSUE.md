# #3693 — PERF-D9-2026-08-30-05: four declared per-frame renderer scratches are absent from `fill_scratch_telemetry`, against that function's own stated maintenance rule

**Severity**: LOW · **Dimension**: Telemetry & Origin Cost
**Location**: `crates/renderer/src/vulkan/context/mod.rs::fill_scratch_telemetry`

## Fix

Per the issue's own suggested fix, added four `rows.push(ScratchRow { … })`
entries and the two accessor methods it named:

- `blend_seen_scratch` (`FxHashSet<(u8, u8, bool, bool)>` on
  `VulkanContext`) — read directly, matching the existing pattern for the
  other in-crate hash-container rows (`skin_dispatch_seen_scratch`,
  `previous_rigid_models`, `current_rigid_models_scratch`).
- `tlas_addresses_scratch` and `tlas_missing_samples_scratch`
  (`AccelerationManager`) — new
  `tlas_addresses_scratch_telemetry()` /
  `tlas_missing_samples_scratch_telemetry()` accessors added right next
  to the existing `tlas_instances_scratch_telemetry()` in
  `acceleration/memory.rs`, and both rows routed through the same
  `if let Some(accel) = &self.accel_manager { … } else { 0, 0 }` shape the
  existing TLAS row already uses.
- `param_scratch` (`WaterPipeline`) — new `param_scratch_telemetry()`
  accessor added on `WaterPipeline` itself (`water.rs`), row routed
  through the same `if let Some(water) = &self.water { … } else { 0, 0 }`
  shape.

## SIBLING (issue's own checklist item)

The issue's own evidence enumerated exactly these four fields as the
complete gap (13 existing rows in the producer, 4 missing); no other
scratch field was found undeclared during this fix.

## TESTS (issue's own checklist item)

`fill_scratch_telemetry` needs a live `VulkanContext` to call — no
fixture exists in this crate. Matching this session's established
convention for that situation (e.g. #3690's `retention_hoisting_tests`,
#3682's extension to `rigid_history_hasher_tests`), added a static
source-scan test,
`scratch_telemetry_coverage_tests::fill_scratch_telemetry_covers_all_four_previously_missing_scratches`,
scoped to the file's production portion (before its first `#[cfg(test)]`
module, same convention `rigid_history_hasher_tests::production_src`
established), asserting all four new row names appear as
`name: "<field>"` inside the function's body (bounded between its own
signature and the next function, `fill_skin_coverage_stats`).

**Reintroduce-and-revert verification**: temporarily removed the
`blend_seen_scratch` row push — confirmed the new test failed with the
expected message naming that row. Restored the fix and reran — all
context-module tests (136, including the 3 `rigid_history_hasher_tests`
and the new coverage test) pass again.

## Verification

- `cargo check -p byroredux-renderer --tests`: clean, zero warnings.
- `cargo test -p byroredux-renderer --lib context::`: 136 tests passing,
  0 failing (+1 new).
- `cargo test -q -p byroredux-renderer`: 827 tests passing (+1), 0
  failing.
- `cargo check -p byroredux --tests`: clean (downstream crate).
- `cargo test -q --no-fail-fast` (full workspace): **7112 passing, 0
  failing**.
