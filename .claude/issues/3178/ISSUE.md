# #3178 — AUD-2026-08-20-D2-01: spatial attenuation distances are authored in metres but consumed in Bethesda units

- **Filed**: 2026-08-20 (`/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3178
- **Labels**: `high,legacy-compat,bug`
- **Source report**: `docs/audits/AUDIT_AUDIO_2026-08-20.md`
- **HEAD at audit**: `bb0b92f2`

---

**Severity**: HIGH
**Dimension**: Listener Pose & Spatial Attenuation Correctness
**Source**: `docs/audits/AUDIT_AUDIO_2026-08-20.md` (`AUD-2026-08-20-D2-01`) — HEAD `bb0b92f2`

## Location

- `crates/audio/src/lib.rs` — `Attenuation::default` (the metre-worded doc, `min_distance: 2.0` / `max_distance: 30.0`)
- `crates/audio/src/lib.rs` — `sync_listener_pose` (`handle.set_position(pose.0, Tween::default())`, raw BU)
- `crates/audio/src/lib.rs` — both `mgr.add_spatial_sub_track(listener_id, p.position, track_builder)` sites (queue path + entity path), raw BU
- `byroredux/src/systems/audio.rs` — `footstep_system` `Attenuation { min_distance: 0.5, max_distance: 12.0 }`
- `byroredux/src/systems/audio.rs` — `water_audio_system` splash + ripple `Attenuation { min_distance: 1.0, max_distance: 24.0 }`
- `crates/core/src/lighting.rs` — `BETHESDA_UNITS_PER_METER: f32 = 70.0`

## Description

The engine's world space is **Bethesda units** — 70 BU per metre, declared once in
`crates/core/src/lighting.rs` as `BETHESDA_UNITS_PER_METER` and consumed as the authority by physics
(`crates/physics/src/world.rs`, `length_unit: BU_PER_METER`) and by the renderer
(`WORLD_UNITS_PER_METER` in the generated shader constants). `GlobalTransform.translation` is therefore
in BU.

The audio crate hands those BU coordinates straight to kira — the listener in `sync_listener_pose`, and
every emitter at both `add_spatial_sub_track` call sites — with **no conversion anywhere**
(`grep -n "70\|meter\|METER" crates/audio/src/lib.rs` finds nothing in code, only prose). But the
attenuation constants those positions are measured against are unambiguously authored in **metres**, and
the code says so in its own words:

```rust
impl Default for Attenuation {
    fn default() -> Self {
        // Defaults chosen for Bethesda interior cells: inside a 2-3m
        // sphere it's full volume; out at 30m it's gone. Footsteps
        // and small impacts will want tighter ranges; ambient loops
        // and music want larger.
        Self { min_distance: 2.0, max_distance: 30.0 }
    }
}
```

kira's contract (`kira-0.10.8/src/track/sub/spatial_builder.rs`) is absolute: `min_distance` = "full
volume", `max_distance` = "**inaudible**", and `relative_distance` clamps to that band. Past
`max_distance` the spatializer interpolates all the way to `Decibels::SILENCE`
(`kira-0.10.8/src/track/sub.rs`) — not "quiet", zero.

The effective audible radii today:

| Site | Authored | Actual (÷70) |
|---|---|---|
| `Attenuation::default()` | 2 m … 30 m | **2.9 cm … 43 cm** |
| footsteps (`footstep_system`) | 0.5 m … 12 m | **7 mm … 17 cm** |
| water splash / ripple (`water_audio_system`) | 1 m … 24 m | **1.4 cm … 34 cm** |

## Why it hid for eleven audit cycles

This is the part worth recording, because the defect is not subtle — it was **structurally
unobservable**.

Until `948f104a` the **only** live emitter in the engine was co-located with the listener.
`byroredux/src/scene.rs` puts `AudioListener` and `FootstepEmitter` on the *same* camera entity, and
`footstep_system` emits at that entity's own `GlobalTransform`. Distance is exactly 0,
`relative_distance` is 0, the sound plays at full volume, always — and it does so under *any* unit
convention. Eleven prior `/audit-audio` cycles (2026-05-05 → 2026-08-16) verified dispatch, sub-track
lifecycle, reverb routing, listener pose and the `#1612` range normalisation, all of which are correct,
against a consumer that could not distinguish metres from BU.

`water_audio_system` (`948f104a`) is the first genuinely **offset** emitter, and it is measurably
crippled at both of its sources:

- **Dynamic-body splashes** (`make_water_interaction_system`, `byroredux/src/systems/water.rs`) fire at
  the body's waterline. A bottle thrown into a pond 2 m from the player is 140 BU away — nearly 6×
  `max_distance` — and is **completely silent**.
- **Camera-path splashes** (`submersion_system`, `byroredux/src/systems/water.rs`) fire at
  `[cam.x, volume.max[1], cam.z]`, offset vertically from the eye by up to `DISTURBANCE_BAND = 24.0` BU.
  Working the kira math at mid-band (12 BU): `relative_distance = (12-1)/(24-1) = 0.478`,
  `relative_volume = 0.52`, and interpolating `Decibels::SILENCE (-60 dB) → IDENTITY (0 dB)` gives
  **≈ -29 dB**, an amplitude of 0.036.

So the feature this session shipped is inaudible or near-inaudible in normal play, and nothing in the
logs says so.

## Evidence

- No conversion exists: `grep -n "BETHESDA_UNITS\|70.0" crates/audio/src/lib.rs` → zero hits.
- The BU authority is unambiguous and already consumed by two other subsystems:
  `crates/core/src/lighting.rs` (`BETHESDA_UNITS_PER_METER = 70.0`), `crates/physics/src/world.rs`
  (`length_unit: BU_PER_METER`).
- The metre wording is in `Attenuation::default`'s own comment, quoted above.
- Co-location: `byroredux/src/scene.rs` places `AudioListener`, `FootstepEmitter`, `SubmersionState` and
  `ActiveCamera` on one camera entity, in one block.
- The same metre assumption is baked into the audit skill: `.claude/commands/audit-audio/SKILL.md`
  Dimension 7 tells the auditor to "flag any widening (distant NPC footsteps audible across a whole
  interior)" of `{0.5, 12.0}` — a worry that is only coherent in metres. It should be corrected in the
  same pass, or the next cycle inherits the blind spot.

