# Locomotion clip census

**Generated** by `cargo run --release -p byroredux-nif --example locomotion_census -- --write`
(PRD [character-movement](../prd/character-movement.md), MOT-DX-005). Do not edit by hand.

It reads the vanilla base-game, patch and DLC archives of each installed game. It holds file
names, labels, counts and one rounded speed per base-set gait, and no keys, curves or
timings (MOT-SUP-007). A role comes from a clip's file name, so it says what is named, not
what a behaviour graph plays. "Read" means the engine's own decoder accepts the clip.

## Archives read

| Game | Archives | Humanoid skeleton |
|---|---|---|
| Oblivion | `Oblivion - Meshes.bsa`, `Knights.bsa`, `DLCBattlehornCastle.bsa`, `DLCFrostcrag.bsa`, `DLCHorseArmor.bsa`, `DLCOrrery.bsa`, `DLCShiveringIsles - Meshes.bsa`, `DLCThievesDen.bsa`, `DLCVileLair.bsa` | not needed |
| Fallout 3 | `Fallout - Meshes.bsa`, `Anchorage - Main.bsa`, `BrokenSteel - Main.bsa`, `PointLookout - Main.bsa`, `ThePitt - Main.bsa`, `Zeta - Main.bsa` | not needed |
| Fallout New Vegas | `Fallout - Meshes.bsa`, `Update.bsa`, `DeadMoney - Main.bsa`, `HonestHearts - Main.bsa`, `OldWorldBlues - Main.bsa`, `LonesomeRoad - Main.bsa`, `GunRunnersArsenal - Main.bsa`, `CaravanPack - Main.bsa`, `ClassicPack - Main.bsa`, `MercenaryPack - Main.bsa`, `TribalPack - Main.bsa` | not needed |
| Skyrim LE | `Skyrim - Animations.bsa` | decoded |
| Skyrim SE | `Skyrim - Animations.bsa` | decoded |
| Fallout 4 | `Fallout4 - Animations.ba2`, `DLCRobot - Main.ba2`, `DLCworkshop01 - Main.ba2`, `DLCCoast - Main.ba2`, `DLCworkshop02 - Main.ba2`, `DLCworkshop03 - Main.ba2`, `DLCNukaWorld - Main.ba2` | decoded |
| Fallout 76 | `SeventySix - Animations.ba2` | not decoded: COM tracks unchecked |
| Starfield | `Starfield - Animations.ba2`, `ShatteredSpace - Main01.ba2` | not needed |

## Base set by role

| Game | Base set | gait cycle | turn | start | stop | reversal | airborne | no role |
|---|---|---|---|---|---|---|---|---|
| Oblivion | `characters/_male/` `walk*`, `jump*` | 8 | 2 | 0 | 0 | 0 | 3 | 0 |
| Fallout 3 | `characters/_male/locomotion/male/mt*`, `locomotion/mtturn*`, `_male/mtjump*` | 8 | 2 | 0 | 0 | 0 | 10 | 0 |
| Fallout New Vegas | `characters/_male/locomotion/male/mt*`, `locomotion/mtturn*`, `_male/mtjump*` | 8 | 2 | 0 | 0 | 0 | 10 | 0 |
| Skyrim LE | `actors/character/animations/` `male/mt_{walk,run,sprint}*`, `male/npc_turn*`, `mt_turn*`, `mt_jump*` | 17 | 10 | 4 | 0 | 0 | 12 | 0 |
| Skyrim SE | `actors/character/animations/` `male/mt_{walk,run,sprint}*`, `male/npc_turn*`, `mt_turn*`, `mt_jump*` | 17 | 10 | 4 | 0 | 0 | 12 | 0 |
| Fallout 4 | `actors/character/animations/mt/neutral/` | 4 | 12 | 14 | 9 | 4 | 11 | 160 |
| Fallout 76 | `actors/character/animations/mt/neutral/` | 30 | 12 | 14 | 9 | 4 | 16 | 167 |
| Starfield | `actors/human/animations/melee/unarmed/relaxed_*` | 12 | 6 | 16 | 32 | 6 | 1 | 99 |

## Gait cycles: what each carries

A footfall label names a foot's step; a sync label alone does not count.

| Game | Read | With a footfall label | With a displacement source | With a `Blend` key |
|---|---|---|---|---|
| Oblivion | 8 / 8 | 8 / 8 | 8 / 8 | 0 / 8 |
| Fallout 3 | 8 / 8 | 8 / 8 | 8 / 8 | 8 / 8 |
| Fallout New Vegas | 8 / 8 | 8 / 8 | 8 / 8 | 8 / 8 |
| Skyrim LE | 17 / 17 | 17 / 17 | 0 / 17 | 0 / 17 |
| Skyrim SE | 17 / 17 | 17 / 17 | 0 / 17 | 0 / 17 |
| Fallout 4 | 4 / 4 | 4 / 4 | 4 / 4 | 0 / 4 |
| Fallout 76 | 0 / 30 | 0 / 30 | 0 / 30 | 0 / 30 |
| Starfield | 0 / 12 | 0 / 12 | 0 / 12 | 0 / 12 |

