# REN-2026-07-02-L02: with_one_time_commands_inner leaks fence/cmd-buffer on error paths

- **Severity**: LOW (pre-existing; fires only on an already-failing GPU submit/wait)
- **Dimension**: Sync/Barriers (error-path resource lifecycle)
- **Location**: `crates/renderer/src/vulkan/texture.rs` :: `with_one_time_commands_inner`
- **Source report**: `docs/audits/AUDIT_RENDERER_2026-07-02.md` (originally raised as REN-2026-07-01-L02 in `docs/audits/AUDIT_RENDERER_2026-07-01.md`, confirmed unchanged in both sessions; this is the first publish run to file it)

## Description

The recording-closure failure path (`if let Err(e) = f(cmd) { … }`) correctly
ends + frees the command buffer before propagating. But three later fallible
calls still propagate via `?` before the fence-destroy/`free_command_buffers`
cleanup tail runs:

- `device.reset_fences(&[**guard])?` — on error, the allocated `cmd` is never freed.
- `device.queue_submit(*q, &[submit_info], fence)?` — on error, neither the
  owned fence (if `owned == true`) nor `cmd` is cleaned up.
- `device.wait_for_fences(&[fence], true, u64::MAX)?` — same gap; most likely
  of the three to actually fire (device-loss mid-wait), and it fires after a
  successful submit, so the GPU may still be mid-execution against `cmd` when
  the handles are abandoned.

The cleanup tail (`destroy_fence` if owned, `drop(fence_guard)`,
`free_command_buffers`) sits strictly after all three fallible calls.

## Evidence

Current source (`crates/renderer/src/vulkan/texture.rs`,
`with_one_time_commands_inner`) — the three `?` sites
(`reset_fences`, `queue_submit`, `wait_for_fences`) precede the
`destroy_fence`/`drop`/`free_command_buffers` cleanup tail.

## Impact

Bounded. Fires only when a one-time submit is already failing
(device-loss/OOM territory). The reusable-fence path (`owned == false`, the
common case post-init) leaks no fence, only the command buffer, which is
reclaimed at pool destruction. No per-frame accumulation in normal operation.

## Suggested Fix

Capture the `reset_fences` / `queue_submit` / `wait_for_fences` `Result`s
instead of using `?` directly, and run the destroy-fence-if-owned +
`free_command_buffers` cleanup unconditionally in all three error arms (or
wrap `cmd` + the owned fence in a small RAII drop guard), then propagate the
original error. Verify it doesn't double-destroy the reusable fence on the
`owned == false` path before landing.

## Related

- #1713 (adjacent queue-mutex re-scope, verified race-free, unaffected by this finding)
- #302 (reusable fence)

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl / cleanup path is still reverse-order correct (destroy fence before free command buffers)
- [ ] **LOCK_ORDER**: If a RwLock/Mutex scope changes, the queue-mutex-only-spans-submit invariant (#1713) is preserved
- [ ] **SIBLING**: Check `with_one_time_commands_reuse_fence` callers and any other one-time-submit helper for the same error-path gap
- [ ] **TESTS**: A regression test pins this specific fix (existing one-time-command test seam)
