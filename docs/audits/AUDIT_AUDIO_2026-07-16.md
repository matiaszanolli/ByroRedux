# Audio Subsystem Audit (M44) — 2026-07-16

- **Command**: `/audit-audio` → all 7 dimensions, `--depth deep` (comprehensive
  audit-suite run, audit 8/21)
- **Branch**: main · **HEAD**: `c3e09bb5` (2026-07-16)
- **kira**: pinned `0.10.8` (workspace `Cargo.toml` → `Cargo.lock`, unchanged)
- **Method**: **Delta-driven regression check**, same methodology as the prior
  cycle. The audio crate and its two live engine consumers were diffed against
  the most recent prior report (`docs/audits/AUDIT_AUDIO_2026-07-14.md`, HEAD
  `02088dd9`). The diff in audio-relevant scope is a single one-line docstring
  fix (already tracked, now landed) plus zero-content line-number churn in
  `boot.rs` from unrelated M42.3–M42.8 AI-package work. Given a genuinely near-
  empty delta, this pass re-verifies the invariant matrix live against current
  HEAD (test suites re-run, key struct/field declarations re-read) rather than
  re-deriving a 7-dimension deep audit against source that is behaviorally
  unchanged. Dedup baseline: `gh issue list` (28 open, zero audio-keyword
  matches) + the full prior-report chain (`_05-05` → `_06-14` → `_06-23` →
  `_07-02` → `_07-03` → `_07-14`).

---

## Delta Analysis (why this is a near-zero-delta cycle)

Every file in audio scope, diffed `02088dd9..HEAD`:

| File | Change since 2026-07-14 audit | Audio-relevant? |
|---|---|---|
| `crates/audio/src/lib.rs` | **1 line** — `SoundCache` docstring path fix (commit `37394005`, "Fix #1859") | **Yes, but it's the fix the last audit was waiting on** — corrects `byroredux/src/asset_provider.rs` → `byroredux/src/asset_provider/texture.rs::try_load_default_footstep`. No behavioral change. |
| `crates/audio/src/tests.rs` | none (byte-identical) | — |
| `byroredux/src/systems/audio.rs` | none (byte-identical) | — |
| `byroredux/src/asset_provider/texture.rs` | none (byte-identical) | — |
| `byroredux/src/components.rs` | `SeatReservations` key widened `EntityId` → `(EntityId, u32)` (M42 seat-per-marker polish) | **No** — zero audio structs touched (`FootstepEmitter`/`FootstepConfig`/`FootstepScratch`/`AudioEmitter`/`AudioListener`/`OneShotSound` all absent from the diff) |
| `byroredux/src/scene.rs` | `apply_interior_cell_lighting` call site ungated (FNV-D1-01 fix) | **No** — the `FootstepEmitter::new()` camera opt-in at `scene.rs:449` is untouched |
| `byroredux/src/boot.rs` | +134/−12 — six new gated `add_exclusive(Stage::PostUpdate, …)` registrations for M42.3–M42.8 (Wander/Travel/Follow/Escort/Guard/Patrol), all opt-in behind env vars, none touching audio | **Line numbers only** — `footstep_system` (`:705`, was `:659`), `reverb_zone_system` (`:911-917`, was `:791`), `audio_system` (`:935`, was `:813`) shifted but the registration content, stage assignment, and relative order (`PostUpdate` footstep → `Late` reverb-then-audio) are **byte-identical in substance** |

**Net: one intentional fix (closes the last cycle's sole open finding) + one
unrelated file-line-shift.** No behavioral change to any of the 7 audit
dimensions.

---

## Executive Summary

**Zero new findings. Zero regressions. One prior finding closed.**

- **CRITICAL / HIGH / MEDIUM**: 0
- **LOW**: 0 open (down from 1) — **AUD-2026-07-02-01 / #1859 (`SoundCache`
  docstring stale path) is now FIXED** (commit `37394005`, 2026-07-14 23:29,
  same day as the last audit). Verified live: `crates/audio/src/lib.rs:1177`
  now reads `byroredux/src/asset_provider/texture.rs::try_load_default_footstep`.
  `gh issue list` confirms #1859 no longer appears among the 28 open issues.
- **Headless-mode boot**: PASS — `audio_world_constructs_without_panic_on_any_environment`
  green; graceful-degradation `Option<AudioManager>` path unchanged.

**Guards re-run live on HEAD `c3e09bb5`** (not merely read):

| Suite | Result |
|---|---|
| `cargo test -p byroredux-audio` | **19 passed, 0 failed, 6 ignored** (ignored = real-game-data tests, same as prior cycles) |
| `cargo test -p byroredux footstep` | **5 passed, 0 failed** |
| `cargo test -p byroredux reverb` | **5 passed, 0 failed** |

**Shipped surface** (re-confirmed via the live API): Phases 1–6 — `AudioWorld`
graceful degradation, `AudioListener`/`AudioEmitter`/`OneShotSound`,
`audio_system`; `load_sound_from_bytes` + `SoundCache`; spatial sub-track
playback + `spawn_oneshot_at`; `play_oneshot` queue (`VecDeque`, cap 256,
drop-oldest via `pop_front`); looping emitters + tweened-`stop()` truncation
with `stop_issued` debounce; streaming music (single-slot, main-track); global
reverb send track (`feedback 0.85`/`damping 0.6`/`Mix::WET`, `NEG_INFINITY` dry
default). Engine consumers: `footstep_system` (the only `play_oneshot`
caller), `reverb_zone_system` (the only `set_reverb_send_db` caller).

