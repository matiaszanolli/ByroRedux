### AUD-2026-09-11-D7-01: `water_audio_system`'s splash/ripple attenuation and the ripple intensity-damping factor are uncommented magic numbers, unlike the sibling footstep attenuation in the same file

- **Severity**: LOW
- **Dimension**: Gameplay Audio Wiring
- **Location**: `byroredux/src/systems/audio.rs:303-306, 314-318, 319`
- **Status**: NEW
- **Source**: `docs/audits/AUDIT_AUDIO_2026-09-11.md`

**Description**: `footstep_system`'s attenuation literal carries an inline rationale (`Attenuation { min_distance: 0.5, max_distance: 12.0 }`, commented "Tighter attenuation than the default — footsteps drop off fast in real environments. 0.5m → full volume, 12m → inaudible", `systems/audio.rs:203-208`, confirmed present). `water_audio_system`'s two `Attenuation { min_distance: 1.0, max_distance: 24.0 }` literals (splash and ripple) and the ripple-only `* 0.45` intensity-damping factor carry no such comment — confirmed directly during publish. The values themselves are plausible (tighter than `Attenuation::default()`'s `{2.0, 30.0}`, consistent with a near-surface sound) and are not being second-guessed here — this is a documentation-parity gap between two sibling systems in the same file, the same class of gap AUD-2026-08-30-D5-01 already flagged (and closed) for the crate's `ReverbBuilder` literals.

**Impact**: Low — a future tuning pass touching water audio has no recorded rationale to preserve or deliberately deviate from.

**Related**: AUD-2026-08-30-D5-01 (the closed `ReverbBuilder` sibling of this exact gap class).

**Suggested Fix**: A one-line comment on each `Attenuation` literal and on the `0.45` factor (e.g. "ripples read quieter than a direct splash — chosen by ear, no cited source") closes the parity gap cheaply.

## Completeness Checks
- [ ] **TESTS**: N/A — comment-only fix
