# Audio Subsystem Audit (M44) — 2026-06-14

**Scope**: `crates/audio/src/lib.rs` (+ `crates/audio/src/tests.rs`), kira `0.10.8`,
and the engine-side consumers `byroredux/src/systems/audio.rs`
(`footstep_system`, `reverb_zone_system`), `byroredux/src/components.rs`
(`FootstepEmitter` / `FootstepConfig` / `FootstepScratch`),
`byroredux/src/asset_provider.rs` (`try_load_default_footstep`), and the
camera opt-in in `byroredux/src/scene.rs`.

**Depth**: deep (per-frame data-flow + lifecycle trace). All 7 dimensions covered.

---

## Executive Summary

The M44 audio crate (Phases 1–6) and its two live engine consumers are in good
shape. The prior audit `docs/audits/AUDIT_AUDIO_2026-05-05.md` (findings
AUD-D1-NEW-01 … AUD-D3-NEW-11, issues #842–#859) is fully addressed: every one
of those findings now lives as a documented invariant + regression guard in
`crates/audio/src/tests.rs` and `byroredux/src/systems/audio.rs`. None of them
has regressed.

**Shipped + live**:
- 3D spatial one-shot dispatch (entity path + queue path), both gated on
  `listener_id`, both routing through identical spatial-sub-track shape.
- Footstep gameplay loop (`footstep_system`, the only live `play_oneshot`
  caller) with XZ-stride accumulation, first-tick seed, `FootstepScratch` Vec
  reuse, lock-drop-before-AudioWorld ordering.
- Per-cell reverb send (`reverb_zone_system`, the only live `set_reverb_send_db`
  caller): `-12 dB` interior / `NEG_INFINITY` exterior, bit-equality gated.
- Streaming music single-slot crossfade API (`play_music` / `stop_music`),
  graceful-degradation manager, sticky listener, despawn-truncation with
  per-emitter `unload_fade_ms`.

**Pending / future-phase (correctly NOT flagged as missing)**:
- Phase 3.5b FOOT records → per-material footstep sound.
- REGN ambient soundscapes.
- **MUSC parse→play gap** — cell-music FormIDs ARE parsed
  (`default_music`/ZNAM and `music_type_form`/XCMO in
  `crates/plugin/src/esm/cell/mod.rs`), but there is **zero** `play_music`
  caller anywhere in `byroredux/` (grep confirms it is defined only in
  `crates/audio/src/lib.rs`). MUSC routing is future-phase.
- Raycast occlusion attenuation; per-cell-acoustics reverb (current detector is
  binary interior/exterior only).
- `SoundCache` is **dormant by design** (#859): never registered as a resource
  in the binary (`grep insert_resource.*SoundCache byroredux/` = 0 hits), zero
  call sites; `try_load_default_footstep` writes the decoded `Arc` straight into
  `FootstepConfig.default_sound`, bypassing the cache. Steady-state `len() == 0`.
- `AudioEmitter` is **never inserted** in the binary — only `AudioListener`
  (camera, `scene.rs:431`). The entity-path dispatch (`dispatch_new_oneshots`)
  has no live producer today; all live audio goes through the queue path.

**Headless-mode boot**: **PASS**. `AudioWorld::new` returns an audioless world
on `AudioManager::new` failure with no panic; every public API gates on
`manager.is_some()` / `is_active()`. Guard:
`audio_world_constructs_without_panic_on_any_environment`. Test run:
`cargo test -p byroredux-audio` = **18 passed, 0 failed, 6 ignored** (the 6 need
an audio device + on-disk FNV sound data).

**kira 0.10.8 contracts** — every type contract the crate depends on was
verified against the registry source (`~/.cargo/registry/.../kira-0.10.8/`):
- `ListenerHandle::set_position/set_orientation` and `AudioManager::add_listener`
  take `mint::Vector3<f32>` / `mint::Quaternion<f32>`; the `glam::Vec3`/`Quat`
  the code passes convert via glam's `mint` feature (`Cargo.toml:73`). Correct.
- `SpatialTrackBuilder::distances` accepts `RangeInclusive<f32>` (the code's
  `min..=max`); exclusive `Range` does NOT impl the conversion — the in-code
  comment is accurate.
- `with_send(id, f32)` treats the f32 as raw `Decibels`; `f32::NEG_INFINITY`
  resolves to clean `0.0` amplitude (≤ `Decibels::SILENCE` = -60) with no NaN.
- `Capacities` defaults: `sub_track_capacity = 128`, `send_track_capacity = 16`,
  `listener_capacity = 8`. The crate's overrides (512 / 32) and the "listener
  capacity is 8" comment are both correct.
- `state()` reports `Stopping` (NOT `Stopped`) during a `.stop(tween)` fade;
  `Stopped` is terminal — validates the `stop_issued` debounce design.

**Findings**: 0 CRITICAL · 0 HIGH · 1 MEDIUM · 3 LOW · 4 TOTAL.

**Delta vs prior report (2026-05-05)**: all 11 prior findings (#842–#859)
remain fixed + regression-guarded. The one new substantive finding
(AUD-2026-06-14-01) is a defense-in-depth gap the prior report did not surface —
a reversed-`Attenuation` panic path in kira's render thread. The other three are
doc-accuracy items.

---

## Lifecycle Invariant Matrix

| Invariant | Owner | Status |
|---|---|---|
| Field-drop order (`active_sounds → pending_oneshots → music → reverb_send → reverb_send_db → listener → manager`) | Dim 6 | **VERIFIED** — declaration order at `lib.rs:215-255` matches exactly; trailing `multi_listener_warned: bool` has no Drop impact. |
| `ActiveSound._track` held for Drop side-effect (underscore name kept) | Dim 1 | **VERIFIED** — `lib.rs:171`; lands in `active_sounds` before each helper returns (`:801`, `:943`). |
| Both dispatch paths gate on `listener_id`, early-return if absent | Dim 1 | **VERIFIED** — `drain_pending_oneshots:740`, `dispatch_new_oneshots:827`. |
| Drain-ordering: manager gate BEFORE `mem::take` (#851) | Dim 1 | **VERIFIED** — gate at `:756`, take at `:759`. |
| Producer queue cap = `VecDeque::pop_front` O(1) at 256 + up-front manager-None drop (#852/#853) | Dim 1 | **VERIFIED** — `play_oneshot:382` (None drop), `:394` (`pop_front`). |
| Loop applied entity-path only; queue path never loops | Dim 1 | **VERIFIED** — `loop_region(..)` only at `:931`; queue path has no loop. |
| Volume→dB (`20·log10`, -60 clamp) identical across 3 copies | Dim 1 | **VERIFIED** — `play_music:435`, `drain:788`, `dispatch:920` byte-identical; no drift. |
| Entity path reads `GlobalTransform` (post-propagation), not `Transform` | Dim 1 | **VERIFIED** — `:855`,`:870`. |
| Sticky listener — never cleared on entity churn (#849) | Dim 2 | **VERIFIED** — `sync_listener_pose` early-returns on missing entity; no `listener = None`. |
| Multi-listener warn debounced (#843) | Dim 2 | **VERIFIED** — `multi_listener_warned` set once at `:697`. |
| `add_listener` failure leaves `listener = None`, retries next frame | Dim 2 | **VERIFIED** — `:724-726` logs WARN, no assignment; lazy re-create idempotent. |
| `set_position`/`set_orientation` use `Tween::default()` (smooth follow) | Dim 2 | **VERIFIED** — `:729-730`. |
| Reverb send default = `NEG_INFINITY`; `with_send` gate `finite && > -60` | Dim 5 | **VERIFIED** — default `:324`; gate identical in both paths `:777`,`:899`. |
| Despawn truncation via `unload_fade_ms` for looping AND non-looping (#845/#858) | Dim 6 | **VERIFIED** — `prune_stopped_sounds:996-1022`; `stop_issued` debounce at `:990`,`:1021`; queue-driven (`entity==None`) exempt at `:993`. |
| Stage order: `reverb_zone_system` (Late) before `audio_system` (Late); `footstep_system` (PostUpdate) before audio | Dim 6/7 | **VERIFIED** — `main.rs:878` (reverb), `:902` (audio); `:794` (footstep, PostUpdate). |
| `audio_system` body order: listener → drain → dispatch → prune | Dim 6 | **VERIFIED** — `lib.rs:646-649`. |
| `FootstepScratch` capacity restored on success AND AudioWorld-absent bail (#932) | Dim 7 | **VERIFIED** — `systems/audio.rs:174-176` (bail), `:197-199` (success). |
| First-tick seed without firing (#848) | Dim 7 | **VERIFIED** — `:140-144`; guard `first_tick_seeds_last_position_without_firing`. |
| Stride reset-to-zero on fire (not subtract-remainder) | Dim 7 | **VERIFIED** — `:152`; guard `single_large_jump_fires_one_footstep_only`. |
| `reverb_zone_system` bit-equality gate, safe no-op on absent resources (#846) | Dim 7 | **VERIFIED** — `:67`; guards `no_cell_lighting_resource_is_safe_noop`, `no_audio_world_is_safe_noop`. |

---

## Findings

### AUD-2026-06-14-01: Reversed `Attenuation` (`min_distance > max_distance`) panics in kira's audio render thread — no `min <= max` validation at any dispatch boundary
- **Severity**: MEDIUM
- **Dimension**: Listener Pose & Attenuation
- **Location**: `crates/audio/src/lib.rs:770` (`drain_pending_oneshots`), `:892` (`dispatch_new_oneshots`), `:535-551` (`Attenuation` struct + `Default`), `:1057-1087` (`spawn_oneshot_at`)
- **Status**: NEW
- **Description**: Both dispatch paths build `SpatialTrackBuilder::distances(att.min_distance..=att.max_distance)` directly from a caller-supplied `Attenuation`, with no `min <= max` check. kira computes attenuation as `distance.clamp(self.min_distance, self.max_distance)` (`kira-0.10.8/src/track/sub/spatial_builder.rs:356`). Rust's `f32::clamp` **panics if `min > max`** (std contract). The panic fires on kira's audio render thread the first time the listener is within range of the source — i.e. at playback, not at dispatch, so it is invisible to the call site. Neither `Attenuation` construction, `spawn_oneshot_at`, `play_oneshot`, nor `AudioEmitter` validate the ordering. The Dim 2 checklist explicitly requires "`min <= max` is never violated by a caller"; today nothing enforces it.
- **Evidence**:
  - `lib.rs:892`: `.distances(p.attenuation.min_distance..=p.attenuation.max_distance)` — value passed verbatim.
  - kira `spatial_builder.rs:356`: `let distance = distance.clamp(self.min_distance, self.max_distance);` — `clamp` panics on `min > max`.
  - Repo-wide: only TWO live `Attenuation` constructions exist — footstep `{0.5, 12.0}` (`systems/audio.rs:183`) and `Default {2.0, 30.0}` — both well-ordered. No `debug_assert!(min <= max)` exists anywhere (`grep` = 0 hits).
- **Impact**: No current code path triggers it (both live callers pass ordered ranges), so this is a latent defense-in-depth gap rather than a live crash. But the public API (`Attenuation`, `play_oneshot`, `spawn_oneshot_at`, `AudioEmitter.attenuation`) accepts a reversed range silently and converts it into a hard panic deep in the audio thread — a hostile failure mode for the eventual FOOT/REGN/scripted producers that will set per-emitter attenuation from data. Blast radius: whole audio thread (process abort).
- **Related**: kira-side; Dim 2 checklist "`min <= max` never violated"; AUD-D1-NEW-01 (capacity, prior, fixed) is adjacent but distinct.
- **Suggested Fix**: Add a `debug_assert!(min_distance <= max_distance)` (or a clamping normalization `let (lo, hi) = (min.min(max), min.max(max))`) at both dispatch sites, or validate once in `Attenuation` construction. A clamp-normalize is the more robust choice since the producer may be data-driven.

### AUD-2026-06-14-02: `SoundCache::bytes_estimate` telemetry docstrings claim present-tense `stats` wiring that does not exist
- **Severity**: LOW
- **Dimension**: SoundCache Growth
- **Location**: `crates/audio/src/lib.rs:1149-1150`, `:1257-1259` (docstrings); fn `bytes_estimate` at `:1260`
- **Status**: NEW
- **Description**: The `SoundCache` and `bytes_estimate` docstrings state "`bytes_estimate` surfaces the cache footprint to telemetry so a future unbounded-growth regression shows up in `stats` output rather than at OOM." There is no such wiring: `bytes_estimate` (and `len` / `active_sound_count` / `pending_oneshot_count`) have **zero non-test call sites** in the binary (`grep` across `byroredux/`, `crates/debug-server`, `crates/debug-ui`, `commands.rs` = nothing outside tests/asserts). `SoundCache` is not even registered as a resource. The `stats` console command contains no audio line (`grep audio|reverb|sound|footstep commands.rs` = 0). The "surfaces to telemetry" claim is forward-looking written in present tense.
- **Evidence**: `commands.rs` has no audio telemetry; `bytes_estimate` only referenced by `tests.rs::sound_cache_clear_drops_entries_and_bytes_estimate_tracks_pcm_size`. `SoundCache` never `insert_resource`- d.
- **Impact**: Doc-accuracy only — the safety claim ("regression shows up in telemetry, not at OOM") is not actually true today. Since `SoundCache` is dormant by design (#859, accepted) the practical risk is nil, but the docstring overstates the present state. Low risk of misleading a future maintainer into believing the telemetry guard already protects them.
- **Related**: #859 (dormant `SoundCache`, accepted); #850 (no LRU, accepted). Dim 3 checklist: "Verify it's wired to a `stats`-style console path (or flag as dead if not)" — flagged as not wired.
- **Suggested Fix**: Soften the docstrings to future tense ("intended to surface ... once a `stats` consumer wires `SoundCache`"), OR wire `bytes_estimate` + `len` into the `stats` console output when the first `SoundCache` consumer lands (per the skill's "flag if anyone wires the first consumer WITHOUT also wiring eviction/telemetry").

### AUD-2026-06-14-03: Crate "Future phases" docstring block lists Phase 4/5/6 as unshipped, colliding with the shipped-phase sections above it
- **Severity**: LOW
- **Dimension**: Manager Lifecycle (docstring integrity)
- **Location**: `crates/audio/src/lib.rs:107-113`
- **Status**: NEW
- **Description**: The module docstring documents Phases 4, 5, and 6 as shipped ("this commit") in dedicated sections (lines 65-105), but the trailing `# Future phases (not in this commit)` block (107-113) still lists "Phase 4: REGN ambient", "Phase 5: MUSC + hardcoded music routing", "Phase 6: Reverb zones ... raycast occlusion". The phase numbers were never renumbered when those phases shipped, so the future-phase numbers collide with the shipped-phase numbers. The actual remaining work is 3.5b FOOT, REGN, MUSC routing, and occlusion — but a reader cross-referencing "Phase 6" gets contradictory answers (shipped reverb send vs. unshipped reverb zones).
- **Evidence**: Lines 90-105 (`# Phase 6 (this commit)` — reverb send, shipped) vs. line 112 (`Phase 6: Reverb zones ... raycast occlusion`, listed as future). `docs/feature-matrix.md:101` marks Phases 1–6 complete; FOOT/REGN are the only ✗ rows.
- **Impact**: Doc-accuracy only. The skill's Phase-5 setup step states a docstring/API drift "is a finding in itself." No runtime impact.
- **Related**: AUD-2026-06-14-04 (same docstring family); `docs/feature-matrix.md` is the authoritative status table and is correct.
- **Suggested Fix**: Renumber the `# Future phases` block to drop the shipped-phase numbers — list the remaining work by name (FOOT/3.5b material sounds, REGN ambient, MUSC routing, occlusion attenuation, per-cell acoustic reverb) without reusing 4/5/6.

### AUD-2026-06-14-04: `SoundCache` docstring references stale `resolve_footstep_sound`; live fn is `try_load_default_footstep`
- **Severity**: LOW
- **Dimension**: Gameplay Audio Wiring (docstring integrity)
- **Location**: `crates/audio/src/lib.rs:1157` (docstring); live fn at `byroredux/src/asset_provider.rs:410`
- **Status**: NEW (the doc-rot is pre-existing and was pre-flagged in the audit-audio skill itself; recorded here for completeness — LOW)
- **Description**: The `SoundCache` docstring (#859 note) says "The footstep dispatch path at `byroredux/src/asset_provider.rs::resolve_footstep_sound` writes directly into `FootstepConfig.default_sound`." No function named `resolve_footstep_sound` exists; the live function is `try_load_default_footstep` (`asset_provider.rs:410`), and it does exactly what the docstring describes (writes the decoded `Arc` into `FootstepConfig.default_sound`, bypassing the cache).
- **Evidence**: `grep resolve_footstep_sound` matches only the docstring at `lib.rs:1157`; the actual loader is `try_load_default_footstep` (`asset_provider.rs:410`), called from `main.rs:554`.
- **Impact**: Doc-accuracy only; the described behavior is correct, only the symbol name is stale. The path-reference does not use backticks-as-existence in a way that breaks the validate gate (it's prose), but the symbol is wrong.
- **Related**: AUD-2026-06-14-03; skill Phase-1 step 5 pre-flagged this exact rot.
- **Suggested Fix**: One-word edit — `resolve_footstep_sound` → `try_load_default_footstep` at `lib.rs:1157`.

---

## Future-Phase Readiness (invariants pinned by this audit)

- **Phase 3.5b FOOT / per-material sounds**: the queue path (`play_oneshot` →
  `drain_pending_oneshots`) is the production-ready dispatch surface; the
  spatial-sub-track shape, listener gate, and reverb-send gate are all in place
  for a per-material producer to plug in. **Caveat**: AUD-2026-06-14-01 — a
  data-driven FOOT producer that emits per-material `Attenuation` from records
  must not be allowed to feed a reversed range; harden the dispatch boundary
  before that producer lands.
- **REGN ambient**: looping-emitter lifecycle (`looping` flag → `loop_region`,
  despawn-truncation via `unload_fade_ms`, `stop_issued` debounce) is complete
  and guarded. A REGN producer should insert `AudioEmitter { looping: true, .. }`
  entities — the entity path (`dispatch_new_oneshots`) is currently producer-less
  but fully implemented and tested.
- **MUSC routing**: single-slot main-track invariant + crossfade are pinned;
  `play_music` / `stop_music` / `is_music_active` are correct and tested. The
  eventual caller MUST gate re-play on FormID equality (re-loading the same
  `StreamingSoundHandle` re-decodes + re-streams). FormIDs are already parsed
  (`default_music`/ZNAM, `music_type_form`/XCMO in `crates/plugin/src/esm/cell/`).
- **Per-cell acoustic reverb**: today's detector is binary interior/exterior.
  The `set_reverb_send_db` "next-dispatch knob, not live fader" limitation (#847)
  is a kira-0.10 constraint (no retroactive send-level setter); a future per-cell
  acoustic handler must re-dispatch long-running ambients to apply a new level.
- **`SoundCache` eviction**: the cache is dormant + safe (`clear()` does not
  invalidate `Arc`s held by live `ActiveSound`s — kira holds its own clone).
  Anyone wiring the first real consumer must wire eviction (and telemetry — see
  AUD-2026-06-14-02) at the same time.

---

## Dedup Status

- GitHub baseline: `/tmp/audit/audio/issues.json` (200 most-recent, no OPEN
  audio issues) + `issues_all.json` (400, state=all). Prior issues #842–#859
  are outside the 400-window (long-closed); all confirmed fixed + guarded via
  `crates/audio/src/tests.rs` (18 guards) and the prior report
  `docs/audits/AUDIT_AUDIO_2026-05-05.md`.
- Open issues touching audio keywords: **none** (#1445/#1333 are NIF particle
  emitter, not audio).
- All four findings above are NEW (the doc-rot items were not previously
  ticketed; AUD-2026-06-14-04 was pre-flagged inside the audit skill but never
  filed).

## Methodology Note

Every finding was re-derived from the live tree on 2026-06-14 and adversarially
checked: the kira 0.10.8 type contracts were verified against the registry
source (not assumed), `cargo test -p byroredux-audio` was run green, and the
`f32::clamp` panic path was confirmed by reading
`kira-0.10.8/src/track/sub/spatial_builder.rs:356`. Field-drop order, stage
order, volume→dB clamp consistency, sticky-listener, despawn-truncation, and the
`FootstepScratch` capacity-restore were all re-confirmed and produced no finding
(they are correct).

## Suggested Next Action

```
/audit-publish docs/audits/AUDIT_AUDIO_2026-06-14.md
```
