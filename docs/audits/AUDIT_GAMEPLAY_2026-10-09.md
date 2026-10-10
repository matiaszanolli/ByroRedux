# Gameplay Audit — 2026-10-09

**HEAD**: 3bcf6c8e8 · **Baseline**: `AUDIT_GAMEPLAY_2026-10-08.md` (`00f580e09`) · **Audited**: Dim 5 (NPC spawn → AI package → locomotion, in depth, streaming-area priority), Dim 2 (interaction / dialogue / loot persistence), Dim 7 (save coverage / stage order / determinism) · **Unchanged since baseline (skimmed)**: Dim 1 (only #5358/#5413 stamp and doc edits), Dim 3 (no consume/restoration change), Dim 4 (only #5357/#5383, physics and sound lock order), Dim 6 (only #5278/#5280 HUD hygiene)

**Scope**: `/audit-gameplay`, delta-first, run solo inside `/audit-suite --preset streaming-deep`. The area emphasis is `byroredux/src/npc_spawn/`, `byroredux/src/cell_loader/` and `byroredux/src/streaming/`. 81 commits landed since the baseline, and 12 of them touch that area. The in-area commits reviewed here:
- `4e58f241d` (#5376, ambient force-greet gates)
- `42aab4c09` (#5391, Eat/Sleep PLDT anchor)
- `9bdfdd1c6` (#5390)
- `edb5fbdfe` (#5359, NPC_.WNAM)
- `479414ffe` (#5358, BODT)
- `5b68792b7` (#5393/#5395, voice resolve)
- `203be9ed4` (#5379) and `faf8e5682` (#5384, cinematic re-adoption)
- `9bbe9304e` (#5418, detach pass)

I also read the unchanged in-area code these touch: `cell_loader/stream_snapshot.rs`, `cell_loader/reference_state.rs`, `cell_loader/interior_spawn.rs` and `cell_loader/references/synth_child.rs`.

**Evidence base**: no engine was launched.
- **Bin suite**: `cargo test -p byroredux --bin byroredux` on 1.96.0 gave 2743 passed, 0 failed, 55 ignored.
- **Plugin tests**: `cargo test -p byroredux-plugin --lib pack` gave 51 passed.
- **ESM censuses**: raw-ESM Python censuses over `FalloutNV.esm` and `Fallout3.esm`, with the scripts under `/tmp/audit/gameplay/scripts/`:
  - `declined_dialogue_census.py`: package-winner simulation under the engine's fail-open rule.
  - `pldt_census.py` / `incell_census.py`: Eat/Sleep PLDT location types and In-Cell target cells.
  - `nearref_targets.py` / `ref_order.py`: Near-Reference target bases, and actor-vs-target placement order.
  - `pkdd.py` / `sayto_census.py`: PKDD layout and Say-To packages.
- **GECK reference**: the local GECK wiki pages *Category:Packages* (Standard Location) and *Sleep Package*.

**Dedup applied**:
- **Already reported this suite**: EXT-D6/D1-2026-10-09-*, CONC-D5-2026-10-09-01 (the #5379/#5384 purge despawning only parentless roots; I found no different defect there), CONC-D7-2026-10-09-01/02.
- **Same-suite ESM scratch**: its D2-02 has the PKDD decode defect, so it is cross-referenced below and not re-filed.

## Executive Summary

| Severity | NEW | Regression | Existing (open) re-confirmed |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 3 | 0 | 2 (#4232, #5031) |
| LOW | 0 | 0 | 6 (#5286, #5287, #5430, #5431, #5432, #5434) |

All three new findings are in the streaming-area Dim 5 code. Two of them are incomplete parts of baseline fixes:
- **#5376** declines the ambient force-greet but leaves the declined package as the winner. Its PKDD Say-To gate is also inert; that half is filed by the ESM audit.
- **#5391** still anchors two of the five authored Eat/Sleep location types on wherever the actor happens to stand.

The third finding is older and was never reported: the stream-boundary snapshot rebuilds `Seated` as a bare marker.

The baseline fixes are in the code: #5376, #5390, #5391, #5392, #5393, #5394, #5395 and #5426 (#5376 and #5391 partly, as noted above).

## Invariant Matrix

| Invariant | Status | Evidence |
|---|---|---|
| Single damage path | VERIFIED | The production `Dead` inserts are unchanged: `combat.rs:363`, `water.rs:60`, `character.rs:1563`, `extensions/commands.rs:543`, `reference_state.rs:299`, plus `apply_starts_dead`. Every other hit sits after a `#[cfg(test)]`. |
| Death reconciled once | VERIFIED | `clear_ambient_behavior` (`ai_package.rs:642-712`) still removes `EatBehavior`/`SleepBehavior`/`EatSleepState`/`ForceGreetDirective`. #5391 added no per-actor runtime component that needs teardown (`EditorPlacement` is static identity). |
| Index stability | VERIFIED | No inventory row-removal path changed. #5358 touched only docs in `inventory.rs`. |
| Fail-closed consumables vs fail-open packages | VERIFIED (policy) / **DRIFTED (consequence)** | `package_conditions_pass` is unchanged (`ai_package.rs:37-57`). #5376 adds a confidence gate at *install*, not at *selection*, so a declined package still wins and nothing runs: GAME-D5-2026-10-09-01. |
| Stage order | VERIFIED | #5415 pins the SM dispatcher and force-greet Update order. `eat_sleep_system` sits inside the PostUpdate locomotion block. The `forcegreet_system` kill-switch gap is still #5430. |
| State saved or re-derived | VERIFIED (save) / **DRIFTED (stream boundary)** | On the save side, #5394 now saves `StoryEventAliasFill` (FORMAT_MAJOR 35), and the `EditorPlacement` and `EatSleepState` allowlist rows are accurate apart from #5458. On the stream side, the snapshot restores `Seated` without the pose or reservation that `Seated` implies: GAME-D5-2026-10-09-03. |
| Determinism | VERIFIED | The grep over the skill's file list plus `eat_sleep.rs`, `forcegreet.rs` and `dialogue_voice.rs` is empty. The only hit in the wider scan is `story_manager.rs:211`, where `StoryManagerRng` takes a `SystemTime` seed. That is documented `NOT_SAVED_BY_DESIGN` (it mirrors `DialogueRandomState`), predates the baseline and is owned by `/audit-scripting`, so it is not filed. |

## Findings

### MEDIUM

### GAME-D5-2026-10-09-01: #5376's decline leaves the declined Dialogue package as the active winner, so 56 FNV NPCs run no ambient procedure at any hour
- **Severity**: MEDIUM
- **Dimension**: NPC Spawn → AI Package Selection → Locomotion (Dim 5)
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:255-307` (Dialogue arm returns `None`), `:795-826` (spawn records `active_package_form_id`, installs nothing), `:971-1016` (runtime does the same; the winner never changes, so the actor is never re-handed a procedure)
- **Status**: NEW (an incomplete part of the #5376 fix; related to the open #5431)
- **Trigger**: FNV, at any hour, in any cell holding one of the affected NPCs. Examples: Goodsprings (`GSSunnySmiles`), Primm (`PrimmNCRTrooper1`–`4`), the Boomer front gate (`BoomerFrontGateGuard1/2`), Nellis (`NellisPearl`, `NellisGeorge`), Camp McCarran (`CaptainCurtis`, `AngelaWilliams`), and the Strip (`Benny`, `VMS18WhiteGloveGreeter`).
- **Description**: #5376 added three install gates to the ambient Dialogue arm: fully-modeled conditions, a player target and Conversation type. Selection is untouched. `select_active_package` still picks the first scheduled package whose conditions pass, and any uncatalogued CTDA function makes the whole list pass. When the arm then declines, `from_package` returns `None`, but the package FormID is stored as `active_package_form_id`. Every later minute tick re-selects the same package (`changed == false`), so the actor gets no `SandboxBehavior`, `TravelBehavior`, `PatrolBehavior`, `EatBehavior`, `SleepBehavior` or `FollowBehavior` for as long as the package's schedule covers the hour. All of these packages run at any time.

  A combat end clears the winner, but re-selection picks the same package again.

  The census (`declined_dialogue_census.py`) simulates selection at hours 3, 12 and 20 using the engine's known-function set. A package whose conditions are all modeled and that is reached first is counted as "uncertain" and skipped, so the result is a lower bound:
  - **56 NPC_ bases** have a declined Dialogue package as winner at every sampled hour. 55 are declined as unmodeled (fn 79 `GetQuestVariable` dominates) and Veronica as off-target.
  - **45–46 of them** have a later scheduled package with a runtime that would otherwise run. Examples:
    - `SunnySmilesStayAtCurrentLocation` (Travel)
    - `DefaultSandboxEditorLocation512` (the Primm troopers)
    - `DefaultPatrolWeaponDrawn` (the Boomer gate guards)
    - `QJChompsEatPackage12x1` (Eat)
    - `eldoradoTrooperSleepPackage2` (Sleep)
    - `FollowersRaulFollowPlayerLONG` (Follow)

  The only acknowledgement of this starvation is the fn-0 doc comment in `crates/scripting/src/condition.rs` ("fail-open let the package win at every hour and starved the scheduled Sleep/Eat procedures below it"). #5431 shows that mapping never fires, and #5431's fix removes it, so nothing addresses the starvation.
- **Evidence**:
  ```rust
  } else {
      log::debug!("#5376: Dialogue package {} ('{}') declined ambient install …");
      None                                   // ai_package.rs:296-307 — winner still recorded
  }
  …
  let active_package_form_id = active.map(|package| package.form_id);   // :972
  updates.push((actor, active_package_form_id, behavior, active_package_form_id != runtime.active_package_form_id));
  ```
- **Impact**: On the reference title, ambient NPCs in the opening area (Goodsprings) and in Primm, Nellis, Camp McCarran and the Strip stand at their spawn point all day and all night. Their scheduled sandbox, patrol, travel, eat, sleep and follow procedures never run, with only a debug-level log. Before #5376 these NPCs force-greeted the player; before `00f580e09` they idled the same way.
- **Related**: #5376 (closed), #5431 (open), GAME-D5-2026-10-08-01.
- **Suggested Fix**: Make the decline a selection decision for this case. When the Dialogue arm declines *because the pass was manufactured by fail-open*, continue the `active_package` scan to the next eligible package (for example, a confidence predicate passed into `select_active_package` for intrusive procedures). The fail-open policy stays as it is for every other procedure. Then pin a two-package test where the declined Dialogue package sits above a Sandbox package.

### GAME-D5-2026-10-09-02: #5391 still anchors In-Cell and Near-Linked-Reference Eat/Sleep packages on wherever the actor stands, and caches an unresolved Near-Reference target as the actor's position for the life of the package
- **Severity**: MEDIUM
- **Dimension**: Dim 5
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:149-161` (`eat_sleep_location`); `byroredux/src/systems/eat_sleep.rs:126-139` (one-shot cache), `:199-218` (`resolve_anchor`), `:289-294` (seat radius)
- **Status**: NEW (an incomplete part of the #5391 fix)
- **Trigger**: FNV/FO3. Any NPC whose winning Eat or Sleep package authors PLDT type 1 (In Cell) or type 6 (Near Linked Reference). Separately, a Near-Reference package that wins at spawn during a budgeted cell load or streaming load.
- **Description**: #5391 fixed Near Editor Location by adding `EditorPlacement`. Three of the remaining cases still land on the actor's current position:
  1. **In Cell.** The anchor is `current` and seats are searched within `radius.unwrap_or(SEAT_SEARCH_RADIUS)` (512 BU) of the arrival point. The GECK's Standard Location says "If 'In Cell' is selected, Radius is greyed out", meaning the whole cell is the location. All 70 FNV In-Cell Eat/Sleep packages author radius 0. A sleeper in a large interior whose bed is more than 512 BU from where it stands takes the nearest chair or nothing. This covers FNV 32 Eat and 142 Sleep NPC-default references (23% of all FNV Sleep references) and FO3 16 Eat and 22 Sleep references. All of the target cells are interiors (`incell_census.py`).
  2. **Near Linked Reference (type 6).** This falls to `_ => NearCurrentLocation`. The commit describes that fallback as "every type with no resolvable anchor", but type 6 is resolvable: `PackageTargetRegistry` already holds every placed reference's XLKR edges and the positions of both ends (`asset_provider/script.rs:716-791`, `linked_reference()` at `crates/scripting/src/package.rs:100`). This covers FNV 11 Eat and 47 Sleep references (e.g. `GamblerEatAtLinkedPackage`, `VRRCJackDianeSleep`) and FO3 2 (`DefaultSleepAlwaysLinkedRef`).
  3. **Unresolved Near Reference.** The target is resolved once and cached: `resolve_near_reference_target(...).unwrap_or(current)` is stored in `EatSleepState` and never retried. The reference loader walks references in authored order and stalls on each NPC job (`references/mod.rs:760`). The scheduler keeps running during budgeted loads (dt is pinned to 0, `app_events.rs:977`). So a spawn-time Eat/Sleep winner can resolve before its marker exists. In FNV, 163 actor × Near-Reference-package pairs place the target *after* the actor in the same cell, and 167 place it in another cell (`ref_order.py`). This leg depends on the frame budget. XMarker targets do resolve once spawned: `spawn_logical_quest_reference` stamps them, and they are the targets for 269 of 288 Eat and 230 of 293 Sleep references.
- **Evidence**:
  ```rust
  PackLocationTarget::Other(_) if location.location_type == 3 => EatSleepLocation::NearEditorLocation,
  _ => EatSleepLocation::NearCurrentLocation,          // types 2, 5, 6, 7
  …
  EatSleepLocation::InCell(cell_form_id) => cell_is_resident(world, cell_form_id).then_some(current),
  ```
- **Impact**: About 31% of FNV Sleep references, and the type-6 Eat references, sleep or dine wherever an earlier package left the actor rather than at the authored bed or table. A load-order race can freeze a Near-Reference diner at its spawn point. This is the same misroute class #5391 was filed for (GAME-D5-2026-10-08-03).
- **Related**: #5391 (closed), #5390, GAME-D5-2026-10-08-02, #5452 (PHYS: `step_toward` discards `blocked`), #5449 (PERF: furniture gather retry).
- **Suggested Fix**:
  - **Type 6**: resolve through `PackageTargetRegistry::linked_reference(<actor ACHR FormID>)` and `position()`.
  - **In Cell**: search seats across the resident cell (no radius cap) rather than 512 BU of the actor.
  - **Unresolved Near Reference**: leave `EatSleepState` unset, so the next tick retries, rather than caching `current`.

### GAME-D5-2026-10-09-03: The stream-boundary snapshot rebuilds `Seated` as a bare marker, without the sit pose, seat rotation or `SeatReservations` entry, so a seated NPC comes back standing in its chair and the seat can be double-booked
- **Severity**: MEDIUM
- **Dimension**: Dim 5 / Dim 7 (stream-boundary state continuity)
- **Location**: `byroredux/src/cell_loader/stream_snapshot.rs:301-389` (`restore_actor_snapshot`; `Seated` rebuilt at `:367`, translation-only restore at `:319`); called from `byroredux/src/cell_loader/references/synth_child.rs:91`, captured at `byroredux/src/cell_loader/unload.rs:344`; seating write path at `byroredux/src/systems/sandbox.rs:192-245`; reservation prune at `byroredux/src/cell_loader/references/mod.rs:1078-1098`
- **Status**: NEW. The code predates the baseline (EX-16 / #3299, unchanged since `99933f87b`), but it sits in this suite's streaming area and no issue covers it.
- **Trigger**: FO3/FNV exterior. A sandboxing or Eat/Sleep actor seated in a tile that radius streaming evicts and then reloads. It also triggers on interior → interior → back door walks, because the snapshot store is cleared only by `drain_streaming_state` and save reloads.
- **Description**: `apply_seat_assignments` is the only place that really seats an actor. It snaps the root's translation *and rotation* to the marker, parks the sit-enter clip's final frame (`playing = false`), and the caller reserves `(furniture, marker_idx)`. The snapshot carries only `seated_furniture_form_id` and `seated_animation_restore`. On respawn it writes `Seated { furniture, animation_restore }` and the translation, and nothing else:
  - The freshly spawned `AnimationPlayer` stays on the standing idle, and the root keeps the authored REFR rotation. The doc's §4 keep-set says "live position/orientation", but only the position is carried.
  - `Seated` is the one-shot "done" guard for `sandbox_seat_system` and `eat_sleep_system`, so neither ever re-seats the actor.
  - The eviction dropped the old claimant's reservation (`prune_seat_reservations` keeps only live claimant ↔ furniture pairs). The marker index is not in the snapshot, so the restore cannot re-reserve it either. `pick_nearest_seat` consults reservations only, so a second actor can be assigned the same marker and snapped on top of the first.

  The design doc deliberately drops "animation-phase/pose state" on the premise that "a fresh spawn re-entering its default pose for whatever package it resumes into" is harmless. Restoring `Seated` is exactly what stops the package from re-entering its pose.
- **Evidence**:
  ```rust
  seats.insert(entity, Seated { furniture, animation_restore });   // stream_snapshot.rs:367 — no park, no rotation, no reservation
  ```
  For contrast, `apply_seat_assignments`: `t.rotation = seat.rotation; … p.clip_handle = sit_handle; p.playing = false;` and `reservations.0.insert(seat_id, npc)` in its callers.
- **Impact**: After a tile round trip, seated NPCs stand upright inside their chairs, beds or benches, facing their editor direction, until their package next changes. Another sandboxer or diner can take the same seat, leaving two actors snapped to one marker. A save load does not show this, because `Seated` is excluded from the live overlay and the actor re-seats normally. Only the streaming and door paths are affected.
- **Related**: #3299 (closed, origin), #2392 / #2147 (reservation lifetime), #5458 (open: `Seated` allowlist claims).
- **Suggested Fix**: Either drop the `Seated` restore and let the package's own system re-seat the actor (the furniture is resident by construction when the restore resolves it), or carry `marker_idx` and the seat transform in the snapshot and restore through `apply_seat_assignments`, which also handles the reservation. In both cases, carry the rotation the doc already promises.

## Cross-referenced (not re-filed)

- **ESM D2-02 (this suite, `/tmp/audit/esm/dim_2.md`): PKDD Dialogue Type is read from byte 0.** `crates/plugin/src/esm/records/misc/pack.rs:818-829` reads `out.dialogue_type = sub.data[0]`. Byte 0 is the low byte of the FOV `f32` (100.0 = `0x42C80000`), and the Dialogue Type `u32` is at offset 16.
  - **Independent census**: byte 0 is 0 on 339/339 FNV and 395/395 FO3 PKDDs. The offset-16 value is Say-To on 21 FNV and 19 FO3 packages.
  - **Effect**: #5376's Say-To gate (`ai_package.rs:291`) is inert on both games.
  - **Gameplay addendum (Dim 5)**: four FO3 NPC-default Say-To packages pass the other two gates, all gated on GetStage (fn 58) in the Vault 101 tutorial. They are `CG02DadTowardsPlayerInDiner` and `CG02DadGreetPlayerAboutRadroach` (CG02Dad), `CG03JonasToPlayer` and `CG04Security02Ambush`. Each opens a full conversation menu where a single spoken line was authored. No FNV NPC-default Say-To package passes the other gates.
  - **Tests**: the unit test sets `PackRecord.dialogue_type = 1` by hand, and `force_greet_resolves_both_dialects` writes an 8-byte PKDD and never asserts the field.
  - **Ownership**: PACK decoders are `/audit-esm`'s, so the finding belongs to that report.
- **#5432 (GAME-D2-2026-10-08-03) is still open.** #5410 (`d1a7ee8c6`) added the stop hook `AudioWorld::stop_sounds_for` (`crates/audio/src/lib.rs:736`), and `dialogue_voice.rs:288` says it "gives the conversation-close path a handle". It has no production caller: `grep` finds only audio tests and doc comments. `apply_selection`, `end_open_conversation` and the speaker's death still leave queued segments playing. This is the tested-but-unwired class. Cite it in #5432 rather than re-filing.
- **CONC-D5-2026-10-09-01.** I reviewed the #5379/#5384 purge in `unload.rs:97-113`. I agree with the concurrency audit's finding and found no different gameplay-side defect there.

## Dimension notes

- **Dim 1**:
  - #5358 (BODT): only stamp and doc edits; the covered-bits gate is now live on Skyrim (80 addons skipped, first wins in authored order).
  - #5413: the player now gets `RaceSpells`.
  - #5359 (WNAM, `npc_spawn.rs:1297-1340`): reads `ResolvedNpc`'s Use-Traits terminal and falls back to `race.default_skin`.

  The guards are green and I found nothing new.
- **Dim 2**:
  - #5392 is verified: `forcegreet_system` gates on `player_can_act` and on `DialogueSurfaceState.npc` (`forcegreet.rs:66-85, 148-156`).
  - #5393 is verified: `voice_owner` resolves through `GlobalFormIdResolver`.
  - `reference_state.rs`, `interior_spawn.rs`, `attach.rs` and `synth_child.rs` are unchanged, and their guards are green.
- **Dim 3**: no consume or restoration change since the baseline.
- **Dim 4**: the `Dead` producer set is unchanged, and #5434 is still open.
- **Dim 5, four-list diff**: Eat, Sleep and Dialogue are still present in `from_package`, `insert_at_spawn`, `insert_at_runtime` and `clear_ambient_behavior`. `EditorPlacement` is registered in `crates/debug-server/src/registration.rs:299`. Both spawn paths (`prebaked.rs:93`, `runtime.rs:181`) stamp `EditorPlacement` through `spawn_placement_root`. The walk detector in `walk_anim` is motion-based, so it covers the Eat/Sleep and force-greet movers; only the ordering guard misses them (#5430).
- **Dim 6**: only HUD hygiene (#5278, #5280). #5286 and #5287 are still open.
- **Dim 7**: #5394 turned the `StoryEventAliasFill` row into a saved column (FORMAT_MAJOR 34 → 35, baseline refreshed). The `EditorPlacement` reason is accurate. The `EatSleepState`, `EatBehavior` and `SleepBehavior` rows still rely on "Seated, which is registered" (#5458, open).

## Known-Open Register

| Issue | State |
|---|---|
| #4232 `effective_actor_level` returns 0 | open, unchanged |
| #5031 mid-life gear queue keeps one root-less equip per wearer | open, unchanged |
| #5286 / #5287 | open |
| #5430 `forcegreet_system` outside the locomotion kill switch; guards blind to the new movers | open, unchanged (`update.rs:442`) |
| #5431 CTDA fn-0 `GetButtonPressed` premise | open, unchanged. GAME-D5-2026-10-09-01 is the starvation it was meant to address. |
| #5432 voice segments not stopped | open. The #5410 stop hook exists but is unwired. |
| #5433 / #5434 / #5435 | open |
| #5452 (PHYS), #5449 / #5448 (PERF), #5458 (SAVE), #5471 (doc rot), #5429 (FO3 guards) | open, cross-audit |
| #5367 | closed. Skyrim+ `.fuz` voice remains (#5464 documents the container). |

## Scratch-file reconciliation

I checked `/tmp/audit/gameplay/dim_1.md` … `dim_7.md` against this report.
- **`dim_5.md`**:
  - F2, F3 and F4 are the three findings above.
  - F1 (PKDD) is moved to the cross-reference: the ESM audit owns it.
  - The In-Cell per-frame `String` allocation in `cell_is_resident` is dropped. It is a performance concern and is already in the performance scratch.
- **`dim_7.md`**: the `story_manager.rs:211` `SystemTime` seed is dropped. It is by design, documented, predates the baseline and is owned by `/audit-scripting`.
- **`dim_1`, `dim_3`, `dim_4`, `dim_6`**: no findings.
- **`dim_2.md`**: the #5432 note is carried in the cross-reference section.

## Summary

| Severity | NEW | Existing (tracked) |
|---|---|---|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 3 | 2 |
| LOW | 0 | 6 |

Suggested next step: `/audit-publish docs/audits/AUDIT_GAMEPLAY_2026-10-09.md`. Suggested labels:
- **GAME-D5-2026-10-09-01**: `gameplay` + `ai`, `game:fnv`.
- **GAME-D5-2026-10-09-02**: `gameplay` + `ai`, `game:fnv`.
- **GAME-D5-2026-10-09-03**: `gameplay` + `ai` + `terrain-exterior`.
