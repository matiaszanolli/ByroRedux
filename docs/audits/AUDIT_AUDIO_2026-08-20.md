# Audio Subsystem Audit (M44) — 2026-08-20

- **Command**: `/audit-audio` → all 7 dimensions, `--depth deep` (one leg of the
  25-audit `comprehensive` sweep)
- **Branch**: main · **HEAD**: `bb0b92f2`
- **kira**: pinned `0.10` (workspace `Cargo.toml`, unchanged) · resolved
  `kira-0.10.8`
- **Method**: single-agent, no sub-agents, no cargo invocation (suite rule —
  the target lock is contended). Every dimension re-derived from live source:
  `crates/audio/src/lib.rs` (1395 lines), `crates/audio/src/tests.rs`,
  `byroredux/src/systems/audio.rs` (712 lines), `byroredux/src/boot.rs`
  (scheduler + resource wiring), `byroredux/src/scene.rs`,
  `byroredux/src/asset_provider/texture.rs`, `byroredux/src/systems/water.rs`,
  `crates/physics/src/water.rs`, plus the vendored `kira-0.10.8` sources for the
  `FilterBuilder` / `SpatialTrackDistances` / sub-track `process` contracts.
  Dedup baseline: `/tmp/audit/issues.json` (400 issues, #2671–#3103) + the full
  prior `docs/audits/AUDIT_AUDIO_*.md` chain.

---

## Delta Analysis (since `AUDIT_AUDIO_2026-08-16.md`, HEAD `85b77371`)

`git log --since=2026-08-16 -- crates/audio/ byroredux/src/systems/audio.rs`
returns **two** commits — the session was water-dominated, but water reached
into audio in both directions:

| Commit | Change | Audio-relevant? |
|---|---|---|
| `948f104a` "Enhance water audio and rendering systems" | New `WaterAudioConfig` / `WaterAudioState` resources, `water_audio_system` (a **second** live `play_oneshot` caller — the first since M44 Phase 3.5), `try_load_default_water_splash` BSA loader, three new `Stage::Late` exclusives in `boot.rs` | **Yes — new consumer surface** |
| `75ad0653` "filter audio while submerged" | Per-spatial-track `FilterBuilder` low-pass, `ActiveSound.underwater_filter` / `.underwater`, `AudioWorld.underwater` + `set_underwater` / `underwater()`, `update_underwater_filters` in `audio_system` | **Yes — new crate API + per-frame pass** |

**Net: the first genuinely new audio surface in eleven cycles.** Every prior
cycle audited a crate whose only live consumer (`footstep_system`) emits sound
*at the listener's own position*. The water work adds the first emitter that is
positionally offset from the listener, and that is what exposes this cycle's
HIGH.

**Dispatch questions answered directly:**

1. *"Does anything in the audio layer need to react to submersion state?"* — It
   already does, as of `75ad0653`: `water_audio_system` pushes
   `SubmersionState.head_submerged` into `AudioWorld::set_underwater`, and
   `audio_system`'s new `update_underwater_filters` pass tweens every active
   spatial track's low-pass between `ABOVE_WATER_CUTOFF_HZ` (20 kHz) and
   `UNDERWATER_CUTOFF_HZ` (900 Hz). This is **not** a gap. What *is* a gap: the
   above-water state is not a bypass (AUD-2026-08-20-D1-01), music is not
   filtered (no `play_music` caller — future-phase, see below), and there is
   still no occlusion model (documented pending phase, correctly not flagged).
2. *"Verify the listener pose still tracks the right transform."* — **It does.**
   `byroredux/src/scene.rs:1054`/`1058`/`1065`/`1067` put `AudioListener`,
   `FootstepEmitter`, `SubmersionState` and `ActiveCamera` on the *same* camera
   entity, so `sync_listener_pose`'s "first `AudioListener` entity" and
   `water_audio_system`'s `ActiveCamera`-keyed `SubmersionState` lookup resolve
   to one entity by construction. The water work displaces the camera through
   `camera_follow_system`, which is a `Stage::Late` **parallel-batch** system and
   therefore completes before the `Stage::Late` exclusives (`water_audio_system`
   → `audio_system`) — so the listener pose is current. The one real ordering
   defect found is on the *submersion* read, not the listener write:
   AUD-2026-08-20-D6-01.

---

## Executive Summary

**7 dimensions run. 6 NEW findings (0 CRITICAL / 1 HIGH / 1 MEDIUM / 4 LOW),
plus 3 carried Existing.**

| # | Dimension | NEW findings |
|---|---|---|
| 1 | Spatial Sub-Track Lifecycle & Leaks | **1** (MEDIUM) |
| 2 | Listener Pose & Attenuation | **1** (HIGH) |
| 3 | SoundCache Growth & Eviction | 0 |
| 4 | Streaming Music Lifecycle | 0 |
| 5 | Reverb Send & Routing | 0 |
| 6 | Manager Lifecycle, ECS & Cell Streaming | **2** (LOW, LOW) |
| 7 | Gameplay Audio Wiring | **2** (LOW, LOW) |

- **Headless-mode boot**: **PASS**, unchanged. `AudioManager::new` failure still
  leaves `manager = None` (`lib.rs:332-339`); zero `.unwrap()` on the manager
  `Option`. The new `underwater` field is a plain `bool` with no device
  dependency, and `update_underwater_filters` sits behind `audio_system`'s
  `is_active()` early-return (`lib.rs:721-723`), so it never runs headless.
  `water_audio_system` sets the flag *before* any of its own early-returns, so
  a device-less or archive-less boot still keeps the state coherent.
- **Guards re-verified structurally** (not run — suite rule forbids cargo).
  Every #842/#843/#844/#845/#848/#849/#851/#852/#853/#858/#932/#1612/#2394/#2405
  anchor is intact at HEAD; see the Lifecycle Invariant Matrix. The two new
  commits added guards `underwater_listener_state_persists` (`tests.rs:471`) and
  `water_splash_event_reaches_audio_dispatcher` (`systems/audio.rs:524`).
- **Prior-cycle findings — all three filed, all three still OPEN and still
  present at HEAD** (noted and skipped per the dedup protocol):

| Issue | Finding | State at HEAD `bb0b92f2` |
|---|---|---|
| **#3086** (MEDIUM) | `AudioEmitter` docstring promises a per-frame spatial position update the code never performs | **Unchanged.** Docstring still at `lib.rs:632-634`; `grep set_position crates/audio/src/lib.rs` still returns only `703` (a doc line) and `826` (the **listener** handle). No emitter reposition exists. |
| **#3087** (LOW) | Stale scheduler-wiring comments | **Unchanged.** `boot.rs:1366-1371` still says *"The Phase 1 body is a stub"*; `systems/audio.rs:40-42` still attributes the `reverb_zone_system` registration to *main.rs*. |
| **#3088** (LOW) | `ROADMAP.md` M44 row stale/self-contradictory | **Half fixed, half re-drifted.** The self-contradiction is resolved (`ROADMAP.md:1085` now marks the reverb-toggle closed and cites #3088). The counts were refreshed to 21/6 + 10 on 2026-08-19 and have already drifted again — live is **22 default + 6 ignored** in the crate (`crates/audio/src/tests.rs`: 28 `#[test]`, 6 `#[ignore]`) and **11** in `byroredux/src/systems/audio.rs`, i.e. **39 audio tests total**, not 37. |

- **Shipped surface, re-confirmed**: `AudioWorld` graceful degradation
  (`SUB_TRACK_CAPACITY = 512` / `SEND_TRACK_CAPACITY = 32`, applied at
  `lib.rs:327-328`); `AudioListener` / `AudioEmitter` / `OneShotSound`;
  `audio_system` = `sync_listener_pose` → **`update_underwater_filters`** →
  `drain_pending_oneshots` → `dispatch_new_oneshots` → `prune_stopped_sounds`;
  both dispatch paths (queue `VecDeque` cap 256 `pop_front`; entity path with
  `loop_region(..)`); tweened-`stop()` despawn truncation with `stop_issued`
  debounce; single-slot streaming music on the main track; global reverb send
  (`NEG_INFINITY` dry default, shared `apply_reverb_send` / `reverb_send_gate_open`).
  Engine consumers are now **two**: `footstep_system` and `water_audio_system`;
  `reverb_zone_system` remains the only `set_reverb_send_db` caller.
- **Pending (future-phase, correctly not flagged as missing)**: Phase 3.5b FOOT
  → per-material sound, REGN ambient soundscapes, MUSC routing, per-cell
  acoustics, raycast occlusion.
- **MUSC parse→play gap — re-confirmed still absent by design.** `grep play_music`
  across `byroredux/` and every non-audio crate returns zero hits; the FormIDs
  are still parsed (`default_music`/ZNAM, `music_type_form`/XCMO in
  `crates/plugin/src/esm/cell/`). Single-slot / main-track / streaming-type
  invariants stay pinned for the eventual caller.

---

## Lifecycle Invariant Matrix

Owned by Dimension 6 per the skill's dedup instruction (Dims 1/4/5 point here).

| Invariant | State | Anchor |
|---|---|---|
| `AudioWorld` field-drop order (`active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager` → `multi_listener_warned` → **`underwater`**) | **HOLDS** — the new field is a `bool` appended last, no Drop participation | `lib.rs:268-310` |
| `ActiveSound` field order (`entity` → `handle` → `_track` → **`underwater_filter`** → **`underwater`** → `unload_fade_ms` → `stop_issued`) | **HOLDS** — `FilterHandle` (`kira/src/effect/filter/handle.rs`) owns only `CommandWriters`, no back-reference to the track, so dropping it after `_track` is inert | `lib.rs:216-241` |
| Manager capacities exceed kira defaults (512 / 32) | HOLDS | `lib.rs:199-200`, applied `327-328` |
| `ActiveSound._track` underscore name intact, Drop-side-effect only | HOLDS | `lib.rs:217`, `905`, `1063` |
| Lazy listener creation, no frame-1 cold-start panic | HOLDS | `lib.rs:796`, `798`, `801`, `807` |
| Sticky listener, never cleared on entity churn (#849) | HOLDS | written only at `lib.rs:817`; no clear site exists |
| Multi-listener diagnostic debounced (#843) | HOLDS | `multi_listener_warned` never reset |
| Listener orientation quat is renderer-space (no residual Z-up→Y-up) | HOLDS | `lib.rs:827`; conversion resolved at NIF import (`crates/nif/src/import/coord.rs`) |
| **Listener entity == `ActiveCamera` == `SubmersionState` owner == `FootstepEmitter` owner** | **HOLDS, by construction** | `byroredux/src/scene.rs:1054`/`1058`/`1065`/`1067` |
| Attenuation `RangeInclusive`, `min<=max` normalized (#1612) | HOLDS | `Attenuation::distance_range()` `lib.rs:612-616` |
| **Attenuation distances are in the same unit as `GlobalTransform`** | **VIOLATED** — constants are metres, world is Bethesda units (70 BU/m) — **AUD-2026-08-20-D2-01** | `lib.rs:619-630` doc vs `crates/physics/src/world.rs:46` |
| `add_listener` failure is transient-retry, not permanent lockout | HOLDS | `lib.rs:821-823` |
| Both dispatch paths gate on `listener_id` before any work | HOLDS | `lib.rs:848` / `927` |
| Both dispatch paths apply the reverb gate through one shared helper (#2405) | HOLDS | `lib.rs:869` / `1006` → `apply_reverb_send` `161-172` |
| **Both dispatch paths build the underwater filter identically** | HOLDS — byte-identical 11-line block at both sites (see AUD-2026-08-20-D1-01 for the copy-paste note) | `lib.rs:874-884` / `1011-1021` |
| **Underwater filter is applied post-effect and therefore feeds the reverb send** | HOLDS — kira's sub-track `process` runs effects → spatialization → volume → sends | `kira-0.10.8/src/track/sub.rs:201-204` |
| `looping` / `loop_region(..)` in the entity path only | HOLDS | `lib.rs:1042-1046`; `PendingOneShot` has no `looping` field |
| Volume→dB centralized (`linear_volume_to_db`) | HOLDS | `lib.rs:147`, called from `492`, `893`, `1039` |
| Drain cap (>32/tick warns) · producer cap (256 `pop_front`) · drain-gate-before-`mem::take` (#851/#852/#853) | HOLDS | `lib.rs:442-451`, `843`→`856` |
| `OneShotSound` marker consumed on success **and** both failure arms (#2394) | HOLDS — exactly 3 `consumed.push` sites | `lib.rs:1031`, `1056`, `1069` |
| Despawn truncation, `stop_issued` debounce (#844/#845/#858) | HOLDS | `lib.rs:1110`, `1141` |
| `SoundCache` lowercase-once, dormant in engine (#859/#850) | HOLDS — still zero producers; only `byroredux/src/ownership_sample.rs:65` reads it diagnostically. Two loaders now bypass it (see AUD-2026-08-20-D7-01) | — |
| Single-slot music · main track · streaming types · fade-then-drop | HOLDS | unchanged since 08-16 |
| Reverb `None`-safe · `NEG_INFINITY` default · `> SILENCE_DB` gate | HOLDS | `lib.rs:161-179` |
| Late-stage exclusive order: ragdoll → `water_damage` → `water_interaction` → **`water_audio`** → **`audio_system`** → `event_cleanup` | **HOLDS** — the new water systems are registered *before* `audio_system`, so `set_underwater` lands the same tick it is consumed, and `event_cleanup_system` drains `SplashEvent`/`RippleEvent` *after* audio has read them | `boot.rs:1317`, `1333`, `1344`, `1355`, `1383`, `1422`; `crates/scripting/src/cleanup.rs:34-35` |
| `footstep_system` (`PostUpdate`) enqueues, `audio_system` (`Late`) drains — same frame | HOLDS | `boot.rs:1054`, `1383` |
| `AudioWorld::new()` called exactly once, at boot | HOLDS | single call site `boot.rs` |
| **`submersion_system` reads a camera pose `camera_follow_system` has not yet written this frame** | **VIOLATED** in player/third-person mode — **AUD-2026-08-20-D6-01** | `boot.rs:1219-1233` (PostUpdate) vs `boot.rs:1271-1279` (Late) |
| **Crate docstring / feature-matrix / ROADMAP describe the shipped audio surface** | **DRIFTED** — water + underwater audio undocumented everywhere — **AUD-2026-08-20-D6-02** | `lib.rs:1-118`, `docs/feature-matrix.md:133-144`, `ROADMAP.md:695` |

---

## Findings

### AUD-2026-08-20-D2-01: Spatial attenuation distances are authored in metres but consumed in Bethesda units — every emitter not co-located with the listener is inaudible past ~17–43 cm

- **Severity**: HIGH
- **Dimension**: Listener Pose & Spatial Attenuation Correctness
- **Location**: `crates/audio/src/lib.rs:619-630` (`Attenuation::default` + its
  metre-worded doc), `crates/audio/src/lib.rs:998-1003` (the "game-units"
  comment), `crates/audio/src/lib.rs:826` (listener position, raw BU),
  `crates/audio/src/lib.rs:885`/`1022` (`add_spatial_sub_track(.., p.position, ..)`,
  raw BU), `byroredux/src/systems/audio.rs:187-188` (footstep `{0.5, 12.0}`),
  `byroredux/src/systems/audio.rs:271-274`/`288-291` (water splash `{1.0, 24.0}`)
- **Status**: NEW
- **Description**: The engine's world space is **Bethesda units** — 70 BU per
  metre, declared once in `crates/core/src/lighting.rs:16`
  (`BETHESDA_UNITS_PER_METER: f32 = 70.0`) and consumed as the authority by
  physics (`crates/physics/src/world.rs:46,178`, `length_unit: BU_PER_METER`)
  and the renderer (`WORLD_UNITS_PER_METER` in the generated shader constants).
  `GlobalTransform.translation` is therefore in BU.

  The audio crate hands those BU coordinates straight to kira — the listener at
  `lib.rs:826` (`handle.set_position(pose.0, ..)`) and every emitter at
  `lib.rs:885`/`1022` — with **no conversion anywhere** (`grep -n "70\|meter\|METER\|scale"
  crates/audio/src/lib.rs` finds only prose). But the attenuation constants those
  positions are measured against are unambiguously authored in **metres**, and
  the code says so in its own words:

  ```rust
  // lib.rs:619-630
  impl Default for Attenuation {
      fn default() -> Self {
          // Defaults chosen for Bethesda interior cells: inside a 2-3m
          // sphere it's full volume; out at 30m it's gone. ...
          Self { min_distance: 2.0, max_distance: 30.0 }
      }
  }
  ```

  kira's contract (`kira-0.10.8/src/track/sub/spatial_builder.rs:346-358`) is
  absolute: `min_distance` = "full volume", `max_distance` = "**inaudible**",
  and `relative_distance` clamps to that band. Past `max_distance` the
  spatializer interpolates all the way to `Decibels::SILENCE`
  (`kira-0.10.8/src/track/sub.rs:312-323`) — not "quiet", zero.

  So the effective audible radii today are:

  | Site | Authored | Actual (÷70) |
  |---|---|---|
  | `Attenuation::default()` | 2 m … 30 m | **2.9 cm … 43 cm** |
  | footsteps (`systems/audio.rs:187-188`) | 0.5 m … 12 m | **7 mm … 17 cm** |
  | water splash (`systems/audio.rs:271-274`) | 1 m … 24 m | **1.4 cm … 34 cm** |

- **Evidence**: The reason eleven prior audit cycles missed this is that until
  `948f104a` the **only** live emitter was co-located with the listener:
  `byroredux/src/scene.rs:1054` puts `AudioListener` and `:1058` puts
  `FootstepEmitter` on the *same* camera entity, and `footstep_system` emits at
  that entity's own `GlobalTransform` — distance 0, `relative_distance` = 0,
  full volume, always. The bug was structurally invisible.

  `water_audio_system` is the first emitter with a real offset, and it is
  measurably crippled by this at both of its sources:
  - **Dynamic-body splashes** (`make_water_interaction_system`,
    `byroredux/src/systems/water.rs:407-434`) fire at the body's waterline. A
    bottle thrown into a pond 2 m from the player is 140 BU away — nearly 6×
    `max_distance` — and is **completely silent**.
  - **Camera-path splashes** (`submersion_system`,
    `byroredux/src/systems/water.rs:264-291`) fire at
    `[cam.x, volume.max[1], cam.z]`, i.e. offset vertically from the eye by up to
    `DISTURBANCE_BAND = 24.0` BU. Working the kira math at the mid-band
    (12 BU): `relative_distance = (12-1)/(24-1) = 0.478`, `relative_volume = 0.52`,
    and interpolating `Decibels::SILENCE (-60 dB) → IDENTITY (0 dB)` gives
    **≈ -29 dB** — an amplitude of 0.036. The splash the session shipped is
    inaudible in normal play.

  The metre assumption is baked into the audit skill too — `.claude/commands/audit-audio/SKILL.md`
  Dimension 7 tells the auditor to "flag any widening (distant NPC footsteps
  audible across a whole interior)" of `{0.5, 12.0}`, which is only a coherent
  worry in metres. That is worth fixing alongside the code.
- **Impact**: The entire spatial-audio contract is wrong by a factor of ~70 for
  any sound not emitted at the listener's own position. Today the blast radius
  is the just-shipped water splash (silent or near-silent in every realistic
  case) plus `Attenuation::default()`, which nothing consumes yet. Tomorrow it
  is every planned producer: Phase 3.5b FOOT (NPC footsteps), REGN ambient
  layers, weapon fire, dialogue, and any scripted emitter — all of which would
  ship silent and be misdiagnosed as a decode, dispatch or sub-track-capacity
  problem, because the dispatch path itself is provably correct and the logs are
  clean. There is no diagnostic that would surface it: `active_sound_count`
  counts the track, kira reports it playing, and the amplitude is only decided
  inside the audio render thread.
- **Related**: #3086 (`AudioEmitter`'s frozen dispatch-time position) is the
  *other* half of the "the entity emitter path has never been exercised at a
  real offset" story — both are latent for the same reason and both surface the
  moment a REGN/FOOT producer lands. Also touches the `--sounds-bsa` footstep
  path, which is unaffected only by the accident of co-location.
- **Suggested Fix**: Convert at the audio boundary, in one place, rather than
  rescaling every call site: multiply positions by
  `1.0 / byroredux_core::lighting::BETHESDA_UNITS_PER_METER` inside
  `sync_listener_pose` and both `add_spatial_sub_track` sites (a metre-space
  listener/emitter pair keeps the metre-authored `Attenuation` constants honest
  and matches kira's metre-scaled internals, including its hardcoded
  `EAR_DISTANCE = 0.1` at `kira-0.10.8/src/track/sub.rs:349`). Add a guard that
  pins the conversion — e.g. a listener at the origin and an emitter at
  `30.0 * BETHESDA_UNITS_PER_METER` must land exactly at
  `Attenuation::default().max_distance` in kira space. Alternatively scale the
  `Attenuation` constants by 70 at construction, but that leaves kira's ear model
  in the wrong space and re-opens the same trap for the next producer.

### AUD-2026-08-20-D1-01: The above-water state of the new per-track low-pass is not a bypass — every spatial sound is permanently routed through a fully-wet SVF at a below-Nyquist cutoff

- **Severity**: MEDIUM
- **Dimension**: Spatial Sub-Track Lifecycle & Leaks
- **Location**: `crates/audio/src/lib.rs:140-141` (`UNDERWATER_CUTOFF_HZ` /
  `ABOVE_WATER_CUTOFF_HZ`), `crates/audio/src/lib.rs:874-884` (queue path),
  `crates/audio/src/lib.rs:1011-1021` (entity path),
  `crates/audio/src/lib.rs:732-746` (`update_underwater_filters`)
- **Status**: NEW
- **Description**: `75ad0653` adds a `FilterBuilder` low-pass effect to **every**
  spatial sub-track at construction, in both dispatch paths, and switches its
  cutoff between two constants:

  ```rust
  const UNDERWATER_CUTOFF_HZ: f64 = 900.0;
  const ABOVE_WATER_CUTOFF_HZ: f64 = 20_000.0;
  ```

  There is no dry/bypass state. `FilterBuilder::default()` is
  `mix: Value::Fixed(Mix::WET)` (`kira-0.10.8/src/effect/filter/builder.rs:81`)
  and the builder chain only sets `.mode(..)` and `.cutoff(..)`, so the filter is
  100 % wet at all times — the "above water" state is a 20 kHz low-pass, not an
  absent one.

  A 20 kHz cutoff is not transparent in kira's filter. It is Simper's SVF
  (`kira-0.10.8/src/effect/filter.rs:88-103`) with `resonance` defaulting to
  `0.0`, giving `k = 2.0` and therefore **Q = 0.5** — two coincident real poles,
  a gentle but early roll-off rather than a brick wall. Evaluating
  `|H| = 1/sqrt((1-r²)² + (k·r)²)` with `r = f/f_c`:

  | Frequency (at `f_c` = 20 kHz) | `r` | Gain |
  |---|---|---|
  | 2 kHz | 0.10 | −0.09 dB |
  | 5 kHz | 0.25 | −0.5 dB |
  | 10 kHz | 0.50 | **−1.9 dB** |
  | 15 kHz | 0.75 | **−3.9 dB** |

  and the digital response is *worse* than this analog prototype, because the
  cutoff is a large fraction of Nyquist: `g = tan(π · clamp(f_c/f_s, 0.0001, 0.5))`
  puts `f_c/f_s` at 0.417 on a 48 kHz device and **0.454 on a 44.1 kHz device**,
  where the tan pre-warp steepens the curve further. The dry path is
  device-rate-dependent, which is the part that makes this a correctness issue
  rather than a taste one: the same content sounds different on a 44.1 kHz and a
  48 kHz output device.
- **Evidence**: `crates/audio/src/lib.rs:879-883` (identical at `1016-1020`):
  ```rust
  let underwater_filter = track_builder.add_effect(
      FilterBuilder::new()
          .mode(FilterMode::LowPass)
          .cutoff(cutoff),
  );
  ```
  No `.mix(..)` call anywhere in the crate (`grep -n "\.mix(" crates/audio/src/lib.rs`
  returns only the `ReverbBuilder` site at `lib.rs:358`, which is deliberately
  `Mix::WET` for a send). `update_underwater_filters` only ever writes
  `set_cutoff`, never `set_mix`, so no later path can restore a dry signal.
  Secondary note, not scored separately: the 11-line filter-construction block is
  duplicated byte-for-byte between the two dispatch paths — precisely the shape
  that #2405 was filed and fixed for on the reverb-send gate, which now lives in
  the shared `apply_reverb_send` helper three lines above each copy.
- **Impact**: Every spatial sound in the engine loses roughly 2 dB at 10 kHz and
  4 dB at 15 kHz, permanently, on dry land — a subtle but global dulling of the
  top octave, and one that shifts with the output device's sample rate. It also
  costs a per-sample biquad on every one of up to `SUB_TRACK_CAPACITY = 512`
  tracks for no benefit in the (overwhelmingly common) above-water case. Not
  audible enough to be a defect anyone would report; exactly the kind of thing
  that is impossible to find later, once REGN/FOOT content is layered on top and
  "the audio sounds a bit dull" has ten candidate causes.
- **Related**: Same construction-time-vs-live-handle shape as #847's documented
  reverb-send limitation, but unlike #847 this one *does* have a live setter
  (`FilterHandle::set_cutoff`) and is already used per-frame — so the fix does
  not need a new mechanism. The duplicated block is the same class as #2405.
- **Suggested Fix**: Extract the filter construction into an
  `apply_underwater_filter(track_builder, underwater)` helper next to
  `apply_reverb_send`, and make the above-water state genuinely transparent —
  either by driving `Mix` alongside the cutoff (`Mix(0.0)` dry / `Mix::WET`
  submerged, tweened together in `update_underwater_filters`), or by setting
  `ABOVE_WATER_CUTOFF_HZ` above any plausible Nyquist (e.g. `96_000.0`, which
  clamps `f_c/f_s` to the `0.5` ceiling and passes the input through unchanged).
  The `Mix` route is preferable: it is exact at every device rate and does not
  rely on the clamp's edge behaviour.

### AUD-2026-08-20-D6-01: `submersion_system` reads a camera pose that `camera_follow_system` writes later in the same frame, so the underwater filter lags the eye by one frame — and the scheduler comment asserts the opposite ordering

- **Severity**: LOW
- **Dimension**: Manager Lifecycle, ECS Lifecycle & Cell Streaming
- **Location**: `byroredux/src/boot.rs:1212-1233` (`submersion_system`,
  `Stage::PostUpdate`), `byroredux/src/boot.rs:1271-1279` (the false ordering
  claim, `camera_follow_system`, `Stage::Late`)
- **Status**: NEW (the registration predates this cycle — `8a404914` — but it
  only became an *audio* dependency with `75ad0653`, and no prior audio report
  covered `submersion_system`)
- **Description**: The comment above `camera_follow_system` states:

  > *M28.5 — camera follow runs in Stage::Late, AFTER `physics_sync_system` has
  > settled the kinematic body's post-step pose. **Must run BEFORE `audio_system`
  > / `submersion_system`** (both read camera GlobalTransform).*

  It runs before `audio_system` — both are `Stage::Late`, and the parallel batch
  completes before the exclusives, so the listener pose is correct (this is the
  half the dispatch asked about, and it holds). It does **not** run before
  `submersion_system`, which is registered in `Stage::PostUpdate`
  (`boot.rs:1219`) — an earlier stage entirely. `submersion_system`'s own comment
  compounds the error: *"runs in PostUpdate after bound propagation so the
  camera's GlobalTransform is already current for the frame"*, which is true only
  in fly-cam mode, where the fly camera writes `Transform` in `Stage::Update` and
  PostUpdate propagation resolves it.

  In player / third-person mode `camera_follow_system` is the pose author and
  writes both `Transform` and `GlobalTransform` directly ("to bypass the missing
  late-stage propagation pass", per its own comment). The value
  `submersion_system` reads at PostUpdate is therefore the previous frame's
  camera pose — it predates both this frame's `Stage::Physics` step and this
  frame's camera follow.
- **Evidence**: Stage order is `Early → Update → PostUpdate → Physics → Late`.
  `grep -n "Stage::" byroredux/src/boot.rs` puts `submersion_system` at `1220`
  (`Stage::PostUpdate`) and `camera_follow_system` at `1279` (`Stage::Late`).
  The consumer chain is `submersion_system` → `SubmersionState.head_submerged` →
  `water_audio_system` (`byroredux/src/systems/audio.rs:212-225`) →
  `AudioWorld::set_underwater` → `update_underwater_filters`.
- **Impact**: Exactly one frame (~16 ms) of latency on the underwater low-pass
  transition, and on the underwater composite tint that reads the same state, in
  player mode only. Below audibility on a normal wade-in; it is the *comment*
  that carries the real cost — it asserts an ordering guarantee that does not
  hold, and the next person hardening this chain (per-cell acoustics, occlusion,
  a submerged-listener reverb send) will reason from it and be wrong. Both
  comments should be corrected even if the stage is left where it is.
- **Related**: Same class as #3087 (stale audio scheduler-wiring comments), in
  the adjacent block of the same file — worth fixing in the same pass. The
  fly-cam-only correctness of the PostUpdate placement is why it has never
  produced a visible symptom.
- **Suggested Fix**: Either move `submersion_system` to `Stage::Late` as an
  exclusive registered immediately after `camera_follow_system` and before
  `water_audio_system` (which restores the intent and costs nothing — it already
  writes only `SubmersionState` + transient markers), or leave the stage and
  correct both comments to state that the camera pose is one frame stale in
  player mode. The first is preferable; it makes the `boot.rs:1271-1279` claim
  true instead of aspirational.

### AUD-2026-08-20-D6-02: The shipped water / underwater audio surface is undocumented in all three status sources

- **Severity**: LOW
- **Dimension**: Manager Lifecycle & ECS/Cell Streaming (documentation)
- **Location**: `crates/audio/src/lib.rs:1-118` (module docstring),
  `docs/feature-matrix.md:133-144` (M44 table), `ROADMAP.md:695` (M44 row)
- **Status**: NEW (the test-count half is **Existing: #3088** — see below)
- **Description**: The audit skill makes the crate docstring a first-class
  contract: *"If the docstring drifts from the user-visible API, that's a finding
  in itself."* Two commits added public API and a per-frame pass, and none of the
  three authoritative status sources mention any of it:

  1. **`crates/audio/src/lib.rs`** — the phase-by-phase docstring ends at
     "# Phase 6" and its "# Future work" list still reads FOOT / REGN / MUSC /
     per-cell acoustics + occlusion. `AudioWorld::set_underwater` /
     `AudioWorld::underwater()` are new public methods with no module-level
     coverage, and `audio_system`'s own numbered docstring (`lib.rs:700-716`)
     lists three steps — listener sync, dispatch, prune — when the body now runs
     five, including the queue drain (which predates this cycle) and
     `update_underwater_filters`.
  2. **`docs/feature-matrix.md:133`** — the section is still titled
     *"Audio (M44 — Phases 1–6 complete)"* with no row for underwater filtering
     or water-surface one-shots, which the skill designates as *"the
     authoritative runtime-status table"*.
  3. **`ROADMAP.md:695`** — the M44 row enumerates Phases 1–6 in detail and stops
     there; no mention of `water_audio_system`, `WaterAudioConfig`, or the
     submersion low-pass, despite this being the first new M44 consumer since
     Phase 3.5.
- **Evidence**: `grep -n "underwater\|submerged\|splash\|water" crates/audio/src/lib.rs`
  finds hits only in the code body (`137-141`, `220-223`, `308-310`, `563-572`,
  `732-746`, `874-884`, `1011-1021`) and none in lines 1-118. `docs/feature-matrix.md`
  lines 137-144 list eight rows, none water-related.

  **Test counts (Existing: #3088, do not re-file):** #3088 is still OPEN and
  already owns this. Its self-contradiction half *has* been fixed
  (`ROADMAP.md:1085`), and the counts were refreshed on 2026-08-19 to
  "21 default + 6 ignored" / "10 more" / "37 total" — but the two new guards
  landed after that refresh, so the live figures are now **22 default + 6
  ignored** (`crates/audio/src/tests.rs`: 28 `#[test]`, 6 `#[ignore]`), **11** in
  `byroredux/src/systems/audio.rs`, **39 total**. Recorded here so the #3088 fix
  lands on the right numbers.
- **Impact**: Documentation only, no runtime behaviour. The concrete cost is the
  next audit cycle or contributor treating underwater filtering as unimplemented
  (the docstring's "Future work" section is what a reader consults for exactly
  that question), and re-deriving or duplicating it — the same trap that
  produced ~5 of 30 bad findings in past sweeps, and the reason the skill lists
  docstring drift as a reportable defect.
- **Related**: #1859 / `AUD-2026-07-02-01` was the same class (`SoundCache`
  docstring citing a pre-Session-34 path). #3087 is the sibling comment rot in
  `boot.rs` / `systems/audio.rs`. All three plus AUD-2026-08-20-D6-01 are one
  documentation pass.
- **Suggested Fix**: Add a phase block to the module docstring covering the
  submersion low-pass (`set_underwater` / `underwater()` /
  `update_underwater_filters` / the two cutoff constants) and move nothing out of
  "Future work" that has not actually shipped; refresh `audio_system`'s numbered
  step list to five; add "Underwater low-pass (submersion-driven)" and
  "Water-surface splash one-shots" rows to `docs/feature-matrix.md`; extend the
  `ROADMAP.md` M44 row with the water-audio consumer and the corrected counts as
  part of closing #3088.

### AUD-2026-08-20-D7-01: `water_audio_system` mixes the position of one `RippleEvent` with the intensity of another, and throttles all water surfaces through a single global cooldown

- **Severity**: LOW
- **Dimension**: Gameplay Audio Wiring
- **Location**: `byroredux/src/systems/audio.rs:257-268` (the ripple selection),
  `byroredux/src/systems/audio.rs:236-241` + `:296-300`
  (`WaterAudioState.ripple_cooldown`), `byroredux/src/components.rs:1288-1305`
  (the resource)
- **Status**: NEW
- **Description**: The ripple selection takes its **position** from the first
  `RippleEvent` the query yields, but its **intensity** from the maximum across
  *all* live `RippleEvent`s:

  ```rust
  let ripple = world.query::<RippleEvent>().and_then(|events| {
      events.iter().next().map(|(_, event)| {
          (
              Vec3::new(event.position[0], event.position[1], event.position[2]),
              events.iter()
                  .map(|(_, candidate)| candidate.intensity)
                  .fold(0.0f32, f32::max),
          )
      })
  });
  ```

  When more than one `RippleEvent` is live the played sound is a hybrid of two
  different events: the loudest disturbance's amplitude, placed at an unrelated
  surface's coordinates. Storage iteration order is not a stable, meaningful
  ordering (both producers insert keyed by *surface* entity —
  `byroredux/src/systems/water.rs:267-277` for the camera path and `:410-420` for
  the physics path), so which event donates the position is effectively
  arbitrary and can change frame to frame, moving the sound between surfaces.

  Separately, `WaterAudioState.ripple_cooldown` is a single scalar for the whole
  world. One ripple anywhere suppresses ripples on every other surface for
  180 ms — so a player standing at the edge of one pond mutes the stream running
  beside it, and vice versa, with no relation to which is closer or louder.
- **Evidence**: Multiple concurrent `RippleEvent`s are ordinary, not
  hypothetical — `submersion_system` inserts one **per water-volume entity**
  whose `disturbance_rate > 0` (`byroredux/src/systems/water.rs:264-277`, looping
  over every `(WaterVolume, ParticleEmitter)` pair), and exterior worldspaces
  spawn one water plane per cell, so a camera standing on a shared cell edge
  produces one event per adjacent plane in the same tick.
  `make_water_interaction_system` adds one more per wet surface
  (`byroredux/src/systems/water.rs:407-420`). The splash path, by contrast, is
  handled correctly — `byroredux/src/systems/audio.rs:245-255` collects **all**
  splashes and plays each at its own position with its own intensity.
- **Impact**: Cosmetic-to-mild audio artefact, confined to multi-surface
  situations: a ripple can be spatialised at the wrong pond, and one surface's
  cooldown silences another's. It does not leak, panic, or accumulate, and — per
  AUD-2026-08-20-D2-01 — it is currently masked by the fact that the ripple is
  barely audible at all. Worth fixing before the fix for D2-01 makes these
  sounds actually reach the listener.
- **Related**: The splash branch in the same function is the correct pattern to
  mirror. The 180 ms literal (`state.ripple_cooldown = 0.18`,
  `systems/audio.rs:299`) is an undeclared magic number sitting inline in the
  system body, unlike `INTERIOR_REVERB_SEND_DB` / `EXTERIOR_REVERB_SEND_DB`
  two functions above, which are named `const`s — worth promoting in the same
  edit.
- **Suggested Fix**: Select one ripple as a unit — e.g.
  `events.iter().max_by(|a, b| a.1.intensity.total_cmp(&b.1.intensity))` — so the
  position and intensity come from the same event; and key the cooldown by
  surface entity (a small `FxHashMap<EntityId, f32>` on `WaterAudioState`, or at
  minimum reset it only for the surface that actually played). Promote `0.18` to
  a named `const RIPPLE_COOLDOWN_SECS`.

### AUD-2026-08-20-D7-02: `try_load_default_water_splash` duplicates the `--sounds-bsa` scan and re-opens the same archive a second time at boot; both loaders still bypass `SoundCache`

- **Severity**: LOW
- **Dimension**: Gameplay Audio Wiring
- **Location**: `byroredux/src/asset_provider/texture.rs:92-131`
  (`try_load_default_footstep`), `byroredux/src/asset_provider/texture.rs:134-181`
  (`try_load_default_water_splash`), `byroredux/src/boot.rs:523-526` (both call
  sites)
- **Status**: NEW
- **Description**: `948f104a` added a second boot-time sound loader that is a
  structural copy of the first. Both are invoked back-to-back
  (`boot.rs:523`/`526`) with the same `args`, both scan for `--sounds-bsa`
  (`try_load_default_footstep` with a hand-rolled `while i < args.len()` loop,
  `try_load_default_water_splash` with `args.windows(2).find(..)` — two different
  idioms for one job), and **both call `Archive::open(path)` independently on the
  same file**, so the BSA header plus the full folder/file record tables are
  parsed twice per boot for one archive. `Fallout - Sound.bsa` is not small; this
  is duplicated I/O and duplicated table allocation for no benefit.

  Neither loader routes through `SoundCache` — each writes its decoded
  `Arc<StaticSoundData>` straight into its own config resource
  (`FootstepConfig.default_sound`, `WaterAudioConfig.splash_sound`). The skill's
  Dimension 3 task is explicitly to "flag if anyone wires the first consumer
  WITHOUT also wiring eviction"; this is not that (nothing was wired *into* the
  cache), but it is the second producer to route around it, which makes the
  dormant-API argument in #859/#850 weaker each cycle — the cache now has two
  natural consumers and zero actual ones.
- **Evidence**: `grep -rn "SoundCache" byroredux/src` returns only
  `byroredux/src/ownership_sample.rs:61,65`, a diagnostic read of a resource
  nothing installs. `grep -n "Archive::open" byroredux/src/asset_provider/texture.rs`
  returns two sites within 50 lines of each other, both fed by the same
  `--sounds-bsa` value.
- **Impact**: Boot-time only, no runtime cost, no correctness issue — the engine
  boots correctly with the archive absent (both loaders log WARN and return, and
  `water_audio_system` no-ops on `splash_sound: None`, which the
  `water_splash_event_reaches_audio_dispatcher` guard covers). The cost is
  maintenance: two divergent arg parsers for one flag, and a third copy is the
  obvious next step when FOOT records or REGN ambients need their own boot-time
  loader.
- **Related**: #859 / #850 (`SoundCache` dormancy). Directly against the project
  standing instruction to *"always prioritize improving existing code rather than
  duplicating logic"*.
- **Suggested Fix**: Fold both into one `try_load_default_sounds(world, args)`
  that resolves `--sounds-bsa` once, opens the archive once, and populates both
  `FootstepConfig.default_sound` and `WaterAudioConfig.splash_sound` from that
  single handle — ideally through `SoundCache::get_or_load`, which would give the
  cache its first real producer and let the eventual FOOT/REGN loaders reuse the
  decode. If the cache is wired, wire eviction with it per #850.

---

## Disproved candidates (investigated, not reported)

Recorded so the next cycle doesn't re-derive them.

- **Double splash for the player: two producers, one water entry.**
  `submersion_system` emits `SplashEvent` keyed on the *water volume* entity and
  `make_water_interaction_system` emits one keyed on the *body* entity, so both
  could in principle fire for one actor and `water_audio_system` plays one
  one-shot per event with no dedup. **Disproved for the player**: the physics
  path skips non-dynamic bodies outright (`crates/physics/src/water.rs:596-598`,
  `if body.body_type() != RigidBodyType::Dynamic { continue; }`) and the player
  is a kinematic `CharacterController`, so it never receives a `WaterContact`.
  The two producers cover disjoint sets — camera vs. dynamic clutter/ragdolls.
  Re-check the moment the player gains a dynamic body or NPCs gain the camera
  path.
- **Camera-proximity splash audible from the shoreline.** `disturbance_rate`
  (`byroredux/src/systems/water.rs:72-87`) fires on `DISTURBANCE_BAND = 24.0`
  vertical / `DISTURBANCE_RADIUS = 18.0` horizontal, which read as an alarmingly
  wide trigger. **Disproved**: those are Bethesda units — 70 BU/m
  (`crates/core/src/lighting.rs:16`) — so the band is ~34 cm vertical and ~26 cm
  horizontal. The camera must be essentially at the waterline. (Chasing this is
  what surfaced AUD-2026-08-20-D2-01.)
- **Per-frame `Vec` allocation in `water_audio_system` as a #932 sibling.** The
  `splashes: Vec<(Vec3, f32)>` collect looks like the allocation #932 removed
  from `footstep_system`. **Disproved**: `.collect()` on an empty iterator does
  not allocate, and `SplashEvent` is an edge-triggered marker drained every frame
  by `event_cleanup_system`, so the allocation only happens on the rare frames
  that actually have a splash. Not comparable to `footstep_system`'s
  every-frame-forever allocation.
- **Reverb send bypasses the underwater filter** (a dry, bright reverb tail while
  submerged). **Disproved**: kira's sub-track `process`
  (`kira-0.10.8/src/track/sub.rs:201-204`) runs effects *before* spatialization,
  volume and sends, so the send is fed the already-filtered signal.
- **`FilterHandle` dropping after its parent `SpatialTrackHandle` violates the
  field-drop-order invariant.** `ActiveSound` declares `_track` before
  `underwater_filter`. **Disproved**: `FilterHandle`
  (`kira-0.10.8/src/effect/filter/handle.rs`) holds only `CommandWriters` and has
  no back-reference to the track or manager; dropping it at any point is inert.
- **Listener/underwater entity mismatch** (the listener resolved from the first
  `AudioListener` marker, the submersion state from `ActiveCamera`). **Disproved**:
  `byroredux/src/scene.rs:1054`/`1065`/`1067` put all three on the same camera
  entity in one block.
- **`water_audio_system` holds a component-query lock across `play_oneshot`.**
  **Disproved**: `config`, the `SplashEvent` guard and the `RippleEvent` guard are
  each explicitly dropped (`systems/audio.rs:234`, `:256`, and the `and_then`
  closure's scope) before `try_resource_mut::<AudioWorld>()` is acquired, and the
  system is registered `add_exclusive_with_access` so nothing runs concurrently.
  Matches `footstep_system`'s established lock-drop pattern.
- **`set_underwater` lost on early-return.** `water_audio_system` has five
  early-return paths (missing `WaterAudioConfig`, `None` splash sound, missing
  `WaterAudioState`, missing `SplashEvent` storage, no events). **Disproved**: the
  `set_underwater` call is the *first* thing the body does
  (`systems/audio.rs:212-225`), before any of them, so the filter state stays
  coherent on a device-less or archive-less boot. This is correct by design and
  the comment above it says so.
- **`Tween::default()` on `set_cutoff` producing a click.** kira's `Tween`
  default is a 10 ms linear ramp; a 20 kHz → 900 Hz sweep over 10 ms is fast but
  continuous. No discontinuity.
- **`std::collections::HashMap` in `SoundCache` as a #2923 violation.** Still
  disproved, unchanged: the `FxHashMap` rule is scoped to the per-frame
  render/skinning path.

---

## Future-Phase Readiness (invariants pinned for the next phase)

- **FOOT / 3.5b (per-material footstep sound)**: `FootstepConfig.default_sound`
  decoupling, `FootstepScratch` Vec reuse and the `{min, max}` attenuation shape
  all survive. **Blocked in effect by AUD-2026-08-20-D2-01** the moment footsteps
  stop being co-located with the listener — NPC footsteps are the first case, and
  they will be silent until the unit conversion lands. Fix D2-01 before FOOT.
- **REGN (ambient soundscapes)**: still blocked by **#3086** (emitter position
  frozen at dispatch) *and* now by **AUD-2026-08-20-D2-01** — region ambients are
  both long-lived and genuinely distant, so they hit both. Sub-track capacity
  (512) still covers the ~400-emitter projection.
- **MUSC routing**: single-slot / main-track / streaming-type invariants
  re-pinned; parse→play wiring re-confirmed absent. Two additional constraints
  for the eventual caller, both new this cycle: gate on FormID equality
  (re-playing the same handle re-decodes and re-streams), and decide explicitly
  whether music should be low-passed underwater — it currently is **not**, since
  `update_underwater_filters` walks `active_sounds` only and the music handle
  lives in its own field on the main track. Non-diegetic music arguably should
  stay dry, but that should be a recorded decision rather than an accident of
  where the filter was attached.
- **Occlusion attenuation**: `apply_reverb_send` and (once extracted per
  AUD-2026-08-20-D1-01) `apply_underwater_filter` give a per-track effect-chain
  seam that a raycast occlusion low-pass can join without touching either
  dispatch path. Attach any new per-track effect handle to `ActiveSound` the way
  `underwater_filter` is, so `update_*` passes can address it live.
- **Submerged-listener acoustics beyond the low-pass**: there is currently no
  underwater reverb send, no muffled-transition crossfade and no notion of a
  listener that is submerged *while the emitter is not* (the boundary case that
  matters for hearing a splash from below the surface). The `underwater` bool is
  a single global flag on `AudioWorld`, not per-emitter, so the cross-surface
  case cannot be expressed today. Honest gap, not a defect — recorded so the
  next water/audio pass does not have to rediscover it.
- **`SoundCache` producer**: still zero consumers and now two loaders routing
  around it (AUD-2026-08-20-D7-02). The decoupled API plus its three guards
  survive; a first consumer must wire eviction at the same time (#850).

---

## Delta vs prior report

This report supersedes `AUDIT_AUDIO_2026-08-16.md`. That cycle closed with one
MEDIUM and two LOW, all three of which were filed as #3086 / #3087 / #3088 and
are all still OPEN and still present at HEAD — carried, not re-reported.

This cycle:

- Audited the first genuinely new audio surface in eleven cycles: the
  submersion low-pass (`75ad0653`) and `water_audio_system` (`948f104a`), the
  second-ever live `play_oneshot` consumer.
- Surfaced one HIGH that the entire ten-report chain structurally could not have
  found: spatial attenuation is authored in metres against a world measured in
  Bethesda units. It was invisible while the only emitter sat *on* the listener;
  the water splash is the first offset emitter and is inaudible because of it.
  The audit skill's own Dimension 7 text carries the same metre assumption and
  should be corrected alongside the code.
- Surfaced one MEDIUM in the new filter path (the dry state is a wet 20 kHz SVF,
  not a bypass — device-rate-dependent, and the construction block is duplicated
  across both dispatch paths in the exact shape #2405 was filed for).
- Surfaced four LOW: a stage-ordering claim in `boot.rs` that the live
  registration contradicts (with a one-frame underwater-filter lag behind it),
  documentation drift across all three M44 status sources, and a
  position/intensity mismatch plus a global cooldown in the new ripple path,
  alongside a duplicated boot-time BSA loader.
- Re-verified every #842–#2405 regression guard structurally; none have drifted.
  Headless boot remains PASS.

---

## Severity Counts

- **CRITICAL**: 0
- **HIGH**: 1 (NEW: AUD-2026-08-20-D2-01)
- **MEDIUM**: 1 (NEW: AUD-2026-08-20-D1-01) · carried Existing: #3086
- **LOW**: 4 (NEW: AUD-2026-08-20-D6-01, AUD-2026-08-20-D6-02,
  AUD-2026-08-20-D7-01, AUD-2026-08-20-D7-02) · carried Existing: #3087, #3088

TALLY: CRITICAL=0 HIGH=1 MEDIUM=1 LOW=4
