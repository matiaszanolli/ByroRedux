**HEAD**: 54e8aeaa8 (audited) · **Baseline**: AUDIT_INCREMENTAL_2026-11-14.md (HEAD 922072af8) · **Audited**: the #6916–#6934 fixes of the 11-14 findings (scripting, save, audio/music, travel, renderer guards, tooling), the Bink hardening and video/scripting fixes that followed (7250362df..1de2673b7), and the overnight fixes (e9c861dea..54e8aeaa8: #6539, #6798, #6841, #5351, #4113, #6823, #6371 step 2, repair commits)

# Incremental Audit: 2026-11-15 (overnight, `922072af8..54e8aeaa8`)

**Method.** Two read-only auditors, each given one commit set and the `/audit-incremental` protocol (route each file, apply the owning skill's dimensions to the hunks, verify every finding at a file:line). No engine was launched by the auditors. Every finding below was re-checked against the code before it was acted on, and every one was fixed or filed the same night; the **Outcome** line names the commit or issue.

**Scope.** 34 commits with code or test changes. Docs-only commits (`7737053f2`, `de8b4182c`, the PRD status flips) were skipped.

## Findings

### INC3-1 — MEDIUM — a Bink audio packet could still decode ~100x its weight, and the soundtrack had no total limit
- **Location**: `crates/binkaudio/src/decoder.rs` (`Decoder::decode`); `crates/video/src/lib.rs` (`bink2_audio`, `extract_audio`)
- **Changed in**: 030c1747a
- **Description**: The new per-packet bound allowed one block per 64 bits. Past the data every bit reads 0, and zeros decode as silent blocks, so a zero-filled packet could expand far beyond what a real stream does. The up-front soundtrack buffer had no total cap, and `Bk2::open` allows 1,000,000 frames. A hostile movie could therefore exhaust memory on the caller's thread, which is the class 030c1747a set out to close.
- **Outcome**: fixed in 706f95884.
  - `min_block_bits` takes what `block`/`channel` read unconditionally: 590 bits for 48 kHz stereo.
  - Both soundtrack loops stop at `pcm_cap`, the running time plus 2 s, times 1.5.
  - Real data: all 236,690 audio packets of every track in the 54 installed Bink 2 movies decode under the new bound and fit the cap. The corpus test still decodes 47 Bink 1 and 54 Bink 2 movies.

### INC3-2 — LOW — the mine console reported a dormant mine as `spent`
- **Location**: `byroredux/src/systems/mines.rs` (`placed_mines`)
- **Changed in**: e9c861dea
- **Outcome**: fixed in 9b9b2806d.
  - `spent` now means `SpentMine` (gone off this session). `disabled` means the reference is off.
  - After a reload a mine that went off also reads `disabled`, because the enable ledger cannot tell the two apart. The doc says so.

### INC2-1 — LOW — a finite but absurd INI fade still panicked `Duration` construction
- **Location**: `crates/audio/src/{world,cache,system}.rs`
- **Changed in**: 596118efc (#6929 filtered only non-finite values)
- **Description**: `iMusicTransitionInMS=1e30` makes a 1e27 s fade. `Duration::from_secs_f32` panics at 2^64 s. The prune-stop site in `system.rs` had no clamp at all.
- **Outcome**: fixed in 4943fe984. `tween_duration` is now the one conversion: negative and non-finite read as 0, and finite values are held at `u32::MAX` s. Every fade goes through `linear_tween`.

### INC2-2 — LOW — a Starfield item worn only on BO64 slots reads as unequipped across the mod-host WIT boundary
- **Location**: `crates/mod-runtime/src/runtime/convert.rs` (`wit_biped_slots`)
- **Changed in**: 8cfdea14a (#6918, which is correct for the 0.1 contract)
- **Outcome**: filed as #6938. The fix is a WIT revision, which is an owner decision.

### INC2-3 — LOW — a reviewed explicit-LOD entry matched every call site with the same LOD text
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` (`REVIEWED_EXPLICIT_LODS`)
- **Changed in**: e4c45c448 (#6932)
- **Outcome**: fixed in 1fa774498. Each entry must now match exactly one sample. Injected fault: a second `lod` sample in `material_sampling.glsl` fails.

### INC2-4 — LOW — `SetCellMusicType`'s across-load behaviour was stated as measured
- **Location**: `crates/scripting/src/obscript_more/extras.rs` (`CellMusicTypes`); its NOT_SAVED reason
- **Changed in**: a4df8f932 (#6930)
- **Outcome**: fixed in 23db80f8f. xOBSE shows the in-memory write and that no form change is marked, so the save omits it. That an in-session load keeps the value is now marked as inferred, not measured.

## Missing tests

| Commit | Gap | Outcome |
|---|---|---|
| 6439145f9 (#6798) | No test that the record parsers call `read_lstring_field` | de1578ec8, `the_record_parsers_read_each_field_from_its_own_file`. Fault: PERK DESC reverted reads "Quick Test Cell". |
| 7250362df | `SaveReloadTeardown` was inserted by hand in its only test | 0565c491d, `a_save_reload_runs_its_teardown_under_the_reload_marker` |
| aa193bd9e | Nothing pinned that both reader threads use `guard_decoder` | 0565c491d, `every_reader_thread_runs_under_the_decoder_guard` |
| e9c861dea (#6539) | Bench lookups skipping a disabled bench | 9b9b2806d, `a_disabled_bench_is_not_found` |
| 264f996b2 (#6931) | The `sting()` assertions passed with or without the fix | e75338e80. The fixture now carries the stings. Fault: removing the decline fails the test. |
| d089cfa45 (#6925) | The `travel_system` half had no test | a52f44fa0, `an_actor_at_its_load_door_is_not_traveled_while_it_fades`. Fault: dropping the guard fails the test. |
| 067568b87 (#6917) | The test pins `ledgered_actors`, not the reconcile | Not added. `reconcile_logical_identities` collects into a `BTreeMap` keyed by FormID, so a duplicate identity is impossible by construction. A reconcile fixture needs travel data, the loaded-cell index and the load order. |
| 688f74956 (#6924) | `step_actor_relocations`' idle gate is untested | Open. `App`-level step, no fixture. |
| a8958fc18 (#6920) | The lock-order test fails only under `BYRO_LOCK_ORDER_CHECK=1` | Covered by CI's ABBA job. No change. |
| bf22b82ef (#6919) | The behavioural test skips without an audio device | Device-bound. The source-count pin stays. |

## Verified clean

All of the following were checked with no finding:
- d5f3eeb47, 1b10b061c, e905b257c, 75d0fe6a2, a37c3e3b1, bf1fc8b03, efb88a1ce, 36c6f4b64, 00472c4a1
- 7250362df, fb99052cd, fa2a17854, 1de2673b7
- 309b43fb1, 7595ad751, 783771851, f464c51f6
- 54e8aeaa8. Every migrated pass still destroys every handle. The changed order is only among objects Vulkan places no destroy-order rule on. The SAFETY comments are accurate.
- 6439145f9's `StringsFile::for_field` against xEdit's `LocalizedValueDecider`. Every remaining `read_lstring_or_zstring` site is `.STRINGS` per the decider.
