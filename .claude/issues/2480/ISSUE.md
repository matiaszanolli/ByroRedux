# REN-D23-2026-08-07-01: FSR-failure fallback stays at the reduced render resolution with no temporal AA, contradicting UpscalerMode::Taa's own doc

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2480
**Finding ID**: REN-D23-2026-08-07-01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 23 — FSR Upscaler
**Location**: `crates/renderer/src/vulkan/upscaling.rs::UpscalerMode::Taa` (doc), `crates/renderer/src/vulkan/frame_upscaler.rs::FrameUpscaler::record` / `record_native_blit`, `crates/renderer/src/vulkan/context/mod.rs:2641` (TAA construction gate)
**Status**: NEW

## Description
`UpscalerMode::Taa`'s doc comment states it is "the compatibility fallback taken whenever FSR context creation or dispatch fails". Nothing in the code takes that fallback. On FSR context-creation failure (`FrameUpscaler::new`, the `Err(error)` arm just logs) or on a latched `dispatch_failure`, `renderer_config.upscaler` remains `Fsr3(..)`, so `frame_extents.render` stays at the preset's reduced extent (1280x720 for Quality at 1080p), `self.taa` is `None` (built only when `renderer_config.upscaler == UpscalerMode::Taa`), and jitter is forced to `(0.0, 0.0)`. The degraded image is therefore a plain bilinear stretch of an un-anti-aliased 720p render, not a TAA-resolved native frame. Since FSR Quality is the engine default, this is the state every user with a non-working FSR provider lands in.

## Evidence
`frame_upscaler.rs` context-creation failure arm — `log::error!("FSR context creation failed: {error}; using native HDR blit fallback")` with no mode change; `context/mod.rs:2641` `let mut taa = if renderer_config.upscaler == UpscalerMode::Taa { ... } else { log::info!("FSR mode active: TAA history/resolve disabled ..."); None };`; `draw.rs:1573` FSR arm returns `(0.0, 0.0, None, false)` when `!is_fsr_dispatch_active()`.

## Impact
Silent, large quality regression (720p bilinear, aliased, no AA) on any machine where the FSR provider fails to initialize, reported only via one `log::error!` and a telemetry string. Blast radius: the whole frame, permanently, for the session.

## Related
`AUDIT_RENDERER_2026-07-28.md` §"FSR 3.1 Residual Scope" (listed forced-failure/live-switching as untested, did not name this).

## Suggested Fix
Either escalate the context-creation failure into a `set_upscaler_mode(UpscalerMode::Taa, ..)` at startup (the machinery already exists and is rollback-safe), or fix the `UpscalerMode::Taa` doc comment to say the fallback is a *native blit at the FSR render extent*, not the TAA mode. The dispatch-failure latch is a harder case (it fires mid-frame) but could set a "re-evaluate mode at next frame boundary" flag.

## Completeness Checks
- [ ] **TESTS**: A regression test (or documented manual repro via `BYRO_FSR_FORCE_DISPATCH_FAIL`) confirms the fallback mode after the fix
