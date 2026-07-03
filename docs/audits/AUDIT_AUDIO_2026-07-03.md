# Audio Subsystem Audit (M44) — 2026-07-03

Static, source-only audit of the M44 `byroredux-audio` crate
(`crates/audio/src/lib.rs`, 1293 lines; tests in
`crates/audio/src/tests.rs`, 1178 lines) plus the engine-side consumers
that drive its API (`byroredux/src/systems/audio.rs`,
`byroredux/src/components.rs`, `byroredux/src/asset_provider/texture.rs`,
schedule wiring in `byroredux/src/main.rs`, listener/footstep opt-in in
`byroredux/src/scene.rs`). kira pinned at `0.10.8` (workspace
`Cargo.toml` → `Cargo.lock`). Run against HEAD `8498e559`
(2026-07-03), as part of a `comprehensive` audit-suite sweep.

Scope: all 7 dimensions, depth = deep. `cargo test -p byroredux-audio`
and the `byroredux` footstep/reverb unit tests were run live (not just
read) to confirm the guards actually pass on this HEAD, in addition to
the static source read-through.

## Zero-Delta Finding

Before auditing dimension-by-dimension, `git log` was checked for every
file in scope against the previous report's date
(`docs/audits/AUDIT_AUDIO_2026-07-02.md`):

| File | Last touched | Relative to prior audit |
|---|---|---|
| `crates/audio/src/lib.rs` | 2026-06-26 (`eb71bcb9`) | before prior audit |
| `crates/audio/src/tests.rs` | 2026-06-15 (`4d349f6c`) | before prior audit |
| `byroredux/src/systems/audio.rs` | 2026-06-04 (`28d08e56`) | before prior audit |
| `byroredux/src/components.rs` | 2026-06-15 (`e5868bac`) | before prior audit |
| `byroredux/src/asset_provider/texture.rs` | 2026-06-28 (`07369f2a`) | before prior audit |
| `byroredux/src/scene.rs` | 2026-07-02 18:37 (`91b8c5df`) | Fix #1846 (player-body FormId) — no audio-relevant lines touched |
| `byroredux/src/main.rs` | 2026-07-02 21:13 (`af6e4c9b`) | Fix #1791 (skin-slot bind-inverse requeue) — unrelated to audio scheduling |

**No commit since the prior audit touches the audio crate or its
engine-side consumers in any way that affects audio behavior.** The two
commits that post-date `AUDIT_AUDIO_2026-07-02.md` (`91b8c5df`,
`af6e4c9b`) are unrelated fixes (player FormId remap, GPU skinning
bind-inverse requeue) and were confirmed by diff to carry zero changes
to `crates/audio/`, `byroredux/src/systems/audio.rs`,
`byroredux/src/components.rs` (audio structs), or the `Stage::Late` /
`Stage::PostUpdate` audio scheduler registration block in `main.rs`.

Given that, this audit is effectively a **regression check** against
the 2026-07-02 baseline rather than a fresh discovery pass — every
claim in that report was independently re-verified against current
source (not merely re-read) per the methodology below.

## Executive Summary

**Crate Phases 1–6 shipped, all re-verified against the live API:**

- Phase 1 — `AudioWorld` (graceful-degradation `Option<AudioManager>`),
  `AudioListener` / `AudioEmitter` / `OneShotSound` components,
  `audio_system`. Confirmed.
- Phase 2 — `load_sound_from_bytes` (symphonia decode), `SoundCache`. Confirmed.
- Phase 3 — spatial sub-track playback, `spawn_oneshot_at`. Confirmed.
- Phase 3.5 — `AudioWorld::play_oneshot` queue API (`VecDeque`, cap
  256, drop-oldest via `pop_front`). Confirmed.
- Phase 4 — looping emitters (`loop_region(..)`); tweened `stop()` on
  emitter-component removal, `stop_issued` debounce, per-emitter
  `unload_fade_ms`. Confirmed.
