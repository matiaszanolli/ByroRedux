# Audio Subsystem Audit (M44) — 2026-08-27

- **Command**: `/audit-audio` → all 7 dimensions, `--depth deep`
  (run as part of an `/audit-suite --preset comprehensive` sweep;
  single-agent by explicit task constraint — no sub-agent fan-out)
- **Branch**: main · **HEAD**: `969d81c8`
- **kira**: pinned `0.10` (workspace `Cargo.toml`, unchanged) → resolved
  `kira-0.10.8`
- **Method**: every dimension re-derived from live source. Files read in
  full or in paginated ranges: `crates/audio/src/lib.rs` (1510 lines),
  `crates/audio/src/tests.rs` (1420), `byroredux/src/systems/audio.rs`
  (768), `byroredux/src/asset_provider/audio.rs` (351),
  `byroredux/src/asset_provider/texture.rs` (footstep/splash loaders),
  `byroredux/src/components.rs` (audio + `RegionAmbientRes` +
  `FootstepEmitter`/`FootstepConfig`/`FootstepScratch`/`WaterAudioConfig`/
  `WaterAudioState` sections), `byroredux/src/boot.rs` (resource wiring +
  the `Stage::PostUpdate`/`Stage::Late` registrations),
  `byroredux/src/scene.rs` (camera opt-in), `byroredux/src/scene/world_setup.rs`
  (`apply_cell_region_ambient`), `byroredux/src/cell_loader/load.rs`
  (interior REGN dispatch), `byroredux/src/systems/camera.rs`,
  `byroredux/src/ownership_sample.rs` (audio telemetry),
  `crates/core/src/ecs/scheduler.rs` (parallel-then-exclusive phase order),
  `crates/core/src/ecs/world.rs` (`despawn`, EntityId non-reuse),
  `crates/plugin/src/esm/records/misc/world.rs`
  (`select_active_region_sound`), and the vendored
  `kira-0.10.8` sources for `Drop` impls, `listener_ear_positions`, and
  `SpatialTrackBuilder`'s send/effect split.
- **Tests actually run** (not trusted from prose):
  `cargo test -q -p byroredux-audio` → **29 passed, 0 failed, 6 ignored**;
  `cargo test -q -p byroredux --bin byroredux systems::audio` → **12 passed**;
  `cargo test -q -p byroredux --bin byroredux asset_provider::audio` →
  **13 passed**. **Headless-mode boot: PASS.**
- **Dedup baseline**: `gh issue list --repo matiaszanolli/ByroRedux
  --limit 400 --state open` → `/tmp/audit/audio/issues.json` (139 open),
  plus a targeted `--state all --search "audio in:title"` pull, plus the
  full prior `docs/audits/AUDIT_AUDIO_*.md` chain (13 reports,
  2026-05-05 → 2026-08-24).

---

## Delta Analysis (since `AUDIT_AUDIO_2026-08-24.md`, HEAD `048a8bd8`)

142 commits landed on `main` in the window. Restricting to the files this
audit owns, **exactly one commit touched `crates/audio/`** and **zero
touched `byroredux/src/systems/audio.rs` or
`byroredux/src/asset_provider/audio.rs`**:

| Commit | Change | Audio-relevant? |
|---|---|---|
| `a924244e` "repair four stale doc/comment passages from 2026-08-24 audit sweep" | Its `#3274` component split `crates/audio/src/lib.rs`'s "Future work" REGN bullet into shipped-music vs pending-`incidental`/`sounds`, flipped `docs/feature-matrix.md`'s single REGN `✗` row into two rows, and refreshed `ROADMAP.md`'s M44 pending-clause + test counts (11→12, 46→60) | **Yes — closes last cycle's only finding** |
| `159307e8` "Fix #3087: refresh audio scheduler docs" | `boot.rs`-only (23 lines). Removed the "The Phase 1 body is a stub" comment | **Yes — but only half of #3087; see AUD-2026-08-27-D7-02** |

Everything else in the window (SDK crate, material glass/soft-lighting
work, physics/scheduler access declarations, NAVM, Skyrim perks, LZ4/CDB
fixes) is outside audio's blast radius. **The audio crate's executable
surface is byte-identical to the 2026-08-24 audit.**

### Verification of last cycle's finding

- **AUD-2026-08-24-D6-01 → #3274 (was LOW)** — verified fixed by direct
  inspection, not trusted from the commit message:
  `crates/audio/src/lib.rs:133-137` now reads
  `- REGN `incidental`/`sounds` ambient-loop selection (blocked on the
  `chance_raw` fixed-point scale). REGN ambient background **music** has
  shipped — see `byroredux/src/asset_provider/audio.rs::dispatch_region_ambient_music`.`;
  `docs/feature-matrix.md:148-149` now carries two rows
  (`| Region ambient (REGN) — background music | ✓ |` and
  `| Region ambient (REGN) — incidental/loop sounds | ✗ |`);
  `ROADMAP.md:706` carries the **REGN ambient background music shipped**
  clause and the corrected `12`/`13`/`60` test counts — all three
  independently re-measured this cycle and **matching** (12 and 13 test
  runs above). **Confirmed fixed.** Two residual sites the fix did not
  reach are filed below as AUD-2026-08-27-D6-01.

