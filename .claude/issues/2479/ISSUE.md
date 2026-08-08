# REN-D22-04: PULSE_SLOW is a half-wave-rectified sine at the same rate, not a half-speed pulse

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2479
**Finding ID**: REN-D22-04 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 22 — Light Animation
**Location**: `byroredux/src/systems/light_anim.rs:145-162` (`flicker_intensity`, pulse branch)
**Status**: NEW

## Description
`speed_scale` (0.5 for the SLOW bits) is applied *inside* the sine, but the argument has already been wrapped to one cycle per `period_secs` by `rem_euclid`. Multiplying the normalized phase by 0.5 therefore does not halve the frequency — it truncates the waveform to its positive half and repeats it at the original rate. This works correctly for the flicker branch (`speed_scale` there multiplies *time*, before the bucket step, so 12 Hz → 6 Hz is genuinely half-rate); only the pulse branch is wrong.

## Evidence
```rust
let phase_secs = (total_time + flicker.phase_offset_secs).rem_euclid(flicker.period_secs);
let phase = phase_secs / flicker.period_secs;             // sawtooth 0..1 once per period
(phase * speed_scale * std::f32::consts::TAU).sin()       // sin(pi*phase) for SLOW
```
With `period = 1s`: modulation peaks at t = 0.5, 1.5, 2.5... and returns to 0 at every integer second — it never goes negative. A true half-rate pulse (`sin(TAU*t/2)`) would trough at t = 1.5. So `PULSE_SLOW` (a) pulses at the same rate as `PULSE`, and (b) only ever *brightens* the light instead of oscillating around the authored intensity.

## Impact
Every `PULSE_SLOW` light (ambience set-pieces, glowing crystals) renders visibly wrong — brighter on average and at the wrong cadence. Same-frequency-but-rectified is also what a "both PULSE and FLICKER_SLOW authored" light gets, since `speed_scale` keys off either SLOW bit while the pulse branch wins the shape selection.

## Related
The existing test `pulse_slow_runs_at_half_angular_velocity` cannot detect this — it samples only `t = period/4`, where the rectified and the true half-rate waveform coincide exactly (`sin(pi/4)` both ways).

## Suggested Fix
Scale the *period*, not the phase — `rem_euclid(period / speed_scale)` then divide by the same value (or drop the wrap and use `sin(TAU * (t + off) * speed_scale / period)`), and extend the test to a sample past one period (e.g. `t = 1.5 * period`) where the two waveforms differ in sign.

## Completeness Checks
- [ ] **TESTS**: `pulse_slow_runs_at_half_angular_velocity` extended to `t = 1.5 * period` where the rectified and true half-rate waveforms diverge in sign