- Phase 5 — `load_streaming_sound_from_bytes` / `_from_file`,
  single-slot `play_music` / `stop_music` (main-track, non-spatial). Confirmed.
- Phase 6 — one global `SendTrackBuilder` reverb send, per-new-track
  `with_send` opt-in gated on `is_finite() && > -60.0`, default
  `f32::NEG_INFINITY`. Confirmed.

**Engine consumers shipped, re-verified:**
- Footstep gameplay loop — `footstep_system` (`Stage::PostUpdate`,
  after transform propagation), XZ-plane stride accumulation,
  `play_oneshot` dispatch, `FootstepScratch` Vec reuse.
- Per-cell reverb — `reverb_zone_system` (`Stage::Late`, registered
  before `audio_system`), interior `-12 dB` / exterior `NEG_INFINITY`,
  bit-equality-gated transition.

**Live test run (not just read) — all PASS on HEAD `8498e559`:**
```
cargo test -p byroredux-audio           → 19 passed; 0 failed; 6 ignored (real-device-gated)
cargo test -p byroredux ... footstep    → 5 passed; 0 failed
cargo test -p byroredux ... reverb_tests → 5 passed; 0 failed
```
The 6 ignored tests in `byroredux-audio` (`play_music_drives_streaming_
playback_on_real_ogg`, `real_fnv_sounds_decode_through_kira`,
`play_oneshot_queue_drives_real_playback`, `non_looping_emitter_stops_
on_emitter_remove_regression_858`, etc.) are gated behind real audio
device / real game-data availability, consistent with their
`#[ignore]` documentation — not a coverage gap introduced by this audit.

**Pending (future-phase, NOT flagged as missing):** Phase 3.5b FOOT
records → per-material sound; REGN ambient soundscapes; MUSC +
hardcoded music routing; per-cell acoustic reverb (current detector is
binary interior/exterior); raycast occlusion attenuation.

**MUSC parse→play gap (confirmed, by design, unchanged):** cell-music
FormIDs are parsed (`default_music` ZNAM, `music_type_form` XCMO in
`crates/plugin/src/esm/cell/`) but **no caller invokes `play_music`** —
`grep -rn play_music byroredux/src` returns zero non-audio-crate hits.
Future-phase gap, single-slot / main-track invariants pinned for the
eventual caller (Dim 4).

**Headless-mode boot status: PASS.** `AudioWorld::new()` falls back to
`manager: None` on `AudioManager::new` failure; `reverb_send` builder
falls back to `None`; every manager-touching path gates on
`is_some()` / `is_active()` with no `unwrap()` on the inner
`Option<AudioManager>`. Guard:
`audio_world_constructs_without_panic_on_any_environment` — ran green.

