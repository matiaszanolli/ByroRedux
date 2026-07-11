# #1860: REN-2026-07-02-L01: DBG_BITS test catalog covers only 13 of 17 DBG_* constants

Severity: LOW · Dimension: GPU-Struct Layout (shader-constant lockstep)
Location: `crates/renderer/src/shader_constants.rs` (`DBG_BITS` array),
`crates/renderer/src/shader_constants_data.rs` (17 `pub const DBG_*`),
`crates/renderer/build.rs` (hand-written writeln! for the 4 missing bits)

DBG_BITS enumerates only 13 of 17 DBG_* constants. The 4 newest
(DBG_DISABLE_MULTISCATTER/ATROUS/RESTIR/SPATIAL) bypass the catalog via
separate hand-written build.rs emits, so neither the header value-pin
test nor the shader no-redeclare guard covers them. Latent, not live —
no shader currently redeclares them.

Suggested fix: add the 4 missing entries to DBG_BITS, route their header
emit through the catalog loop, add a count-parity regression test.

# #1861: REN-2026-07-02-L02: with_one_time_commands_inner leaks fence/cmd-buffer on error paths

Severity: LOW · Dimension: Sync/Barriers (error-path resource lifecycle)
Location: `crates/renderer/src/vulkan/texture.rs` :: `with_one_time_commands_inner`

Three fallible calls (reset_fences, queue_submit, wait_for_fences) propagate
via `?` before the cleanup tail (destroy_fence-if-owned, free_command_buffers)
runs, so any of the three failing leaks the command buffer (and the owned
fence, when not reusable). Bounded impact — only fires on already-failing
GPU calls (device-loss/OOM).

Suggested fix: capture each Result, run cleanup unconditionally before
propagating the original error (or wrap in an RAII guard); verify no
double-destroy on the reusable-fence (owned == false) path.
