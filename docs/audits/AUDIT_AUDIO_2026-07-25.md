# Audio Subsystem Audit (M44) — 2026-07-25

- **Command**: `/audit-audio` → all 7 dimensions, `--depth deep` (one leg of a
  `comprehensive` audit-suite sweep)
- **Branch**: main · **HEAD**: `ca7a4e0e` (2026-07-25)
- **kira**: pinned `0.10.8` (workspace `Cargo.toml` → `Cargo.lock`, unchanged;
  verified `StaticSoundData` struct + doc comment directly in the vendored
  registry source, see Dimension 1)
- **Method**: Full re-verification, not a rubber-stamped delta. Every
  dimension's checklist was checked against the live `crates/audio/src/lib.rs`
  (read in full, 1293 lines), `byroredux/src/systems/audio.rs` (read in full,
  561 lines), the relevant slices of `byroredux/src/components.rs`,
  `byroredux/src/scene.rs`, `byroredux/src/boot.rs`, `byroredux/src/main.rs`,
  and `byroredux/src/asset_provider/texture.rs`. A `git diff` against the prior
  audit's HEAD (`c3e09bb5`, `docs/audits/AUDIT_AUDIO_2026-07-16.md`) was used
  to scope where real behavioral change could exist, then every dimension's
  invariants were independently re-derived from source rather than assumed
  carried-forward. Dedup baseline: `gh issue list` (29 open, zero audio-keyword
  matches) + the full prior-report chain (`_05-05` → `_06-14` → `_06-23` →
  `_07-02` → `_07-03` → `_07-14` → `_07-16`).

---

## Delta Analysis (`c3e09bb5..HEAD`, scope-relevant files only)

