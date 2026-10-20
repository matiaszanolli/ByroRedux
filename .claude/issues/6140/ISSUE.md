# TD1-2026-10-19-03: `crates/audio/src/lib.rs` crossed 2000 production LOC (1977 -> 2034); it holds seven responsibilities and is 48% comments

Labels: low,audio,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/audio/src/lib.rs:1-2036` (tests already external, `mod tests;` L2036; `pub mod fuz;` L2033)
- **Status**: NEW (watch-listed at 1977 on 10-15)
- **Age**: +57 since 10-15: `acd577a79` (#5912 .fuz voice), `74d841165` (orphaned dialogue voice)
- **Effort**: medium
- **Description**: 969 of the 2036 lines are comments, ~970 are code, so this is a low-urgency crossing. By line: module docs L1-153; DSP/unit helpers (`bu_to_audio_space`, underwater filter, reverb send gates, constants) L155-335; sound bookkeeping types (`ActiveSound`, `SoundSource`, `PendingOneShot`) L336-472; `AudioWorld` + `impl` L474-911 (376 lines); ECS components (`AudioListener`, `Attenuation`, `AudioEmitter`, `OneShotSound`) L912-1070; the system and its sub-steps (`audio_system`, `sync_emitter_positions`, `update_underwater_filters`, `sync_listener_pose`, `build_spatial_track`, `drain_pending_oneshots`, `dispatch_new_oneshots`, `prune_stopped_sounds`) L1077-1653; loaders + `SoundCache` + `VoiceSoundCache` L1654-2031.
- **Evidence**: `prod_loc` 2034; `grep -cE '^\s*//' lib.rs` 969; offsets from `grep -nE '^(pub(\(crate\))? )?(fn|struct|enum|impl)'`.
- **Proposed fix**: `lib.rs` keeps docs and `pub use`; new `dsp.rs` (units + filter/reverb gates), `world.rs` (`AudioWorld`, `ActiveSound`, `SoundSource`, `PendingOneShot`), `components.rs`, `system.rs` (the per-frame steps; #5738 already shares `build_spatial_track` between the two dispatch loops), `cache.rs` (`SoundCache`, `VoiceSoundCache`, loaders). Hazard: four source-scan tests read `lib.rs` verbatim (`audio/src/tests.rs:1350, 1417, 1502, 1632`: `drain_pending_oneshots`, both dispatch failure arms, the `bu_to_audio_space` seam, `FilterBuilder::new()` count == 1); each must read `concat!` of the new files or the pins go vacuous (feedback_file_split_include_str).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