## Impact

The entire spatial-audio contract is wrong by a factor of ~70 for any sound not emitted at the
listener's own position.

Today the blast radius is the just-shipped water splash/ripple (silent or ≈-29 dB) plus
`Attenuation::default()`, which nothing consumes yet. **Tomorrow it is every planned producer**: Phase
3.5b FOOT (NPC footsteps), REGN ambient layers, weapon fire, dialogue, and any scripted emitter — all of
which would ship silent and be misdiagnosed as a decode, dispatch, or sub-track-capacity problem,
because the dispatch path itself is provably correct and the logs are clean. There is no diagnostic that
would surface it: `active_sound_count` counts the track, kira reports it playing, and the amplitude is
only decided inside the audio render thread.

## The fix is a units decision, not a multiply

Please **state the convention explicitly in code** rather than sprinkling a `* 70.0`, because every
future emitter will inherit whatever is decided here.

Recommended: **kira lives in metres; convert at the audio boundary, in exactly one place per direction.**

- Divide by `BETHESDA_UNITS_PER_METER` inside `sync_listener_pose` and at both `add_spatial_sub_track`
  sites (ideally via one `fn bu_to_audio_space(Vec3) -> Vec3` so there is a single named seam).
- This keeps the metre-authored `Attenuation` constants honest *and* matches kira's own metre-scaled
  internals — notably its hardcoded `EAR_DISTANCE = 0.1` (`kira-0.10.8/src/track/sub.rs`), which is
  10 cm of head width in kira's space and would otherwise be 10 cm *of Bethesda unit*, i.e. 1.4 mm of
  world, collapsing the stereo image.
- Document the convention on `Attenuation` itself ("distances are **metres**; the audio boundary
  converts world BU on the way in") so the next producer cannot get it wrong by reading the struct.

The alternative — scaling the `Attenuation` constants by 70 at construction — leaves kira's ear model in
the wrong space and re-opens the same trap for the next producer. Not recommended.

Guard: a test pinning the conversion — listener at origin, emitter at `30.0 * BETHESDA_UNITS_PER_METER`
BU must land exactly at `Attenuation::default().max_distance` in kira space.

## Related

- **#3086** (OPEN) — `AudioEmitter`'s dispatch-time frozen position. The *other* half of "the entity
  emitter path has never been exercised at a real offset"; both are latent for the same reason and both
  surface the moment a REGN/FOOT producer lands.
- Blocks Phase 3.5b FOOT: NPC footsteps are the first case where footsteps stop being co-located with
  the listener, and they will be silent until this lands. Fix this before FOOT.
- The `--sounds-bsa` footstep path is unaffected only by the accident of co-location.

## Completeness Checks

- [ ] **SIBLING**: both `add_spatial_sub_track` call sites (queue path *and* entity path) and the
      listener write converted — one missed site is a silent half-fix
- [ ] **CANONICAL-BOUNDARY**: the conversion lives at the audio boundary (one named helper), not
      re-derived per call site or pushed into gameplay systems
- [ ] **TESTS**: a regression test pins the BU→metre conversion at the boundary (listener at origin,
      emitter at `30 * BETHESDA_UNITS_PER_METER` ⇒ `max_distance`)
- [ ] **DOCS**: `Attenuation`'s docstring states the unit convention; `.claude/commands/audit-audio/SKILL.md`
      Dimension 7's metre-worded guidance corrected
