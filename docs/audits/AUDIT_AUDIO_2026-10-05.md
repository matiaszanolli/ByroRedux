# Audio Subsystem Audit (M44) — 2026-10-05

**HEAD**: `a2c24b16e` · **Baseline**: [`AUDIT_AUDIO_2026-09-29.md`](AUDIT_AUDIO_2026-09-29.md) (HEAD `9fcfdc3fc`) · **Audited**: Dim 4 (Manager & ECS Lifecycle), Dim 5 (Engine Consumers) · **Unchanged since baseline (skimmed)**: Dim 1 (Spatial Dispatch & Listener), Dim 2 (Send Graph), Dim 3 (Music & SoundCache). `crates/audio/` has had no commits since `9fcfdc3fc`, and the only `SoundCache`-consumer diff is one test line in `combat_anim.rs`. The guards were spot-checked.

- **Command**: `/audit-audio` (default scope), one leg of `/audit-suite --preset comprehensive`. Each dimension was analysed serially in this session with no sub-agents. Scratch notes are in `/tmp/audit/audio/dim_1.md` to `dim_5.md`, and every one is reconciled into this report.
- **kira**: pinned `0.10` (unchanged).
- **Tests run**:
  - `cargo test -j 4 -p byroredux-audio`: **33 passed, 0 failed, 7 ignored**.
  - `cargo test -p byroredux --bin byroredux -- audio combat_anim scheduler_access footstep` (toolchain 1.96.0): **74 passed, 0 failed, 1 ignored**. This includes the #5146 trio.
  - Two device-free `--ignored` real-data tests, both PASS: `real_fnv_sounds_decode_through_kira` and `draugr_combat_sound_assets_extract_and_decode_when_available`.
  - Four lifecycle guards need an audio device and were **not run**: `looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`, `non_looping_emitter_stops_on_emitter_remove_regression_858`, `play_music_looping_survives_track_end` and `play_music_drives_streaming_playback_on_real_ogg`. The suite forbids device-opening processes while other audits run. The crate code they cover has no diff since the baseline, which traced it.
  - `BYRO_LOCK_ORDER_CHECK=1 … -- --test-threads=1 footstep_tests systems::water::tests systems::character::tests::camera_follow` gives **28 passed, 4 FAILED**. That reproduces ECS-2026-10-05-D1-01; see below.
- **Headless-mode boot: PASS** (`audio_world_constructs_without_panic_on_any_environment` and `explicit_headless_world_discards_playback_without_retaining_sound`).

## Executive Summary

