# Audio Subsystem Audit (M44) — 2026-07-14

- **Command**: `/audit-audio` → all 7 dimensions, `--depth deep`
- **Branch**: main · **HEAD**: `02088dd9` (2026-07-14)
- **kira**: pinned `0.10.8` (workspace `Cargo.toml` → `Cargo.lock`)
- **Method**: **Delta-driven regression check.** The audio crate and its
  engine-side consumers were diffed against the most recent prior report
  (`docs/audits/AUDIT_AUDIO_2026-07-03.md`, HEAD `8498e559`). Because the entire
  audio scope is behaviorally identical to that HEAD (see Delta Analysis), this
  pass verifies (a) the one structural change that touched audio wiring survived
  intact, (b) the single open finding is still open, and (c) every guard still
  passes **live on current HEAD** — rather than re-deriving the 7-dimension deep
  audit already completed 11 days ago against byte-identical source. Dedup
  baseline: `gh issue list` (27 open) + the full prior-report chain
  (`_2026-05-05` → `_06-14` → `_06-23` → `_07-02` → `_07-03`).

---

## Delta Analysis (why this is a zero-delta cycle)

Every file in audio scope, diffed `8498e559..HEAD`:

| File | Change since 2026-07-03 audit | Audio-relevant? |
|---|---|---|
| `crates/audio/src/lib.rs` | none (byte-identical) | — |
| `crates/audio/src/tests.rs` | none (byte-identical) | — |
| `byroredux/src/systems/audio.rs` | none (byte-identical) | — |
| `byroredux/src/asset_provider/texture.rs` | none (byte-identical) | — |
| `byroredux/src/components.rs` | +49 lines (`VisibleWhenDistant`, `SeatReservations`, `SandboxSitClip`) | **No** — VWD #1889 + M42 sandbox-seat work; zero audio structs (`FootstepEmitter`/`FootstepConfig`/`FootstepScratch`/`AudioEmitter`/`AudioListener`/`OneShotSound`) touched |
| `byroredux/src/main.rs` → `boot.rs` | **#1858** split `main.rs` into `boot.rs` + `app_step.rs` | **Structural only** — the audio scheduler block was relocated verbatim (see below) |

**The only change touching audio wiring is the #1858 file split**, and it moved
the registration block without altering it:

| System | Pre-split (`main.rs`) | Post-split (`boot.rs`) | Mechanism |
|---|---|---|---|
| `footstep_system` | `add_exclusive(Stage::PostUpdate, …)` @ :855 | :659 | **identical** |
| `reverb_zone_system` | `add_to_with_access(Stage::Late, reads CellLightingRes / writes AudioWorld)` @ :951 | :791 | **identical** |
| `audio_system` | `add_exclusive(Stage::Late, …)` @ :973 | :813 | **identical** |

Ordering invariants preserved: `PostUpdate` (footstep enqueue) precedes `Late`
(audio drain) → a footstep is heard the same tick; within `Late`,
`reverb_zone_system` (parallel batch) runs before `audio_system` (exclusive,
sequences after the parallel batch), so a spatial track built this tick picks up
this tick's reverb send level. The boot.rs comment block (`:783-812`) that
encodes the "MUST run BEFORE audio_system" dependency is intact.

---

## Executive Summary

**Zero new findings. Zero regressions.** The M44 crate (Phases 1–6) and its two
live engine consumers are byte-identical to the 2026-07-03 deep audit, which
verified all 7 dimensions with zero new findings; that verification therefore
still stands. The one structural change in scope (#1858 main→boot split)
relocated the audio scheduler registration verbatim — stages, order, and access
declarations unchanged.

- **CRITICAL / HIGH / MEDIUM**: 0
- **LOW**: 0 new — **1 pre-existing, tracked as #1859, still OPEN** (`SoundCache`
  docstring stale path). Re-confirmed open; **do not re-file**.
- **Headless-mode boot**: PASS — `audio_world_constructs_without_panic_on_any_environment`
  green; graceful-degradation `Option<AudioManager>` path unchanged.

**Guards re-run live on HEAD `02088dd9`** (not merely read):

| Suite | Result |
|---|---|
| `cargo test -p byroredux-audio` | **19 passed, 0 failed** |
| `cargo test -p byroredux footstep` | **5 passed, 0 failed** |
| `cargo test -p byroredux reverb` | **5 passed, 0 failed** |

**Shipped surface** (re-confirmed via the live API + prior report): Phases 1–6 —
`AudioWorld` graceful degradation, `AudioListener`/`AudioEmitter`/`OneShotSound`,
`audio_system`; `load_sound_from_bytes` + `SoundCache`; spatial sub-track
playback + `spawn_oneshot_at`; `play_oneshot` queue (`VecDeque`, cap 256,
drop-oldest via `pop_front`); looping emitters + tweened-`stop()` truncation with
`stop_issued` debounce; streaming music (single-slot, main-track); global reverb
send track (`feedback 0.85`/`damping 0.6`/`Mix::WET`, `NEG_INFINITY` dry default).
Engine consumers: `footstep_system` (the only `play_oneshot` caller),
`reverb_zone_system` (the only `set_reverb_send_db` caller).

