# CONC-2026-11-09-2.1: local-grid brick-buffer `cmd_fill_buffer` has no unconditional FRAGMENT->TRANSFER edge back to composite's read two frames earlier (PLAUSIBLE — needs a syncval capture)

Labels: low,concurrency,sync,renderer,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW — **PLAUSIBLE, not CONFIRMED** (no validation capture exists; filed as a verification task). **Status**: NEW. Refines a hypothesis the 2026-11-08 concurrency baseline dropped.

**Location**: `crates/renderer/src/vulkan/volumetrics/local_grid.rs:1203-1213` (`record_sim`, `cmd_fill_buffer` at L1204); reader at `composite.frag` binding 16 (bricks).

## Description
`record_sim` clears `bricks[frame]` with `vkCmdFillBuffer` (TRANSFER stage). The same buffer was last read by composite's FRAGMENT shader two submissions earlier. `record_sim`'s own barrier is `TRANSFER -> COMPUTE` after the fill and says nothing about the earlier read. The baseline dismissed the WAR because the depth-history copy barrier (src `FRAGMENT_SHADER`, dst `TRANSFER`) "covers" it, but that barrier is emitted only when `has_effect_soft_material` is set (`context/draw.rs` ~L370-380). Other FRAGMENT-source barriers in the frame have a COMPUTE destination stage, which does not order a later TRANSFER-stage command. A FRAGMENT->TRANSFER edge therefore exists only on frames that also record the soft-particle depth copy, a HUD/UI upload, a capture copy, or the ground-cover counter fill. (The SVGF/SSAO entry barriers were not individually read by the auditor.)

## Trigger Conditions
A scene with at least one local-grid box, no soft-particle effect material, no HUD and no capture, under `BYRO_VALIDATION=1`. Hardware execution is not at risk: `sync_and_acquire_frame.rs` waits on every in-flight fence, so frames never overlap on the queue.

## Impact
A possible `SYNC-HAZARD-WRITE-AFTER-READ` naming a brick-buffer `VkBuffer` that would redden a `vulkan-validation` run on a fire scene. Nothing user-visible.

## Verification Path
`BYRO_VALIDATION=1` (not `cargo test`) on a flame cell with the soft-material bit off. Confirming signal: WAR hazard with `vkCmdFillBuffer` as the later access and composite's draw as the earlier read. Absence of the report on such a capture closes this issue.

## Suggested Fix (only after confirmation)
Add `FRAGMENT_SHADER -> TRANSFER` (`SHADER_READ -> TRANSFER_WRITE`) to the buffer barrier ahead of the fill in `record_sim`, or widen Stage D `pre_int_write`'s destination to include `TRANSFER`.

Report: `docs/audits/AUDIT_CONCURRENCY_2026-11-09.md`. Safety audit (`docs/audits/AUDIT_SAFETY_2026-11-09.md`) reached the same observation and judged it covered by the all-slots fence wait.

## Completeness Checks
- [ ] **SYNC**: If barriers change, the rider list in `sync.rs` still names every dependent resource
- [ ] **TESTS**: A regression test pins this specific fix (or a syncval capture attached to this issue)

