# #3675 — PERF-D9-2026-08-30-02: batches_scratch's per-frame reserve() and its end-of-frame shrink fight each other

**Severity**: MEDIUM · **Dimension**: Telemetry & Origin Cost (chronic scratch over-reserve)
**Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`

## Fix

Deleted `batches.reserve(draw_commands.len())`, per the issue's own "drop
the reserve" suggested-fix option — `Vec::push`'s own amortized O(1)
growth from whatever capacity the end-of-frame shrink policy left the
scratch at is a better fit than reserving to a quantity (command count)
the issue's own baseline measurements put 13-19x larger than `batches`'
actual working set (merged batch count).

Note: the issue's cited line numbers (`draw.rs:2810-2812` /
`:3978-4006`) predate a prior split (`build_and_upload_instances.rs` was
extracted from `draw.rs` under #3282) — re-located the real call site
before touching anything; confirmed the described defect still holds
verbatim at the new location.

## SIBLING (issue's own checklist item)

Checked the other three scratch buffers reserved in the same block
(`gpu_instances`, `previous_models`, `current_rigid_models`) — all three
are correctly sized: their working sets genuinely scale with
`draw_commands.len()` (one `GpuInstance`/`GpuPreviousModel` per command,
one `current_rigid_models` HashMap entry per qualifying command), unlike
`batches` (one entry per MERGED batch). Matches the issue's own framing
exactly — no fix needed there.

## TESTS (issue's own checklist item)

`build_and_upload_instances` needs a real `VulkanContext` — this crate's
own established convention for that situation (e.g.
`pose_dirty_crosses_the_crate_boundary_without_siphash` in `context/
mod.rs`) is a static source-scan test, not a live one. Added
`batches_scratch_is_not_reserved_to_draw_command_count`, pinning the
`mem::take` site is unchanged and the `reserve(draw_commands.len())` call
stays gone.

**Caught and fixed my own instance of the exact self-matching trap #3674
surfaced earlier this session**: an unscoped `include_str!` search over
the whole file matches this test's own `.contains("...")` argument string
(byte-identical to the needle), so it would ALWAYS find a match even with
the production call deleted outright — verified by deliberately
reintroducing the bug with the unscoped version and observing the test
still pass. Fixed by scoping the search to `&src[..module_start]`
(everything before this test module's own `mod` line). Re-verified with
the scoped version: reintroducing the bug now correctly fails, and
restoring the fix passes again.

## Verification

- `cargo check -p byroredux-renderer --tests`: clean.
- `cargo test -q -p byroredux-renderer`: 818 tests passing, 0 failing
  (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7098 passing, 0
  failing**.
