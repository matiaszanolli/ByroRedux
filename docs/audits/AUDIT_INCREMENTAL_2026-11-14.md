**HEAD**: 922072af8 · **Baseline**: AUDIT_INCREMENTAL_2026-11-07.md · **Audited**: audio Dims 3-5 (music controller, movie hold), scripting (OBSE music natives, PlayBink lowering + VM, placement effects, ObScript `~`), gameplay Dim 5 + save + concurrency (cross-cell travel), esm (music sources, XCT spaces, BO64), character/gameplay (BO64 consumers), parsers (`crates/video` stream driving only), renderer (#6629 explicit LODs, SPIR-V parity), tooling (debug-server bind retry, ownership map), tech-debt/runtime (clippy sweep, prune script, smoke gates) · **Unchanged since baseline (skimmed)**: nifal (two 1-line clippy hunks in `material_translate.rs`), physics, exterior (clippy-only hunks in terrain/water/groundcover), ui (clippy-only hunks in `crates/ui` examples and `crates/menuxml`)

# Incremental Audit: 2026-11-14 (Session 112, `72010bd15..HEAD`)

Read-only. No engine launched, no issues created. The one cargo invocation was a no-op `cargo check -p byroredux-bsa`, used to see which lock file cargo 1.100 holds during a build.

**Scope note.** The working tree was clean at the start, so the default scope was empty. The diff since the last incremental audit's HEAD (310444b69) is 308 commits and 830 files, too large for a delta pass. Following the 11-07 precedent, this run covers the latest session only: everything after the Session 111 closeout (72010bd15..922072af8), which is 52 commits (23 of them docs-only), 321 files, +12 678 / -2 721.

**Out of scope, with follow-up recommended:**
- **`crates/bink2` and `crates/binkaudio`.** These are the clean-room decoders from 8e11d49a4 and 3ebecb7ec: about 5.7k lines that parse untrusted Bink video and audio. They predate this window, and only their clippy hunks were read. No `/audit-parsers` run has covered them (the last one is AUDIT_PARSERS_2026-10-31). Run `/audit-parsers` focused on those two crates.
- **The window between the two incremental audits.** The 256 commits from 310444b69 to 72010bd15 (2026-11-07 to 11-13) have had no delta pass. The owner audits for audio, scripting, gameplay, save, tooling and papyrus were last run on 10-31.
- **Concurrent edits.** Another session had uncommitted edits in the working tree while this report was written (`live_load.rs`, `relocated.rs`, `reference_state.rs`, `live_reload_tests.rs`, `registry_completeness_tests.rs`). They appear to fix INC-3. Every finding below was verified against HEAD 922072af8, not the working tree.

## 1. Change summary

| Theme | Commits | Code |
|---|---|---|
| Game-music controller (Oblivion, FO3, FNV) | 45b3b0a64, d86ae6634 (#6889), 3981afb82 (#6904), d8654678f (#6899), d8cf5c171 (#6900), 0f48b9b18 (#6903), ad06d54b0 (#6901), bf42dca77 (#6902), 18d7dd331 | New `byroredux/src/music/{mod,controller,directive,translate}.rs` (about 2.7k LOC). The old REGN path in `asset_provider/audio.rs` is removed (-431). Also: music voices in `crates/audio/src/{world,cache}.rs`, OBSE `Get/SetCellMusicType`, `StreamMusic`, `game_ini.rs`, and the `audio.music_volume` setting. |
| Music sources in the ESM | 782e23d0a (#6870), 18d7dd331, cf562b3c5 (#6865) | DOBJ music slots, WRLD SNAM, ALOC (`media_location.rs`, NAM1), MUSC (`music.rs`), REFR audio markers, `music_census.rs` example |
| Cross-cell actor travel (XCT) | 7af55a0df, 4b71155ca (#6852 #6853 #6854), 98aa53db9, 4c1d54cc0 | `crates/plugin/src/esm/cell/spaces.rs` (SpaceGraph, NVCI door links), `byroredux/src/cell_loader/relocated.rs` (+1 092: routing, journeys, scripted moves, logical identities, door fade), `systems/travel.rs`, `created_references.rs`, the Papyrus and ObScript `MoveTo` fallthrough, the ABBA lock-order fix |
| Bink playback wiring | 6aa314bcf (#6858 #6859), d0247fac2, 118afd645, 8cf1a7a22 | Papyrus `Game.PlayBink` (lowering + VM), the ObScript and console paths, `AudioWorld::hold_for_movie`, `MovieEnded` resume, `Failed` vs `Played` in `crates/video` |
| Starfield BO64 biped mask | 9405138e9 (#6778) | `MAX_BIPED_SLOTS` 32 → 64, `crates/plugin/src/equip.rs`, ARMO/ARMA decode, FORMAT_MAJOR 128 → 129 |
| Streamed-texture explicit LODs | f754eee28 (Refs #6629) | `residentLod` plus a per-slot first-level SSBO (set 0, binding 2), `texture_registry/*`, 4 `.spv` rebuilt |
| Tooling, toolchain, smoke | b9749eb46, 3c9949ec9, c576c7a3a, 5565a12dd, 39687cb76, 78df9d297, 4989949b3, 34268f498, ce438eb1a (#6905) | Debug-server bind retry; rustc 1.100.0 pin with clippy drift across 108 files; `prune-target-cache.sh`; smoke-contract fixes; forcegreet grid bound; ObScript `~` as unary minus |

## 2. Routing map

Routed by `_audit-route.sh`; the risk floor is in brackets. No changed path was `(unrouted)`; see INC-21 for the tracked-tree gaps. Only the dimensions whose `Paths:` intersect the diff were run.

| Files | Owner / dimension audited | Result |
|---|---|---|
| `byroredux/src/music/*`, `asset_provider/audio.rs`, `commands/music.rs`, `game_ini.rs`, `crates/audio/src/{world,cache,lib}.rs` [MEDIUM] | audio Dims 3, 4, 5; save; performance Dim 1 | INC-8, INC-9, INC-10, INC-16 to INC-19. The removed REGN path has no surviving caller and nothing plays twice. |
| `crates/scripting/src/{obscript.rs,obscript_more/extras.rs,combat.rs}`, `crates/sdk/src/compatibility/source_alias.rs` [MEDIUM] | scripting Dim 6, tooling | Clean apart from INC-18. |
| `crates/plugin/src/esm/**` [HIGH] | esm (sub-record accounting, schemas, walkers, FormID remap, per-game gates) | Clean. NVCI decode reconciles 4 771 / 4 771 FNV rows and 7 198 / 7 198 FO3 rows. DOBJ music slots, Oblivion `WRLD.SNAM` and ALOC NAM1 were checked against the shipped masters. Every new FormID goes through the remap. |
| `crates/plugin/src/equip.rs`, `records/{items,misc/equipment}.rs`, `npc_spawn/equip.rs`, `core/.../inventory.rs`, `save` [HIGH/MEDIUM] | esm, character Dim 4, gameplay, save, starfield | INC-4. Save shape is clean: FORMAT_MAJOR 129, the fingerprint is refreshed, and a 32-slot array is refused. |
| `byroredux/src/cell_loader/relocated.rs`, `app_step.rs`, `systems/travel.rs`, `commands/scene.rs` [HIGH] | gameplay Dim 5, save, ecs Dim 7, concurrency, performance Dim 7 | INC-1, INC-2, INC-3, INC-11, INC-12, INC-13, INC-15. 4c1d54cc0 is complete: no site in the slice takes `FormIdPool` before `FormIdComponent`. |
| `crates/scripting/src/{created_references.rs,fragment/effects/placement.rs,obscript_actions.rs,translate/effects/prims/placement.rs}` [MEDIUM] | scripting Dim 4, concurrency | INC-1, INC-14 |
| `byroredux/src/video.rs`, `crates/video/src/lib.rs`, `commands/video.rs`, `crates/scripting/src/{fragment/vm/*,translate/effects/*,obscript_more/tail.rs,obscript_quests.rs}` [HIGH/MEDIUM] | ui, audio Dim 3, scripting (VM + recognizer chain), parsers (`crates/video` only), concurrency, save | INC-5, INC-6, INC-7. Flag order and defaults match across the console, ObScript, Papyrus and start-sequence paths. The movie hold is released on every `finish`. Save shape is clean. |
| `crates/scripting/src/obscript_vm.rs` (#6905) [MEDIUM] | scripting (M47.3) | Clean. `~` occurs 70 / 86 / 74 times as a standalone token in the Oblivion / FO3 / FNV SCDA data, and the corpus contains no bitwise operator. |
| `crates/renderer/shaders/include/{bindings,ray_hit}.glsl`, `triangle.frag`, `.spv`, `texture_registry/*`, `vulkan/texture.rs` [HIGH/MEDIUM] | renderer (descriptors, SPIR-V parity, GPU-struct lockstep, upload sync, destroy order), performance, safety | Clean apart from INC-20. All four `.spv` files are byte-identical to a fresh `glslangValidator -V` (`triangle_early` built with `-DBYRO_OPAQUE_EARLY_TESTS=1`). No `#[repr(C)]` GPU struct was touched. The one new `unsafe` has a SAFETY comment that holds. |
| `crates/debug-server/src/listener.rs`, `main.rs` [MEDIUM] | tooling (debug server), concurrency Dim 7 | Clean. The retry is bounded (5 s, `AddrInUse` only), the server still binds loopback only, the `BYRO_DEBUG_SERVER` opt-in is unchanged, and `--bench-hold` exits 2 on bind failure. |
| 3c9949ec9 / c576c7a3a clippy sweep (108 + 14 files), `.github/workflows/*`, `rust-toolchain.toml` [mixed] | tech-debt, runtime | Clean. Every non-syntax hunk was deep-read and preserves semantics: `as_chunks`, `is_multiple_of` with nonzero divisors, `is_none_or` (De Morgan checked), iterator `take(n)` bounds checked against the array types. The toolchain is 1.100.0 everywhere, and no gate was dropped or made `continue-on-error`. |
| `scripts/prune-target-cache.sh` [LOW] | tech-debt (dev tooling) | Clean. `set -euo pipefail`, a `CACHEDIR.TAG` guard, quoted `rm --`, and NUL-separated `find` output. The in-flight guard was verified empirically: cargo 1.100 holds `.cargo-lock` (as well as `.cargo-build-lock`) during a build, so `flock -n` on it does detect a running build. |
| `docs/smoke-tests/*.sh`, `scripts/check-playable-smoke-contracts.sh` [LOW] | runtime (read only) | Clean. "Green again" comes from real fixes and stricter gates: zero captures now exits 77 instead of passing vacuously. No assertion was removed. |
| `byroredux/src/systems/forcegreet.rs` [MEDIUM] | gameplay Dim 2, concurrency Dim 5 | Clean. No new acquisition. |

## 3. Findings

Verdict key: **CONFIRMED** means the code path was re-read against HEAD for this report. **PLAUSIBLE** means the dimension agent traced it and I checked part of the chain, with the unverified part named.

### INC-1: A Papyrus `MoveTo` on a `PlayerRef` property toward an unloaded target evicts the player body
- **Severity**: HIGH
- **Verdict**: CONFIRMED (the code chain). Not measured: how often vanilla Skyrim content calls `PlayerRef.MoveTo(<marker in an unloaded cell>)`.
- **Dimension**: scripting Dim 4, gameplay Dim 5 (XCT scripted moves)
- **Location**: `crates/scripting/src/fragment/effects/placement.rs:19-39`, `byroredux/src/cell_loader/relocated.rs:587` (`is_actor` gate), `relocated.rs:681` (`live_body` special-cases `PLAYER_REF`), `byroredux/src/app_step.rs:1284-1297`
- **Changed in**: `placement.rs`, `relocated.rs`, `app_step.rs` (commit `4b71155ca`)
- **Status**: NEW (not tracked by #6891 to #6895)
- **Description**:
  - Before 4b71155ca, `Effect::MoveTo` returned early when either end had no body. Now it falls through to `object_form_id(moved)` and queues `ActorMoveRequest::ToReference`.
  - The lowering recognises the player only through `player_expr_ref` (`translate/effects/receivers.rs:289`), which matches `Game.GetPlayer()` and locals assigned from it. A `PlayerRef` script property lowers to a plain `Effect::MoveTo`, and `resolve_property_form_id` resolves it to `0x14`.
  - `resolve_actor_moves` admits `0x14`. Its gate, `locate_refr(actor).is_none_or(is_actor)`, passes both when the reference is unknown and when it is the player.
  - `step_actor_relocations` then calls `ledger.relocate(0x14, ...)`. `live_body(0x14)` returns the player entity, and `evict_actor` releases it.
  - The ObScript path is safe, because `move_to` diverts the player to `PlayerMoveRequest` first (`obscript_actions.rs:265`).
- **Evidence**: `placement.rs:35-37` (`let actor = object_form_id(...)?; ... queue_actor_move(ToReference { actor, target, .. })`); `relocated.rs:681` (`if form == PLAYER_REF { return resolve_entity_by_global_form_id(world, form) }`); `app_step.rs:1295-1296` (`live_body` then `evict_actor`).
- **Impact**:
  - The player body is despawned on a common Skyrim quest idiom (a scripted teleport to a marker in another cell).
  - A bogus `0x14` row lands in `ActorRelocations`, which is saved.
  - The same `is_none_or` gate also admits quest or script FormIDs and runtime-created actors. A created actor is evicted and never respawned, because `locate_refr` is `None` for it.
- **Related**: XCT-LOC-004, #6852
- **Suggested Fix**:
  - In `resolve_actor_moves`, reject `actor == 0x14` and the player entity, and route the move to `PlayerMoveRequest`.
  - Require `locate_refr(actor).is_some()`, or add an explicit created-actor path.
  - Add a regression test with a `PlayerRef`-valued property receiver.

### INC-2: An actor with both a placement and a journey gets two logical-identity stubs
- **Severity**: HIGH
- **Verdict**: CONFIRMED
- **Dimension**: gameplay Dim 5, ECS lifetime
- **Location**: `byroredux/src/cell_loader/relocated.rs:778-860` (`reconcile_logical_identities`, `moved` at `:815`); `crates/scripting/src/created_references.rs:297-305` (`relocate` / `start_journey`)
- **Changed in**: `relocated.rs` (commit `4b71155ca`)
- **Status**: NEW
- **Description**:
  - `relocate` removes the actor's journey, but `start_journey` does not remove its placement. Any multi-hop route therefore leaves the actor in both maps: hop 1 lands it in a loaded cell (`relocate`), then hop 2 or an unload starts a journey (`continue_hop` / `depart_unwatched`).
  - `moved` chains `ledger.iter()` and `ledger.journeys()` without de-duplicating, so the form is pushed to `wanted` twice.
  - The spawn loop checks the `stubs`, `carried` and `live` snapshots, all taken before the loop. The stub spawned for the first entry is invisible to the second, and a second `RemoteSceneActorStub` is spawned.
- **Evidence**: `let moved: Vec<u32> = ledger.iter().map(..).chain(ledger.journeys().map(..)).collect();` (`:815`). `start_journey` only does `self.journeys.insert(actor, journey)` (`created_references.rs:303-305`).
- **Impact**:
  - Two entities carry the same `reference_form_id` and script set, so alias fills and scripts run twice.
  - `identity_stubs` (keyed by form) tracks one of them. `promote` and the "retire stub beside a body" step despawn only that one, and the other leaks for the session.
- **Related**: XCT-LOC-005
- **Suggested Fix**:
  - `moved.sort_unstable(); moved.dedup();`.
  - Make `start_journey` and `relocate` keep the two maps mutually exclusive.
  - Add a placement-plus-journey reconcile test. `reconcile_logical_identities` has no unit test today.

### INC-3: `ActorRelocations` is restored after the cell reload, so a live load spawns home-cell actors against the outgoing session's ledger
- **Severity**: HIGH
- **Verdict**: CONFIRMED against HEAD. An uncommitted fix is in the working tree at report time: it adds `ActorRelocations` to `PRE_RELOAD_RESOURCES` and `LiveReloadLedgers`, and a `SaveReloadTeardown` early return to `depart_unwatched`. Re-check after it lands.
- **Dimension**: save (live load-apply), gameplay Dim 5
- **Location**: `byroredux/src/save_io/live_load.rs:43` (`PRE_RELOAD_RESOURCES`); consumers `byroredux/src/cell_loader/relocated.rs:160` (`skipped_at_home`), `cell_loader/references/mod.rs:191`, `cell_loader/exterior/persistent.rs:159`; teardown writer `relocated.rs:425` (`depart_unwatched`)
- **Changed in**: `relocated.rs`, `references/mod.rs`, `exterior/persistent.rs` (commit `4b71155ca` and its XCT predecessors)
- **Status**: NEW. This is the "spawn-time ledger check" question the audit-save skill already raises.
- **Description**:
  - `skipped_at_home` reads the live `ActorRelocations` while the reload builds the home cell. At HEAD the resource is not in `PRE_RELOAD_RESOURCES`, so it only arrives with the post-reload `restore_resources`.
  - After that, `step_actor_relocations` only spawns ledgered actors that are not alive. It never evicts a body that the saved ledger places elsewhere.
  - `depart_unwatched` also runs during the reload teardown, so it starts journeys in whichever ledger is live then. That is the #4135 contamination pattern.
- **Evidence**: `git show HEAD:byroredux/src/save_io/live_load.rs | grep -c ActorRelocations` gives 0. The only round-trip test (`consumable_tests.rs:716`) calls `restore_resources` and then `skipped_at_home` directly, not the live-load flow.
- **Impact**:
  - Loading a save in which an actor has been relocated away spawns them at home in the reloaded cell, where they stay.
  - The reverse case: an actor relocated in the outgoing session vanishes from a loaded cell where the save keeps them.
- **Related**: #4135, #6081, #6852
- **Suggested Fix**: As the working-tree change does: pre-reload restore, a `LiveReloadLedgers` field, and a teardown guard. Also, after the post-reload restore, evict live bodies the ledger places elsewhere, and add a live-load test with a non-empty ledger.

### INC-4: The SDK inventory capture still folds 64 biped slots into a `u32` (incomplete fix of #6778)
- **Severity**: MEDIUM
- **Verdict**: CONFIRMED
- **Dimension**: esm → ECS handoff / contract break (biped mask u32 → u64), starfield
- **Location**: `byroredux/src/extensions/capture.rs:289-301`; `crates/sdk/src/inventory.rs:86,95,120` (`InventoryEntry::biped_slots: u32`)
- **Changed in**: `crates/core/src/ecs/components/inventory.rs` (commit `9405138e9` widened `MAX_BIPED_SLOTS` to 64 without touching this caller)
- **Status**: NEW (incomplete fix of closed #6778)
- **Description**:
  - `slots.occupants.iter().enumerate().fold(0_u32, |mask, (bit, occupant)| ... mask | (1_u32 << bit))` iterates all 64 entries.
  - A debug build panics with "attempt to shift left with overflow" for any occupied slot at bit ≥ 32.
  - Release has no `overflow-checks` override in `[profile.release]`, so the shift is masked and bit 35 (Starfield spacesuit body) aliases onto bit 3.
  - The SDK type is `u32`, so it cannot express Starfield's 64-bit layer at all.
- **Evidence**: `capture.rs:293-296`; `MAX_BIPED_SLOTS: usize = 64` (`inventory.rs:217`). `git grep occupants` finds no other production fold that was not narrowed with `.take(32)` or widened to `u64`.
- **Impact**: A Studio/SDK snapshot of an actor wearing a Starfield spacesuit piece panics the engine thread in a debug build, and reports the wrong slot in release.
- **Related**: #6778; `crates/mod-runtime/src/runtime/convert.rs:110`
- **Suggested Fix**:
  - Either widen the fold and `InventoryEntry::biped_slots` to `u64`, updating `mod-runtime` (an SDK contract change), or `.take(32)` with a documented "clothing layer only".
  - Add a capture test with bit 35 occupied.

### INC-5: `Effect::PlayBink` suspends a run but is missing from `has_latent_tail`
- **Severity**: MEDIUM
- **Verdict**: CONFIRMED
- **Dimension**: scripting, AST→ECS recognizer chain (decline-on-unmodeled)
- **Location**: `crates/scripting/src/translate/effects/lower.rs:376-390` (`has_latent_tail`; consumers at `:205`, `:353-366` `locals_never_suspend`, `inline.rs:181`, `inline.rs:205`); suspension at `crates/scripting/src/fragment/effects.rs:576-588`
- **Changed in**: `translate/effects/{model.rs, prims/world.rs}`, `fragment/effects.rs` (commit `6aa314bcf`). `lower.rs` was not updated.
- **Status**: NEW
- **Description**:
  - `apply_effects` treats `PlayBink` exactly like `Wait`: it returns `Some((0.0, FragmentResumeCondition::MovieEnded))`, and `effects.rs:293` groups it with the waits.
  - `has_latent_tail` lists only `Wait`, `WaitForActors3DLoaded`, `WaitWhile` and `ProviderCall`.
  - As a result, three guards that decline unmodelled suspensions let `PlayBink` through:
    - locals across a suspension (`locals_never_suspend`);
    - `uses_speaker` with a latent tail;
    - an inlined quest-script callee that would suspend (`inline.rs:181/205`).
  - When an inlined callee calls `PlayBink`, the inner run suspends but the outer fragment continues, so effects after the call (such as `SetStage`) run before the movie starts.
- **Evidence**: `lower.rs:376-390`; `effects.rs:293`, `:576-588`. The only lowering test, `lowers_game_play_bink`, uses bare statements.
- **Impact**:
  - BIK-PRES-006 ("the calling script waits for the movie") is violated for those shapes.
  - Locals and the speaker are lost across the movie.
  - FO4's intro is driven by quest-script `PlayBink` calls, so the inlined case is realistic.
- **Related**: M47.2 `locals_never_suspend`, BIK-PRES-006
- **Suggested Fix**:
  - Add `Effect::PlayBink { .. }` to `has_latent_tail`.
  - The VM's `is_latent` check in `call_statement_native` runs before its PlayBink special case, so reorder the two so PlayBink stays accepted there. Check `checker.rs`'s `is_latent` use too.
  - Add tests for PlayBink with a local, a speaker, and an inlined callee.

### INC-6: A panicking movie-decode thread ends the movie as `Played`, not `Failed`
- **Severity**: MEDIUM
- **Verdict**: CONFIRMED
- **Dimension**: parsers (`crates/video` stream driving), concurrency
- **Location**: `crates/video/src/lib.rs:345-372` (Bink 1 reader thread), `:386-411` (Bink 2), `:501` (`Disconnected => exhausted`), `:504-508`, `:518-523` (`Drop` ignores `join()`)
- **Changed in**: `crates/video/src/lib.rs` (commit `118afd645`, which routes decode *errors* to `Failed` but not panics)
- **Status**: NEW (the audit-concurrency skill's "Movie decode" item anticipates it)
- **Description**:
  - A panic in `VideoDecoder::decode`, `bink2::Decoder::decode` or `fit` unwinds the `movie-decode` thread. There is no `catch_unwind`, and `panic = "unwind"` keeps the process alive.
  - The channel closes with no message, and `advance` maps `TryRecvError::Disconnected` to `exhausted` with `failure == None`.
  - Once the clock passes the last decoded frame, `advance` returns `Playhead::Finished`, which records `Played { ended_early: false }`.
- **Evidence**: `lib.rs:501` and `:504-508`. Untrusted decode elsewhere in the repo is wrapped in `catch_unwind` (`streaming/pre_parse.rs`, `texture_streamer.rs`, `terrain_height.rs`). `bik-start-sequence.sh` extracts `after N frame(s)` but never compares N to the container's frame count.
- **Impact**:
  - A corrupt or hostile stream that trips a decoder bug is reported as a clean playthrough, and the gate stays green.
  - The held game (`dt = 0`) shows a frozen last frame until the clock runs out. Papyrus movies default to non-interruptable.
- **Related**: BIK-DEC-004, #6855
- **Suggested Fix**:
  - Wrap each reader body in `catch_unwind` and send `Err("…decoder panicked…")`.
  - As a backstop, in `advance`, treat `Disconnected` before the container's frame count as `Failed`.
  - Make the smoke gate compare the played frame count with the container's.

### INC-7: A muting movie does not stop entity-emitter sounds dispatched during the hold
- **Severity**: MEDIUM
- **Verdict**: CONFIRMED (the code path). Not observed at runtime: whether the boot start-sequence frame dispatches emitters after the hold is applied.
- **Dimension**: audio Dims 3 and 5 (BIK-PRES-005)
- **Location**: `crates/audio/src/system.rs:268-273` (the only `world_muted` check, in `drain_pending_oneshots`); `system.rs:383` (`dispatch_new_oneshots`, ungated); `crates/audio/src/world.rs:650-673` (`hold_for_movie` / `release_movie_hold`)
- **Changed in**: `crates/audio/src/{system.rs, world.rs}` (commit `6aa314bcf`)
- **Status**: NEW
- **Description**:
  - `hold_for_movie(mute_world = true)` pauses the current `active_sounds` and clears the queued one-shots.
  - `audio_system` then calls `dispatch_new_oneshots` every frame with no `movie_hold` check. Any `OneShotSound` + `AudioEmitter` marker raised during the movie (looping ambients, machinery, torches from a cell or streaming load, scripted spawns) starts at full volume.
  - Those new sounds were never paused, so the release does not affect them.
- **Evidence**: `system.rs:53-54` (`drain_pending_oneshots` then `dispatch_new_oneshots`); `world.rs:654-658`.
- **Impact**: Ambient loops are audible under a movie that asked to mute the world.
- **Related**: BIK-PRES-005
- **Suggested Fix**:
  - While `world_muted`, either dispatch and immediately pause each new handle (so release resumes it), or leave the markers undispatched until release.
  - Add a headless `AudioWorld` test.

### INC-8: The music system and `music.status` take `MusicCatalog` / `MusicController` / `AudioWorld` in opposite orders
- **Severity**: MEDIUM. Both callers are exclusive (`music_system` via `add_exclusive`; `status_lines` from the debug drain), so no deadlock is reachable today, and the "ECS deadlock potential → HIGH" floor does not apply. The lock-order detector does trip on it.
- **Verdict**: CONFIRMED
- **Dimension**: concurrency, audio Dim 4
- **Location**: `byroredux/src/music/mod.rs:177-183` (`music_system`: `MusicCatalog` W → `MusicController` W → `AudioWorld` W → `SoundArchiveProvider` R, then many component and resource reads while all four are held); `mod.rs:323-330` (`status_lines`: `MusicController` R → `AudioWorld` R → `MusicCatalog` R, with the controller guard still alive)
- **Changed in**: `byroredux/src/music/mod.rs` (commits `45b3b0a64`, `3981afb82`)
- **Status**: NEW
- **Description**:
  - The two functions create the edges `MusicCatalog → MusicController` and `MusicController → MusicCatalog`, plus `MusicCatalog → AudioWorld` and `AudioWorld → MusicCatalog`.
  - `crates/core/src/ecs/lock_tracker.rs` (`BYRO_LOCK_ORDER_CHECK=1`) panics on the second observation of a cycle-closing edge, whether or not the two acquisitions overlap in time.
  - `music_system` also holds `AudioWorld` while it reads `Dead`, `GlobalTransform`, `FactionRanks`, `AiCombatState` and others. The audio code otherwise keeps to "one resource lock at a time" (`systems/audio.rs`, #5556) precisely to avoid AudioWorld-to-many edges.
- **Evidence**: `mod.rs:177-183`, `:324-330`
- **Impact**:
  - A debug run with the detector on panics when `music.status` (or `cell.owners`) runs after any music frame. CI's ABBA job would fail as soon as a test exercises both paths.
  - Any existing `X → AudioWorld` edge would close a real cycle. That is the class just fixed in 4c1d54cc0.
- **Suggested Fix**:
  - In `status_lines`, take the catalog before the controller, or drop the controller guard before taking the catalog.
  - In `music_system`, compute the component-derived inputs first and take `AudioWorld` last.

### INC-9: A music stem that fails to start is re-read, re-decoded and re-logged every frame
- **Severity**: MEDIUM
- **Verdict**: CONFIRMED
- **Dimension**: audio Dim 3, performance Dim 1 (per-frame file I/O on the main thread)
- **Location**: `byroredux/src/music/controller.rs:401-409` (Stems branch); `byroredux/src/music/mod.rs:33-50` (`AudioOutput::play`); `crates/audio/src/world.rs:543-555` (`play_music_voice` returns `()`)
- **Changed in**: `controller.rs`, `mod.rs` (commits `45b3b0a64`, `bf42dca77`)
- **Status**: NEW
- **Description**:
  - When `out.play` returns false, a playlist marks itself Missing. The Stems branch instead leaves `state.stems[i] = None` with `wanted` still true. Every frame it runs `provider.extract(key)` (a full archive or loose read), a decode, and a `log::warn!`.
  - Separately, `play_music_voice` swallows a `mgr.play` error, and `AudioOutput::play` still returns `true`. The controller then records a track that is not playing, and a live non-looping playlist sees `ended` and starts the "next" track every frame.
- **Evidence**: `controller.rs:405-408`; `mod.rs:37-48`
- **Impact**: Rare (it needs a listed but corrupt or unreadable file, or a kira play failure). When it happens: archive read, decode and log spam every frame on the main thread, against PRD row MUS-SRC-012 ("a missing file is noted once").
- **Suggested Fix**:
  - Make `play_music_voice` return `bool`.
  - Remember a failed stem key per directive and treat it as Missing.

### INC-10: The silence from a death sting can survive a load or respawn into the same place
- **Severity**: MEDIUM
- **Verdict**: PLAUSIBLE.
  - Confirmed: `silenced` is reset only in `set(layer != Sting, Some(..))`, and `music_system` calls `set(Location, ..)` only when the place, marker or OBSE counter changes, or when the catalog is rebuilt (`state.place = None` on a new `LoadedCellIndex`).
  - Not exercised: whether an in-session save load or exterior live-load keeps the same index and place across the load frame.
- **Dimension**: audio Dims 3 and 4 (controller state machine)
- **Location**: `byroredux/src/music/controller.rs:179-183`, `:232-239`, `:270`; `byroredux/src/music/mod.rs:165-169` (`end_script_layer` only sets `end_script`), `:196-201`, `:226-249`
- **Changed in**: `controller.rs` (commits `45b3b0a64`, `ad06d54b0`)
- **Status**: NEW
- **Description**:
  - The FO3/FNV death sting and both Oblivion stings set `silenced = true`.
  - Interior door transitions pass through a frame with no place, which clears the flag. A reload into the same cell or grid without such a frame leaves Battle and Location suspended as "a sting's silence" until the place changes, combat starts, or a script plays music.
  - `docs/engine/music.md` §9's "after a load, the place changes" is not guaranteed.
- **Impact**: Die, reload the autosave in the same area, and there is no cell music until the player leaves the area.
- **Related**: #6901
- **Suggested Fix**:
  - Clear `silenced` in the `end_script_layer` path (cell leave and load), or on the player-dead falling edge.
  - Add a controller test for sting → load → music returns.

### INC-11: Door-fade visuals depend on an `AnimatedAlpha` storage that boot never registers
- **Severity**: MEDIUM
- **Verdict**: CONFIRMED
- **Dimension**: gameplay Dim 5, ECS
- **Location**: `byroredux/src/cell_loader/relocated.rs:864-890` (`door_fade_system`, `query_mut::<AnimatedAlpha>()` at `:881`); the test registers the storage itself at `:1271`
- **Changed in**: `relocated.rs` (commits `4b71155ca`, `98aa53db9`)
- **Status**: NEW
- **Description**:
  - `query_mut` returns `None` for an unregistered component. `byroredux/src/boot/world.rs` registers `CrossSpaceHop` and `DoorFade` but not `AnimatedAlpha`, which only comes into existence lazily, when an alpha-animated clip inserts it.
  - Until such content is loaded, the door-fade ramp (XCT-ROUTE-008) is silently skipped, and so is `SetActorAlpha`.
  - The ramp test hides this by calling `world.register::<AnimatedAlpha>()`.
- **Evidence**: `git show HEAD:byroredux/src/boot/world.rs | grep -c AnimatedAlpha` gives 0. The only non-test `register::<AnimatedAlpha>` is absent.
- **Impact**: Actors pop through load doors instead of fading.
- **Suggested Fix**: Register `AnimatedAlpha` in `build_world`, and add a boot-registration test that exercises the storages `door_fade_system` needs.

### INC-12: `step_actor_relocations` now allocates every frame while idle
- **Severity**: MEDIUM
- **Verdict**: CONFIRMED
- **Dimension**: performance Dim 7
- **Location**: `byroredux/src/app_step.rs:1253-1267` (the `cells` / `loaded` build before the `relocation_step_key` early return)
- **Changed in**: `app_step.rs` (commit `4b71155ca`)
- **Status**: NEW
- **Description**:
  - The `cells` and `loaded` vectors, with the worldspace `String` cloned twice per loaded grid cell, are now built before the idle gate, and the old `!ledger.is_empty()` filter is gone.
  - A radius-7 exterior window is about 450 small allocations per frame that do nothing.
  - `ledger.digest()` also hashes every frame.
- **Suggested Fix**: Compute the key first (`loaded_cells_key` needs only the grids). Build `cells` and `loaded` only when there is a pending request, a due journey, or a changed key.

### INC-13: Saving during a door fade leaves `Traveled` without `CrossSpaceHop`, so the actor never passes
- **Severity**: MEDIUM
- **Verdict**: PLAUSIBLE. This is the agent's trace; I did not re-read `travel.rs:490-495` or the registry lines myself.
- **Dimension**: save, gameplay Dim 5
- **Location**: `byroredux/src/systems/travel.rs:490-495`, `byroredux/src/cell_loader/relocated.rs` (door-fade hop), `byroredux/src/save_io/registry.rs:115,338`
- **Changed in**: commit `4b71155ca`
- **Status**: NEW
- **Description**:
  - `Traveled` is saved; `CrossSpaceHop` and `DoorFade` are not.
  - The pass is now delayed by `fActorTeleportFadeSeconds`, where it used to happen within one frame. A save in that window restores the actor at the door with `Traveled` set.
  - `travel_system` then treats the actor as already arrived, and it never passes the door or reports the package end.
- **Impact**: The actor stays stuck at the door until its package changes.
- **Suggested Fix**: After a load, drop `Traveled` for an actor whose hop is gone, or resolve pending fades before the save capture.

### INC-14: The Papyrus placement effect now takes three more locks under the quest guards, and the #3949 nested-lock list was not updated
- **Severity**: MEDIUM (coverage gap in a guard; no deadlock, since every quest-resource system is exclusive)
- **Verdict**: PLAUSIBLE. I did not re-read the `#3949` scan's file list.
- **Dimension**: concurrency, scripting
- **Location**: `crates/scripting/src/fragment/effects/placement.rs:25-39`; the residual list in `crates/scripting/src/fragment/effects.rs` (around lines 95-200); the helpers in `crates/scripting/src/created_references.rs`
- **Changed in**: commit `4b71155ca`
- **Status**: NEW
- **Description**:
  - `apply_placement_effect` now reaches three more types through `created_references.rs` helpers: `PendingActorMoves` (W) through `queue_actor_move`, and `UnloadedReferenceStub` and `RemoteSceneActorStub` (R) through `is_logical_stub`.
  - `the_nested_lock_residual_list_names_every_type_apply_effect_acquires` follows only turbofish sites in the fragment sources and `equipment.rs`. It stays green, and the list names none of the three.
- **Suggested Fix**: Add the three types to the list, and extend the scan to the `created_references.rs` helpers.

### INC-15: A live load leaves `PendingRelocations` / `PendingActorMoves` / `TravelReports` from the outgoing session
- **Severity**: MEDIUM
- **Verdict**: PLAUSIBLE. This is the agent's trace of the frame order (`step_save_loads` before `step_cell_transition`).
- **Dimension**: save
- **Location**: `byroredux/src/save_io/live_load.rs` (`discard_outgoing_session_requests`)
- **Changed in**: new resources in commit `4b71155ca`
- **Status**: NEW
- **Description**:
  - A relocation raised earlier in the load frame (by `travel_system`, `door_fade_system` or a script) belongs to the outgoing timeline. It is applied by FormID in the loaded session, evicting and ledgering an actor there.
  - `TravelReports` also carries stale reasons into `actor.where` and the census.
- **Suggested Fix**: Clear all three in `discard_outgoing_session_requests`.

### INC-16: Battle intro and outro one-shots bypass suspension, so an outro can play after a death sting or during a movie
- **Severity**: LOW
- **Verdict**: PLAUSIBLE (agent trace)
- **Dimension**: audio Dim 3
- **Location**: `byroredux/src/music/controller.rs:282-315` (`step_battle`, `one_shot`)
- **Changed in**: `controller.rs` (commits `45b3b0a64`, `ad06d54b0`)
- **Status**: NEW
- **Description**:
  - `step_battle` runs before suspension is computed, and plays the intro and outro on voice 23 directly.
  - The death sting stops the Battle voices but keeps the directive. `in_combat` goes false, so `battle_hold` counts down and the NV outro plays over the death.
  - Hold (a movie) and Script-layer suspension are bypassed the same way.
- **Suggested Fix**: Skip the one-shot when `hold || silenced || script_holds`, or clear the Battle directive in `sting()`.

### INC-17: `ini_f32` accepts `inf` and `NaN`, and an infinite music fade panics in `Duration::from_secs_f32`
- **Severity**: LOW (local config file, not game-archive input)
- **Verdict**: CONFIRMED
- **Dimension**: audio Dim 3, robustness
- **Location**: `byroredux/src/game_ini.rs:54-56` (`ini_f32` is a bare `.parse()`); `byroredux/src/music/translate.rs:107` (`iMusicTransitionInMS`); `crates/audio/src/cache.rs:69,100` (`Duration::from_secs_f32(fade.max(0.0))`)
- **Changed in**: `game_ini.rs` (commit `45b3b0a64`)
- **Status**: NEW
- **Description**:
  - `iMusicTransitionInMS=inf` (or `1e39`) becomes an infinite fade, and `inf.max(0.0)` is still infinite, so `Duration::from_secs_f32` panics.
  - A `NaN` volume passes `clamp(0.0, 1.0)` unchanged.
- **Suggested Fix**: `.filter(|v| v.is_finite())` in `ini_f32`.

### INC-18: OBSE `SetCellMusicType` overrides are neither saved nor reset on an in-session load
- **Severity**: LOW
- **Verdict**: PLAUSIBLE. The premise of the allowlist reason ("lasts the session, like the original's") is unverified against xOBSE.
- **Dimension**: save, scripting
- **Location**: `crates/scripting/src/obscript_more/extras.rs` (`CellMusicTypes`), `byroredux/src/save_io/registry_completeness_tests.rs` (NOT_SAVED allowlist entry)
- **Changed in**: commit `d86ae6634`
- **Status**: NEW
- **Description**: Nothing clears `overrides` on load, so an in-session load keeps overrides that a fresh process would not have.
- **Suggested Fix**: Clear it on load (as session state), or save it.

### INC-19: `battle()` / `sting()` lack the `untranslated()` per-game decline, and the controller clones its directives every frame
- **Severity**: LOW
- **Verdict**: PLAUSIBLE (agent trace; nothing audible today)
- **Dimension**: audio Dim 5, performance Dim 1
- **Location**: `byroredux/src/music/translate.rs` (`battle`, `sting`), `byroredux/src/music/controller.rs:354`, `byroredux/src/music/mod.rs:147-152`
- **Changed in**: commits `45b3b0a64`, `bf42dca77`
- **Status**: NEW
- **Description**:
  - Skyrim, FO4 and Starfield walk the FNV marker and region path for combat and death, and `sting()` scans every `SOUN` linearly. Both resolve to Silent or Unresolved, so the problem is the missing explicit decline.
  - Every frame, each layer's directive is deep-cloned (playlist `Vec<String>`), and `current_place` builds lowercased `String`s.
- **Suggested Fix**: Gate both functions on `untranslated()`. Borrow `&Directive`, or `Arc` the playlist.

### INC-20: The explicit-LOD shader guard scans only two of the files that sample `textures[]` with an explicit LOD
- **Severity**: LOW
- **Verdict**: CONFIRMED by the renderer agent's glob. Every current site is view-relative, LOD 0, or a coarsest-mip clamp.
- **Dimension**: renderer (shader contract), test coverage
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` (`explicit_bindless_lods_are_view_relative_under_streaming`)
- **Changed in**: `shader_contract_tests.rs` (commit `f754eee28`)
- **Status**: NEW
- **Description**:
  - The guard checks only `include/ray_hit.glsl` and `triangle.frag`. Unscanned explicit-LOD samplers of `textures[]` remain in:
    - `include/material_sampling.glsl:45`
    - `include/clouds.glsl:177`
    - `include/sky.glsl:317,529`
    - `volumetrics_inject.comp:101`
  - `sky.glsl`, `volumetrics_inject.comp` and `ui.frag` declare `textures[]` without `bindings.glsl`, so they could not call `residentLod` even if they needed it.
- **Impact**: None today. A future full-chain absolute LOD at one of those sites would read mips up to `first_level` levels too coarse, with no validation error.
- **Suggested Fix**: Scan every shader source for `textureLod` / `texelFetch` on `textures[` and allow-list the reviewed sites.

### INC-21: Five tracked paths have no row in `_audit-owners.md`
- **Severity**: LOW
- **Verdict**: CONFIRMED (`git ls-files | .claude/commands/_audit-route.sh | grep unrouted`)
- **Dimension**: tech-debt (audit infrastructure)
- **Location**: `.claude/commands/_audit-owners.md`
- **Changed in**: not introduced in this window. The audit-skill re-sync (922072af8) rewrote the map without closing the gap.
- **Status**: NEW
- **Description**: The unrouted paths are `.claude/settings.json`, `.gitattributes`, `.gitignore`, `LICENSE`, and `screenshot_1790471263.png`. The PNG is a 167 KB stray screenshot committed at the repo root in de808add3 (2026-09-26). A change to `.claude/settings.json` (permissions, hooks) routes to no audit.
- **Suggested Fix**: Add a repo-root-config row (owner `audit-tooling`, LOW) covering `.claude/settings.json` and the git config files, and `git rm` the stray screenshot.

### INC-22: Doc comments left stale or attached to the wrong item by the delta
- **Severity**: LOW
- **Verdict**: PLAUSIBLE (agent trace; doc-only)
- **Dimension**: tech-debt (doc rot)
- **Location**:
  - `crates/plugin/src/equip.rs` (around lines 247-251): "Starfield … rule is a total no-op". This is now false: all 1 106 Starfield ARMAs carry BO64.
  - `crates/plugin/src/esm/cell/index.rs` (around lines 828-842): `interior_cell_key_of_refr` was inserted between `locate_refr` and its doc comment.
  - `crates/plugin/src/esm/records/misc/equipment.rs` (around lines 578-583): the new BO64 test sits under the #5358 BODT doc comment.
  - `byroredux/src/inventory/mod.rs:48`: "`MAX_BIPED_SLOTS = 32`".
  - `crates/audio/src/lib.rs:150-153` and `crates/plugin/src/esm/records/misc/world.rs:788-796`: both point at the deleted `dispatch_region_ambient_music`.
- **Changed in**: commits `7af55a0df`, `9405138e9`, `45b3b0a64`
- **Status**: NEW
- **Suggested Fix**: Move each doc comment back to its item. Restate the Starfield BO64 measurement, and repoint the REGN music references to `crate::music`.

## 4. Missing tests

Changed paths that have no test, or a test that misses the changed branch, even where the code is right:

- **Cross-cell travel** (`relocated.rs`):
  - `reconcile_logical_identities`, `promote`, `promote_for_interior` and `identity_stubs` have no unit tests (INC-2).
  - `depart_unwatched`, `player_passes_door`, and `resolve_actor_moves` with `ToPackageLocation` are untested.
  - The Papyrus `MoveTo` fallthrough has no test; only the ObScript branch does. There is no test that a player, non-actor or created-reference `MoveTo` is declined (INC-1).
  - There is no live-load test with a non-empty `ActorRelocations` (INC-3), and no save-during-fade test (INC-13).
  - There is no boot-registration test for the storages `door_fade_system` needs (INC-11).
- **Music**:
  - `music_system` has no test at all: first-frame catalog build, place/marker change gating, `end_script` ordering against `PendingMusic`, the death edge raising the sting, volume application, and the OBSE re-resolve.
  - The removed `play_music_is_no_one_shot_and_a_headless_session_only_drains_it` has no system-level replacement.
  - There are no failure-path tests for stems (INC-9), sting → load (INC-10), or the outro under a sting (INC-16).
  - `status_lines` is untested, and `ini_f32` has no non-finite case (INC-17).
- **Bink**:
  - Nothing checks that a reader-thread panic ends the movie as `Failed` (INC-6).
  - Nothing tests `dispatch_new_oneshots` under a muting hold (INC-7).
  - `Game.PlayBink` lowering has no test with a local, `akSpeaker`, or an inlined callee (INC-5).
  - No `video.rs` test shows that `finish` releases the hold after a `Failed` frame.
- **ObScript `~`** (#6905): only literal operands are tested. There is no test with a local or command-result operand, `~~`, or `- -`.
- **BO64**: no `capture.rs` test with an occupant at bit ≥ 32 (INC-4); no `SpaceGraph::build` test for worldspace-persistent-cell doors; no `PlacedRef::is_persistent` test; no test of an NVCI shorter than 16 bytes.
- **Renderer (#6629)**:
  - No test that `resolve_pending_set_writes` takes `first_level` from the live texture (or the fallback) rather than the queued one. The existing test only uses `first_level: 0`.
  - No device-free test that `write_first_level` skips the cube binding.
- **Tooling**:
  - forcegreet: only the pure `within_loaded_grid` is tested, not the system-level branch.
  - Debug server: no test that a non-`AddrInUse` bind error returns without retry.
  - `read_default_ini` / `plugin_game`: no test for a missing file, a case-mismatched filename, or a plugin with no parent folder.
  - `prune-target-cache.sh` has no harness.

## 5. Not verified

- No cargo test or lock-order detector run. The dynamic lock graph behind INC-8 and INC-14 is inferred from source, and the save fingerprint `0x9f1f_00da_2f6e_7e77` was not recomputed.
- No engine run. INC-7's boot-frame ordering, INC-10's load flow and INC-13's save window were not exercised.
- No vanilla-content count of `PlayerRef.MoveTo(<unloaded marker>)` in Skyrim (INC-1).
- Engine-behaviour claims in the music commits (Oblivion.exe / Fallout3.exe addresses, the OBSE opcodes `0x1617` / `0x19e5`) were taken as authored.

Next: `/audit-publish docs/audits/AUDIT_INCREMENTAL_2026-11-14.md`. Run it after the uncommitted INC-3 fix in the working tree lands, then confirm INC-3 against the new HEAD before publishing.