| File | Change | Audio-relevant? |
|---|---|---|
| `crates/audio/src/lib.rs` | 2 hunks, 4 lines — `cargo fmt` re-wrapping `SpatialTrackBuilder::new().distances(...)` onto one line in both `drain_pending_oneshots` and `dispatch_new_oneshots` | **No** — cosmetic reformat only, confirmed via `git diff`; both call sites are behaviorally byte-identical (same builder call, same argument) |
| `crates/audio/src/tests.rs` | none | — |
| `byroredux/src/systems/audio.rs` | none | — |
| `byroredux/src/asset_provider/texture.rs` | none | — |
| `byroredux/src/components.rs` | +101/−9 — new `IsDecalMesh` marker + `decal_uses_implicit_alpha_blend` classifier (FO4 decal work) | **No** — zero audio structs touched; `FootstepEmitter`/`FootstepConfig`/`FootstepScratch` byte-identical |
| `byroredux/src/scene.rs` | +131/−45 — door-spawn floor-probe rewrite (#2013 follow-up: capsule-shaped downward probe instead of trusting door height) | **No** — the `FootstepEmitter::new()` / `AudioListener` camera opt-in at `scene.rs:445-449` is untouched; the diff is entirely inside the door-teleport spawn-Y branch above it |
| `byroredux/src/boot.rs` | +57/−22 — `--bench-camera` deterministic path flag, `RendererConfig` plumbing, sandbox/wander/travel/follow/escort/guard/patrol systems switched to `make_*_system()` factory functions | **No** — `footstep_system` (`:735`), `reverb_zone_system` (`:946`), `audio_system` (`:968`) registrations are unchanged in content; only line numbers shifted from unrelated insertions earlier in the file |
| `byroredux/src/main.rs` | +182/−34 — presentation-pass / FSR upscaler runtime-switch plumbing | **No** — no audio symbol appears in the diff |

**Net: zero behavioral change to any of the 7 audit dimensions.** All churn
this cycle is renderer (FSR/upscaler), physics (door-spawn floor probe), and
FO4-decal work — none of it touches the audio crate's logic or its two live
engine consumers.

---

## Executive Summary

**Zero CRITICAL / HIGH / MEDIUM findings. One new LOW finding (doc-rot,
pre-existing since the feature's introduction, missed by six prior audit
cycles of this exact subsystem).**

- **Headless-mode boot**: PASS — `audio_world_constructs_without_panic_on_any_environment`
  green; graceful-degradation `Option<AudioManager>` path confirmed, zero
  `.unwrap()` calls anywhere in `crates/audio/src/lib.rs` on the manager option.
- **Guards re-run live on HEAD `ca7a4e0e`** (not merely read):

| Suite | Result |
|---|---|
| `cargo test -p byroredux-audio` | **19 passed, 0 failed, 6 ignored** (ignored = real-audio-device + vanilla-FNV-data tests, confirmed via their `#[ignore]` doc comments — not broken, gated on hardware/game-data) |
| `cargo test -p byroredux footstep` | **5 passed, 0 failed** |
| `cargo test -p byroredux reverb` | **5 passed, 0 failed** |

**Shipped surface** (re-confirmed by reading the live API top to bottom, not
by trusting the docstring): Phases 1–6 — `AudioWorld` graceful degradation
(`Option<AudioManager<DefaultBackend>>`, `SUB_TRACK_CAPACITY=512`/
`SEND_TRACK_CAPACITY=32` above kira defaults), `AudioListener`/`AudioEmitter`/
`OneShotSound` (all `SparseSetStorage`), `audio_system` (`sync_listener_pose`
→ `drain_pending_oneshots` → `dispatch_new_oneshots` → `prune_stopped_sounds`);
`load_sound_from_bytes` + `SoundCache` (case-insensitive path keys, manual
`clear()`-only eviction); spatial sub-track playback via both the entity
(`OneShotSound`+`AudioEmitter`) and queue (`play_oneshot`, `VecDeque` cap 256,
drop-oldest via `pop_front`) paths; looping emitters + tweened-`stop()`
despawn truncation (looping AND non-looping, `stop_issued` debounce); single-
slot streaming music on the main (non-spatial) track; global reverb send
track (`feedback 0.85`/`damping 0.6`/`stereo_width 1.0`/`Mix::WET`,
`f32::NEG_INFINITY` dry default, `>-60.0` gate). Engine consumers:
`footstep_system` (the only `play_oneshot` caller — stride accumulation,
first-tick seed, `FootstepScratch` Vec-reuse, `{0.5, 12.0}` tight attenuation)
and `reverb_zone_system` (the only `set_reverb_send_db` caller — binary
interior/exterior detector, bit-equality-gated transition, `-12.0`/
`NEG_INFINITY`).

**Pending (future-phase, not flagged as missing)**: Phase 3.5b FOOT → per-
material sound, REGN ambient soundscapes, MUSC routing, per-cell-acoustics
reverb (detector is binary interior/exterior only), raycast occlusion
attenuation.

**MUSC parse→play gap (confirmed still absent, by design)**: cell-music
FormIDs are parsed (`default_music`/ZNAM, `music_type_form`/XCMO in
`crates/plugin/src/esm/cell/`) but no engine caller invokes `play_music` —
`grep play_music byroredux/` returns zero hits. Single-slot / main-track
invariants remain pinned for the eventual caller.

---

## Lifecycle Invariant Matrix

All re-derived live from source this cycle (not carried forward by
assertion). Owned by Dimension 6, with pointers from Dims 1/4/5 collapsed
here per the skill's dedup instruction.

| Invariant | State | Anchor |
|---|---|---|
| `AudioWorld` field-drop order (`active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager` → `multi_listener_warned`) | HOLDS | `lib.rs:234-274` struct decl, re-read live top-to-bottom |
| Manager capacities exceed kira defaults (`SUB_TRACK_CAPACITY=512`, `SEND_TRACK_CAPACITY=32`) | HOLDS | `lib.rs:170-171`, applied at `lib.rs:288-295`; `manager_capacities_exceed_kira_defaults` passing |
| Sticky listener (never cleared on entity churn) | HOLDS | `sync_listener_pose`, `lib.rs:699-761` — no clear-on-missing path exists |
| `ActiveSound._track` held for Drop side-effect (underscore-name intact, lands in `active_sounds` before helper returns) | HOLDS | `lib.rs:187-207`, `826-842`, `961-968` |
| Two dispatch paths both gate on `listener_id`, both apply the identical reverb-send gate (`is_finite() && > -60.0`) | HOLDS, no drift | `lib.rs:769`/`852` (gate), `lib.rs:805-809`/`923-927` (send gate) — byte-for-byte identical logic in both paths |
| `looping` / `loop_region(..)` applied ONLY in the entity path | HOLDS | `lib.rs:944-950` (entity path sets it); `drain_pending_oneshots` has no `loop_region` call anywhere |
| Volume→dB conversion centralized (no per-site drift) | HOLDS | single `linear_volume_to_db` fn (`lib.rs:144-150`), called from all 3 sites (`lib.rs:817`, `942`, `454`) — the historical 3-copy duplication (AUD-2026-06-23-01) stays fixed |
| `Arc<StaticSoundData>` clone is cheap (Arc-share, not PCM deep-copy) | HOLDS, verified against vendored kira source | `kira-0.10.8/src/sound/static_sound/data.rs:24-33` — `frames: Arc<[Frame]>`, struct doc: "these can be cheaply cloned, as the audio data is shared among all clones" |
| Despawn truncation — tweened `stop()` on emitter removal, looping + non-looping, `stop_issued` debounce | HOLDS | `prune_stopped_sounds`, `lib.rs:987-1064`; guards `looping_emitter_survives_natural_duration_and_stops_on_emitter_remove` / `non_looping_emitter_stops_on_emitter_remove_regression_858` (both `#[ignore]`, real-device-gated, confirmed present and correctly written) |
| Queue path — `VecDeque` cap 256 drop-oldest, manager-`None` up-front drop, active-gate before `mem::take` | HOLDS | `lib.rs:401-421` (cap), `lib.rs:775-788` (gate-before-take) |
| Scheduler stages/order (`PostUpdate` footstep → `Late` reverb-then-audio, `Stage` discriminant order `Early<Update<PostUpdate<Physics<Late`) | HOLDS (line numbers shifted by unrelated M42/bench-camera insertions, content identical) | `boot.rs:735` (footstep) / `:946` (reverb) / `:968` (audio); `Stage` enum, `crates/core/src/ecs/scheduler.rs:27-38` |
| `AudioWorld::new()` called exactly once, at boot | HOLDS | single call site, `boot.rs:363`; zero cell-transition / resize re-construction found |
| Camera `AudioListener` + `FootstepEmitter` opt-in | HOLDS (component-driven, no hardcoded camera assumption in the system) | `scene.rs:445-449`, inside `setup_scene`, untouched by the #2013 door-spawn diff in the same file |
| `try_load_default_footstep` no-ops cleanly on missing arg / open-fail / missing-file / decode-fail, bypasses `SoundCache` | HOLDS | `byroredux/src/asset_provider/texture.rs:79-119` |
| `footstep_system` doc pointer for the camera opt-in location | **DRIFTED — new LOW finding**, see AUD-2026-07-25-01 below | `byroredux/src/systems/audio.rs:97-98` |

---

## Findings

### AUD-2026-07-25-01: `footstep_system` docstring misattributes the fly-camera opt-in site to `main.rs::App::new`
- **Severity**: LOW
- **Dimension**: Gameplay Audio Wiring (Dimension 7)
- **Location**: `byroredux/src/systems/audio.rs:97-98`
- **Status**: NEW
- **Description**: The doc comment on `footstep_system` reads:
  ```
  /// Spawn a `FootstepEmitter` on the player entity to opt in. The
  /// fly-camera attach is wired in `main.rs::App::new`.
  ```
  This is factually wrong on two counts. First, the actual attach call
  (`world.insert(cam, crate::components::FootstepEmitter::new());`) lives in
  `byroredux/src/scene.rs:449`, inside `setup_scene` — not in `main.rs`.
  Second, even the *caller chain* doesn't reach `App::new`: `App::new` is
  `main.rs:259-402` (verified by full-body grep, zero `Footstep`/`footstep`
  hits) — a constructor. `setup_scene` is invoked from `App::setup_scene()`
  (`main.rs:369`), which is itself called from `ApplicationHandler::resumed`
  (`main.rs:856`, inside the winit event-loop callback), never from `App::new`.
  So the doc doesn't just point at the wrong file — it points at the wrong
  *kind* of call site (constructor vs. window-resume callback).
- **Evidence**: `git log -S"fly-camera attach is wired"` finds exactly one
  origin, commit `3987ecd1` ("M44 Phase 3.5: footstep gameplay loop..."),
  2026-05-05 — the *same commit* that introduced the attach call in
  `scene.rs` (verified via `git show 3987ecd1 -- byroredux/src/scene.rs`,
  which shows the `world.insert(cam, ...FootstepEmitter::new())` line landing
  in `scene.rs`, not `main.rs`, in that very commit). The docstring has been
  wrong since day one and has survived the `systems.rs` → `systems/audio.rs`
  module split (`2bdbc365`, 2026-05-12) and six subsequent `/audit-audio`
  cycles (`_05-05` through `_07-16`) without being caught — none of those
  audits' Dimension 7 checklists asked "does the docstring's claimed call
  site match the real one," only "is the opt-in component-driven."
- **Impact**: Purely a documentation-accuracy bug — the actual opt-in
  behavior is correct and component-driven (confirmed: `scene.rs:449` is a
  plain `world.insert`, no hardcoded camera-entity special-casing inside
  `footstep_system` itself). Impact is confined to future maintainers or
  audit passes who trust the docstring instead of grepping — they'd look in
  `main.rs::App::new` for the FootstepEmitter attach and not find it. Same
  class of bug as the already-fixed AUD-2026-07-02-01/#1859 (`SoundCache`
  docstring citing a stale path), just a different docstring in the same
  subsystem that no prior cycle happened to check.
