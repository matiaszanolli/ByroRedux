# Audio Subsystem Audit (M44) — 2026-10-31

**HEAD**: 670f81917 · **Baseline**: [`AUDIT_AUDIO_2026-10-27.md`](AUDIT_AUDIO_2026-10-27.md) (HEAD `fa6be4a6b`, 136 commits back) · **Audited**: Dim 5 delta (asset_provider/audio.rs, script_requests.rs, new-caller re-grep), Dim 4 delta (`boot/schedule/late.rs`) · **Unchanged since baseline (skimmed)**: Dim 1 (Spatial Dispatch), Dim 2 (Send Graph), Dim 3 (Music & SoundCache). `crates/audio/` has zero commits in `fa6be4a6b..HEAD`.

- **Command**: `/audit-audio` (delta scope), one leg of `/audit-suite --preset comprehensive`. Run solo, no sub-agents, no engine launch, no audio device opened.
- **Tests** (repo `target/`, `CARGO_INCREMENTAL=0`):
  - `cargo test -q -p byroredux-audio`: **52 passed, 0 failed, 9 ignored** (same as baseline).
  - `cargo test -q -p byroredux --bin byroredux -- audio dialogue_voice footstep impact scheduler_access combat_anim scripted_sound script_requests music`: **114 passed, 0 failed, 2 ignored**. The baseline's one failure (the `render_one_frame` size pin) is gone.
  - Device-gated `#[ignore]` lifecycle guards not run (suite rule). Unchanged code, so the baseline's trace-based verification stands.
- **Headless-mode boot: PASS** (`audio_world_constructs_without_panic_on_any_environment`, `explicit_headless_world_discards_playback_without_retaining_sound`).

## Executive Summary

No new findings. The audio crate, `systems/audio.rs`, `dialogue_voice.rs`, `scripted_sound.rs` and `combat_anim.rs` are untouched since the baseline. The delta in audio-adjacent files is:
- `asset_provider/audio.rs`: comment/log wording only (9ee802d57, FNV `MSET` now decoded into `EsmIndex::media_sets`; no playback policy reads it). Still consistent with #3816.
- `script_requests.rs`: +72 lines, all the UseWeapon volley fix (#6467); `music_system` unchanged.
- `boot/schedule/late.rs`: a `bridged_script_getter_reads` access helper on `npc_dialogue_selection` (#6460); no audio registration changed.
- m43 gameplay commits (mines, crafting, DEAD, HACK/LOCK, armor) add no `play_oneshot`/`play_music`/`stop_sounds_for` callers (re-grepped across `byroredux/src`). The proximity-mine explosion produces a `HitEvent` and so reaches impact sound only through the existing combat path.

| Severity | NEW | Existing (not re-reported) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 0 | 1 (#6437) |
| LOW | 0 | 2 (#6438, #6439) |

Existing, all three 10-27 findings were published and are still present in the code:
- **#6437** (MEDIUM): non-positional sounds placed at the listener's head play ~11 dB quieter and fold to mono. `spatialization_strength` is still never set anywhere in `crates/` or `byroredux/`.
- **#6438** (LOW): folder-form MUSC reaches `PlayMusic` with a misleading "failed to decode" warning. `script_requests.rs` still has no folder gate.
- **#6439** (LOW): `play_oneshot` (`world.rs:377`) still has no rustdoc; the contract paragraph sits above `oneshot_slot_ready`.
- Also open and unchanged: #6480 (PAR-D2-01, `.fuz` header), #3816, #3301, #4747, #5349.

No regressions: #3086, #4146, #4743, #5148, #5305, #5382, #5383, #5410, #5432, #5545, #5556, #5675, #5678, #6207, #6226, #6281, #6307 hold (code unchanged). Cross-audit overlaps: none relevant to audio (ECS-2026-10-31-01 is crafting lock order; REG-2026-10-31-02 clippy errors are in scripting, not audio).

## Lifecycle Invariant Matrix (HEAD `670f81917`)

| Invariant | State | Note |
|---|---|---|
| Field-drop order `active_sounds → pending_oneshots → oneshots_requested → music → script_music → reverb_send → reverb_send_db → listener → manager` | verified (unchanged) | `world.rs` untouched |
| `AudioWorld::new()` boot-only | verified (unchanged) | `boot/world.rs` |
| Sticky listener | verified (unchanged) | `system.rs` untouched |
| Despawn truncation / orphaned-Anchored stop / emitter follow | verified by trace (guards `#[ignore]`d) | `system.rs` untouched |
| Shared `build_spatial_track`, `queue_oneshot` → `oneshot_slot_ready`, `distance_range` min<max | verified (unchanged) | |
| Schedule: footstep → impact → (parallel reverb_zone) → water_audio → `audio_system` in Late; script producers Update exclusives | verified | `late.rs` delta touches only `npc_dialogue_selection`'s Access row |
| `stop_script_music` at both cell-leave choke points | verified (unchanged) | `transition.rs` untouched |
| Music on main track, streaming only | verified (unchanged) | |
| Archive precedence `.rev()` | verified (unchanged) | |

## Findings

None new.

## Future-Phase Readiness

- **FOOT / IPDS / IPCT**: mechanism live, tables still empty, `impact_audio_system` still a production no-op.
- **MUSC / MUST / MSET** (#3816): FNV `MSET` decoded, no runtime reads it. CELL `XCMO`/`XCMT` and WRLD `default_music` still unread in `byroredux/src` (re-grepped). Folder-form MUSC (#6438) needs a variant-selection policy first.
- **Non-spatial playback**: #6437's fix should double as the path for future UI/menu `2D` sounds.
- **Occlusion**: not started.

---
Suggested next step: no new publish needed (all findings already tracked as #6437-#6439).
