# Issue #3520 — AUD-2026-08-27-D7-01

Source: `docs/audits/AUDIT_AUDIO_2026-08-27.md` · https://github.com/matiaszanolli/ByroRedux/issues/3520

Filed from `docs/audits/AUDIT_AUDIO_2026-08-27.md` (finding `AUD-2026-08-27-D7-01`).

- **Severity**: HIGH
- **Dimension**: Gameplay Audio Wiring
- **Location**: `byroredux/src/components.rs:1462-1496` (the constant + its docstring), consumed at `byroredux/src/systems/audio.rs:161-171` (the comparison); opt-in site `byroredux/src/scene.rs:1215`
- **Related**: #3178 (the BU→metre seam this is the producer-side counterpart of), #848 (first-tick seed)

## Description

`footstep_system` accumulates the XZ-plane delta of the emitter entity's `GlobalTransform.translation` — which is in **Bethesda units** (that is the entire premise of #3178's `bu_to_audio_space` seam, which divides the very same `GlobalTransform.translation` by `BETHESDA_UNITS_PER_METER = 70` on its way into kira) — and compares that BU accumulation against `stride_threshold`, whose default is `1.5` and whose docstring calls it "**~1.5m at FNV scale**". At 70 BU/m the effective threshold is **1.5 / 70 = 2.14 cm of world travel**, not 1.5 m.

#3178 fixed the seam for *positions crossing into kira*. It did not examine gameplay constants compared against BU deltas **before** the seam, and `stride_threshold` is the only such constant in the audio subsystem. The two live `play_oneshot` producers' `Attenuation` constants (`{0.5, 12.0}` footstep, `{1.0, 24.0}` splash) are correctly metres, because they are handed to kira and never compared against a world delta — which is exactly why the seam audit found nothing here.

## Evidence

The comparison, verbatim (`byroredux/src/systems/audio.rs:161-171`):

```rust
// XZ-plane delta only — vertical (Y) motion isn't a step.
let dx = pos.x - fs.last_position.x;
let dz = pos.z - fs.last_position.z;
let horizontal = (dx * dx + dz * dz).sqrt();
fs.accumulated_stride += horizontal;
fs.last_position = pos;
if fs.accumulated_stride >= fs.stride_threshold {
    fs.accumulated_stride = 0.0;
    scratch.triggers.push(pos);
}
```

The constant (`byroredux/src/components.rs:1490-1495`):

```rust
Self {
    last_position: Vec3::ZERO,
    accumulated_stride: 0.0,
    stride_threshold: 1.5,
    initialised: false,
}
```

and its docstring (`byroredux/src/components.rs:1463-1465`):

```
/// by `footstep_system`; `stride_threshold` is read-only configuration
/// — a stride distance that triggers one footstep. Defaults to 1.5
/// game-units (~1.5m at FNV scale; reasonable walking cadence).
```

The per-frame travel that threshold is measured against is stated by the engine's own code, twice, independently of this audit:

- `byroredux/src/components.rs:1449` — `move_speed: 200.0, // Bethesda units per second` (fly-cam `InputState`), consumed as `let speed = input.move_speed * dt;` (`byroredux/src/systems/camera.rs:40`).
- `crates/physics/src/components.rs:182` — `move_speed: 220.0` on the `CharacterController` used in player mode.
- `byroredux/src/boot.rs:1108-1110`, the comment on `footstep_system`'s own registration: *"the commit comment claimed '~3 cm of motion' stale but that underestimated by ~100× for fly-cam boost (**~3 game units / frame at 60 FPS**, audible spatial-pan offset on a ~50-200-unit interior cell)"*.

200 BU/s ÷ 60 FPS = **3.33 BU/frame**, already 2.2× the 1.5 BU threshold before the ×3 sprint boost (`byroredux/src/systems/camera.rs:71-75`) or the character controller's 220 BU/s. Since the fire branch is an `if` (not a `while`) it is capped at one trigger per emitter per tick, so the observable behaviour is **exactly one footstep every frame while moving** — 60 Hz at 60 FPS against a realistic human cadence of ~1.8 Hz. A correct BU threshold for a ~0.75 m human stride is ≈ 52 BU.

## Impact

The only live gameplay-audio producer is audibly wrong under normal play on the reference title (`--sounds-bsa "Fallout - Sound.bsa"` + any FNV cell): a continuous machine-gun footstep buzz instead of a walk cadence. Secondary cost: ~33× the intended spatial sub-track churn — one `SpatialTrackBuilder` + `add_spatial_sub_track` + `StaticSoundData` clone + `ActiveSound` push per frame, with ~24 concurrent `active_sounds` entries at a 0.4 s WAV instead of ~1. Still far below `SUB_TRACK_CAPACITY = 512`, so it degrades rather than drops.

**This is invisible to every guard the subsystem has.** The `drain_pending_oneshots` >32-per-tick WARN (`crates/audio/src/lib.rs:548-556`) exists precisely to catch "footstep-tempo gone wrong", but only one item is enqueued per emitter per tick, so it never trips with a single emitter. `active_sound_count` *is* wired to telemetry (`byroredux/src/ownership_sample.rs:63`) and would show the elevated count, but nothing alerts on it. And the two stride regression tests (`stride_threshold_fires_exactly_one_footstep`, `single_large_jump_fires_one_footstep_only`, `byroredux/src/systems/audio.rs:417-497`) are unit-agnostic — they move the emitter "1.5 game-units" and "6.0 horizontal units" and assert trigger counts, which pass identically whether the unit is a metre or a BU. There is no `stride_threshold_is_bethesda_units_not_metres` sibling to `default_attenuation_band_is_metres_not_bethesda_units` (`crates/audio/src/tests.rs`), which is the guard shape #3178 already established for the other half of this seam.

## Suggested Fix

Make the unit explicit at the constant. Either set `stride_threshold` to a BU value (≈ 52.0 for a ~0.75 m stride, or `0.75 * BETHESDA_UNITS_PER_METER`) and rewrite the docstring to say Bethesda units, or keep the metre authoring and convert at the compare site. The first is preferable: it keeps `footstep_system` operating entirely in world space with no second unit seam, matching the `bu_to_audio_space`-is-the-only-seam rule. Add a `stride_threshold_is_bethesda_units_not_metres` guard mirroring `default_attenuation_band_is_metres_not_bethesda_units`, and a cadence test that walks an emitter at 200 BU/s for one simulated second and asserts a plausible step count (~2–4, not 60).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other gameplay constants compared against a BU delta before the `bu_to_audio_space` seam)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
