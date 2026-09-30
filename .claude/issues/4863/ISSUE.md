# #4863: REN-D11-2026-09-24-04: the exposure meter weighs every sampled texel unconditionally — non-finite and near-black texels dominate the log-average (opt-in `--auto-exposure`)

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D11-2026-09-24-04**._

- **Severity**: LOW (hardening on an opt-in path: the default `ExposureTuning::default().auto == false`; EX-05 censuses report zero non-finite pre-tonemap pixels on shipped games).
- **Dimension**: FSR/Presentation
- **Location**: `crates/renderer/shaders/exposure_meter.comp` — `main`; host clamp `crates/renderer/src/vulkan/exposure.rs` (`MIN_AUTO_EXPOSURE` / `MAX_AUTO_EXPOSURE`).
- **Status**: NEW
- **Description / Evidence**: The pass reads 4,096 texels (`texelFetch`, RGBA16F, no validity test) and accumulates `log2(max(l, 1.0e-6))`. (a) Since `0572bfd5a`, `hdr_clear` is `[0,0,0,0]` for **every** interior; pixels that `boundedInteriorOpening` does not accept stay ~black and each contributes about -19.9 EV (5 % black texels on a true geometric mean of 0.15 give exposure 1.81 instead of 1.0, +0.86 stop, before adaptation). (b) A `+Inf` texel (RGBA16F overflow above 65,504) makes `log_sum = +inf`, so the target clamps to `MIN_AUTO_EXPOSURE` = 1/256; NaN behaviour is implementation-defined. (c) The grid is anchored at (0,0) with `max(extent / 64, 1)`, so at 1920×1080 the bottom ~6.7 % and right ~1.6 % are never metered. The #4597 division was verified independently for 9 extents including non-multiples of 64.
- **Suggested Fix**: Skip non-finite texels and clamp per-texel luminance to a finite ceiling; choose the luminance floor, or exclude clear-depth / zero-coverage texels, from measurement, not by guess. Needs a live `--auto-exposure` run on an interior with a void.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

