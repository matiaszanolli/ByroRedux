# #5280: UI-D7-2026-10-05-03: `hud.values` and `hud.heading` accept NaN and ±inf

Labels: low,ui,bug,safety
Filed from: docs/audits/AUDIT_UI_2026-10-05.md

**Source**: `docs/audits/AUDIT_UI_2026-10-05.md` (UI-D7-2026-10-05-03) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: MenuXml & HUD Drivers
- **Profile**: both
- **Location**:
  - `byroredux/src/commands/hud.rs:88` (`t.parse::<f32>().map(|v| v.clamp(0.0, 1.0))`)
  - `byroredux/src/commands/hud.rs:130-132` (`deg.rem_euclid(360.0)`)
  - `byroredux/src/hud.rs:829` (`fraction` re-clamps)
- **Status**: NEW
- **Description**:
  - `"nan".parse::<f32>()` succeeds, and `f32::clamp(NaN)` returns NaN, so `hud.values nan nan nan` pins NaN bars.
  - `hud.heading inf` (or `nan`) becomes NaN through `rem_euclid`.
  - The #4724 test comment says "the count and the 0-1 domain are enforced", but it only tests non-numeric junk.
- **Impact**:
  - MenuXml bar and compass geometry turns non-finite, so the raster's non-finite reject draws nothing and the bar
    vanishes.
  - Scaleform pushes `Number(NaN)`.
  - Signature hashing uses `to_bits` and `as i32`, so nothing hangs. The surface is the console/debug server only.
- **Related**: #4724 (closed)
- **Suggested Fix**: reject values that fail `is_finite()` in both parsers, and pin `nan` and `inf`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