## All base-set locomotion clips: displacement source

A source counts when the clip's net horizontal travel is at least one game unit. A small travel
on a root or COM track may be a shift of weight and not locomotion.

| Game | Role | accumulation root | extracted motion | root or COM track | none read | not read | with a footfall label |
|---|---|---|---|---|---|---|---|
| Oblivion | gait cycle | 8 | 0 | 0 | 0 | 0 | 8 |
| Oblivion | turn | 0 | 0 | 0 | 2 | 0 | 0 |
| Oblivion | airborne | 0 | 0 | 0 | 3 | 0 | 1 |
| Fallout 3 | gait cycle | 8 | 0 | 0 | 0 | 0 | 8 |
| Fallout 3 | turn | 0 | 0 | 0 | 2 | 0 | 0 |
| Fallout 3 | airborne | 5 | 0 | 0 | 5 | 0 | 4 |
| Fallout New Vegas | gait cycle | 8 | 0 | 0 | 0 | 0 | 8 |
| Fallout New Vegas | turn | 0 | 0 | 0 | 2 | 0 | 2 |
| Fallout New Vegas | airborne | 5 | 0 | 0 | 5 | 0 | 4 |
| Skyrim LE | gait cycle | 0 | 0 | 0 | 17 | 0 | 17 |
| Skyrim LE | turn | 0 | 0 | 0 | 10 | 0 | 0 |
| Skyrim LE | start | 0 | 0 | 1 | 3 | 0 | 0 |
| Skyrim LE | airborne | 0 | 0 | 9 | 3 | 0 | 0 |
| Skyrim SE | gait cycle | 0 | 0 | 0 | 17 | 0 | 17 |
| Skyrim SE | turn | 0 | 0 | 0 | 10 | 0 | 0 |
| Skyrim SE | start | 0 | 0 | 1 | 3 | 0 | 0 |
| Skyrim SE | airborne | 0 | 0 | 9 | 3 | 0 | 0 |
| Fallout 4 | gait cycle | 0 | 4 | 0 | 0 | 0 | 4 |
| Fallout 4 | turn | 0 | 0 | 0 | 12 | 0 | 2 |
| Fallout 4 | start | 0 | 14 | 0 | 0 | 0 | 14 |
| Fallout 4 | stop | 0 | 8 | 1 | 0 | 0 | 9 |
| Fallout 4 | reversal | 0 | 4 | 0 | 0 | 0 | 4 |
| Fallout 4 | airborne | 0 | 5 | 5 | 1 | 0 | 7 |
| Fallout 76 | gait cycle | 0 | 0 | 0 | 0 | 30 | 0 |
| Fallout 76 | turn | 0 | 0 | 0 | 0 | 12 | 0 |
| Fallout 76 | start | 0 | 0 | 0 | 0 | 14 | 0 |
| Fallout 76 | stop | 0 | 0 | 0 | 0 | 9 | 0 |
| Fallout 76 | reversal | 0 | 0 | 0 | 0 | 4 | 0 |
| Fallout 76 | airborne | 0 | 0 | 0 | 0 | 16 | 0 |
| Starfield | gait cycle | 0 | 0 | 0 | 0 | 12 | 0 |
| Starfield | turn | 0 | 0 | 0 | 0 | 6 | 0 |
| Starfield | start | 0 | 0 | 0 | 0 | 16 | 0 |
| Starfield | stop | 0 | 0 | 0 | 0 | 32 | 0 |
| Starfield | reversal | 0 | 0 | 0 | 0 | 6 | 0 |
| Starfield | airborne | 0 | 0 | 0 | 0 | 1 | 0 |

## Base-set clips one by one

Clips the engine can read. A speed is given for gait cycles only: the horizontal travel of the
displacement source over the clip's length, in game units a second, before any playback-rate
scale.

