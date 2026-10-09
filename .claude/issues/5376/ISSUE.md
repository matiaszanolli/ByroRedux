# #5376: GAME-D5-2026-10-08-01: Ambient Dialogue-procedure wiring + fail-open package conditions = 60 FNV NPCs force-greet the player unconditionally on every spawn and load; a failed open is never consumed

**Labels**: high,gameplay,ai,dialogue,bug,game:fnv,game:fo3
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5376

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-08.md` — `GAME-D5-2026-10-08-01` (HEAD `00f580e09`)

- **Severity**: HIGH
- **Dimension**: NPC Spawn → AI Package Selection → Locomotion (Dim 5) / Interaction (Dim 2)
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:209-219` (the `from_package` Dialogue arm), `:410-418` / `:529-536` (installed at spawn and at runtime), `:36-56` (`package_conditions_pass`); `byroredux/src/systems/forcegreet.rs:69-124`; `crates/scripting/src/condition.rs:197-206`
- **Status**: NEW
- **Trigger**: FNV, any cell holding one of the affected NPCs, e.g. Goodsprings (Sunny Smiles, `GSSunnySmiles` 0x104E84), at any hour, with fresh quest state; repeats on every save load and after every combat.
- **Description**: Before `00f580e09` the only installer of `ForceGreetDirective` was the `dialogue.forcegreet` console door (the `forcegreet.rs` module doc still says vanilla lists these packages on 0 NPC_ defaults). Now any winning PKDT-15 package installs the directive, both at spawn and at re-selection.

  The selector is `active_package`, which takes the first package whose schedule matches and whose conditions pass, and package conditions fail open: any `ConditionFunction::Unknown` makes the whole list pass. The PACK census over FalloutNV.esm, simulated with the engine's own known-function set and schedule rule, gives the same result at hours 3, 12 and 20: 60 NPC_ base records whose first eligible package is a Dialogue package.
  - 56 of them pass only because a condition fails open. The unknown functions are fn 79 `GetQuestVariable` (59 occurrences), then fns 50, 53, 310, 136, 289, 612 and 421.
  - 4 carry no conditions at all.
  - Examples: `VFSGrannyGangerMugPackage`, `VRRCRepConsequenceBark`, `ElDoradoTrooperBarkPackage`, `VHDLegionOliverFightLegateWalkOut`, `VRRCMessengerDialogue`, Sunny's `SunnyMeetPlayerDialoguePackage`.

  The commit adds a CTDA fn 0 → `GetButtonPressed` mapping to stop exactly this for Sunny, but it cannot help:
  - Sunny's package condition is `CTDA 00000000 00000000 4f000000 …`. That is function 79 (`GetQuestVariable` on quest 0x104C66), still `Unknown`, so it still fails open.
  - FalloutNV.esm and Fallout3.esm author zero fn-0 CTDAs anywhere.

  Three further problems:
  - **No consumption on failure.** `forcegreet_open` returns `false` when the topic is missing or no INFO passes. `forcegreet_system` consumes the directive only on success or refusal, so the NPC keeps stepping toward the player (or standing within 128 units) and re-runs `select_on_topic` every frame for as long as it carries the directive.
  - **Re-greets on every load.** Saved state cannot suppress a re-greet: the directive is re-derived by `reseat_ambient_packages_after_restore` on every load and on every combat end (`suspend_ambient_behavior_for_combat` clears the winner).
  - **Wrong target and dialogue type.** The procedure's PTDT target and PKDD Dialogue Type are ignored. Of the 141 NPC-default references, 4 target a non-player reference (e.g. `FollowersVeronicaForceGreetIntercom` → 0xE27FC) and 5 are "Say To". The GECK defines Say To as one spoken line with no dialogue menu. Both still force a player conversation.
- **Evidence**:
  ```rust
  } else if package.procedure_type == PROCEDURE_DIALOGUE {
      Some(Self::Dialogue { topic: package.dialogue_topic })   // no PTDT / dialogue-type / condition-confidence check
  ```
  ```rust
  if forcegreet_open(world, npc, directive.topic) { opened.push(npc); }  // false ⇒ directive kept, retried next frame
  ```