---

## Executive Summary

**7 dimensions run. 4 NEW findings (0 CRITICAL / 1 HIGH / 0 MEDIUM /
3 LOW).**

| # | Dimension | NEW findings |
|---|---|---|
| 1 | Spatial Sub-Track Lifecycle & Leaks | **1** (LOW) |
| 2 | Listener Pose & Attenuation | 0 |
| 3 | SoundCache Growth & Eviction | 0 |
| 4 | Streaming Music Lifecycle | 0 |
| 5 | Reverb Send & Routing | 0 |
| 6 | Manager Lifecycle, ECS & Cell Streaming | **1** (LOW) |
| 7 | Gameplay Audio Wiring | **2** (1 HIGH, 1 LOW) |

**The headline finding is a unit-seam bug on the *producer* side that
#3178's fix did not cover.** #3178 (closed 2026-08-20) established
`bu_to_audio_space` as the single BU→metre seam for *positions crossing
into kira*. It did not audit the constants that gameplay producers
compare against BU deltas *before* the seam.
`FootstepEmitter::stride_threshold` is one such constant, authored as
`1.5` and documented as "~1.5m at FNV scale"
(`byroredux/src/components.rs:1465`) — but the value it is compared
against is a Bethesda-unit delta, so the real threshold is **1.5 / 70 =
2.1 cm**. The engine's own fly-cam moves 3.33 BU/frame at the default
`move_speed: 200.0` and 60 FPS (a figure `boot.rs:1108-1110` states
explicitly). Every walking frame crosses the threshold, so the only live
`play_oneshot` producer fires **one footstep per frame** instead of the
intended ~2/s. Details in AUD-2026-08-27-D7-01.

- **Headless-mode boot**: **PASS** (54 default tests green across the
  three audio files, 6 device/data-gated `#[ignore]`d, 0 failing).
- **Shipped surface, re-confirmed at HEAD**: `AudioWorld` graceful
  degradation (`SUB_TRACK_CAPACITY = 512` / `SEND_TRACK_CAPACITY = 32`);
  `AudioListener` / `AudioEmitter` / `OneShotSound`; `audio_system` =
  `sync_listener_pose` → `update_underwater_filters` →
  `drain_pending_oneshots` → `dispatch_new_oneshots` →
  `prune_stopped_sounds`; both dispatch paths (queue `VecDeque` cap 256
  `pop_front`; entity path with `loop_region(..)`); tweened-`stop()`
  despawn truncation with `stop_issued` debounce; single-slot
  main-track streaming music with a live caller; global reverb send
  (`NEG_INFINITY` dry default); the `bu_to_audio_space` unit seam; the
  `Mix::DRY` underwater bypass.
- **Live engine consumers remain three**: `footstep_system`
  (`play_oneshot`), `water_audio_system` (`play_oneshot` +
  `set_underwater`), `dispatch_region_ambient_music` (`play_music` /
  `stop_music`). `reverb_zone_system` remains the only
  `set_reverb_send_db` caller. The **entity dispatch path
  (`spawn_oneshot_at` / `AudioEmitter` / `OneShotSound`) still has zero
  engine callers** — confirmed by grep across `byroredux/src`; its only
  non-test mentions are two `save_io/registry_completeness_tests.rs`
  exclusion rationales.
- **Prior-cycle carried findings — still OPEN, still present at HEAD**
  (noted and skipped per the dedup protocol):

| Issue | Finding | State at HEAD `969d81c8` |
|---|---|---|
| **#3086** (LOW) | Entity-path spatial sub-track position frozen at dispatch; `AudioEmitter`'s docstring promises a per-frame update the code never performs | **Unchanged.** `grep -n set_position crates/audio/src/lib.rs` returns only the listener sites (`949`, doc `817`). No emitter reposition exists. Still latent-only — the entity path has no engine producer. |
| **#3189** (LOW) | `try_load_default_water_splash` duplicates the `--sounds-bsa` scan and re-opens the same archive; both loaders bypass `SoundCache` | **Unchanged, still three independent `Archive::open()` calls** against the same path at boot: `asset_provider/texture.rs:110` (footstep), `:151` (splash), `asset_provider/audio.rs:110` (`build_sound_archive_provider`). |
| **#3301** / **#2372** | REGN `incidental` spatial emitter + non-`Sound` `RDAT` kind selectors; EX-16 umbrella | Correctly still open; out of scope for this audit (future-phase). |

---

## Lifecycle Invariant Matrix

Owned by Dimension 6 per the skill's dedup instruction (Dims 1/4/5 point
here).

