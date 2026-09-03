# #3694 — PERF-D9-2026-08-30-06: `ScratchTelemetry` covers zero of the seven engine-binary per-frame scratches, including `draw_commands` — the largest per-frame Vec in the process

**Severity**: LOW · **Dimension**: Telemetry & Origin Cost
**Location**: `byroredux/src/app_events.rs`, `byroredux/src/commands/world_info.rs`

Note: the GitHub issue body has a large "Prioritized Fix Order" appendix
tacked onto the end (items 1-23, spanning nearly every performance issue
from the `AUDIT_PERFORMANCE_2026-08-30.md` sweep) — that's the audit
document's own tail, not scoped to this individual finding, and the vast
majority of it was already fixed as separate numbered issues earlier this
session. Only the actual PERF-D9-2026-08-30-06 finding (the missing
engine-side scratch rows) is addressed here.

## Fix

`ScratchTelemetry` was populated exclusively by
`ctx.fill_scratch_telemetry(&mut tlm.rows)` (renderer-owned scratches
only, closed out for the renderer's own gaps in #3693). The engine
binary's own seven `build_render_data` scratches — `draw_commands`,
`water_commands`, `gpu_lights`, `gpu_fog_volumes`, `light_sort_scratch`,
`bone_world`, `skin_offsets` — are `App` fields, cleared on entry and
refilled every frame with capacity retained, and none appeared in any
row. `draw_commands` in particular is the quantity five of the renderer's
own reported rows (`gpu_instances_scratch`, `previous_models_scratch`,
`batches_scratch`, both rigid-motion maps) are `reserve()`d against, so
the driving quantity was itself invisible.

Per the issue's own suggested fix, appended seven `ScratchRow`s in
`byroredux/src/app_events.rs`, right after the existing
`ctx.fill_scratch_telemetry(&mut tlm.rows)` call (not folded into that
function — it only ever sees renderer-owned fields; these live on `App`).

Also implemented the "label the two groups" half of the suggested fix,
but via a lighter mechanism than a per-row tag: added
`ScratchTelemetry::renderer_row_count: usize`, set to `tlm.rows.len()`
right after the renderer's own rows land and before the engine rows are
appended. `ctx.scratch` (`CtxScratchCommand`) now `split_at`s on that
index and prints a `renderer:` / `engine:` header before each half —
avoids threading a new field through all 24+ existing `ScratchRow`
construction sites for a purely cosmetic grouping.

## SIBLING (issue's own checklist item)

This is the direct engine-binary counterpart to #3693 (the renderer-side
half of the same coverage question, fixed immediately before this issue
in the same session) — no other engine-owned per-frame scratch was found
missing during this fix; the issue's own evidence enumerated exactly
these seven as `render/mod.rs`'s own documented "owned by the caller"
set.

## TESTS (issue's own checklist item)

Unlike most renderer-adjacent commands, `CtxScratchCommand::execute`
takes `&World` only (no `VulkanContext`) — directly unit-testable. Added
`ctx_scratch_tests` in `world_info.rs`:
- `rows_are_grouped_by_renderer_row_count` — seeds a `ScratchTelemetry`
  with one renderer row and one engine row, asserts a `renderer:` header
  precedes a `engine:` header, the renderer row prints between them, and
  the engine row (`draw_commands`) prints after — pinning the exact
  defect this fix closes.
- `out_of_range_split_point_does_not_panic` — a `renderer_row_count`
  past `rows.len()` degrades to "no engine section" rather than
  panicking on `split_at`.
- `empty_rows_still_reports_renderer_not_initialized` — the pre-existing
  early-out message is unchanged by this fix.

**Reintroduce-and-revert verification**: temporarily reverted
`CtxScratchCommand`'s printer to the old flat, unlabeled loop — confirmed
`rows_are_grouped_by_renderer_row_count` failed ("must print a renderer:
header"). Restored the fix and reran — all 3 new tests pass again.

## Verification

- `cargo check -p byroredux-core -p byroredux --tests`: clean, zero
  warnings.
- `cargo test -p byroredux commands::world_info::ctx_scratch_tests`: 3
  tests passing, 0 failing (all new).
- `cargo test -q -p byroredux-core`: passing (unchanged — `ScratchTelemetry`
  gained one `Default`-initialized field, no behavior change to existing
  callers).
- `cargo test -q -p byroredux`: passing.
- `cargo test -q --no-fail-fast` (full workspace): **7115 passing, 0
  failing**.
