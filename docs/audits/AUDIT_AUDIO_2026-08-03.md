# Audio Subsystem Audit (M44) — 2026-08-03

- **Command**: `/audit-audio` → all 7 dimensions, `--depth deep` (one leg of a
  `comprehensive` audit-suite sweep)
- **Branch**: main · **HEAD**: `1ae86f62` (2026-08-03)
- **kira**: pinned `0.10` (workspace `Cargo.toml`, unchanged)
- **Method**: Full independent re-verification via 7 dimension sub-agents (one
  per checklist area), each re-reading `crates/audio/src/lib.rs` in full
  (1293 lines) and `crates/audio/src/tests.rs`, plus the engine-side
  consumers (`byroredux/src/systems/audio.rs`, `byroredux/src/scene.rs`,
  `byroredux/src/asset_provider/texture.rs`, `byroredux/src/boot.rs`). Each
  agent independently re-derived every invariant from live line numbers
  rather than trusting the prior report's table. Dedup baseline: `gh issue
  list` (JSON snapshot, `/tmp/audit/audio/issues.json`) + the full
  prior-report chain back to `_05-05`.

---

## Delta Analysis (`ca7a4e0e..HEAD`, scope-relevant files only)

| File | Change | Audio-relevant? |
|---|---|---|
| `crates/audio/src/lib.rs` | **none** | — byte-identical to the 2026-07-25 audit's HEAD |
| `crates/audio/src/tests.rs` | none | — |
| `byroredux/src/systems/audio.rs` | 2 lines — `footstep_system` docstring corrected + unrelated `fog_medium` test-struct field addition | **Yes, but it's the fix landing**: AUD-2026-07-25-01 (docstring misattributing the fly-camera opt-in to `main.rs::App::new`) is now corrected to `scene.rs::setup_scene` |
| `byroredux/src/components.rs` | none in audio structs | — |
| `byroredux/src/scene.rs` | +243/−69 (door-spawn floor-probe rewrite, #2013 follow-ups) | **No** — confirmed the `AudioListener`/`FootstepEmitter` camera opt-in (moved 445-449 → 575-583 by unrelated insertions above it) is untouched in content |
| `byroredux/src/boot.rs` | 13 commits (HavokAnimationTarget/AnimationPlayer registration, MQ101 scripting systems, scheduler timing gate) | **No** — `footstep_system`/`reverb_zone_system`/`audio_system` registrations are byte-identical; all new insertions land in `Stage::Update`, disjoint from the audio stages |
| `byroredux/src/asset_provider/texture.rs` | +163/−15 (cubemap/environment-texture resolution work) | **No** — `try_load_default_footstep` is byte-for-byte unchanged |

**Net: the only in-scope change since the last audit is the docstring fix from
AUD-2026-07-25-01 landing correctly.** Everything else this cycle is
scripting (MQ101 cinematics), animation (Havok), renderer (cubemaps/shadows),
and physics (door-spawn) work — none of it touches the audio crate's logic or
its two live engine consumers.

---

## Executive Summary

**Zero CRITICAL / HIGH / MEDIUM / LOW findings.** This is the first fully
clean cycle in the report chain — the prior cycle's sole LOW finding
(AUD-2026-07-25-01) is confirmed fixed and not re-flagged.

- **Headless-mode boot**: PASS — `audio_world_constructs_without_panic_on_any_environment`
  green; graceful-degradation `Option<AudioManager>` path confirmed, zero
  `.unwrap()` calls anywhere in `crates/audio/src/lib.rs` on the manager
  option.
- **Guards re-run live on HEAD `1ae86f62`**:

| Suite | Result |
|---|---|
| `cargo test -p byroredux-audio` | **19 passed, 0 failed, 6 ignored** (ignored = real-audio-device + vanilla-FNV-data tests, gated on hardware/game-data, not broken) |
| `cargo test -p byroredux --bin byroredux footstep` | **5 passed, 0 failed** |
| `cargo test -p byroredux --bin byroredux reverb` | **5 passed, 0 failed** |

**Shipped surface** (Phases 1–6, re-confirmed line-by-line): `AudioWorld`
graceful degradation (`Option<AudioManager<DefaultBackend>>`,
`SUB_TRACK_CAPACITY=512`/`SEND_TRACK_CAPACITY=32` above kira defaults);
`AudioListener`/`AudioEmitter`/`OneShotSound` (all `SparseSetStorage`);
`audio_system` (`sync_listener_pose` → `drain_pending_oneshots` →
`dispatch_new_oneshots` → `prune_stopped_sounds`); `load_sound_from_bytes` +
`SoundCache` (case-insensitive path keys, manual `clear()`-only eviction);
spatial sub-track playback via both the entity (`OneShotSound`+`AudioEmitter`)
and queue (`play_oneshot`, `VecDeque` cap 256, drop-oldest via `pop_front`)
paths; looping emitters + tweened-`stop()` despawn truncation (looping AND
non-looping, `stop_issued` debounce); single-slot streaming music on the main
(non-spatial) track; global reverb send track (`feedback 0.85`/`damping
0.6`/`stereo_width 1.0`/`Mix::WET`, `f32::NEG_INFINITY` dry default, `>-60.0`
gate). Engine consumers: `footstep_system` (the only `play_oneshot` caller)
and `reverb_zone_system` (the only `set_reverb_send_db` caller).

**Pending (future-phase, not flagged as missing)**: Phase 3.5b FOOT → per-
material sound, REGN ambient soundscapes, MUSC routing, per-cell-acoustics
reverb (detector is binary interior/exterior only), raycast occlusion
attenuation.

**MUSC parse→play gap (confirmed still absent, by design)**: cell-music
FormIDs are parsed (`default_music`/ZNAM, `music_type_form`/XCMO in
`crates/plugin/src/esm/cell/`) but no engine caller invokes `play_music` —
`grep play_music byroredux/` returns zero hits, re-confirmed live this cycle.
Single-slot / main-track invariants remain pinned for the eventual caller.

---

## Lifecycle Invariant Matrix

Owned by Dimension 6, with pointers from Dims 1/4/5 collapsed here per the
skill's dedup instruction. All re-derived independently this cycle.

| Invariant | State | Anchor |
|---|---|---|
| `AudioWorld` field-drop order (`active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager` → `multi_listener_warned`) | HOLDS | `lib.rs:234-274` |
| Manager capacities exceed kira defaults (`SUB_TRACK_CAPACITY=512`, `SEND_TRACK_CAPACITY=32`) | HOLDS | `lib.rs:170-171`, applied `lib.rs:288-295` |
| Sticky listener (never cleared on entity churn) | HOLDS | `sync_listener_pose`, `lib.rs:699-761` |
| `ActiveSound._track` held for Drop side-effect (underscore-name intact) | HOLDS | `lib.rs:190`, `826-829`, `961-964` |
| Two dispatch paths gate identically on `listener_id` + reverb-send gate | HOLDS, byte-identical | `lib.rs:769`/`852` (listener gate), `805-809`/`923-927` (send gate) |
| `looping`/`loop_region(..)` applied ONLY in the entity path | HOLDS | `lib.rs:944-950`; `PendingOneShot` (`213-218`) has no `looping` field at all — structurally incapable |
| Volume→dB conversion centralized (`linear_volume_to_db`) | HOLDS | `lib.rs:144-150`, called from `817`, `942`, `454` |
| `Arc<StaticSoundData>` clone is cheap (Arc-share, not PCM deep-copy) | HOLDS, verified against vendored `kira-0.10.8` source | `frames: Arc<[Frame]>`, `#[derive(Clone)]` |
| Drain cap (>32/tick warns), producer cap (256, `VecDeque::pop_front`), drain-gate-before-`mem::take` | HOLDS | `lib.rs:789-796` (warn), `394-421` (cap), `785-788` (gate-before-take) |
| Despawn truncation — tweened `stop()` on emitter removal, looping + non-looping, `stop_issued` debounce | HOLDS | `prune_stopped_sounds`, `lib.rs:987-1064` |
| `SoundCache` lowercase-once, `clear()` doesn't invalidate live `Arc`s, dormant (`grep SoundCache byroredux/` = 0 hits) | HOLDS | `lib.rs:1207/1215/1228` (lowercase), `1267` (clear); `try_load_default_footstep` bypasses cache |
| Single-slot music, main-track (not spatial), streaming (not buffered) types, fade-then-drop `stop_music` | HOLDS | `lib.rs:249` (field), `435-465` (`play_music`), `1134-1149` (streaming loaders), `475` (`fade_out_secs.max(0.0)`) |
| Reverb send-track creation None-safe, default `NEG_INFINITY`, `>-60.0` gate | HOLDS | `lib.rs:319-337`, `343` |
| Scheduler stages/order (`PostUpdate` footstep → `Late` reverb-then-audio) | HOLDS, structurally guaranteed | `boot.rs:824` (footstep, `PostUpdate`), `boot.rs:1033-1057` (reverb/audio, `Late`); `Scheduler::run` (`crates/core/src/ecs/scheduler.rs:~475-514`) runs the entire parallel batch before any exclusive system, so `reverb_zone_system` (parallel) always completes before `audio_system` (exclusive) regardless of registration order |
| `AudioWorld::new()` called exactly once, at boot | HOLDS | single call site, `boot.rs:375` |
| Camera `AudioListener` + `FootstepEmitter` opt-in, component-driven | HOLDS (line shifted 445-449 → 575-583 by unrelated door-spawn work, content identical) | `scene.rs:579,583`, inside `setup_scene` |
| `try_load_default_footstep` no-ops cleanly, bypasses `SoundCache` | HOLDS, byte-unchanged | `byroredux/src/asset_provider/texture.rs:79-119` |
| `footstep_system` docstring accurately cites `scene.rs::setup_scene` | **FIXED this cycle** (was AUD-2026-07-25-01, LOW) | `byroredux/src/systems/audio.rs:97-98` |
| `reverb_zone_system` constants, bit-equality gate, dual no-op safety | HOLDS | `byroredux/src/systems/audio.rs` (interior `-12.0`/exterior `NEG_INFINITY`, `to_bits()` gate) |