| Game | Clip | Role | Speed | Displacement | Footfall and sync labels |
|---|---|---|---|---|---|
| Oblivion | `walkbackward` | gait cycle | 109 | accumulation root | Enum: Left, Enum: Right |
| Oblivion | `walkfastbackward` | gait cycle | 320 | accumulation root | Enum: Left, Enum: Right |
| Oblivion | `walkfastforward` | gait cycle | 320 | accumulation root | Enum: Left, Enum: Right |
| Oblivion | `walkfastleft` | gait cycle | 250 | accumulation root | Enum: Left, Enum: Right |
| Oblivion | `walkfastright` | gait cycle | 250 | accumulation root | Enum: Left, Enum: Right |
| Oblivion | `walkforward` | gait cycle | 109 | accumulation root | Enum: Left, Enum: Right |
| Oblivion | `walkleft` | gait cycle | 81 | accumulation root | Enum: Left, Enum: Right |
| Oblivion | `walkright` | gait cycle | 81 | accumulation root | Enum: Left, Enum: Right |
| Oblivion | `walkturnleft` | turn | - | none read | - |
| Oblivion | `walkturnright` | turn | - | none read | - |
| Oblivion | `jumpland` | airborne | - | none read | Enum: Left, Enum: Right |
| Oblivion | `jumploop` | airborne | - | none read | - |
| Oblivion | `jumpstart` | airborne | - | none read | - |
| Fallout 3 | `mtbackward` | gait cycle | 67 | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtfastbackward` | gait cycle | 282 | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtfastforward` | gait cycle | 354 | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtfastleft` | gait cycle | 354 | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtfastright` | gait cycle | 354 | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtforward` | gait cycle | 85 | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtleft` | gait cycle | 68 | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtright` | gait cycle | 68 | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtturnleft` | turn | - | none read | - |
| Fallout 3 | `mtturnright` | turn | - | none read | - |
| Fallout 3 | `mtjumpland` | airborne | - | none read | - |
| Fallout 3 | `mtjumplandbackward` | airborne | - | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtjumplandforward` | airborne | - | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtjumplandleft` | airborne | - | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtjumplandright` | airborne | - | accumulation root | Enum: Left, Enum: Right |
| Fallout 3 | `mtjumploop` | airborne | - | accumulation root | - |
| Fallout 3 | `mtjumploopbackward` | airborne | - | none read | - |
| Fallout 3 | `mtjumploopforward` | airborne | - | none read | - |
| Fallout 3 | `mtjumploopleft` | airborne | - | none read | - |
| Fallout 3 | `mtjumploopright` | airborne | - | none read | - |
| Fallout New Vegas | `mtbackward` | gait cycle | 67 | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtfastbackward` | gait cycle | 282 | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtfastforward` | gait cycle | 354 | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtfastleft` | gait cycle | 354 | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtfastright` | gait cycle | 354 | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtforward` | gait cycle | 85 | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtleft` | gait cycle | 87 | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtright` | gait cycle | 86 | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtturnleft` | turn | - | none read | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtturnright` | turn | - | none read | Enum: Left |
| Fallout New Vegas | `mtjumpland` | airborne | - | none read | - |
| Fallout New Vegas | `mtjumplandbackward` | airborne | - | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtjumplandforward` | airborne | - | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtjumplandleft` | airborne | - | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtjumplandright` | airborne | - | accumulation root | Enum: Left, Enum: Right |
| Fallout New Vegas | `mtjumploop` | airborne | - | accumulation root | - |
| Fallout New Vegas | `mtjumploopbackward` | airborne | - | none read | - |
| Fallout New Vegas | `mtjumploopforward` | airborne | - | none read | - |
| Fallout New Vegas | `mtjumploopleft` | airborne | - | none read | - |
| Fallout New Vegas | `mtjumploopright` | airborne | - | none read | - |
| Skyrim LE | `mt_runbackward` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_runbackwardleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_runbackwardright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_runforward` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_runforwardleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_runforwardright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_runleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_runright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_sprintforward` | gait cycle | - | none read | FootSprintLeft, FootSprintRight |
| Skyrim LE | `mt_walkbackward` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_walkbackwardleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_walkbackwardright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_walkforward` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_walkforwardleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_walkforwardright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_walkleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_walkright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim LE | `mt_turnleft180` | turn | - | none read | - |
| Skyrim LE | `mt_turnleft360` | turn | - | none read | - |
| Skyrim LE | `mt_turnleft60` | turn | - | none read | - |
| Skyrim LE | `mt_turnright180` | turn | - | none read | - |
| Skyrim LE | `mt_turnright360` | turn | - | none read | - |
| Skyrim LE | `mt_turnright60` | turn | - | none read | - |
| Skyrim LE | `npc_turnleft180` | turn | - | none read | - |
| Skyrim LE | `npc_turnleft90` | turn | - | none read | - |
| Skyrim LE | `npc_turnright180` | turn | - | none read | - |
| Skyrim LE | `npc_turnright90` | turn | - | none read | - |
| Skyrim LE | `npc_turntowalkleft180` | start | - | none read | - |
| Skyrim LE | `npc_turntowalkleft90` | start | - | none read | - |
| Skyrim LE | `npc_turntowalkright180` | start | - | root or COM track | - |
| Skyrim LE | `npc_turntowalkright90` | start | - | none read | - |
| Skyrim LE | `mt_jump` | airborne | - | root or COM track | - |
| Skyrim LE | `mt_jumpfall` | airborne | - | none read | - |
| Skyrim LE | `mt_jumpfallleft` | airborne | - | none read | - |
| Skyrim LE | `mt_jumpfallright` | airborne | - | none read | - |
| Skyrim LE | `mt_jumpfast` | airborne | - | root or COM track | - |
| Skyrim LE | `mt_jumpfastleft` | airborne | - | root or COM track | - |
| Skyrim LE | `mt_jumpfastright` | airborne | - | root or COM track | - |
| Skyrim LE | `mt_jumpland` | airborne | - | root or COM track | - |
| Skyrim LE | `mt_jumplandleft` | airborne | - | root or COM track | - |
| Skyrim LE | `mt_jumplandright` | airborne | - | root or COM track | - |
| Skyrim LE | `mt_jumpleft` | airborne | - | root or COM track | - |
| Skyrim LE | `mt_jumpright` | airborne | - | root or COM track | - |
| Skyrim SE | `mt_runbackward` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_runbackwardleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_runbackwardright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_runforward` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_runforwardleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_runforwardright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_runleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_runright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_sprintforward` | gait cycle | - | none read | FootSprintLeft, FootSprintRight |
| Skyrim SE | `mt_walkbackward` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_walkbackwardleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_walkbackwardright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_walkforward` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_walkforwardleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_walkforwardright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_walkleft` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_walkright` | gait cycle | - | none read | FootLeft, FootRight |
| Skyrim SE | `mt_turnleft180` | turn | - | none read | - |
| Skyrim SE | `mt_turnleft360` | turn | - | none read | - |
| Skyrim SE | `mt_turnleft60` | turn | - | none read | - |
| Skyrim SE | `mt_turnright180` | turn | - | none read | - |
| Skyrim SE | `mt_turnright360` | turn | - | none read | - |
| Skyrim SE | `mt_turnright60` | turn | - | none read | - |
| Skyrim SE | `npc_turnleft180` | turn | - | none read | - |
| Skyrim SE | `npc_turnleft90` | turn | - | none read | - |
| Skyrim SE | `npc_turnright180` | turn | - | none read | - |
| Skyrim SE | `npc_turnright90` | turn | - | none read | - |
| Skyrim SE | `npc_turntowalkleft180` | start | - | none read | - |
| Skyrim SE | `npc_turntowalkleft90` | start | - | none read | - |
| Skyrim SE | `npc_turntowalkright180` | start | - | root or COM track | - |
| Skyrim SE | `npc_turntowalkright90` | start | - | none read | - |
| Skyrim SE | `mt_jump` | airborne | - | root or COM track | - |
| Skyrim SE | `mt_jumpfall` | airborne | - | none read | - |
| Skyrim SE | `mt_jumpfallleft` | airborne | - | none read | - |
| Skyrim SE | `mt_jumpfallright` | airborne | - | none read | - |
| Skyrim SE | `mt_jumpfast` | airborne | - | root or COM track | - |
| Skyrim SE | `mt_jumpfastleft` | airborne | - | root or COM track | - |
| Skyrim SE | `mt_jumpfastright` | airborne | - | root or COM track | - |
| Skyrim SE | `mt_jumpland` | airborne | - | root or COM track | - |
| Skyrim SE | `mt_jumplandleft` | airborne | - | root or COM track | - |
| Skyrim SE | `mt_jumplandright` | airborne | - | root or COM track | - |
| Skyrim SE | `mt_jumpleft` | airborne | - | root or COM track | - |
| Skyrim SE | `mt_jumpright` | airborne | - | root or COM track | - |
| Fallout 4 | `jogforward` | gait cycle | 225 | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `runforward` | gait cycle | 373 | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `sprintforward` | gait cycle | 500 | extracted motion | FootSprintLeft, FootSprintRight, SyncLeft, SyncRight |
| Fallout 4 | `walkforward` | gait cycle | 92 | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `turninplaceleft180` | turn | - | none read | - |
| Fallout 4 | `turninplaceleft180_fast` | turn | - | none read | - |
| Fallout 4 | `turninplaceleft45` | turn | - | none read | - |
| Fallout 4 | `turninplaceleft45_fast` | turn | - | none read | - |
| Fallout 4 | `turninplaceleft90` | turn | - | none read | - |
| Fallout 4 | `turninplaceleft90_fast` | turn | - | none read | - |
| Fallout 4 | `turninplaceright180` | turn | - | none read | FootLeft, FootRight |
| Fallout 4 | `turninplaceright180_fast` | turn | - | none read | FootLeft, FootRight |
| Fallout 4 | `turninplaceright45` | turn | - | none read | - |
| Fallout 4 | `turninplaceright45_fast` | turn | - | none read | - |
| Fallout 4 | `turninplaceright90` | turn | - | none read | - |
| Fallout 4 | `turninplaceright90_fast` | turn | - | none read | - |
| Fallout 4 | `stand_to_run` | start | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `stand_to_run_l180` | start | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `stand_to_run_l30` | start | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `stand_to_run_l90` | start | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `stand_to_run_r180` | start | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `stand_to_run_r30` | start | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `stand_to_run_r90` | start | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `stand_to_walk` | start | - | extracted motion | FootRight, SyncRight |
| Fallout 4 | `stand_to_walk_l180` | start | - | extracted motion | FootLeft, FootRight, SyncRight |
| Fallout 4 | `stand_to_walk_l30` | start | - | extracted motion | FootRight, SyncRight |
| Fallout 4 | `stand_to_walk_l90` | start | - | extracted motion | FootLeft, FootRight, SyncRight |
| Fallout 4 | `stand_to_walk_r180` | start | - | extracted motion | FootLeft, FootRight, SyncLeft |
| Fallout 4 | `stand_to_walk_r30` | start | - | extracted motion | FootRight, SyncRight |
| Fallout 4 | `stand_to_walk_r90` | start | - | extracted motion | FootLeft, FootRight, SyncLeft |
| Fallout 4 | `run_to_stand` | stop | - | root or COM track | FootLeft, FootRight |
| Fallout 4 | `walk_to_stand` | stop | - | extracted motion | FootLeft, FootRight, SyncLeft |
| Fallout 4 | `walk_to_stand_l180` | stop | - | extracted motion | FootLeft, FootRight, SyncRight |
| Fallout 4 | `walk_to_stand_l45` | stop | - | extracted motion | FootLeft, FootRight, SyncRight |
| Fallout 4 | `walk_to_stand_l90` | stop | - | extracted motion | FootLeft, FootRight, SyncRight |
| Fallout 4 | `walk_to_stand_r` | stop | - | extracted motion | FootLeft, FootRight, SyncRight |
| Fallout 4 | `walk_to_stand_r180` | stop | - | extracted motion | FootLeft, FootRight, SyncLeft |
| Fallout 4 | `walk_to_stand_r45` | stop | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `walk_to_stand_r90` | stop | - | extracted motion | FootLeft, FootRight, SyncLeft |
| Fallout 4 | `run_to_run_l180` | reversal | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `run_to_run_r180` | reversal | - | extracted motion | FootLeft, FootRight, SyncRight |
| Fallout 4 | `walk_to_walk_l180` | reversal | - | extracted motion | FootLeft, FootRight, SyncLeft |
| Fallout 4 | `walk_to_walk_r180` | reversal | - | extracted motion | FootLeft, FootRight, SyncRight |
| Fallout 4 | `jumpimpactland` | airborne | - | root or COM track | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `jumpinplace` | airborne | - | root or COM track | - |
| Fallout 4 | `jumpinplacefall` | airborne | - | root or COM track | - |
| Fallout 4 | `jumpinplaceland` | airborne | - | root or COM track | FootLeft, FootRight |
| Fallout 4 | `jumprun` | airborne | - | extracted motion | FootLeft, SyncLeft |
| Fallout 4 | `jumprunfall` | airborne | - | root or COM track | - |
| Fallout 4 | `jumprunimpactland` | airborne | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `jumprunland` | airborne | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |
| Fallout 4 | `jumpwalk` | airborne | - | extracted motion | FootLeft, SyncLeft |
| Fallout 4 | `jumpwalkfall` | airborne | - | none read | - |
| Fallout 4 | `jumpwalkland` | airborne | - | extracted motion | FootLeft, FootRight, SyncLeft, SyncRight |

## Other labels on base-set locomotion clips

Digits are shown as `N`. The count is the number of clips that carry the label.

| Game | Labels |
|---|---|
| Oblivion | `end` 13, `m:L` 8, `m:R` 8, `start` 13 |
| Fallout 3 | `Blend: N` 10, `Blend:N` 10, `end` 20, `m:L` 8, `m:R` 8, `start` 20 |
| Fallout New Vegas | `-GlobalRatio N -GlobalCompressFloats true -GlobalDontCompress false` 1, `-GlobalRatio N -GlobalCompressFloats true -GlobalDontCompress true` 4, `Blend: N` 4, `Blend:N` 14, `Enum: ArmorRun` 3, `Enum: ArmorWalk` 5, `Enum: FoleyRun` 3, `Enum: FoleyWalk` 5, `end` 20, `m:L` 3, `m:R` 3, `start` 20 |
| Skyrim LE | - |
| Skyrim SE | - |
| Fallout 4 | `JumpDown` 5, `JumpUp` 3, `NNextClip` 18, `PathTweenerEnd` 22, `PathTweenerStart` 20, `jumpEndToRun` 1, `startAllowRotation` 18 |
| Fallout 76 | - |
| Starfield | - |

## Every actor clip: what the engine reads

| Game | Humanoid clips read | Other actor clips read |
|---|---|---|
| Oblivion | 860 / 860 | 1467 / 1467 |
| Fallout 3 | 2028 / 2028 | 1031 / 1031 |
| Fallout New Vegas | 2551 / 2551 | 1414 / 1414 |
| Skyrim LE | 2235 / 2235 | 1939 / 1939 |
| Skyrim SE | 2609 / 2610 | 2510 / 2510 |
| Fallout 4 | 8339 / 8339 | 7078 / 7081 |
| Fallout 76 | 0 / 9394 | 0 / 10656 |
| Starfield | 0 / 15200 | 0 / 3049 |

| Game | Why a clip was not read | Clips |
|---|---|---|
| Skyrim SE | HKX object class 'hkaSplineCompressedAnimation' is missing | 1 |
| Fallout 4 | HKX object class 'hkaSplineCompressedAnimation' is missing | 2 |
| Fallout 4 | unsupported HKX layout: lossless clip with per-track flags | 1 |
| Fallout 76 | not a Havok binary packfile | 20050 |
| Starfield | no reader for `.af` | 18249 |

## Slope and stair names

Actor clips whose path names a slope, uphill, downhill, incline or stair.

| Game | Matching clips |
|---|---|
| Oblivion | none |
| Fallout 3 | none |
| Fallout New Vegas | none |
| Skyrim LE | none |
| Skyrim SE | none |
| Fallout 4 | none |
| Fallout 76 | none |
| Starfield | none |

## Climbing names

Actor clips whose path names a climb, vault, mantle, ladder or traversal, or going over or
up a wall. A name match is a candidate, not proof that the clip climbs.

### Oblivion (0)

None.

### Fallout 3 (10)

| Directory | Clips |
|---|---|
| `meshes/characters/_male/idleanims` | `dlc04climboverhalfdoor`, `dlc04climbupladder`, `dlc05sallyclimboutofwindow`, `dlcanchclimbingcliffcanyon`, `dlcanchclimbtrenchwall`, `dlcpittclimbingdownbuilding` |
| `meshes/creatures/libertyprime/idleanims` | `dlc03libertyprimeclimbdown`, `dlc03libertyprimeclimbdown1of2`, `dlc03libertyprimeclimbdown2of2`, `dlc03libertyprimeclimbdown2of2accum` |

### Fallout New Vegas (11)

| Directory | Clips |
|---|---|
| `meshes/characters/_male/idleanims` | `dlc04climboverhalfdoor`, `dlc04climbupladder`, `dlc05sallyclimboutofwindow`, `dlcanchclimbingcliffcanyon`, `dlcanchclimbtrenchwall`, `dlcpittclimbingdownbuilding`, `specialidle_nvdlc02climbingdown` |
| `meshes/creatures/nvsporecarrier/idleanims` | `specialidle_climb`, `specialidle_climbbip`, `specialidle_climbbipalt`, `specialidle_climbelevator` |

### Skyrim LE (3)

| Directory | Clips |
|---|---|
| `meshes/actors/dragon/animations` | `mtascend_climb`, `mtascend_climbbankleft`, `mtascend_climbbankright` |

### Skyrim SE (3)

| Directory | Clips |
|---|---|
| `meshes/actors/dragon/animations` | `mtascend_climb`, `mtascend_climbbankleft`, `mtascend_climbbankright` |

### Fallout 4 (41)

| Directory | Clips |
|---|---|
| `meshes/actors/character/animations` | `rifleidlereadykneelrightvault48`, `rifleidlereadykneelrightvault48forward20`, `rifleidlereadykneelrightvault64`, `rifleidlereadykneelrightvault64forward20`, `rifleidlereadykneelrightvault64nonsneak`, `rifleidlereadykneelrightvault64nonsneakforward20`, `rifleidlereadykneelrightvault80`, `rifleidlereadykneelrightvault80forward20` |
| `meshes/actors/character/animations/synth` | `rifleidlereadykneelrightvault48`, `rifleidlereadykneelrightvault48forward20`, `rifleidlereadykneelrightvault64`, `rifleidlereadykneelrightvault64forward20`, `rifleidlereadykneelrightvault80` |
| `meshes/actors/character/animations/weapon/gripriflestraight` | `rifleidlereadykneelrightvault48`, `rifleidlereadykneelrightvault48forward20`, `rifleidlereadykneelrightvault64`, `rifleidlereadykneelrightvault64forward20`, `rifleidlereadykneelrightvault64nonsneak`, `rifleidlereadykneelrightvault64nonsneakforward20`, `rifleidlereadykneelrightvault80`, `rifleidlereadykneelrightvault80forward20` |
| `meshes/actors/character/animations/weapon/gripriflestraight/synth` | `rifleidlereadykneelrightvault48`, `rifleidlereadykneelrightvault48forward20`, `rifleidlereadykneelrightvault64`, `rifleidlereadykneelrightvault64forward20` |
| `meshes/actors/character/animations/weapon/pistol` | `rifleidlereadykneelrightvault48`, `rifleidlereadykneelrightvault48forward20`, `rifleidlereadykneelrightvault64`, `rifleidlereadykneelrightvault64forward20`, `rifleidlereadykneelrightvault64nonsneak`, `rifleidlereadykneelrightvault64nonsneakforward20`, `rifleidlereadykneelrightvault80`, `rifleidlereadykneelrightvault80forward20` |
| `meshes/actors/character/animations/weapon/pistol/synth` | `rifleidlereadykneelrightvault48`, `rifleidlereadykneelrightvault48forward20`, `rifleidlereadykneelrightvault64`, `rifleidlereadykneelrightvault64forward20`, `rifleidlereadykneelrightvault80` |
| `meshes/actors/deathclaw/animations/ambush/climboverwall` | `ambush` |
| `meshes/actors/feralghoul/animations/ambush/crawlupwall` | `ambush` |
| `meshes/actors/feralghoul/animations/ambush/overwallambush` | `ambush` |

### Fallout 76 (51)

| Directory | Clips |
|---|---|
| `meshes/actors/character/animations` | `rifleidlereadykneelrightvault48`, `rifleidlereadykneelrightvault48forward20`, `rifleidlereadykneelrightvault64`, `rifleidlereadykneelrightvault64forward20`, `rifleidlereadykneelrightvault64nonsneak`, `rifleidlereadykneelrightvault64nonsneakforward20`, `rifleidlereadykneelrightvault80`, `rifleidlereadykneelrightvault80forward20` |
| `meshes/actors/character/animations/weapon/gripbird` | `rifleidlereadykneelrightvault48`, `rifleidlereadykneelrightvault48forward20`, `rifleidlereadykneelrightvault64`, `rifleidlereadykneelrightvault64forward20`, `rifleidlereadykneelrightvault64nonsneak`, `rifleidlereadykneelrightvault64nonsneakforward20`, `rifleidlereadykneelrightvault80`, `rifleidlereadykneelrightvault80forward20` |
| `meshes/actors/character/animations/weapon/griprifleshort` | `rifleidlereadykneelrightvault48`, `rifleidlereadykneelrightvault48forward20`, `rifleidlereadykneelrightvault64`, `rifleidlereadykneelrightvault64forward20`, `rifleidlereadykneelrightvault64nonsneak`, `rifleidlereadykneelrightvault64nonsneakforward20`, `rifleidlereadykneelrightvault80`, `rifleidlereadykneelrightvault80forward20` |
| `meshes/actors/character/animations/weapon/gripriflestraight` | `rifleidlereadykneelrightvault48`, `rifleidlereadykneelrightvault48forward20`, `rifleidlereadykneelrightvault64`, `rifleidlereadykneelrightvault64forward20`, `rifleidlereadykneelrightvault64nonsneak`, `rifleidlereadykneelrightvault64nonsneakforward20`, `rifleidlereadykneelrightvault80`, `rifleidlereadykneelrightvault80forward20` |
| `meshes/actors/character/animations/weapon/pistol` | `rifleidlereadykneelrightvault48`, `rifleidlereadykneelrightvault48forward20`, `rifleidlereadykneelrightvault64`, `rifleidlereadykneelrightvault64forward20`, `rifleidlereadykneelrightvault64nonsneak`, `rifleidlereadykneelrightvault64nonsneakforward20`, `rifleidlereadykneelrightvault80`, `rifleidlereadykneelrightvault80forward20` |
| `meshes/actors/deathclaw/animations/ambush/climboverwall` | `ambush` |
| `meshes/actors/feralghoul/animations/ambush/crawlupwall` | `ambush` |
| `meshes/actors/feralghoul/animations/ambush/overwallambush` | `ambush` |
| `meshes/actors/scorched/animations/ranged` | `rifleidlereadykneelleftvault64nonsneak`, `rifleidlereadykneelleftvault64nonsneakforward20`, `rifleidlereadykneelleftvault80`, `rifleidlereadykneelleftvault80forward20`, `rifleidlereadykneelrightvault64nonsneak`, `rifleidlereadykneelrightvault64nonsneakforward20`, `rifleidlereadykneelrightvault80`, `rifleidlereadykneelrightvault80forward20` |

### Starfield (121)

| Directory | Clips |
|---|---|
| `meshes/actors/bipeda/animations` | `jumptraversal` |
| `meshes/actors/hexapoda/animations` | `jumptraversal` |
| `meshes/actors/hoppera/animations` | `jumptraversal` |
| `meshes/actors/human/_1stperson/animations/gun/gripcompact` | `mantle`, `mantlelow`, `mantletogether` |
| `meshes/actors/human/_1stperson/animations/gun/gripextended` | `mantle`, `mantlelow`, `mantletogether` |
| `meshes/actors/human/_1stperson/animations/gun/gripextendedlow` | `mantle`, `mantlelow`, `mantletogether` |
| `meshes/actors/human/_1stperson/animations/gun/gripextendedvertical` | `mantle`, `mantlelow`, `mantletogether` |
| `meshes/actors/human/_1stperson/animations/gun/gripheavy` | `mantle`, `mantlelow`, `mantletogether` |
| `meshes/actors/human/_1stperson/animations/gun/grippistol` | `mantle`, `mantlelow`, `mantletogether` |
| `meshes/actors/human/_1stperson/animations/melee/common` | `mantle`, `mantlelow`, `mantletogether` |
| `meshes/actors/human/_1stperson/animations/melee/onehanded` | `mantle`, `mantlelow`, `mantletogether` |
| `meshes/actors/human/_1stperson/animations/melee/onehanded/knife` | `mantle`, `mantlelow`, `mantletogether` |
| `meshes/actors/human/animations/common` | `ladder_climbdown_lefthand`, `ladder_climbdown_righthand`, `ladder_climbup_lefthand`, `ladder_climbup_righthand`, `ladder_dismount_bottom_lefthand`, `ladder_dismount_bottom_righthand`, `ladder_dismount_left_lefthand`, `ladder_dismount_left_righthand`, `ladder_dismount_right_lefthand`, `ladder_dismount_right_righthand`, `ladder_dismount_top_lefthand`, `ladder_dismount_top_righthand`, `ladder_mount_bottom`, `ladder_mount_left`, `ladder_mount_right`, `ladder_mount_top`, `traversal_pressbutton` |
| `meshes/actors/human/animations/furniture/shipladder/player` | `activate` |
| `meshes/actors/human/animations/furniture/videoonlyladder` | `enterfromstand`, `exittostand`, `idle` |
| `meshes/actors/human/animations/gun/common` | `mantle` |
| `meshes/actors/human/animations/gun/gripheavy` | `jumptraversal`, `relaxed_jumptraversal`, `traversal_80cm_runforward`, `traversal_pressbutton` |
| `meshes/actors/human/animations/gun/gripheavy/female` | `jumptraversal`, `relaxed_jumptraversal` |
| `meshes/actors/human/animations/gun/gripheavy/player` | `mantle`, `mantlelow` |
| `meshes/actors/human/animations/gun/grippistol` | `jumptraversal`, `relaxed_jumptraversal`, `traversal_80cm_runforward`, `traversal_pressbutton` |
| `meshes/actors/human/animations/gun/grippistol/female` | `relaxed_jumptraversal`, `traversal_pressbutton` |
| `meshes/actors/human/animations/gun/grippistol/pirate` | `jumptraversal`, `relaxed_jumptraversal`, `traversal_80cm_runforward`, `traversal_pressbutton` |
| `meshes/actors/human/animations/gun/grippistol/player` | `mantle`, `mantlelow` |
| `meshes/actors/human/animations/gun/griprifle` | `jumptraversal`, `relaxed_jumptraversal`, `traversal_80cm_runforward`, `traversal_pressbutton` |
| `meshes/actors/human/animations/gun/griprifle/female` | `jumptraversal`, `relaxed_jumptraversal` |
| `meshes/actors/human/animations/gun/griprifle/pirate` | `jumptraversal`, `relaxed_jumptraversal` |
| `meshes/actors/human/animations/gun/griprifle/player` | `mantle`, `mantlelow` |
| `meshes/actors/human/animations/melee/onehanded` | `jumptraversal`, `relaxed_jumptraversal` |
| `meshes/actors/human/animations/melee/onehanded/female` | `jumptraversal`, `relaxed_jumptraversal`, `traversal_pressbutton` |
| `meshes/actors/human/animations/melee/onehanded/pirate` | `jumptraversal`, `relaxed_jumptraversal`, `traversal_pressbutton` |
| `meshes/actors/human/animations/melee/onehanded/player` | `mantle`, `mantlelow` |
| `meshes/actors/human/animations/melee/unarmed` | `jumptraversal`, `mantle`, `relaxed_jumptraversal`, `traversal_pressbutton`, `traversal_pulllever` |
| `meshes/actors/human/animations/melee/unarmed/female` | `jumptraversal`, `traversal_pressbutton`, `traversal_pulllever` |
| `meshes/actors/human/animations/melee/unarmed/pirate` | `jumptraversal`, `relaxed_jumptraversal`, `traversal_pressbutton` |
| `meshes/actors/human/animations/melee/unarmed/player` | `mantle`, `mantlelow` |
| `meshes/actors/human/animations/scenes/video_2021` | `video_2021_ladderdismount_righthand`, `video_2021_laddermount` |
| `meshes/actors/human/animations/scenes/video_2021/ladderfurn` | `enterfromstand`, `exittostand`, `idle` |
| `meshes/actors/mantida/animations` | `jumptraversal` |
| `meshes/actors/modela/animations` | `jumptraversal`, `traversal_door` |
| `meshes/actors/models/animations` | `jumptraversal` |
| `meshes/actors/modelt/animations` | `traversal_door` |
| `meshes/actors/octopedea/animations` | `jumptraversal` |
| `meshes/actors/quadrupeda/animations` | `jumptraversal` |
| `meshes/actors/quadrupedb/animations` | `jumptraversal` |
| `meshes/actors/quadrupedc/animations` | `jumptraversal` |
| `meshes/actors/terrormorph/animations` | `jumptraversal`, `traversal_door` |