| Invariant | State | Anchor |
|---|---|---|
| `AudioWorld` field-drop order (`active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager` → `multi_listener_warned` → `underwater`) | **HOLDS** | `lib.rs:374-417` |
| Drop-order rationale re-verified against kira, not assumed: only `TrackHandle`, `SpatialTrackHandle`, `SendTrackHandle`, `ListenerHandle`, `ClockHandle`, `LfoHandle`, `TweenerHandle` carry `impl Drop`. `StaticSoundHandle` / `StreamingSoundHandle` / `FilterHandle` do **not** — so `stop_music`'s "kira keeps the sound alive until the fade completes" comment is accurate, and `ActiveSound.underwater_filter` dropping *after* `_track` is harmless | HOLDS | `kira-0.10.8/src/**/handle.rs` |
| `ActiveSound` field order (`entity` → `handle` → `_track` → `underwater_filter` → `underwater` → `unload_fade_ms` → `stop_issued`) | HOLDS | `lib.rs:323-347` |
| `ActiveSound._track` underscore name intact, Drop-side-effect only | HOLDS | `lib.rs:326` |
| Manager capacities exceed kira defaults (512 / 32) and `new()` applies them | HOLDS | consts `lib.rs:306-307`, applied `lib.rs:433-434` |
| Lazy listener creation; no frame-1 cold-start panic; `add_listener` failure is transient-retry not permanent lockout | HOLDS | `lib.rs:888-952` |
| Sticky listener, never cleared on entity churn (#849) | HOLDS | written only at `lib.rs:942`; no clear site exists |
| Multi-listener diagnostic debounced (#843) | HOLDS | `multi_listener_warned` set once, never reset (`lib.rs:914`) |
| Listener orientation is in kira's convention (ears at ±X of the listener quat, `Vec3::NEG_X`/`Vec3::X`, forward −Z) and the engine's camera quat is built the same way (`Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)`, forward `* -Vec3::Z`) — **no channel inversion** | HOLDS (newly verified against kira source this cycle) | `kira-0.10.8/src/track/sub.rs:347-366`; `byroredux/src/systems/camera.rs:79-86` |
| Spatial positions cross the BU→metre seam at every kira site (#3178) | HOLDS | `bu_to_audio_space` `lib.rs:200-202`; calls at `934`/`949`/`1002`/`1136`; pinned by `every_kira_position_site_goes_through_the_unit_seam` |
| **Gameplay *producer* constants that compare against a BU delta are in BU** | **DRIFTED — AUD-2026-08-27-D7-01** | `components.rs:1477`/`1493` vs `systems/audio.rs:169` |
| Attenuation `RangeInclusive`, `min<=max` normalized (#1612); NaN-safe (`f32::clamp` never sees `min > max`) | HOLDS | `Attenuation::distance_range()` `lib.rs:726-730` |
| Both dispatch paths gate on `listener_id` before any work | HOLDS | `lib.rs:960` / `1048` |
| Both dispatch paths apply the reverb gate through one shared helper (#2405) | HOLDS | `lib.rs:992` / `1126` → `apply_reverb_send` `267-278` |
| Both dispatch paths build the underwater filter through one call site (#3179) | HOLDS | `apply_underwater_filter` `lib.rs:217-227`, called `999`/`1133` |
| Above-water filter state is a genuine `Mix::DRY` bypass | HOLDS | `underwater_mix` `lib.rs:241-247` |
| `looping` / `loop_region(..)` in the entity path only | HOLDS | `lib.rs:1158-1163`; `PendingOneShot` has no `looping` field |
| Volume→dB centralized (`linear_volume_to_db`) | HOLDS | `lib.rs:253`, called from `598`, `1011`, `1154` |
| Drain cap (>32/tick warns) · producer cap (256 `pop_front`) · drain-gate-before-`mem::take` (#851/#852/#853) | HOLDS | `lib.rs:548-558`, `959`→`979` |
| **`pending_oneshots` heap capacity survives a drain** | **DRIFTED — AUD-2026-08-27-D1-01** | `lib.rs:979` |
| `OneShotSound` marker consumed on success **and** both failure arms (#2394) | HOLDS | `lib.rs:1146`, `1171`, `1184` |
| Despawn truncation, `stop_issued` debounce (#844/#845/#858) | HOLDS | `lib.rs:1225`, `1256` |
| `EntityId` is never reused (`World::despawn` grows `next_entity` monotonically), so `prune_stopped_sounds`' `AudioEmitter`-presence test can never be fooled by a recycled id | HOLDS | `crates/core/src/ecs/world.rs:139-149` |
| `SoundCache` lowercase-once, dormant in engine (#859/#850) | HOLDS — still zero producers; `len()` is wired to telemetry (`ownership_sample.rs:70-73`), `bytes_estimate` is not (correctly documented as unwired at `lib.rs:1494-1498`) | — |
| Single-slot music · main track · streaming types · fade-then-drop | HOLDS, exercised by a real caller | `lib.rs:579-627`; `asset_provider/audio.rs:158-205` |
| Reverb `None`-safe · `NEG_INFINITY` default · `> SILENCE_DB` gate | HOLDS | `lib.rs:267-286` |
| `reverb_zone_system` runs before `audio_system` in `Stage::Late` — mechanism re-derived: it is registered **parallel** (`add_to_with_access`, `boot.rs:1411`) while `audio_system` is **exclusive** (`add_exclusive`, `boot.rs:1481`), and `Scheduler::run` executes a stage's whole parallel batch before its exclusive list (`scheduler.rs:475-520`) | HOLDS (but the in-file comment states the wrong mechanism — AUD-2026-08-27-D7-02) | `boot.rs:1411-1417`, `1481`; `scheduler.rs:9`, `511` |
| Late-stage exclusive order: ragdoll → submersion → water_damage → reconcile_dead → water_interaction → water_audio → audio_system → event_cleanup | HOLDS, live-scheduler-tested (#3180) | `boot.rs:1386-1481`; `scheduler_access_tests.rs` |
| `footstep_system` (`PostUpdate` exclusive, after transform propagation) enqueues; `audio_system` (`Late`) drains — same frame | HOLDS | `boot.rs:1091-1115`, `1481` |
| `SplashEvent`/`RippleEvent` are drained by `event_cleanup_system` after `audio_system`, so a splash cannot replay every frame | HOLDS | `crates/scripting/src/cleanup.rs:88-89` |
| `AudioWorld::new()` called exactly once, at boot | HOLDS | single call site `boot.rs:518` |
| REGN ambient dispatch change-guarded against `RegionAmbientRes`'s *prior* `music_form` at all four call sites; all three interior callers write `result.region_ambient` after the load returns | HOLDS | `cell_loader/load.rs:536-556`; `scene/world_setup.rs:523-541`; writes at `debug_load.rs:314`, `scene.rs:848`, `cell_loader/transition.rs:474` |
| REGN `Sound`-entry priority resolution is a stable sort | HOLDS | `select_active_region_sound` `misc/world.rs:775-787` (`sort_by_key` on `Reverse(priority)`, stable) |
| Shipped audio surface described consistently across crate docstring / `feature-matrix.md` / `ROADMAP.md` M44 row | **HOLDS for all three primary sources (#3274 fixed)** — two secondary sites still drifted (AUD-2026-08-27-D6-01) | `lib.rs:133-137`; `feature-matrix.md:148-149`; `ROADMAP.md:706` |

---

## Findings

### AUD-2026-08-27-D7-01: `FootstepEmitter.stride_threshold` is authored in metres but compared against a Bethesda-unit delta — footsteps fire once per frame, ~33× the intended cadence

- **Severity**: HIGH
- **Dimension**: Gameplay Audio Wiring
- **Location**: `byroredux/src/components.rs:1462-1496` (the constant and
  its docstring) consumed at `byroredux/src/systems/audio.rs:161-171`
  (the comparison); opt-in site `byroredux/src/scene.rs:1215`
- **Status**: NEW
- **Description**: `footstep_system` accumulates the XZ-plane delta of
  the emitter entity's `GlobalTransform.translation` — which is in
  **Bethesda units** (this is the entire premise of #3178's
  `bu_to_audio_space` seam, which divides the very same
  `GlobalTransform.translation` by `BETHESDA_UNITS_PER_METER = 70` on its
  way into kira) — and compares that BU accumulation against
  `stride_threshold`, whose default is `1.5` and whose docstring calls it
  "**~1.5m at FNV scale**". At 70 BU/m the effective threshold is
  **1.5 / 70 = 2.14 cm of world travel**, not 1.5 m.

  #3178 fixed the seam for *positions crossing into kira*. It did not
  examine gameplay constants that are compared against BU deltas
  **before** the seam, and `stride_threshold` is the only such constant
  in the audio subsystem. The two live `play_oneshot` producers'
  `Attenuation` constants (`{0.5, 12.0}` footstep, `{1.0, 24.0}` splash)
  are correctly metres, because they are handed to kira and never
  compared against a world delta — which is exactly why the seam audit
  found nothing here.
- **Evidence**: The comparison, verbatim (`systems/audio.rs:161-171`):
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
  The constant (`components.rs:1490-1495`):
  ```rust
  Self {
      last_position: Vec3::ZERO,
      accumulated_stride: 0.0,
      stride_threshold: 1.5,
      initialised: false,
  }
  ```
  and its docstring (`components.rs:1463-1465`):
  ```
  /// by `footstep_system`; `stride_threshold` is read-only configuration
  /// — a stride distance that triggers one footstep. Defaults to 1.5
  /// game-units (~1.5m at FNV scale; reasonable walking cadence).
  ```
  The per-frame travel that threshold is measured against is stated by
  the engine's own code, twice, independently of this audit:
  - `byroredux/src/components.rs:1449` — `move_speed: 200.0, // Bethesda
    units per second` (fly-cam `InputState`), consumed as
    `let speed = input.move_speed * dt;`
    (`byroredux/src/systems/camera.rs:40`).
  - `crates/physics/src/components.rs:182` — `move_speed: 220.0` on the
    `CharacterController` used in player mode.
  - `byroredux/src/boot.rs:1108-1110`, the comment on
    `footstep_system`'s own registration: *"the commit comment claimed
    '~3 cm of motion' stale but that underestimated by ~100× for fly-cam
    boost (**~3 game units / frame at 60 FPS**, audible spatial-pan
    offset on a ~50-200-unit interior cell)"*.

  200 BU/s ÷ 60 FPS = **3.33 BU/frame**, already 2.2× the 1.5 BU
  threshold before the ×3 sprint boost
  (`systems/camera.rs:71-75`) or the character controller's 220 BU/s.
  Since the fire branch is an `if` (not a `while`) it is capped at one
  trigger per emitter per tick, so the observable behaviour is
  **exactly one footstep every frame while moving** — 60 Hz at 60 FPS
  against a realistic human cadence of ~1.8 Hz. A correct BU threshold
  for a ~0.75 m human stride is ≈ 52 BU.
- **Impact**: The only live gameplay-audio producer is audibly wrong
  under normal play on the reference title (`--sounds-bsa "Fallout -
  Sound.bsa"` + any FNV cell): a continuous machine-gun footstep buzz
  instead of a walk cadence. Secondary cost: ~33× the intended spatial
  sub-track churn — one `SpatialTrackBuilder` + `add_spatial_sub_track` +
  `StaticSoundData` clone + `ActiveSound` push per frame, with ~24
  concurrent `active_sounds` entries at a 0.4 s WAV instead of ~1.
  Still far below `SUB_TRACK_CAPACITY = 512`, so it degrades rather than
  drops.

  **This is invisible to every guard the subsystem has.** The
  `drain_pending_oneshots` >32-per-tick WARN (`lib.rs:548-556`) exists
  precisely to catch "footstep-tempo gone wrong", but only one item is
  enqueued per emitter per tick, so it never trips with a single
  emitter. `active_sound_count` *is* wired to telemetry
  (`ownership_sample.rs:63`) and would show the elevated count, but
  nothing alerts on it. And the two stride regression tests
  (`stride_threshold_fires_exactly_one_footstep`,
  `single_large_jump_fires_one_footstep_only`,
  `systems/audio.rs:417-497`) are unit-agnostic — they move the emitter
  "1.5 game-units" and "6.0 horizontal units" and assert trigger counts,
  which pass identically whether the unit is a metre or a BU. There is
  no `stride_threshold_is_bethesda_units_not_metres` sibling to
  `default_attenuation_band_is_metres_not_bethesda_units`
  (`crates/audio/src/tests.rs`), which is the guard shape #3178 already
  established for the other half of this seam.
- **Related**: #3178 (the BU→metre seam this is the producer-side
  counterpart of — same root class, uncovered by that fix). #848
  (first-tick seed, the last time this accumulator was audited).
  `boot.rs:1108-1110`'s own "~3 game units / frame" note is the
  strongest single piece of evidence and predates this audit.
- **Suggested Fix**: Make the unit explicit at the constant. Either set
  `stride_threshold` to a BU value (≈ 52.0 for a ~0.75 m stride, or
  `0.75 * BETHESDA_UNITS_PER_METER`) and rewrite the docstring to say
  Bethesda units, or keep the metre authoring and convert at the compare
  site. The first is preferable: it keeps `footstep_system` operating
  entirely in world space with no second unit seam, matching the
  `bu_to_audio_space`-is-the-only-seam rule. Add a
  `stride_threshold_is_bethesda_units_not_metres` guard mirroring
  `default_attenuation_band_is_metres_not_bethesda_units`, and a
  cadence test that walks an emitter at 200 BU/s for one simulated
  second and asserts a plausible step count (~2–4, not 60).

---

### AUD-2026-08-27-D1-01: `drain_pending_oneshots`' `std::mem::take` strands the `pending_oneshots` heap capacity on every drain tick

- **Severity**: LOW
- **Dimension**: Spatial Sub-Track Lifecycle & Leaks
- **Location**: `crates/audio/src/lib.rs:979`
- **Status**: NEW
- **Description**: The drain replaces the live `VecDeque` with a fresh
  default and consumes the old one by value:
  ```rust
  let pending = std::mem::take(&mut audio_world.pending_oneshots);
  ```
  `VecDeque::default()` allocates nothing, and `pending` is moved through
  `for p in pending` and dropped at end of scope — so the queue's heap
  buffer is **freed every tick that had any queued one-shot**, and the
  next `play_oneshot` re-allocates from zero. The `VecDeque` was
  deliberately chosen over `Vec` in #852 to make the cap-eviction path
  O(1); this undoes the adjacent half of that intent by making the
  steady-state cost an allocate+free pair per drain.

  This is the exact class the project has already filed and fixed three
  times elsewhere: `FootstepScratch` (#932 — "pre-#932 a fresh
  `Vec<Vec3>` was allocated every frame"), `InteractionCandidateScratch`
  (#3059), and the `submersion_system` disturbance scratch (#3257,
  landed this window in `bbfd742f`). `footstep_system` itself carries
  the canonical remedy in-line — it `std::mem::take`s the scratch buffer
  and then **restores it on both the success path and the
  `AudioWorld`-absent bail path** (`systems/audio.rs:174-176`,
  `184-190`, `205-208`) precisely so the capacity is not stranded.
  `drain_pending_oneshots` has no such restore.
- **Evidence**: `lib.rs:977-993` — after the loop over `pending` ends
  there is no write back into `audio_world.pending_oneshots`; the next
  producer call reaches `self.pending_oneshots.push_back(..)`
  (`lib.rs:557-563`) on a zero-capacity deque. The `mem::take` is not
  gratuitous — `audio_world.manager.as_mut()` is held mutably across the
  loop, so a `drain(..)` over a sibling field would not borrow-check —
  but a reusable second `VecDeque` field swapped back at the end would.
- **Impact**: One `alloc`/`free` pair per tick that dispatches any
  queued one-shot, on the `Stage::Late` per-frame path. Not a leak, not
  a correctness bug, and negligible next to the kira dispatch it wraps.
  It compounds with AUD-2026-08-27-D7-01 (which makes *every* frame a
  drain frame rather than ~2/s), so fixing that one raises this from
  "every frame" to "every stride" and lowers its priority accordingly —
  fix D7-01 first.
- **Related**: #852 (the `VecDeque` choice), #932 / #3059 / #3257 (the
  same scratch-capacity class, all filed and fixed).
- **Suggested Fix**: Add a `drain_scratch: VecDeque<PendingOneShot>`
  field to `AudioWorld` (declared adjacent to `pending_oneshots`, before
  `music`, so the drop order is unchanged), `std::mem::swap` it with
  `pending_oneshots` at the top of the drain, and swap the (now empty,
  capacity-retaining) buffer back at the end. Same shape as #3257's fix.

---

### AUD-2026-08-27-D7-02: `reverb_zone_system`'s registration comment survived #3087's fix — still attributes registration to *main.rs* and states the wrong ordering mechanism

- **Severity**: LOW
- **Dimension**: Gameplay Audio Wiring (documentation)
- **Location**: `byroredux/src/systems/audio.rs:55-57`
- **Status**: NEW (residual half of CLOSED #3087)
- **Description**: #3087 ("stale audio scheduler-wiring comments —
  `audio_system` described as a 'Phase 1 stub', `reverb_zone_system`
  registration attributed to main.rs") was closed by `159307e8` on
  2026-08-26. That commit touched **`byroredux/src/boot.rs` only** (23
  lines, `git show --stat 159307e8`). The first half of the finding is
  genuinely fixed — `grep -n "Phase 1 body is a stub" byroredux/src/boot.rs`
  returns nothing. The second half is untouched:
  ```rust
  /// Runs in `Stage::Late` alongside `audio_system` (registered first
  /// in main.rs so the level is in place before any new spatial track
  /// gets constructed this frame).
  ```
  Two errors in one sentence. (a) The registration is in
  `byroredux/src/boot.rs:1411-1417`, not `main.rs` — `main.rs` has been
  a thin App-construction module since #1858/#2731 and contains no
  scheduler registration at all. `boot.rs`'s own companion comment
  (`boot.rs:1406-1408`) even asserts *"This `build_scheduler` block is
  the registration authority"*, which the `systems/audio.rs` comment
  directly contradicts. (b) The stated mechanism — "registered first" —
  is not what guarantees the ordering. `reverb_zone_system` is
  registered **parallel** (`add_to_with_access`) and `audio_system`
  **exclusive** (`add_exclusive`, `boot.rs:1481`); the guarantee comes
  from `Scheduler::run` executing a stage's entire parallel batch before
  its exclusive list (`crates/core/src/ecs/scheduler.rs:9`, `475-520`),
  not from registration order. Under the comment's stated mechanism, a
  maintainer converting `reverb_zone_system` to an exclusive registered
  *after* `audio_system` (a plausible "make the ordering structural"
  refactor, exactly what #2731-era work did to `audio_system` itself)
  would silently invert the dependency and give every spatial track
  built this frame *last* frame's send level.
- **Evidence**: `git show --stat 159307e8` → `byroredux/src/boot.rs | 23
  +++---`, one file. `grep -n "main.rs" byroredux/src/systems/audio.rs`
  → line 56, the only hit and still present at HEAD `969d81c8`.
- **Impact**: Documentation only, no runtime behaviour — but it is the
  live in-file comment a maintainer reads while editing
  `reverb_zone_system`, and it misdirects both to a file that no longer
  registers anything and to an ordering mechanism that does not hold.
  Also a process signal: a closed issue whose two-clause title named two
  sites, fixed at one.
- **Related**: #3087 (closed 2026-08-26, this is its unfixed half);
  #1858 / #2731 (the `main.rs` → `boot.rs` / `app_*.rs` splits this
  comment predates).
- **Suggested Fix**: Rewrite the parenthetical as "(registered in
  `boot.rs::build_scheduler` as a `Stage::Late` **parallel** system,
  while `audio_system` is a `Stage::Late` **exclusive**; the scheduler
  runs a stage's parallel batch before its exclusive list, so the send
  level is in place before any spatial track is constructed this
  frame)". Consider reopening #3087 rather than filing fresh.

---

### AUD-2026-08-27-D6-01: two secondary status sites still describe REGN ambient music as unbuilt after #3274 fixed the three primary ones

- **Severity**: LOW
- **Dimension**: Manager Lifecycle & ECS/Cell Streaming (documentation)
- **Location**: `byroredux/src/components.rs:498-503`
  (`RegionAmbientRes` docstring); `ROADMAP.md:1107` (the struck-through
  Known-Issues M44 bullet)
- **Status**: NEW (residual of CLOSED #3274)
- **Description**: `#3274` (closed by `a924244e`) correctly fixed the
  three sources the audit skill designates authoritative — the crate
  docstring, `docs/feature-matrix.md`, and `ROADMAP.md`'s M44 active-
  milestone row. Two secondary sites still assert the opposite:

  1. **`byroredux/src/components.rs:498-503`**, the docstring on
     `RegionAmbientRes` — the resource the feature is built on:
     ```
     /// carries FormIDs, not decoded audio; `asset_provider::audio`'s
     /// `resolve_sound_path`/`sound_archive_path` resolve them to archive
     /// paths, and a consumer (item 5's REGN-keyed `AudioEmitter`, not yet
     /// built) dispatches actual playback.
     ```
     The consumer *is* built — `dispatch_region_ambient_music`
     (`byroredux/src/asset_provider/audio.rs:158-205`), wired at four
     call sites, with 13 passing tests. It is not the "REGN-keyed
     `AudioEmitter`" that sentence anticipates (a spatial emitter is
     still correctly future-phase, tracked as #3301), but the
     parenthetical reads as "nothing dispatches playback", which is now
     false.
  2. **`ROADMAP.md:1107`**, the closed Known-Issues bullet, still lists
     `Pending: FOOT records → per-material lookup (drops dirt hardcode),
     **REGN region-keyed ambient layers**, raycast-occlusion
     attenuation` — the exact clause `#3274` corrected one screen up in
     the M44 row.
- **Evidence**: `grep -rn "not yet" byroredux/src/components.rs` → line
  501. `git show a924244e -- crates/audio/src/lib.rs` shows the fix
  touched the crate docstring's Future-work bullet only; the commit's
  own message enumerates `lib.rs` / `feature-matrix.md` / `ROADMAP.md`'s
  M44 row and nothing else.
- **Impact**: Documentation only. Lower stakes than #3274 itself — the
  three primary sources are now right, so a contributor scoping REGN
  work from `docs/feature-matrix.md` lands correctly. The
  `components.rs` site matters more than the ROADMAP one because it is
  the docstring on the resource a REGN contributor would open first.
- **Related**: #3274 (closed 2026-08-25, this is its residual);
  #3301 (the genuinely-still-pending REGN spatial emitter this docstring
  conflates with the shipped music path); #3181, #1859 (same doc-drift
  class).
- **Suggested Fix**: In `components.rs`, replace "and a consumer (item
  5's REGN-keyed `AudioEmitter`, not yet built) dispatches actual
  playback" with "and `asset_provider::audio::dispatch_region_ambient_music`
  dispatches `music_form` through `AudioWorld::play_music`;
  `incidental_form` still has no consumer (#3301)". In `ROADMAP.md:1107`,
  narrow "REGN region-keyed ambient layers" to "REGN
  `incidental`/`sounds` ambient layers (background *music* shipped
  2026-08-23)".

---

## Disproved candidates (investigated, not reported)

Recorded so the next cycle doesn't re-derive them.

- **Listener orientation handed to kira in the wrong frame (left/right
  channel inversion).** The skill flags this as "subtle and lethal", and
  it had never been checked against kira's actual panning math in any
  prior report. Traced it this cycle:
  `listener_ear_positions` / `listener_ear_directions`
  (`kira-0.10.8/src/track/sub.rs:347-366`) place the ears at
  `orientation * (Vec3::NEG_X * 0.1)` and `orientation * (Vec3::X *
  0.1)` and splay them ±π/8 about **Y** — i.e. kira assumes right = +X,
  up = +Y, and (by exclusion) forward = −Z, the glam/renderer
  convention. `fly_camera_system` builds the camera quat as
  `Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch)` and
  derives its own forward as `Quat::from_rotation_y(yaw) * -Vec3::Z`
  (`byroredux/src/systems/camera.rs:79-86`), the same convention.
  **Disproved** — the quat handed to `set_orientation` is already in
  kira's frame; no residual Z-up → Y-up mismatch survives to the audio
  boundary.
- **`EntityId` reuse fooling `prune_stopped_sounds`' emitter-presence
  test** (a despawned emitter's id reallocated to a new entity that
  *does* carry an `AudioEmitter`, leaving the old sound playing
  forever). **Disproved**: `World::despawn` never reclaims ids —
  `crates/core/src/ecs/world.rs:139-141`, *"Entity IDs are NOT reclaimed
  — `next_entity` keeps growing. Reuse without generational tagging
  would cause silent component-data corruption"* — and `EntityId` is a
  plain `u32` with no generation (`crates/core/src/ecs/storage.rs:10`).
- **Dropping the outgoing `StreamingSoundHandle` in `play_music` cutting
  the crossfade short.** `play_music` calls `existing.stop(fade)` and
  then overwrites `self.music`, dropping the old handle mid-fade
  (`lib.rs:594-604`). **Disproved**: a full `grep -rn "impl Drop"` over
  `kira-0.10.8/src/` returns eight impls, none of them on
  `StreamingSoundHandle`, `StaticSoundHandle`, or `FilterHandle` — the
  renderer owns the sound, the handle is a command channel. The crossfade
  completes. This also validates `stop_music`'s in-code claim ("kira
  keeps the sound alive internally until the fade completes"), which had
  been asserted in prior reports without checking.
- **`spawn_oneshot_at` leaking entities.** Re-checked rather than
  inherited: the helper spawns `Transform` + `GlobalTransform` +
  `AudioEmitter` + `OneShotSound` (`lib.rs:1296-1320`) and
  `prune_stopped_sounds` removes only the `AudioEmitter`, never
  despawning; the "downstream cleanup system" its docstring defers to
  still does not exist. **Disproved as a present defect** by fresh grep:
  `spawn_oneshot_at` / `AudioEmitter` / `OneShotSound` have zero
  non-test references in `byroredux/src` (the only hits are two
  `save_io/registry_completeness_tests.rs` exclusion rationales). The
  entity path has no engine producer. Unchanged from the 2026-08-16
  conclusion — re-verify the moment a producer lands.
- **`play_oneshot`'s queue filling unboundedly when the manager is
  active but no `AudioListener` exists** (e.g. an archive `--menu` route
  with a working audio device). Both dispatch helpers early-return on a
  missing `listener_id` *before* the drain (`lib.rs:960`, `1048`), so
  queued items would persist. **Not escalated**: the producer cap is 256
  with FIFO drop-oldest and a WARN (`lib.rs:546-558`), and the queue
  drains in full the frame a listener appears. Bounded and diagnosed by
  design.
- **`select_active_region_sound` picking a highest-priority `Sound`
  entry with `music: None` and thereby suppressing a lower-priority
  entry that does carry music** (`misc/world.rs:775-787` filters on
  `kind == Sound` and sorts by priority alone, never on
  `payload.music.is_some()`). Plausible on a multi-region-tagged FNV
  cell, and it would present as "ambient music silently absent in one
  specific place". **Not reported**: confirming it requires a corpus
  census of REGN `Sound` entries whose winning priority carries `RDSI`
  but no `RDSB`, which this audit did not run — and filing it on the
  shape of the code alone would be exactly the guessed-premise class the
  no-guessing policy forbids. Recorded here as a concrete, cheap
  follow-up for whoever next has an ESM census harness open.
- **`RegionAmbientRes::resolve`'s unconditional per-tick sort as a perf
  regression.** Re-confirmed as the 2026-08-24 report concluded (bounded
  candidate list, cheaper than the LOD work on the same tick, and the
  function's own doc makes the tradeoff explicitly). No new evidence.

---

## Future-Phase Readiness (invariants pinned for the next phase)

- **FOOT / 3.5b (per-material footstep sound)**: `FootstepConfig.
  default_sound` decoupling, `FootstepScratch` capacity reuse on both
  paths, the metre-authored `{0.5, 12.0}` attenuation shape, and the
  BU→metre position seam all survive. **The one thing 3.5b must not
  inherit is `stride_threshold`'s unit confusion** — a FOOT-driven
  per-material lookup that keeps the current cadence will multiply the
  wrong tempo across every surface type instead of one. Fix
  AUD-2026-08-27-D7-01 before 3.5b lands, not after.
- **REGN `incidental` / `sounds`**: the `music` half is shipped and
  correct. `incidental_form` is resolved into `RegionAmbientRes` and has
  no consumer (#3301); the chance-based `sounds` list stays deferred on
  the unresolved `chance_raw` fixed-point scale. The single-slot /
  main-track / streaming-type invariants that the eventual spatial
  emitter must *not* route through are pinned in the matrix above — an
  `incidental` emitter belongs on the **spatial** path
  (`play_oneshot` / `AudioEmitter`), never on the music slot.
- **MUSC / ZNAM / XCMO cell music**: still zero consumers. `default_music`
  (ZNAM) and `music_type_form` (XCMO) are parsed into `CellData` and read
  by nothing. When a caller lands it will contend with
  `dispatch_region_ambient_music` for the **single** music slot — that
  arbitration (cell music vs region music priority) is an unmade design
  decision, not a bug, and the single-slot invariant is pinned so it
  surfaces as a crossfade fight rather than stacked tracks.
- **`SoundCache` first consumer**: still dormant, `len() == 0` steady
  state, `len()` wired to `ownership_sample.rs` telemetry and
  `bytes_estimate` correctly documented as unwired. Whoever wires the
  first producer should wire eviction and `bytes_estimate` in the same
  commit — and should prefer migrating the three existing bypassing
  loaders (footstep, splash, REGN) onto it, which also closes #3189.
- **Per-cell acoustics beyond binary interior/exterior**: unchanged.
  `set_reverb_send_db` remains a next-dispatch knob (#847), correctly
  documented as such; a real per-cell acoustic model needs the
  re-dispatch handler that limitation names.

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_AUDIO_2026-08-27.md
```

Domain label: `audio`. AUD-2026-08-27-D7-01 additionally warrants
`game:fnv` (it is reproducible on the reference title's sound archive and
is the only title with a working `try_load_default_footstep` path).
AUD-2026-08-27-D7-02 and AUD-2026-08-27-D6-01 are `doc-rot`; consider
reopening #3087 and #3274 respectively rather than filing fresh issues,
since each is the unfixed remainder of that issue's own stated scope.