---

## Findings

**None.** All 7 dimensions returned zero new findings. This is a genuinely
clean cycle, not an artifact of skipped verification — every dimension agent
independently re-derived its invariants from live line numbers (confirmed via
git diff that `crates/audio/src/lib.rs` has had zero commits since the prior
audit's HEAD).

---

## Future-Phase Readiness (invariants pinned for the next phase)

- **FOOT / 3.5b (per-material footstep sound)**: `FootstepConfig.default_sound`
  decouple + `FootstepScratch` Vec-reuse survive unchanged; a producer can
  wire per-material sounds without touching `footstep_system`'s stride/seed/
  attenuation logic (`{0.5, 12.0}` tight falloff).
- **REGN (ambient soundscapes)**: sub-track capacity (512) still exceeds the
  ~400-emitter populated-interior projection; sticky-listener + despawn-
  truncation guards cover mass emitter churn on cell streaming.
- **MUSC routing**: single-slot / main-track / streaming-type invariants
  pinned; the eventual caller must gate on FormID equality (parse→play wiring
  confirmed absent — zero `play_music` callers in `byroredux/`).
- **SoundCache producer**: decoupled API + tests survive so the first
  consumer can land — but it MUST also wire eviction (no automatic LRU;
  `bytes_estimate` telemetry exists for the growth-regression signal but has
  no console-`stats` consumer yet — confirmed still dormant this cycle).
- **Reverb per-cell acoustics**: detector is binary interior/exterior; the
  bit-equality-gated transition (`reverb_zone_system`) is the extension point.
- **M42/M47 coexistence**: the seven PostUpdate AI-package locomotion systems
  plus the newer MQ101 cinematic/scripting systems added since the last cycle
  all sit disjoint from the audio stages (`Stage::Update`/`PostUpdate` vs.
  audio's `Stage::Late`); none reads or writes `AudioEmitter`/`AudioListener`/
  `OneShotSound`/`FootstepEmitter` — confirmed no cross-talk introduced by
  this cycle's scripting/animation churn.

---

## Delta vs prior report

This report supersedes `AUDIT_AUDIO_2026-07-25.md`. That cycle closed with one
LOW finding (AUD-2026-07-25-01). This cycle:

- Confirmed the audio crate (`crates/audio/src/lib.rs`) has had **zero**
  commits since the prior audit's HEAD (`ca7a4e0e`) — a true no-op interval
  for the crate itself.
- Confirmed AUD-2026-07-25-01 is fixed: the `footstep_system` docstring now
  correctly cites `scene.rs::setup_scene`.
- Re-verified all 7 dimensions independently against live source (not a
  delta-inference rubber stamp) despite the near-zero diff, per the
  methodology this subsystem's audits have followed since `_07-25`.
- Confirmed the two substantial adjacent-file diffs this cycle (`scene.rs`
  door-spawn rewrite, `texture.rs` cubemap work) do not touch the audio
  opt-in sites they sit next to.
- Zero new findings. All of #843–#859 and AUD-2026-07-25-01 remain
  regression-guarded / fixed; no regressions found.
