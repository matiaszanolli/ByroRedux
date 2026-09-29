# #5135: EXT-D1-2026-09-29-02: `BASE_FOG_STRENGTH` (0.8) scales every medium's extinction at the frame boundary — absent from the exterior specs, and env.health reports the unscaled value

**Labels**: low, documentation, doc-rot, terrain-exterior

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-09-29.md`
**Severity**: LOW
**Dimension**: EXAL boundary discipline (no-fabrication: documentation only; no-leak: telemetry)

## Location
`byroredux/src/fog.rs` (`BASE_FOG_STRENGTH`); `byroredux/src/app_frame.rs` (applied where `FrameInputs` is built); `byroredux/src/commands/env_health.rs` (`env: fog … extinction=` line)

## Description
`a070baaad` added a global 0.8 multiplier on `fog_medium.extinction_per_meter`, applied where `FrameInputs` is built; the in-code rationale is a 2026-09-28 presentation direction. It is game-invariant (not a `GameKind` fallback), and the composite height fog, froxel volumetrics and `assemble_camera_and_lights` all receive the scaled value consistently.

Why it is a finding:
1. The engine choice sits outside `FogMedium`, and no exterior spec records it (`exal.md`, `skyal.md`, `docs/engine/*` say nothing), breaking the "constants cite a source or are a documented engine choice" rule where readers look for it.
2. `env.health` and the translate tests report the unscaled medium, so every live readout is 1.25× what the GPU integrates.
3. It also thins interior XCLL/LGTM fog, although the rationale cites exterior haze only.

## Evidence
`env_health.rs` prints `lit.fog_medium.extinction_per_meter` directly; the `* crate::fog::BASE_FOG_STRENGTH` scale exists only in `app_frame.rs`; `grep BASE_FOG_STRENGTH docs/engine/` is empty.

## Impact
Tuning or debugging fog from `env.health` is off by 25%. A future fix to `fit_legacy_fog_extinction` (whose comment implies the fit reads denser than vanilla) would stack with an undocumented global scale.

## Related
EXT-D1-2026-09-29-01 (the Starfield unit defect is ~70×, not something a 0.8 trim addresses).

## Suggested Fix
Document the choice in exal.md / skyal.md, including whether it is exterior-only. Either fold it into `FogMedium` at the translate so `env.health` shows what the GPU gets, or have `env.health` print both values.

Validated at HEAD 9fcfdc3fc: `BASE_FOG_STRENGTH = 0.8` in `fog.rs`, consumed only in `app_frame.rs`; env.health prints the unscaled medium; no mention in exal.md/skyal.md.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other fog readouts/telemetry)
- [ ] **TESTS**: A regression test pins this specific fix
