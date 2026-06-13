## Finding REN2-20 — Renderer Audit 2026-06-11

- **Severity**: LOW
- **Dimension**: Debug Overlay & GPU Telemetry (doc-rot)
- **Location**: `crates/renderer/src/vulkan/gpu_timers.rs:48` (actual reset code: `:218`, `:305`, gated by `caps.host_query_reset_supported` at `:195`)
- **Status**: NEW (extends Existing #1484). Validated CONFIRMED at HEAD `1e8a25ab` **with corrected scope**: the audit's "line 5" anchor and the `egui_pass.rs:181-184` inclusion did not hold up (line 5 is just the module summary table; the egui_pass range is RenderPass/cmd_draw commentary with no timer/reset content) — this issue covers the line-48 claim only.

## Description

`gpu_timers.rs:48` documents "…then `cmd_reset_query_pool` for the upcoming frame", but the code uses host-side `device.reset_query_pool` (VK_KHR_host_query_reset) everywhere (`:218`, `:305`); `cmd_reset_query_pool` appears nowhere in the renderer except this doc string. This is the residual of the claim the 2026-06-09 audit called out, still present post-`73a43fc8`.

## Suggested Fix

One-line doc fix to "host-side `reset_query_pool` (VK_KHR_host_query_reset)". Fold into the open #1484 doc-rot batch.

## Related

#1484 (renderer doc-rot batch), #1483 (gpu_timers Drop-path leak, same file).

## Completeness Checks
- [ ] **SIBLING**: Re-read the rest of gpu_timers.rs module docs against the post-73a43fc8 code while there
- [ ] **TESTS**: N/A (doc-only)

---
Source: `docs/audits/AUDIT_RENDERER_2026-06-11.md` · Filed by `/audit-publish`
