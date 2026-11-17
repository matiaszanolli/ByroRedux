# #6893 — observation protocol (original game)

Question (XCT-OFF-002): an actor's Travel crossed spaces while the player was elsewhere. After the player enters the destination space, where does the actor stand?

| Outcome | Meaning | PRD consequence |
|---|---|---|
| **A** | At the package location (position within about 1 unit of the Travel target) | XCT-OFF-002 stands as written |
| **B** | At the far side of the load door it came through | XCT-OFF-002 changes to "far-side door" |
| **C** | Somewhere between the door and the target (neither A nor B) | needs a path model; record the numbers |
| **D** | Still in the origin space | the original has not moved it yet; XCT-OFF-001's timing needs a second look |

The same run also gives timing for #6892 (record the game hours).

## Rules that keep the observation clean (each is there for a reason)

- **Do not save and load between the trigger and the observation.** The original places every actor at its package location when a save loads (the disassembly shows the placement routine is called from the save-load function), so a reload would show outcome A whatever the live game does.
- **Do not use `Enable`, `Disable`, `MoveTo`, `ResetAI`, `EvaluatePackage` or `SetPos` on the test actor after the trigger.** `Enable` places an actor at its package location too.
- **Do not fast travel between trigger and observation.** The original's fast-travel routine runs the low-process update itself, which would change the timing.
- Use the in-game Wait (or just play) to pass time, and note the game clock before and after.
- Do not look at the actor while it travels. The point is that nobody is in the origin or the destination space.

## Route 1 — controlled test plugin (recommended; answers exactly this question)

Needs the GECK for Fallout: New Vegas (or Fallout 3). FNV is the PRD's reference game; run FO3 as well if you can.

Build, in a new plugin:

1. Three interior cells: **ObsA** (origin), **ObsB** (destination) and **ObsC** (a neutral waiting room that is not connected to the others).
2. A load door pair joining ObsA and ObsB. In ObsB, keep the arrival marker of that door and a test marker **M** at least 10 metres from it and in a different direction, so outcomes A, B and C are three different places.
3. A generic NPC **T** placed in ObsA, with the reference marked **Persistent**.
4. A global **ObsGo**, default 0.
5. A Travel package on T: location "near reference M", radius 0, no schedule restriction, with the condition `GetGlobalValue ObsGo == 1`. Give T no other package (no sandbox, no wander) so nothing competes.
6. Record the editor positions of M and of the ObsB door's arrival marker (X, Y, Z).

Run:

1. Start the game, open the console and enter ObsA (`coc ObsA`). Select T (`prid <T's reference ID>`), then read `getpos x`, `getpos y`, `getpos z` and `getincell ObsA`. Note the game clock.
2. Move to ObsC (`coc ObsC`). Do not use anything else on T.
3. Set the global: `set ObsGo to 1`. Note the game clock (this is the start time).
4. Wait. Pass time in steps of 1 game hour and, after each step, note the clock. Do not enter ObsA or ObsB yet.
5. After a chosen delay (try 2 game hours first), enter ObsB (`coc ObsB`) and, within a few seconds, select T and read `getpos x/y/z` and `getincell ObsB`. If you cannot select it, take a screenshot and note what you see.
6. Compare with the positions recorded in step 6 of the build, and classify A, B, C or D.

Repeat with different delays (30 game minutes, 2 hours, 8 hours) and with a fresh game each time (do not reuse a run, because a moved actor is no longer a test). For each delay, also run **control C1**: enter ObsA instead of ObsB and note whether T is still there (this is outcome D and tells you when it left).

## Route 2 — vanilla data, no GECK (shares the observation for #6891)

Fallout: New Vegas, quest VCG02 stage 10: Sunny Smiles (`SunnyRef`, 0x00104E85) leaves `GSProspectorSaloonInterior` for the package `VCG02SunnyTravelOutside`.

1. Before playing, open the package in the GECK (or FNVEdit) and write down its location type and target (reference, cell or coordinates) and any conditions. That target is the "package location" for outcome A.
2. Play to VCG02 stage 10 by the normal dialogue (the "Sunny goes outside" line), then leave the saloon at once, and travel away on foot far enough that Goodsprings' exterior cells around the saloon unload. Do not return.
3. Wait the game time you choose (note the clock).
4. Enter the package target's space and note where Sunny stands (`prid` her, `getpos`, `getincell`). Classify A, B, C or D against the package target from step 1 and the door she used.

Limit: in Route 2 she starts walking while you are in the saloon, so only the crossing and the arrival are unwatched. If you also want to know which door she left through, watch the door once on a separate run (that is #6891).

## Record sheet (one row per run)

| Run | Delay (game h) | Start clock | Observation clock | Space entered | T cell (`getincell`) | T x, y, z | A / B / C / D | Notes |
|---|---|---|---|---|---|---|---|---|

Also record: game (FNV or FO3), executable version, load order, and whether NVSE or JIP LN were running.

## What to send back

The filled record sheet and the editor positions of M and the door marker (Route 1) or the package target (Route 2). I will update `docs/prd/cross-cell-actor-travel.md` (XCT-OFF-002, and XCT-OFF-001 if timing differs), the verify table in `docs/engine/cross-cell-travel.md`, and close #6893 (and #6892 if the timing is recorded).

## Why this and not more disassembly

Eight static passes on `Fallout3.exe` decoded the placement routines (save load and `Enable` put an actor at its package location, position and cell) but not the trigger that handles an unwatched arrival during play; see `INVESTIGATION.md`. This run settles it directly.