**Pending (future-phase, not flagged as missing)**: Phase 3.5b FOOT → per-
material sound, REGN ambient soundscapes, MUSC routing, per-cell-acoustics
reverb (detector is binary interior/exterior only), raycast occlusion
attenuation.

**MUSC parse→play gap (confirmed still absent, by design)**: cell-music
FormIDs are parsed (`default_music`/ZNAM, `music_type_form`/XCMO in
`crates/plugin/src/esm/cell/`) but no engine caller invokes `play_music` —
`grep play_music byroredux/` returns zero hits. The single-slot / main-track
invariants remain pinned for the eventual caller.

---

## Lifecycle Invariant Matrix

All owned by Dim 6 (single source of truth); re-verified live against current
HEAD source, not carried forward by assertion.

| Invariant | State | Anchor |
|---|---|---|
| `AudioWorld` field-drop order (`active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager`) | HOLDS | `lib.rs:237-267` struct decl, re-read live |
| Manager capacities exceed kira defaults (`SUB_TRACK_CAPACITY=512`, `SEND_TRACK_CAPACITY=32`) | HOLDS | `manager_capacities_exceed_kira_defaults` (passing) |
| Sticky listener (never cleared on entity churn) | HOLDS | `#849` guard |
| `ActiveSound._track` held for Drop side-effect (underscore-name intact) | HOLDS | `lib.rs`, unchanged |
| Despawn truncation — tweened `stop()` on emitter removal, looping + non-looping, `stop_issued` debounce | HOLDS | `looping_emitter_survives…`, `non_looping_emitter_stops_on_emitter_remove_regression_858` |
| Queue path — `VecDeque` cap 256 drop-oldest, manager-`None` up-front drop, active-gate before `mem::take` | HOLDS | `#851`/`#852`/`#853` guards, `play_oneshot_queue_caps_at_max_pending_when_active` passing |
| Scheduler stages/order (PostUpdate → Late; reverb before audio in Late) | HOLDS (line numbers shifted by #1858-adjacent M42 registrations, content identical) | `boot.rs:705` (footstep) / `:911-917` (reverb) / `:935` (audio) |
| Camera `FootstepEmitter` opt-in | HOLDS | `scene.rs:449`, untouched by the FNV-D1-01 lighting-gate fix in the same file |
| `SoundCache` docstring path | **FIXED** (was AUD-2026-07-02-01 / #1859) | `lib.rs:1177` |

---

## Findings

**No new findings. No open findings.**

The subsystem's only outstanding item from prior cycles (#1859, LOW doc-rot)
was fixed same-day as the last audit and is confirmed absent from the current
open-issue list and from the live docstring text.

---

## Future-Phase Readiness (invariants pinned for the next phase)

- **FOOT / 3.5b (per-material footstep sound)**: the `FootstepConfig.default_sound`
  decouple + `FootstepScratch` Vec-reuse (`#932`) survive; a producer can wire
  per-material sounds without touching `footstep_system`'s stride/seed/attenuation
  logic (`{0.5, 12.0}` tight falloff).
- **REGN (ambient soundscapes)**: sub-track capacity (512) already exceeds the
  ~400-emitter populated-interior projection; the sticky-listener + despawn-
  truncation guards cover mass emitter churn on cell streaming.
- **MUSC routing**: single-slot / main-track / streaming-type invariants
  pinned; the eventual caller must gate on FormID equality (parse→play wiring
  confirmed absent).
- **SoundCache producer**: decoupled API + tests survive so the first
  consumer can land — but it MUST also wire eviction (no automatic LRU;
  `bytes_estimate` telemetry exists for the growth-regression signal).
- **Reverb per-cell acoustics**: detector is binary interior/exterior; the
  bit-equality-gated transition (`reverb_zone_system`) is the extension point.
- **M42 AI-package coexistence**: the six new PostUpdate locomotion systems
  (Wander/Travel/Follow/Escort/Guard/Patrol) all sit in the same exclusive
  lane as `footstep_system`, registered after it, and are opt-in via env vars
  (default off). None reads or writes `AudioEmitter`/`AudioListener`/
  `OneShotSound`/`FootstepEmitter` — confirmed no cross-talk with the audio
  domain as this AI work continues to expand.

---

## Prioritized Fix Order

Nothing to fix in this cycle. Zero open findings.

## Delta vs prior report

This report supersedes `AUDIT_AUDIO_2026-07-14.md`. Change since that cycle:
the sole open finding (#1859) is now fixed and confirmed closed; all guards
re-run green on current HEAD; the only other in-scope diff is inert line-
number churn in `boot.rs` from unrelated M42 AI-package registrations. All of
#843–#859 remain regression-guarded in `crates/audio/src/tests.rs` +
`byroredux/src/systems/audio.rs`.
