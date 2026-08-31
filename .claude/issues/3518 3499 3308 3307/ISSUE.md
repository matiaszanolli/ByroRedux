# Batch fix: #3518, #3499, #3308, #3307

## #3518 — OBL-2026-08-27-03: parse_placement_lod unbounded u32 group-count allocation
**Domain**: binary (`byroredux`) — Oblivion placement-LOD reader
**Severity**: MEDIUM (safety)
**Location**: `byroredux/src/cell_loader/placement_lod.rs:119-122`
(`parse_placement_lod`)
**Bug**: `num_groups = u32_at(bytes, 0)?` feeds `Vec::with_capacity` before any
bounds check — a hostile/corrupt `0xFFFFFFFF` header word requests ~137 GB,
which aborts the process (`handle_alloc_error`) instead of returning the
`Err` the function's own doc promises. Every sibling untrusted-count site
(`bsa/archive/open.rs` `checked_entry_count`, `ba2.rs` file_count cap,
`nif/stream.rs` `allocate_vec`/`allocate_vec_sized`) already guards this;
this is the one that doesn't.
**Fix**: bound `num_groups` against the smallest legal per-group encoding
(8 bytes: `base_form_id` + `count`) before allocating — same one-line shape
as the issue's own suggested fix. Add a `u32::MAX`-header regression test.

## #3499 — SAVE-D6-2026-08-27-04: FullRadius bootstrap worker-disconnect break reopens #3280-shaped save-load window
**Domain**: binary (`byroredux`) — exterior save/load reload path
**Severity**: LOW (data-loss, narrow window)
**Location**: `byroredux/src/scene/world_setup.rs:837-847` (wait loop's
worker-disconnect `break`), `byroredux/src/save_io.rs:1367-1368`
(`build_form_id_remap` + `apply_deltas`, no `state.pending` guard between)
**Bug**: If the streaming worker thread dies mid-`FullRadius` bootstrap,
`stream_initial_radius` returns early via `break` with `state.pending`
non-empty. `reload_exterior_session` still returns `Some` unconditionally,
so `execute_pending_save_loads` runs `build_form_id_remap` + `apply_deltas`
against a world missing the still-pending cells — silently dropping their
saved delta rows. Same mechanism #3280 fixed, narrower trigger (worker
death, not queue non-emptiness).
**Fix**: `reload_exterior_session` returns `None` (with a `notify_player`
message) when `state.pending` is non-empty after the `FullRadius` bootstrap
returns — same posture as `validate_snapshot_types`/`validate_cell_loadable`.

## #3308 — EX-10/11 item 9: reversed-Z depth buffer
**Domain**: renderer — enhancement, not a bug fix
Explicitly NOT ready for direct implementation: touches projection, depth
clear, pipeline compare state, and 6+ depth consumers (SSAO, SVGF, TAA,
composite, water, FSR3) — none of which are `cargo test`-visible. The
issue's own suggested approach requires a GPU capture/comparison gate
(RenderDoc) before touching any consumer, per this project's standing
no-speculative-Vulkan-fixes policy. Needs user decision on how to proceed
(see below).

## #3307 — EX-10/11 item 8: active VWD full-model culling
**Domain**: renderer/streaming — enhancement, not a bug fix
Explicitly deferred in its own body: `docs/engine/exal.md` §5.2 states the
per-REFR streaming-radius decoupling "needs real-game visual validation
before it is enabled" — the issue itself calls building-and-shipping this
blind a violation of the project's speculative-Vulkan-change policy. Needs
user decision on how to proceed (see below).

## Plan
Fix #3518 and #3499 as scoped single-site bug fixes. Flag #3308/#3307 to
the user — both are enhancement-labeled design/research tasks that their
own issue bodies say should not be implemented without GPU validation
infrastructure this session doesn't have.