**Findings count by severity:**
- CRITICAL: 0
- HIGH: 0
- MEDIUM: 0
- LOW: 0 new (1 pre-existing, tracked as issue #1859, still OPEN)
- Total NEW: 0

**Delta vs the prior report `docs/audits/AUDIT_AUDIO_2026-07-02.md`:**
zero new findings, zero regressions. The single LOW finding from that
report (`AUD-2026-07-02-01`, the `SoundCache` docstring's stale
`asset_provider.rs` file-path reference) was published as GitHub issue
**#1859** (confirmed `OPEN` via `gh issue list`) and is **still present
verbatim** in current source — re-verified below, not re-reported as
NEW.

## Lifecycle Invariant Matrix

Field-drop order (Rust drops struct fields in declaration order).
Current declaration in `pub struct AudioWorld`
(`crates/audio/src/lib.rs:234-274`), unchanged from the prior audit:

| Order | Field | Type | Drop side effect | Status |
|---|---|---|---|---|
| 1 | `active_sounds` | `Vec<ActiveSound>` (owns `StaticSoundHandle` + `SpatialTrackHandle`) | drops handles → kira marks resources for removal | Confirmed |
| 2 | `pending_oneshots` | `VecDeque<PendingOneShot>` (data only) | none | Confirmed |
| 3 | `music` | `Option<StreamingSoundHandle<FromFileError>>` | none (kira-internal mark-for-removal) | Confirmed |
| 4 | `reverb_send` | `Option<SendTrackHandle>` | drops send track | Confirmed |
| 5 | `reverb_send_db` | `f32` | none | Confirmed (no-op) |
| 6 | `listener` | `Option<ListenerHandle>` | mark-for-removal | Confirmed |
| 7 | `manager` | `Option<AudioManager<DefaultBackend>>` | tears down audio device | Confirmed |
| 8 | `multi_listener_warned` | `bool` | none | Confirmed (no-op) |

Invariant "track handles drop before listener drops before manager
drops" is preserved — re-read at `lib.rs:234-274`, byte-identical to
the prior audit's citation.

Scheduler registration order re-verified directly in
`byroredux/src/main.rs`:
- `footstep_system` → `Stage::PostUpdate` (line 855), after transform
  propagation and after `spin_system`/`particle_system`.
- `camera_follow_system` → `Stage::Late` *parallel* batch (line ~917),
  runs before Late *exclusive* systems per scheduler semantics.
- `reverb_zone_system` → `Stage::Late` *exclusive* (line 951),
  registered immediately before `audio_system`.
- `byroredux_audio::audio_system` → `Stage::Late` *exclusive* (line 973).

This ordering — `PostUpdate` (footstep enqueue) → `Late` parallel
(camera pose write) → `Late` exclusive (`reverb_zone_system` →
`audio_system`, in that order) — matches the prior report's citation
exactly, confirming the same-frame footstep-to-playback contract and
the reverb-level-before-dispatch contract both still hold.

Per-handle owners, dispatch-path equivalence (entity path vs. queue
path), and the single `linear_volume_to_db` helper consumed by all
three play sites (`drain_pending_oneshots`, `dispatch_new_oneshots`,
`play_music`) were all re-read at `lib.rs:140-150, 454, 817-818, 942-943`
and are unchanged and correct.

## Findings

No NEW findings. One pre-existing LOW finding, tracked and OPEN:

### AUD-2026-07-02-01 (re-confirmed, not re-reported as NEW): `SoundCache` docstring stale path

- **Severity**: LOW
- **Dimension**: SoundCache Growth
- **Location**: `crates/audio/src/lib.rs:1176-1178`
- **Status**: Existing: #1859 (OPEN)
- **Description**: The `SoundCache` struct docstring's "Dormant API
  (#859)" block still reads
  `` `byroredux/src/asset_provider.rs::try_load_default_footstep` ``.
  Post-Session-34 module split, `asset_provider.rs` is a directory
  (`byroredux/src/asset_provider/`); the function lives in
  `asset_provider/texture.rs`. Re-confirmed today:
  `ls byroredux/src/asset_provider*` shows a directory (`archive.rs`,
  `material.rs`, `mod.rs`, `script.rs`, `tests.rs`, `texture.rs`);
  `grep -rn 'fn try_load_default_footstep' byroredux/src/` resolves
  only to `asset_provider/texture.rs:79`.
- **Evidence**: unchanged from the 2026-07-02 report; the exact three
  lines at `lib.rs:1176-1178` are byte-identical to that report's
  citation.
- **Impact**: Documentation only, no runtime effect.
- **Related**: GitHub issue #1859 (open, filed from the prior audit);
  #1615 (closed — fixed the adjacent function-name reference in the
  same docstring block, left the file-path stale).
- **Action**: No action needed from this audit — already tracked.
  `/audit-publish` should not re-file it; it is confirmed still open
  and accurately described by the existing issue.

## Dimension-by-Dimension Verification (clean, all re-confirmed)

- **Dim 1 (Spatial Sub-Track Lifecycle & Leaks)** — `_track` field name
  intact (`lib.rs:190`, held for Drop side effect only); both dispatch
  paths (`drain_pending_oneshots`, `dispatch_new_oneshots`) gate on
  `listener_id` and short-circuit before any listener exists;
  `active_sounds.push` happens before each helper returns (handle never
  drops at end of scope); `looping` applied only in the entity path
  (`sound.loop_region(..)` gated on `p.looping`, `dispatch_new_oneshots`
  line 944 — the queue path builds `PendingOneShot` with no `looping`
  field at all, so it structurally cannot loop); drain-cap WARN fires
  at `pending.len() > 32` (line 789); producer cap enforced at 256 via
  `VecDeque::pop_front` (O(1)) with the `manager.is_none()` early
  return preceding any push (lines 401-403); the manager-active gate in
  `drain_pending_oneshots` precedes `std::mem::take` (lines 785-788);
  entity path reads `GlobalTransform.translation` (post-propagation
  pose, line 895), never falls back to raw `Transform`.
- **Dim 2 (Listener Pose & Attenuation)** — lazy `add_listener` on
  first resolved `GlobalTransform` (`sync_listener_pose`, no panic path
  on frame-1 cold start — every step is an early-return `Option`
  chain); sticky listener handle, never cleared on entity despawn
  (documented invariant at `lib.rs:685-698`, matches #849); debounced
  multi-listener WARN via `multi_listener_warned` (set once, checked
  before logging); `Tween::default()` used for both `set_position` and
  `set_orientation` every frame (no immediate-jump regression);
  `Attenuation::distance_range()` normalizes `min<=max` via
  `.min()`/`.max()` swap (line 562-565, #1612 guard); `add_listener`
  failure logs WARN and leaves `listener = None`, retried next frame
  (no permanent audio breakage on transient init failure).
- **Dim 3 (SoundCache Growth)** — keys lowercased exactly once at each
  of `get`/`insert`/`get_or_load` (`to_ascii_lowercase()` called once
  per call site, no double-lowering); eviction is `clear()`-only, no
  auto-LRU (documented, acceptable per #850); `clear()` doesn't
  invalidate live `ActiveSound` Arcs (kira holds its own clone,
  verified structurally — `SoundCache` is never read through after the
  initial `play` call); zero engine call sites confirmed via
  `grep -rn 'SoundCache' byroredux/src` (only test/registration hits,
  no live producer) — `try_load_default_footstep` bypasses the cache
  entirely, `len()==0` steady state holds; `bytes_estimate` sums
  `frames.len() * size_of::<Frame>()`, one finding (doc-rot, above) on
  its "not yet wired to stats" claim which remains accurate — no stats
  consumer exists; `get_or_load` invokes the loader closure only inside
  the `None` branch of the `HashMap::get` match (no double-decode on
  hit).
- **Dim 4 (Streaming Music Lifecycle)** — exactly one `music: Option<...>`
  slot, `play_music` always fades-then-replaces (lines 451-464); routes
  through `mgr.play(...)` main track, never a spatial sub-track (no
  `add_spatial_sub_track` call anywhere near `play_music`); uses
  `StreamingSoundData::from_cursor`/`from_file` (not `StaticSoundData`)
  — confirmed at `load_streaming_sound_from_bytes`/`_from_file`;
  `stop_music`'s fade duration is `fade_out_secs.max(0.0)` (line 475,
  never negative/instant-click); `is_music_active()` returns `false`
  once `PlaybackState::Stopped` (line 491); MUSC parse→play gap
  reconfirmed absent via fresh grep (zero non-crate callers).
- **Dim 5 (Reverb Send & Routing)** — one global send track built in
  `AudioWorld::new()` (`feedback(0.85) damping(0.6) stereo_width(1.0)
  mix(WET)`, lines 319-337); `None` path (`add_send_track` failure or
  inactive manager) never reaches an `unwrap()` — both dispatch sites
  gate with `if let Some(reverb) = audio_world.reverb_send.as_ref()`;
  both `drain_pending_oneshots` (lines 805-809) and
  `dispatch_new_oneshots` (lines 923-927) apply the byte-identical
  `is_finite() && > -60.0` gate — no drift between the two copies;
  default `reverb_send_db = f32::NEG_INFINITY` set in `new()` (line
  343); construction-time-only send level is documented as a known
  limitation (#847), not re-flagged as a bug.
- **Dim 6 (Manager Lifecycle, ECS & Cell Streaming)** — graceful
  degradation confirmed live via
  `audio_world_constructs_without_panic_on_any_environment` (passed);
  capacities `SUB_TRACK_CAPACITY=512` / `SEND_TRACK_CAPACITY=32` applied
  in `AudioManagerSettings.capacities` at construction (lines 288-295),
  above kira defaults, guard `manager_capacities_exceed_kira_defaults`
  passed; field-drop order verified in the matrix above;
  `AudioWorld`/`SoundCache` are both plain `Resource` impls (`&self`
  access, no `&mut World` requirement snuck in); `AudioWorld::new()` is
  called exactly once at boot (`main.rs:560`), no call on cell
  transition or resize; `audio_system` registration and internal body
  order (`sync_listener_pose → drain_pending_oneshots →
  dispatch_new_oneshots → prune_stopped_sounds`, lines 675-678)
  unchanged; cross-stage `footstep_system` (`PostUpdate`) → `audio_system`
  (`Late`) ordering re-verified in `main.rs` directly (see Lifecycle
  Invariant Matrix above); `OneShotSound` removed after dispatch (lines
  974-980, single-frame marker, no re-trigger path); despawn-truncation
  invariant (`prune_stopped_sounds`) re-read end-to-end: emitter-absence
  check via `AudioEmitter` query (line 1014-1017), `stop_issued` set
  after issuing the tween (line 1039), `retain` drops only on
  `PlaybackState::Stopped` (line 1044), `AudioEmitter` removed via
  `emitter_q.remove` on completion (lines 1057-1063) — queue-driven
  (`entity==None`) sounds are structurally exempt (the `let Some(entity)
  = s.entity else { continue }` guard at line 1011-1013).
- **Dim 7 (Gameplay Audio Wiring)** — re-read `byroredux/src/systems/
  audio.rs` in full: stride accumulator resets to `0.0` on fire, not
  subtract-remainder (line 152); first-tick seed-without-fire via
  `!fs.initialised` (lines 140-144, #848 guard); `FootstepScratch`
  buffer `clear()`-reused and `mem::take`-drained, capacity restored on
  BOTH the success path (line 197-199) and the `AudioWorld`-absent bail
  path (lines 174-177) — both branches call
  `world.try_resource_mut::<FootstepScratch>()` and reassign
  `scratch.triggers = triggers`; the `FootstepScratch` mut-lock is
  explicitly `drop()`-ed (line 167) before `AudioWorld` is acquired (no
  concurrent resource-mut hold, no TypeId-sort violation risk); footstep
  attenuation is `{0.5, 12.0}` (lines 187-188), tighter than the crate
  default `{2.0, 30.0}`; all four silent no-op contracts
  (`FootstepConfig` absent, `default_sound` `None`, `FootstepScratch`
  absent, `AudioWorld` absent) verified as early `return`s with no
  panic and no per-frame log spam; camera opt-in at `scene.rs:443/447`
  is component-driven (`world.insert(cam, AudioListener)` /
  `world.insert(cam, FootstepEmitter::new())`, no camera-entity-ID
  special-casing inside the systems themselves);
  `try_load_default_footstep` (`asset_provider/texture.rs:79-119`)
  no-ops cleanly on missing `--sounds-bsa` arg, unopenable archive,
  missing canonical path, or decode failure — each branch is a WARN log
  + early return, boot continues; `reverb_zone_system`
  (`byroredux/src/systems/audio.rs:40-76`) re-read end-to-end:
  `-12.0`/`NEG_INFINITY` consts local to the fn, bit-equality gate via
  `.to_bits()` (line 67), safe no-op on missing `CellLightingRes` (line
  49-51) and missing `AudioWorld` (line 60-62), registered before
  `audio_system` in `main.rs`.

**Live-test corroboration of the above** (not present in the prior
report, added this cycle): every guard named above that has a
corresponding `#[test]` was executed, not merely read —
`cargo test -p byroredux-audio` (19/19 pass) plus the `byroredux`
binary's `footstep_tests` (5/5) and `reverb_tests` (5/5) modules, all
green on HEAD `8498e559`.

## Future-Phase Readiness

Unchanged from the prior audit — re-affirmed, no drift:

- **Phase 3.5b (FOOT → per-material sound):** `play_oneshot` queue path
  is stable; per-material lookup only needs to select the right
  `Arc<StaticSoundData>` before enqueue. `SUB_TRACK_CAPACITY=512` gives
  headroom for combat-time overlapping footsteps.
- **REGN ambient soundscapes:** cell-unload must remove the
  `AudioEmitter` component (or despawn the entity) — the prune sweep
  then issues the tweened `unload_fade_ms` stop. Long-tail ambients
  should author a longer `unload_fade_ms` (200–500 ms).
- **MUSC routing:** single-slot `music` field + main-track dispatch are
  pinned. The eventual caller must gate `play_music` on FormID equality.
- **Per-cell acoustic reverb / occlusion:** current detector is binary
  interior/exterior; `with_send` is build-time-only (#847), so a
  reverb-flip won't retro-apply to already-playing looping ambients —
  the flip handler must re-dispatch them.

## Methodology Note

Per `feedback_audit_findings.md` and the incremental-audit protocol,
every prior finding was checked against a live `git log` diff before
re-verifying in source, rather than assuming the prior report's
conclusions still hold:

- Confirmed via `git log -1 --format="%H %ci"` per file that no commit
  touching `crates/audio/`, `byroredux/src/systems/audio.rs`,
  `byroredux/src/components.rs`, or `byroredux/src/asset_provider/
  texture.rs` post-dates the prior audit.
- The two post-dating commits touching `scene.rs`/`main.rs` (`91b8c5df`,
  `af6e4c9b`) were inspected directly (`git show --stat` /
  `git show -- <path>`) and confirmed unrelated to audio (player FormId
  remap; GPU skin-slot bind-inverse requeue).
- Re-ran `cargo test -p byroredux-audio` and the two `byroredux`-binary
  audio test modules live — all pass — rather than relying solely on
  the prior report's static-analysis claims.
- Re-confirmed the one open LOW finding resolves to a real, currently
  OPEN GitHub issue (#1859) via `gh issue list`, so it is reported as
  "Existing: #1859", not double-filed as NEW.

No candidate findings were disproven this cycle beyond the ones the
prior report already worked through, since the source is unchanged;
this audit's value-add is the live test corroboration and the explicit
git-log-based zero-delta confirmation.

## Dedup Status

`/tmp/audit/audio/issues.json` (200 most recent, all states) scanned
for `audio`, `kira`, `sound`, `reverb`, `listener`, `music`, `oneshot`,
`footstep`, `attenuat`, `aud-`, `m44`. One match:

- **#1859** (`AUD-2026-07-02-01`) — OPEN. `SoundCache` docstring stale
  path. Confirmed still present verbatim in current source (see
  Findings section above). Not re-filed.

The prior audio report `docs/audits/AUDIT_AUDIO_2026-07-02.md` (and by
extension `AUDIT_AUDIO_2026-06-23.md`, `AUDIT_AUDIO_2026-06-14.md`,
`AUDIT_AUDIO_2026-05-05.md`) remains fully current — no regression, no
new finding, this report supersedes none of them in content, only in
recency and live-test corroboration.

## Suggested Next Action

No new GitHub issues to file — issue #1859 already tracks the sole
open finding and needs no update. The audio subsystem remains in a
clean, fully-guarded state on HEAD `8498e559`: headless boot PASSES
(live-tested), all prior findings are fixed and regression-guarded, and
no CRITICAL/HIGH/MEDIUM issues exist across the 7 dimensions.

```
/audit-publish docs/audits/AUDIT_AUDIO_2026-07-03.md
```
(Will find nothing new to publish — informational run only.)
