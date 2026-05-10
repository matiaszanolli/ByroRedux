---
issue: 0
title: REN-D5-NEW-02: First-sight skin compute prime + sync BLAS BUILD stalls per-frame command buffer
labels: renderer, M29, medium, vulkan, performance
---

**Severity**: MEDIUM (per-NPC frame hitch on first sight)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 5)

## Location

- `crates/renderer/src/vulkan/context/draw.rs` — first-sight skin prime + sync BLAS BUILD path
- `crates/renderer/src/vulkan/buffer.rs` — `with_one_time_commands_reuse_fence` helper

## Why it's a bug

First-sight skin compute prime + sync BLAS BUILD use a separate one-time cmd buffer + `transfer_fence` host-wait, stalling `draw_frame` per newly-visible NPC. With M41-EQUIP NPCs spawning into view this becomes user-visible hitching.

Recording into the per-frame `cmd` would eliminate the hitch since the work would batch into the existing GPU submission.

## Fix sketch

Move the first-sight skin compute prime + initial BLAS BUILD recording into the per-frame command buffer (between TLAS build and render pass begin). Drop the separate one-time cmd buffer + transfer_fence path.

Caveat: the current pattern exists because the BLAS needs to be built before the TLAS references it. Reordering to: skin-prime → BLAS-build → TLAS-build → render-pass within the per-frame cmd should satisfy the dependency without the host-wait.

## Completeness Checks

- [ ] **UNSAFE**: Vulkan command recording; verify barrier between skin-compute-write and BLAS-build-read covers the new ordering.
- [ ] **SIBLING**: Audit one-time-cmd-buffer use elsewhere (texture upload, mesh upload) — those are correctly out-of-band.
- [ ] **TESTS**: Manual repro: spawn 10 NPCs into view, measure frame-time spike.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
