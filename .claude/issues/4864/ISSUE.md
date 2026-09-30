# #4864: REN-D11-2026-09-24-05: water and the screen-composition proxy write reactive = 1.0, against the plan's `min(alpha, 0.9)` rule and `triangle.frag`'s own "never a full 1.0" comment

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D11-2026-09-24-05**._

- **Severity**: LOW as filed (contract divergence; MEDIUM if a capture shows the full-strength mask hurting water reconstruction).
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/shaders/water.frag` `main` (`outFsrReactive = 1.0; outFsrTransparency = 1.0;`); `triangle.frag` screen-composition proxy branch and the tail policy comment ("clamped to 0.9 — never a full 1.0"); `docs/engine/fsr3-upscaler-integration-plan.md` §1.4 "Reactive mask" row ("Glass, particles, water, and alpha-blended decals write `min(alpha, 0.9)`").
- **Status**: NEW. Related closed: #3604, #4297 / #4540.
- **Description**: `water.rs`'s doc and blend table deliberately write both masks at 1.0 (pinned by `attachment_doc_pin_tests`); the plan row and the main shader's comment say 0.9. Which is right is an evidence question. All other writers conform.
- **Suggested Fix**: Pick one policy and align plan, comments and shaders, decided by an `upscaler_quality` A/B or RenderDoc mask capture, not by reasoning.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