- **Related**: Analogous to #1859 (closed). Not a regression of it — a
  distinct docstring, never previously flagged.
- **Suggested Fix**: Update the doc comment to `The fly-camera attach is
  wired in \`scene.rs::setup_scene\`.` (one-line fix, matches the existing
  `SoundCache` docstring-fix pattern from #1859).

---

## Future-Phase Readiness (invariants pinned for the next phase)

- **FOOT / 3.5b (per-material footstep sound)**: `FootstepConfig.default_sound`
  decouple + `FootstepScratch` Vec-reuse (`#932`) survive unchanged; a
  producer can wire per-material sounds without touching `footstep_system`'s
  stride/seed/attenuation logic (`{0.5, 12.0}` tight falloff).
- **REGN (ambient soundscapes)**: sub-track capacity (512) still exceeds the
  ~400-emitter populated-interior projection; sticky-listener + despawn-
  truncation guards cover mass emitter churn on cell streaming.
- **MUSC routing**: single-slot / main-track / streaming-type invariants
  pinned; the eventual caller must gate on FormID equality (parse→play wiring
  confirmed absent — zero `play_music` callers in `byroredux/`).
- **SoundCache producer**: decoupled API + tests survive so the first
  consumer can land — but it MUST also wire eviction (no automatic LRU;
  `bytes_estimate` telemetry exists for the growth-regression signal but has
  no console-`stats` consumer yet).
- **Reverb per-cell acoustics**: detector is binary interior/exterior; the
  bit-equality-gated transition (`reverb_zone_system`) is the extension point.
- **M42 AI-package coexistence**: the seven PostUpdate locomotion systems
  (Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol, now built via
  `make_*_system()` factories per this cycle's `boot.rs` diff) sit in the
  same exclusive lane as `footstep_system`, registered around it, all opt-in
  via env vars (default off). None reads or writes `AudioEmitter`/
  `AudioListener`/`OneShotSound`/`FootstepEmitter` — confirmed no cross-talk
  as this AI work continues to expand, and the `make_*_system()` factory
  refactor this cycle didn't change that.

---

## Prioritized Fix Order

1. **AUD-2026-07-25-01** (LOW) — one-line docstring correction in
   `byroredux/src/systems/audio.rs:98`. Trivial, no functional risk.

---

## Delta vs prior report

This report supersedes `AUDIT_AUDIO_2026-07-16.md`. That cycle closed with
zero open findings. This cycle:

- Re-verified all 7 dimensions against live source (full-file reads of
  `lib.rs` and `systems/audio.rs`, not delta-inference) rather than treating
  the near-zero code diff as license to skip verification.
- Confirmed the sole in-scope code change since `c3e09bb5` (the `lib.rs`
  formatting diff) is behaviorally inert.
- Found one new LOW finding (AUD-2026-07-25-01) that predates this cycle by
  81 days but was never previously flagged — a docstring accuracy gap in
  `footstep_system`'s doc comment, structurally identical to the already-fixed
  #1859 but never checked by name in any prior cycle's Dimension 7 pass.
- All of #843–#859 remain regression-guarded in `crates/audio/src/tests.rs` +
  `byroredux/src/systems/audio.rs`; no regressions found.
