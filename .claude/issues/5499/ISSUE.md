# #5499: GAME-D5-2026-10-09-01: #5376's decline leaves the declined Dialogue package as the active winner, so 56 FNV NPCs run no ambient procedure at any hour

**Labels**: ai, bug, dialogue, game:fnv, game:fo3, gameplay, medium

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-09.md` — finding `GAME-D5-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

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

---

## Addendum to GAME-D5-2026-10-09-01 — FO3 population (not new; recommend `game:fo3`)

*(From `AUDIT_FO3_2026-10-09.md`.)*

- **Census**: `py/declined_dialogue_census.py` is the gameplay audit's script, pointed at `Fallout3.esm`, with a `CREA` variant. It uses the same known-function set and lower-bound "uncertain" rule.
  - **NPC_**: a declined Dialogue package is the winner for 55 / 56 / 57 bases at hours 3 / 12 / 20.
  - 53 / 54 / 55 of them have a later scheduled package whose runtime would otherwise run.
  - **CREA**: 9 bases at every hour, all with a fallback. Examples: `FFEU255Dogmeat` → `FollowersDogmeatFiredWaitV101`, `MQ08Fawkes`, `RHSProtectron` → `ProtectronPatrol`, `MQ03Robobrain`/`2`. FNV-2026-10-09-D5-01 confirms that creatures do run packages.
- **FO3-distinctive placements**:
  - The **Vault 101 opening**: `CG02Amata`, `CG04Amata`, `CG04Butch`, `CG04Vault101Security04` and `CG02Vault101Security04` (Gomez), and `Vault101Security04/07`.
  - **Megaton**: `MQ01Silver`, `MegatonSettler01` → `MegSettler1PatrolMid10x4`, `HardenSimms` and `Jericho`.
  - The **FFER79 / FFHitSquadDC encounter squads** (12 bases): their `…FollowLeader` packages never run, so the squads stand where they spawned.
- PKDD Say-To on FO3 (17 packs, including the CG02Dad/CG03Jonas Vault 101 tutorial) is ESM-2026-10-09-D2-02, not re-filed.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