Two commits since the baseline touch audio behaviour:
- **`ccc743160` (#5146)** fixes the baseline's only MEDIUM, AUD-2026-09-29-D5-01. In character mode the player body now carries the `FootstepEmitter`, and stride accumulates only while the body is grounded and not swimming. Gated frames re-seed, so neither a landing nor leaving the water replays the distance as a burst of steps. The camera emitter is kept re-seeded while a body exists. All three new regression tests pass. The teleport paths also set `is_grounded = false`: the body snap, the door transition, the save load and `combat.approach`. However, they run outside the window between the Early `character_controller_system` write and the Late footstep read. The controller rewrites the flag first, so a teleport that lands grounded still fires at most one footstep, unchanged from before (`single_large_jump_fires_one_footstep_only`).
- **`70d9896fb` (#4709)** moves `make_combat_feedback_system` out of the `BYRO_NO_AI_LOCOMOTION` block (`boot/schedule/post_update.rs:132-148`). It is pinned by `locomotion_kill_switch_gates_exactly_the_motion_systems`, whose needles are built with `format!` so the test cannot match its own source.

The crate core (unit seam, listener, send graph, music slot and `SoundCache` contract) is byte-identical to the baseline.

**Cross-referenced, not re-filed: ECS-2026-10-05-D1-01 (HIGH, owned by `AUDIT_ECS_2026-10-05.md`)**. In `footstep_system`, #5146 added `world.get::<CharacterController>` and `world.get::<WaterContact>` inside the scope that holds the `FootstepScratch`, `GlobalTransform` and `FootstepEmitter` guards (`systems/audio.rs:177-216`). That closes three production lock-order cycles, and the ABBA CI lane is red. This audit reproduced 4 of the 5 panics independently. The audio-specific contribution is D5-01 below: the `WaterContact` read, which closes cycles B and C, is redundant in production. The D1-01 fix can therefore drop it rather than reorder it, and only the single `CharacterController` read needs to move ahead of the queries.

**New findings** (all LOW and all follow-ups to #5146):
- **D5-01**: the swim gate duplicates `is_grounded`, and its regression test cannot detect its removal. The redundant read is the edge behind cycles B and C of D1-01.
- **D5-02**: the "skip non-body emitters" gate is keyed on `PlayerEntity`, not on the camera. It would mute every future NPC `FootstepEmitter` in character mode, and it keeps an F-toggled FlyCam silent, which contradicts the new comments.
- **D5-03**: three new or now-stale doc sites. One cites a function that does not exist, one still says the character controller will own the emitter "in future", and one gives a Late-pin rationale that no audible emitter depends on any more.

**Severity counts (this run)**:

| Severity | NEW | Existing (residual) | Total |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 0 | 0 | 0 |
| LOW | 3 | 3 | 6 |
| **Total** | **3** | **3** | **6** |

The three existing items are the unchanged residuals #5148, #4743 and #4747. They are listed in the disposition table and not re-reported in full. The cross-referenced HIGH, ECS D1-01, is not counted here.

No regressions:
- #4146: the reverb-zone comment (`systems/audio.rs:54-62`, `late.rs`) still attributes ordering to parallel-before-exclusive.
- Archive precedence: `.rev()`, untouched.
- `reverb_zone_system` is still bit-gated.
- REGN change gates are intact at all 3 sites.

## Lifecycle Invariant Matrix (HEAD `a2c24b16e`)

| Invariant | State | Anchor |
|---|---|---|
| Field-drop order `active_sounds → pending_oneshots → (oneshots_requested) → music → reverb_send → reverb_send_db → listener → manager → multi_listener_warned → underwater` | HOLDS (no diff) | `crates/audio/src/lib.rs:394-436` |
| `headless()` never contacts a device; capacities 512 / 32 applied in `new()` | HOLDS (no diff) | `lib.rs` |
| `AudioWorld::new()` boot-only | HOLDS | `byroredux/src/boot/world.rs` (diff since baseline adds only `GracefulExitRequested` + `PendingGearRelease` registration) |
| Sticky listener (never cleared; kira capacity 8) | HOLDS (no diff) | `lib.rs` `sync_listener_pose` |
| Despawn truncation (tweened stop over `unload_fade_ms`, `stop_issued` debounce, queue sounds exempt) | HOLDS. No diff; the device-gated guards were not run | `lib.rs` `prune_stopped_sounds` |
| `audio_system` body order: listener → underwater → drain → dispatch → prune | HOLDS | `lib.rs:902-906` |
| `footstep_system` Late exclusive before `audio_system` | HOLDS, pinned by `footstep_runs_after_camera_follow_in_late` (pass) | `boot/schedule/late.rs:102-115` vs `:250` |
| `submersion_system` → `water_audio_system` → `audio_system` | HOLDS | `late.rs:154`, `:226`, `:250` |
| `reverb_zone_system` Late parallel, `audio_system` Late exclusive | HOLDS, pinned by `reverb_zone_is_parallel_and_audio_is_exclusive_in_late` (pass) | `late.rs:176`, `:250` |
| PostUpdate `make_combat_feedback_system()` → Late drain, same frame; **outside** the locomotion kill switch | HOLDS, moved by #4709 and pinned | `boot/schedule/post_update.rs:132-148`; `boot/schedule/mod.rs:304` |
| REGN music change-gated at every call site | HOLDS | `cell_loader/load.rs:700-714`, `:1092-1097`; `scene/world_setup.rs:549-585` |
| `SoundCache` → `SoundArchiveProvider` nesting, no reverse edge | HOLDS (combat_anim production code unchanged) | `systems/combat_anim.rs:382-402` |
| `footstep_system` Access row declares every read | HOLDS. #5146 added `PlayerEntity`, `CharacterController` and `WaterContact` | `late.rs:105-115` |
| `footstep_system` lock nesting is acyclic | **DRIFTED**: see ECS-2026-10-05-D1-01 (HIGH, ECS-owned) and D5-01 | `systems/audio.rs:177-216` |

## Findings

### AUD-2026-10-05-D5-01: The footstep swim gate duplicates `is_grounded`, so its test cannot detect its removal. The redundant `WaterContact` read is the edge that closes cycles B and C of ECS D1-01
- **Severity**: LOW. There is no audible defect. The lock-order consequence is graded HIGH under ECS-2026-10-05-D1-01 and is not double-counted here.
- **Dimension**: Engine Consumers
- **Location**:
  - `byroredux/src/systems/audio.rs:205-216`: the gate.
  - `byroredux/src/systems/character.rs:1676-1684`: `resolve_ground_contact`.
  - `character.rs:297-303`: the swim verdict.
  - `character.rs:1328-1358` and `:620`: `sync_player_water_contact`.
  - `systems/audio.rs:857-917`: the test.
- **Status**: NEW (introduced by `ccc743160`, #5146)
- **Description**: The #5146 body gate is `if !grounded || swimming`, where `swimming = depth_reaches_swimlevel(WaterContact.depth, half_height + radius)`. In production that second operand cannot change the result:
  1. `character_controller_system` computes `swim` from `player_water_state` at the **pre-move** pose (`:297-303`). It passes `swim.is_some()` to `resolve_ground_contact`, which returns `(false, …)` whenever it is swimming (`:1682-1684`). So `is_grounded` is false on every swimming frame.
  2. The same tick publishes the retained `WaterContact` with `depth = surface_y - current_pos.y`, measured at the same pre-move pose (`sync_player_water_contact`; its doc says so explicitly). `depth_reaches_swimlevel(contact.depth, …)` therefore equals that frame's `swim.is_some()`. This is the same round-trip that `was_swimming` relies on.

  So `!grounded || swimming` reduces to `!grounded`. The regression test `swimming_body_is_silent_and_exit_does_not_replay_the_swim_distance` sets `is_grounded = false` **and** `depth = 30` for the whole swim leg (`:860-872`). It would still pass with the `WaterContact` get deleted, or with the predicate replaced by `false`. It only catches an always-true break, through the exit step.

  The redundant read is not free. ECS-2026-10-05-D1-01 attributes cycle B (`GlobalTransform → WaterContact → GlobalTransform`) and cycle C (`ActorVitals → ActorValues → GlobalTransform → WaterContact → ActorVitals`) to exactly this `world.get::<WaterContact>` under the `GlobalTransform` / `FootstepEmitter` guards. Cycle A (`… → CharacterController`) comes from the `CharacterController` get, which is needed.
- **Evidence**:
  ```rust
  // character.rs:1682
  if swimming { return (false, vertical_velocity); }
  // systems/audio.rs:210-216
  let swimming = match (&controller, world.get::<WaterContact>(entity)) {
      (Some(c), Some(contact)) => depth_reaches_swimlevel(contact.depth, c.half_height + c.radius),
      _ => false,
  };
  if !grounded || swimming { … }
  ```
  - Reproduction of the D1-01 edges: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux -- --test-threads=1 footstep_tests systems::water::tests systems::character::tests::camera_follow` gives 28 passed and 4 failed.
  - The four failures are `camera_follow_does_not_close_character_lock_cycle` (cycle A) and three `systems::water::tests` (cycle B).
- **Impact**:
  - Today there is none audibly. The extra read is a dead operand that holds a lock edge.
  - The test gives false assurance: if someone later decouples swimming from `is_grounded` (for example, a swim state that keeps "grounded" for wading on the lake floor), the swim gate's silence will rest on a predicate no test exercises.
- **Related**: ECS-2026-10-05-D1-01 (HIGH, the fix owner), cross-referenced in `AUDIT_CONCURRENCY_2026-10-05.md`. Also #5146 (closed) and PHYS-D5 sampler-parity in `AUDIT_PHYSICS_2026-10-05.md`, which notes swim and camera submersion can disagree near the waterline. That is a different sampler, not this gate.
- **Suggested Fix**:
  - Before taking any query guard, resolve the body's gate once: `let body_grounded = player_body.and_then(|b| world.get::<CharacterController>(b).map(|c| c.is_grounded));`. Then drop the `WaterContact` read, or derive it in the same pre-pass if it is kept as defence in depth. This closes all three D1-01 cycles.
  - Split the swim test so that one leg holds `is_grounded = true` with a swimlevel depth, if the gate stays. Otherwise document that swimming is covered through `resolve_ground_contact`.

### AUD-2026-10-05-D5-02: Non-body emitters are suppressed whenever `PlayerEntity` is set, not just the camera's. Every future NPC `FootstepEmitter` would be silent in character mode, and an F-toggled FlyCam stays silent contrary to the new comments
- **Severity**: LOW. It is latent: no production NPC carries a `FootstepEmitter` today. The FlyCam behaviour is arguably desirable but disagrees with the code's own stated intent.
- **Dimension**: Engine Consumers
- **Location**:
  - `byroredux/src/systems/audio.rs:155-158`, `:197-204`: the gate.
  - `systems/character.rs:1063-1083`: `toggle_player_mode` leaves `PlayerEntity` set.
  - `components.rs:1835-1844`: the `FootstepScratch` sizing doc ("5–10 walking NPCs, peak ~50").
  - `scene.rs:977-981`: comment.
  - `late.rs:84-86`: comment.
- **Status**: NEW (introduced by `ccc743160`, #5146)
- **Description**: The new branch is `if let Some(body) = player_body { if entity != body { re-seed; continue } … }`. Two problems follow.
  1. **Opt-in is no longer component-driven.** The skill invariant, the system doc ("Spawn a `FootstepEmitter` on the player entity to opt in") and `FootstepScratch`'s capacity rationale all describe a component that any walker can carry. Under #5146, in character mode, which is the only mode where NPC footsteps matter, every emitter except the player body's is re-seeded and never fires. The #5146 intent was to skip the *camera's* emitter, but the predicate skips everything that is not the body. The first NPC or FOOT (3.5b) consumer that inserts a `FootstepEmitter` will be silently muted, and none of the #5146 tests would notice.
  2. **FlyCam after the F toggle.** `toggle_player_mode` flips `PlayerMode` but leaves `PlayerEntity(Some(body))`, and the body is frozen in FlyCam. The camera emitter therefore stays suppressed and flying is silent. That is arguably right, since flying is not walking. But the code says otherwise in three places:
     - The camera re-seed comment (`:199-201`) says it exists "so a later FlyCam switch doesn't replay the whole boom arc as one stride burst", which assumes the camera resumes stepping after the switch.
     - `late.rs:84-86` says "the emitter lives on the active camera in FlyCam".
     - The `footstep_system` doc says "FlyCam scenes (no body) keep the original camera-is-the-mover behaviour".

     So a `--fly` boot steps while flying and an F-toggled FlyCam does not. Neither behaviour is pinned.
- **Evidence**: see Location. `git grep -n 'FootstepEmitter::new()' -- byroredux/src ':!*/systems/audio.rs'` finds only `scene.rs:982` (the camera) and `scene.rs:1230` (the body). `toggle_player_mode` has no `PlayerEntity` write, and the only production `PlayerEntity` insert is `scene.rs:1223`.
- **Impact**: there is no audible regression today. A latent trap is set for the per-NPC and FOOT footstep work, and the documentation and code disagree on FlyCam.
- **Related**: #5146 (closed), FOOT phase 3.5b, D5-03.
- **Suggested Fix**:
  - Skip only the entity that actually stands in for the player: the `ActiveCamera` entity while `PlayerMode == Character`. Alternatively, remove the camera's `FootstepEmitter` when the body spawns and re-add it on Character → FlyCam if stepping while flying is wanted.
  - Pin whichever FlyCam behaviour is chosen. Add a test in which a third, non-player emitter walking in character mode fires.

### AUD-2026-10-05-D5-03: #5146 left three stale or wrong doc sites about which entity carries the footstep emitter
- **Severity**: LOW
- **Dimension**: Engine Consumers / Manager & Schedule (doc)
- **Location**:
  - `byroredux/src/scene.rs:1227-1229`: "The camera's emitter (inserted in `setup_camera_and_lights`)". No such function exists (`git grep` has 0 definitions; the insert is in `spawn_initial_camera`, `scene.rs:903`/`:982`).
  - `scene.rs:977-981`: "character mode **moves** it onto the player body". Nothing moves: the body gets a second emitter and the camera keeps its own (see D5-02).
  - `byroredux/src/components.rs:1717-1719`: the `FootstepEmitter` doc says "today the fly-camera entity; an `M28.5` character controller will own this in future". The body has owned it since #5146.
  - `byroredux/src/boot/schedule/late.rs:83-99`: the footstep pin rationale still says that in player/third-person mode the system reads "`camera_follow_system`'s pose, not last frame's". In character mode the only accumulating emitter is now the body, whose `GlobalTransform` comes from Early `character_controller_system` plus PostUpdate propagation. `camera_follow_system` no longer feeds any audible footstep, and the camera emitter is only re-seeded. `footstep_runs_after_camera_follow_in_late` still pins an order that is now harmless but unmotivated.
- **Status**: NEW (introduced by `ccc743160`)
- **Description**: see Location. The dead function name is the kind of path/symbol reference that `_audit-validate.sh`-style checks flag. The `components.rs` "in future" line is directly falsified by the commit.
- **Impact**: documentation only. The next reader of the Late pin will look for a camera-pose dependency that no longer exists.
- **Related**: D5-02, #4146 (the earlier comment drift class in the same file).
- **Suggested Fix**:
  - Name `spawn_initial_camera` at `scene.rs:1228`.
  - Say "adds a body emitter" at `:979`.
  - Rewrite the `FootstepEmitter` doc to say the body carries it in character mode and the camera in FlyCam boots.
  - Re-word the `late.rs` rationale. The Late pin now matters only for FlyCam-boot camera emitters, or it could move to PostUpdate after propagation if it is kept for the body alone. Keep the pin test or retire it accordingly.

## Prior-Finding Disposition

| Issue | Baseline ID | State at HEAD | Evidence |
|---|---|---|---|
| #5146 (closed) | 09-29 D5-01 camera-borne footsteps | **Fixed** | `ccc743160`. The three regression tests pass. Follow-ups are D5-01, D5-02 and D5-03 |
| #5148 (open) | 09-29 D5-02 P2 gate satisfied by footsteps | **Unchanged** | `docs/smoke-tests/p2-melee-core.sh:318-323` still gates on the total `oneshots_requested > 0`. `combat.approach` (`commands/view.rs:455-476`) teleports the **body**, which now carries the emitter. Whether that teleport still produces the satisfying footstep depends on the controller's first post-teleport grounded verdict. Either way, a total counter cannot detect a silent combat-sound path |
| #4743 (open) | 09-29 D5-03 Draugr swing gated on take install | **Unchanged** | `combat_anim.rs` production code has no diff since the baseline |
| #4747 (open) | 09-29 D5-04 status docs | **Unchanged** | `ROADMAP.md:77-78` still says "footsteps, ambient, music". `docs/feature-matrix.md:228` still has combat sound at ✗ |
| #4739, #4740, #4741, #4744, #4745, #4746 (open) | 09-21 D3-01 / D4-01 / D4-02 / D5-03 / D5-04 / D5-05 | **Fixed** (re-confirmed: code untouched since the 09-29 verification). Still recommend closing | See `AUDIT_AUDIO_2026-09-29.md` disposition table |
| #3086, #3816, #3301 (open) | entity-path frozen position; MUSC/MUST/MSET/RDMD; REGN incidental | **Unchanged** | No crate or REGN-dispatch diff |

## Future-Phase Readiness

- **FOOT records → per-material footsteps (3.5b)**:
  - The prerequisite the baseline named is now in place: the pose source is the body, gated on ground contact.
  - Two blockers remain. Per-NPC emitters are muted by D5-02. Ground material is not plumbed: `is_grounded` is a bool, and the support probe's surface (`cast_capsule_down_surface_and_normal`) returns a height and normal, not a collider material or `bhk` Havok material id.
  - Wading below swimlevel (grounded in water) still plays the dirt-walk sound. That is a natural FOOT/WATAL case once material selection exists.
- **MUSC/MUST/MSET/RDMD (#3816)**: the mechanism is unchanged, change-gated at 3 sites, and fails closed. It plays nothing in production.
- **Occlusion**: none. `Attenuation` is distance-only and no code touched it.
- **`SoundCache` eviction**: not yet needed. The only consumer uses three `&'static str` keys. A runtime archive swap would still need a `clear()`, because negative entries are never re-probed.

## Dedup Method

- **Open issues**: checked against `/tmp/audit/issues.json` (97 open). Titles were searched for audio, sound, footstep, music, reverb, oneshot, splash, swing, kira, emitter and 5146. Matches were #5148, #4739–#4747, #3816, #3301 and #3086. None covers the swim-gate redundancy, the non-body suppression or the #5146 doc sites.
- **Closed issues**: `gh issue view 5146` returned CLOSED, and its fix was verified above.
- **Sibling reports (2026-10-05)**:
  - ECS D1-01 owns the footstep lock-order HIGH; it is cross-referenced, not re-filed.
  - Concurrency cross-references the same finding and notes a benign `CharacterController → WaterContact` edge.
  - The gameplay report defers audio issues to this audit.
  - Physics' sampler-parity finding mentions swim and audio disagreement near the waterline. That is the submersion sampler, not the footstep gate, so it is cross-referenced only.
  - The character report places `ccc743160` outside CHARAL.

Publish with `/audit-publish docs/audits/AUDIT_AUDIO_2026-10-05.md` (labels `audio`, `low`, plus `doc-rot` for D5-03 and `test-gap` for D5-01).