**Pending (future-phase, not flagged as missing)**: Phase 3.5b FOOT → per-material
sound, REGN ambient soundscapes, MUSC routing, per-cell-acoustics reverb (detector
is binary interior/exterior only), raycast occlusion attenuation.

**MUSC parse→play gap (confirmed still absent, by design)**: cell-music FormIDs are
parsed (`default_music`/ZNAM, `music_type_form`/XCMO in `crates/plugin/src/esm/cell/`)
but no engine caller invokes `play_music` — `grep play_music byroredux/` returns
zero hits. The single-slot / main-track invariants remain pinned for the eventual
caller.

---

## Lifecycle Invariant Matrix

All owned by Dim 6 (single source of truth); verified unchanged since 07-03.

| Invariant | State | Anchor |
|---|---|---|
| `AudioWorld` field-drop order (`active_sounds` → `pending_oneshots` → `music` → `reverb_send` → `reverb_send_db` → `listener` → `manager`) | HOLDS | struct decl order, `lib.rs` |
| Manager capacities exceed kira defaults (`SUB_TRACK_CAPACITY=512`, `SEND_TRACK_CAPACITY=32`) | HOLDS | `manager_capacities_exceed_kira_defaults` |
| Sticky listener (never cleared on entity churn) | HOLDS | `#849` guard |
| `ActiveSound._track` held for Drop side-effect (underscore-name intact) | HOLDS | `lib.rs` |
| Despawn truncation — tweened `stop()` on emitter removal, looping + non-looping, `stop_issued` debounce | HOLDS | `looping_emitter_survives…`, `non_looping_emitter_stops_on_emitter_remove_regression_858` |
| Queue path — `VecDeque` cap 256 drop-oldest, manager-`None` up-front drop, active-gate before `mem::take` | HOLDS | `#851`/`#852`/`#853` guards |
| Scheduler stages/order (PostUpdate → Late; reverb before audio in Late) | HOLDS (relocated verbatim by #1858) | `boot.rs:659/791/813` |

---

## Findings

**No new findings.**

### AUD-2026-07-02-01 (re-confirmed, NOT re-reported as new): `SoundCache` docstring stale path
- **Severity**: LOW (doc rot)
- **Dimension**: SoundCache Growth
- **Status**: **Existing: #1859 (OPEN)** — verified via `gh issue view 1859`.
- **Description**: The `crates/audio/src/lib.rs` module docstring still cites
  `try_load_default_footstep`'s home as *byroredux/src/asset_provider.rs* — a
  pre-Session-34 path that is now a directory; the live location is
  `byroredux/src/asset_provider/texture.rs::try_load_default_footstep`. The
  function name itself is correct (post-#1615); only the file path rots.
- **Action**: None here. Tracked as #1859; `/audit-publish` must not re-file it.

---

## Future-Phase Readiness (invariants pinned for the next phase)

- **FOOT / 3.5b (per-material footstep sound)**: the `FootstepConfig.default_sound`
  decouple + `FootstepScratch` Vec-reuse (`#932`) survive; a producer can wire
  per-material sounds without touching `footstep_system`'s stride/seed/attenuation
  logic (`{0.5, 12.0}` tight falloff).
- **REGN (ambient soundscapes)**: sub-track capacity (512) already exceeds the
  ~400-emitter populated-interior projection; the sticky-listener + despawn-truncation
  guards cover mass emitter churn on cell streaming.
- **MUSC routing**: single-slot / main-track / streaming-type invariants pinned;
  the eventual caller must gate on FormID equality (parse→play wiring confirmed absent).
- **SoundCache producer**: decoupled API + tests survive so the first consumer can
  land — but it MUST also wire eviction (no automatic LRU; `bytes_estimate`
  telemetry exists for the growth-regression signal).
- **Reverb per-cell acoustics**: detector is binary interior/exterior; the
  bit-equality-gated transition (`reverb_zone_system`) is the extension point.

---

## Prioritized Fix Order

Nothing to fix in this cycle. The single open item (#1859, LOW doc-rot) is already
filed and awaiting a `/fix-issue` pass; it is a one-line docstring path correction.

## Delta vs prior report

This report supersedes `AUDIT_AUDIO_2026-07-03.md` only in currency (re-verified
against HEAD `02088dd9`), not in content. All of #843–#859 remain regression-guarded
in `crates/audio/src/tests.rs` + `byroredux/src/systems/audio.rs`; the sole open
finding (#1859) is unchanged.
