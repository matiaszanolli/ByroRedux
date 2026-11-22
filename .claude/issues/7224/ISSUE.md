# #7224: GAME-2026-11-22-01: Must Complete is honoured for procedures that never report done, so an Eat/Sleep/Patrol/Follow package outlives its window forever

**Labels**: medium,gameplay,ai,bug,game:fnv,game:oblivion

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-01**), audited at HEAD 2b40f76b6.

- **Severity**: MEDIUM
- **Dimension**: 5 (NPC spawn → AI package selection)
- **Domain**: ai · **Game**: FNV, Oblivion
- **Trigger**: FNV and Oblivion (same PKDT bit 2). An actor whose first-open Eat, Sleep, Patrol, Follow, Use Item At, Accompany or Wander package carries *Must Complete* and has a lower-priority package that should take over when its window closes. Vanilla example: FNV `VSpecialViperShadow*` (`NPC_` 0x168bd7, 0x168bd6, …):
  - `VSpecialViperShadowSleepAny22x10` (Sleep 22:00–08:00, flags 0x81406)
  - `...SupperCampAny20x2` and `...LunchCampAny11x2` (Eat, bit 2 set)
  - `...BreakfastCampAny8x2`
  - `...SandboxLinked`

  Any save state. A normal game day ends the window.
- **Location**: `byroredux/src/npc_spawn/package_flags.rs:34-45` (`procedure_done`, the `_ => false` arm), `:76-82` (`kept_past_window`), `:106-117` (`scan_with_flags`); caller `byroredux/src/npc_spawn/ai_package.rs:991-1005` (`select_active_package`)
- **Status**: NEW
- **Description**: AIP-FLG-002 keeps a Must Complete package winning past its window "until its procedure reports done: Travel arrived, Find count met, UseWeapon bursts fired". `procedure_done` implements those three and answers `false` for every other procedure, so `kept_past_window` makes the running package win with no end. Because it is the earlier entry in the priority list, nothing below it can win again. The Viper Shadow NPC enters Sleep at 22:00 and never leaves it, so breakfast, lunch, supper and the sandbox package never run.

  The GECK pages in the local reference say Must Complete means nothing for several of these procedures:
  - Sandbox: "Invalid".
  - Patrol: "a repeatable patrol has no notion of 'Must Complete' (like a wander)".
  - Follow: "Disabled unless End Location is checked".
  - Guard: "can never complete".

  For Eat and Sleep no source says either way. Keeping them forever is an invented rule, not a declined case: nothing is logged and `pack.ambient` shows no decline.
- **Evidence**: A read-only scan of `FalloutNV.esm` covered all 3,816 NPCs, using PKID order and PSDT hour windows and ignoring conditions and day-of-week, so it is an upper bound. 117 NPCs have a Must Complete package of a no-signal procedure that wins in some hour, followed by a lower-priority package that would win after its window closes. By procedure: Eat 166 pairs, Sleep 114, Follow 16, Patrol 11. `Oblivion.esm`: 14 NPCs (Sleep 6, Use Item At 5, Eat 3, Accompany 2, Wander 1). GECK text: `/mnt/data/src/reference/geck-uesp-wiki/(main)/{Sandbox,Patrol,Follow} Package.wiki`.
- **Impact**: The actor is pinned to its first Must Complete package for the rest of the session: it loses its day/night cycle and stays seated or asleep through the day. The wrong package also reports as the running one to package fragments and `GetIsCurrentPackage`.
- **Related**: AIP-FLG-002 (`docs/prd/ai-packages-completion.md` §5.7), #6656
- **Suggested Fix**: Honour Must Complete only for procedures that report done (Travel, Find, UseWeapon) and treat it as unset elsewhere. Show the ignored flag in `pack.ambient`. Add a test that a Must Complete Sleep package yields to the next open package when its window ends.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