- **Impact**: On FNV, the reference title, NPCs whose force-greets vanilla gates on quest state open a conversation with the player at first contact, regardless of quest progress. They do it again after every load and every fight. With ECS-2026-10-08-D6-01 the walk step is currently discarded, so today the conversation opens when the player comes within 128 BU. Once that bug is fixed, the NPC walks to the player. A greet whose INFO pool is empty leaves the NPC glued to the player.
- **Related**: ECS-2026-10-08-D6-01, PHYS-D4-2026-10-08-01 (walk mechanics); #5367; GAME-D5-2026-10-08-05 (the fn-0 mapping); GAME-D2-2026-10-08-01.
- **Suggested Fix**: Install the force-greet only from a Dialogue package whose conditions were *all* evaluable (do not let the fail-open pass arm a player-control takeover), whose PTDT targets the player, and whose PKDD type is Conversation. Consume or cool down the directive when the open fails. Drop or justify the fn-0 mapping.

## Addendum (from `AUDIT_FO3_2026-10-08.md`) to GAME-D5-2026-10-08-01 (FO3 population, measured independently; not a new finding)
- **Status**: Related: GAME-D5-2026-10-08-01 (HIGH, `/audit-gameplay`). The same mechanism reaches FO3 at the same scale, so publish that finding with `game:fo3` as well as `game:fnv`.
- **Evidence** (`/tmp/audit/fo3/py/fo3_pack_census.py` over `Fallout3.esm`; the full output is in `/tmp/audit/fo3/pack_census.log`). The engine's known-function set and schedule rule were mirrored from `crates/scripting/src/condition.rs:206-227`, and NPC_ TPLT inheritance follows ACBS template flag 0x20:
  ```
  PACK 3266, procedure-15 (Dialogue) 386; on NPC_ PKID lists: 341 packs / 365 refs; on CREA lists 27 (no CREA package runtime)
    PTDT target of those 365 refs: player 186, ANOTHER REFERENCE 179;  PKDD type: Conversation 345, Say-To 15
  always-winning Dialogue package (fail-open or unconditioned), NPC_ base records:
    hour 3: 58 (56 fail-open, 2 no-cond) · hour 12: 59 (57, 2) · hour 20: 60 (58, 2)
    unknown fns gating them: 79 GetQuestVariable x39, 53, 50, 45, 84, 74, 289, 161
    hour-12 split: 48 player-targeted Conversations, 11 target another ref, 5 Say-To total
  placements of the hour-12 set: 221 ACHRs over 51 bases; 160 in exterior cells
    Vault101d: CG02Amata (CG02AmataFindPlayer), CG04Amata, CG04Butch, CG04/CG02 Officer Gomez  · Vault101aMS16: 2 Gomez
    Megaton: HardenSimms (MS11ForcegreetPlayer), MegatonSettler01 (gift), Jericho (DEMOMegJerichoDialogueJennyStahl -> targets Jenny Stahl)
    also ThreeDog (ThreeDogDialogPackageFirstTimePlayer), Betty (MQ04BettyKillPlayerDialogue), Dukov, OldLadyDithers, the 12 FFER79/FFHitSquad encounter actors
  fn-0 CTDAs anywhere in PACK/NPC_/CREA/DIAL/INFO: 0
  ```
- **What is FO3-distinctive**:
  1. The force-greets sit in **Vault 101**, FO3's opening (Sunny Smiles is FNV's opening equivalent). Its tutorial actors force-greet on cell load whatever the CG02/CG04 stage is.
  2. **Non-player targets are far more common**: 179 of 365 FO3 Dialogue-package refs (49%) name another reference in `PTDT`, against 4 of 141 on FNV per GAME-D5-01. GAME-D5-01's "wrong target" bullet is therefore a large class on FO3. Examples are Jericho→Jenny Stahl and Officer Gomez→Butch, which both become player conversations.
  3. `forcegreet.rs:7-9` still says vanilla FO3/FNV lists these packages on "0 references across both masters". On FO3 it is 365 NPC_ references, so that premise is false for both masters.
- **Fixture reach**: none of the 51 bases is placed in `MegatonPlayerHouse` or `MegatonMoriartysSaloon`, so the declared `fo3.env` gates (P0 / P2 Nova / P5) should not be hijacked. This was not live-verified.

## Completeness Checks
- [ ] **SIBLING**: other intrusive procedures installed from fail-open package conditions (Escort/Follow/Dialogue) checked
- [ ] **TESTS**: A regression test pins this specific fix
