# Sneak and Detection

**Status**: DESIGN (Phase 0, 2026-11-15); the stance (Phase 1, #6910), Wave 1 detection with its consumers (Phase 2, #6911), Wave 1 sneak attacks and perks (Phase 3, #6912), the indicator (Phase 4, #6913) and Wave 2 (Oblivion, Skyrim: detection, stealth points, sneak attacks, the Skyrim sneak hooks and the sneak locomotion clips; Phase 5, #6914) are BUILT (2026-11-15). The design the [Sneak and Detection
PRD](../prd/sneak-and-detection.md) names as the owner of the stance, the detection
tick, the per-pair level and alert state, the actor-to-actor sight test (§10 Q7), the
light input (Q3), consumer migration and FO3 / FNV sneak-attack damage (PRD
*Related*). Tracking: #6908; phases #6909 (0, this document), #6910 (1), #6911 (2),
#6912 (3), #6913 (4), #6914 (5), #6915 (6). The PRD states the promise and the `SNK-`
requirements; this document states how the engine keeps them, and §13 records what
Phase 0 measured.

Statements about an original engine cite either the sources the PRD cites, or a
static read of the executable (address given). `Fallout3.exe` (Steam GOTY, image
base `0x400000`) and `Oblivion.exe` have a plain `.text`. `FalloutNV.exe` is
SteamStub-encrypted, so New Vegas rules are read from `FalloutNV.esm` where it
authors them and otherwise carried over from `Fallout3.exe` (both games run the
"Fallout Shared" code). §4.2 shows the community FNV formula's constants matching
those defaults one for one.

## 1. Pieces

| Piece | Where | Role |
|---|---|---|
| Stance | `Sneaking` marker component on the player (`SparseSetStorage`) | Present while the player is in the stance (SNK-STN-002). The only state a save keeps (§12) |
| Stance input | `InputAction::Sneak` on Left Ctrl; `sneak_stance_system` (`byroredux/src/systems/stance.rs`), an Early exclusive after VATS | Toggles the stance unless sneaking is disabled (SNK-STN-001, -004) |
| Stance path | `byroredux_scripting::stance` (`set_sneaking`, `toggle_sneaking`, the `sneaking_enabled` gate) behind the binary's `set_player_sneaking` | The one write of `Sneaking`: the key, `stealth.sneak` and Papyrus `StartSneaking` (SNK-DX-002) |
| Stance speed | `StanceModel` profile row (`crates/core/src/character/stance.rs`) resolved per load into the `SneakMovement` resource; `MOVT` decoded into `EsmIndex::movement_types` | SNK-STN-005 |
| Ruleset row | `StealthRules` built by `CharacterRulesProfile` from the load order's game settings (`crates/core/src/character/`) | Which formula, which settings, thresholds, cadence, view cone, indicator table. The only per-game data (SNK-DET-002) |
| Formulas | `crates/core/src/stealth.rs` | Pure functions, one per ruleset kind: `fo3_value`, `fnv_value`, `oblivion_value`, `skyrim_value`. No ECS. Today's `detection_score` becomes `fnv_value` with its constants turned into `StealthRules` fields |
| Detection module | `byroredux/src/detection/` (`byroredux/src/detection/sight.rs`, `byroredux/src/detection/light.rs`, `byroredux/src/detection/tick.rs`) | The tick, the light level, the actor-to-actor sight test and the line test. The only caller of `PhysicsWorld::line_of_sight_blocked` and of the range from `DetectionConfig` (SNK-USE-007) |
| Awareness | `Awareness` component on each detector (`crates/core/src/ecs/components/detection.rs`, `SparseSetStorage`): value, level toward the player, alert state, per-detector timer, last terms | Produced by the tick only; read by every consumer (SNK-DET-001) |
| Summary | `PlayerDetection` resource: worst counted level, its detector's alert state and hostility | What the indicator draws (SNK-HUD-001) |
| Observable | `stealth.status`, `stealth.sneak <on\|off>`, a new `stealth` console-command module | SNK-DX-001, -002 |

## 2. The stance

- **Input (SNK-STN-001, Q1).** A new `InputAction::Sneak`, bound to Left Ctrl in
  `ActionBindings::default` (unbound at HEAD; C stays Block). A press toggles the
  stance. `stealth.sneak` reaches the same toggle function, so a headless smoke
  drives the same path (SNK-DX-002).
- **Gate (SNK-STN-004).** The toggle refuses to *enter* while
  `PlayerControlState.sneaking_enabled` is false or a `Control::Sneaking` input layer
  disables sneaking. Whether disabling stands a sneaking player up is V1 (#6935).
  Until it is answered, Phase 1 leaves the player in the stance, the smaller change.
- **Condition and natives (SNK-STN-002, -003).** `IsSneaking` (`CMD_IS_SNEAKING`
  `0x111e`, CTDA 286) answers whether the run-on actor carries `Sneaking`. Papyrus
  `Actor.IsSneaking` reads the same, and `Actor.StartSneaking` calls the toggle.
  Only the player can carry the marker (SNK-NG-001).
- **Speed (SNK-STN-005).** Oblivion, FO3 and FNV multiply the base speed by
  `fMoveSneakMult`. `Fallout3.exe`'s base-speed routine (`0x6017c0`) computes base ×
  (1 − encumbrance) and then × `fMoveSneakMult` while sneaking; the run multiplier is
  applied elsewhere. The masters author 0.6 (Oblivion) and 0.57 (FO3, FNV).
  Skyrim's playable races author no `SNMV` (§13.3, V2), so the PRD's race `SNMV`
  clause does not hold for the player: sneaking uses the movement type
  `NPC_Sneaking_MT` (`0003580B`, MNAM `NPCSneaking`). Its `SPED` gives forward 47.2
  walk / 222 run, left and right 41.44 / 200, back 43.38 / 150, against
  `NPC_Default_MT`'s forward 80.1 / 370. The Skyrim row takes the sneak speeds from
  that record, not from a multiplier: the ratio of the forward walk (`SPED[4]`) and
  run (`SPED[5]`) speeds. Fallout 4 does the same with `NPC_Sneaking` over
  `NPC_Default_MT`, whose `SPED` holds four floats per direction (xEdit
  `wbDefinitionsFO4`: walk `SPED[9]`, run `SPED[10]`). Starfield and Fallout 76
  have no source read, so the stance leaves their speed unscaled. A load order
  missing the named types also stays unscaled; the install log names the source.
- **Running (SNK-STN-006).** Phase 1 lets Sprint work in the stance on FO3 / FNV
  and counts it as running in the value. The archives ship `sneakmtfast*` clips,
  and Silent Running (entry point 31) only has an effect if the player can run while
  sneaking. V3 (#6935) confirms it in game.
- **Camera (SNK-STN-007).** None of the three readable executables has a
  camera-height setting for the stance (§13.3, V4). Phase 1 leaves the
  first-person camera at standing height (the owner's call, 2026-11-15) until
  #6935 measures the original; nothing is invented.
- **Locomotion clips (SNK-STN-008).** FO3 and FNV:
  `meshes\characters\_male\sneakmt{idle,forward,backward,left,right,fastforward,fastbackward,fastleft,fastright,turnleft,turnright}.kf`,
  with the `sneakmtjump*` set and per-weapon `sneak1h*` / `sneak2h*` sets.
  Oblivion: `sneak{idle,forward,backward,left,right,fast*,turnleft,turnright}.kf`
  with `sneakonehand*`, `sneaktwohand*`, `sneakhandtohand*`, `sneakstaff*` and
  `sneakbow*`. Skyrim's master graph `meshes\actors\character\behaviors\0_master.hkx`
  carries the variables `IsSneaking`, `iIsInSneak` and `iState_NPCSneaking`, and the
  clips `SneakWalk_*`, `SneakRun_*` and `SneakMTIdle`. Built (Phase 5): the player
  body carries `StanceLocomotion` (`npc_spawn::sneak_locomotion_paths`), and the stance
  system swaps its walk and idle when the stance changes by any path: FO3 / FNV
  `sneakmtforward` / `sneakmtidle`, Oblivion `sneakforward` / `sneakidle`, Skyrim the
  decoded `sneakwalk_forward.hkx` (no standing idle, as for its walk). The engine
  plays Skyrim's HKX takes directly, not through the behaviour graph, so the graph
  variables are not set; the directional and running sneak takes are not wired.

## 3. The ruleset row

The stance speed is its own profile row, `StanceModel` (Phase 1), because every
game has the stance while only some have a detection formula. The detection row
is `CharacterRulesProfile::stealth()`, a `StealthKind`, resolved per load into
the `StealthRules` resource (`byroredux_core::stealth`; `install_stealth_rules`)
as `VatsModel` is. It holds:

- **kind**: `Fo3`, `Fnv`, `Oblivion`, `Skyrim`, or `SightOnly` (Fallout 4 and
  Starfield, per Q2);
- **settings**: every game setting its kind reads, each resolved by
  `EsmIndex::game_setting_float` and otherwise taken from the executable default
  censused in §13.1, with its provenance (master, executable, or FO3 executable
  standing in for FNV) kept for the install log (SNK-DX-003);
- **levels**: which thresholds produce which levels (§4.3);
- **cadence**: a fixed period (Oblivion, FO3, FNV `fDetectionTimerSetting` 0.3 s)
  or a range (Skyrim `fDetectionUpdateTimeMin` / `Max`, 1–2 s; the `Complex` pair
  when a loaded cell holds ComplexSceneMarkers);
- **view cone**: `fDetectionViewCone` in degrees where the game authors one (FO3,
  FNV, Skyrim: 190). None for Oblivion until #6936 rules out a hardcoded one;
- **indicator** (`CharacterRulesProfile::indicator`, an `IndicatorModel`): the table of §10,
  how it is drawn, and Starfield's Stealth-rank gate (SNK-HUD-005). It is resolved per
  load into the `StealthIndicator` resource with its labels.

Walls blocking sound (SNK-DET-017) needs no field. Every formula multiplies sound
heard without line of sight by `fSneakSoundLosMult`: the CS formula does too, and
`Oblivion.esm` authors it as 1, so Oblivion's "not blocked by walls" is the data
(FO3, FNV 0.1; Skyrim 0.3).

No consumer, system or HUD code names a game. The four formula kinds are the only
branch, and they live inside `stealth.rs`.

## 4. Detection

### 4.1 The tick

`detection_tick` is a new system, registered as the first segment of `Stage::Update`
(`register_update_systems`, before `register_interaction_and_crime`). Every consumer
then reads levels from this frame: witnesses in interaction and crime, the strike's
`sneak_attack` in combat input, hostility and guard rush in combat, dead-body
discovery in quest and scene, flee in script runtime. It reads the player's
`Sneaking`, transforms, `CellLightingRes` and the placed lights (§6), and
`PhysicsWorld` for rays. It writes `Awareness` and `PlayerDetection`.

- **Detectors (SNK-DET-001, -014; Q7).** Every live, placed, non-player actor
  within the row's range of the player: `fSneakMaxDistance`, times
  `fSneakExteriorDistanceMult` in an exterior. Pairs not involving the player are
  not computed. The original computes every high-process pair: `Fallout3.exe`'s
  update `0x8740c0`–`0x874435` loops over the high-process list, as the CS
  *Performance* section describes for Oblivion. That is the PRD §6 row 8 deviation.
  `SetNotShowOnStealthMeter` (SNK-DET-014) is a Skyrim / Fallout 4 Papyrus
  function and only the indicator reads it: the fold skips a flagged, non-hostile
  detector (§10).
- **Cadence and phase (SNK-DET-006; V7, V22).** Each detector carries its own timer.
  `Fallout3.exe` stores it in the detector's process at `+0x2f0` and re-arms it as
  remainder + `fDetectionTimerSetting` (`0x87445a`–`0x874471`). One path re-arms it
  to a full period instead. Each detector therefore keeps its own phase. The tick
  does the same with the row's period. Skyrim draws each re-arm uniformly from
  `[Min, Max]`.
- **Per detector, when its timer fires:** gather the inputs (§4.5), compute the
  value with the row's formula, derive the level (§4.3) and step the alert state
  (§4.4). The terms are kept for `stealth.status`.

### 4.2 The formulas

**Fallout 3** (`Fallout3.exe` `0x57e300`, called from the per-pair routine
`0x7db0e0`, which supplies line of sight, the view cone and night eye). The
decoded shape is the GECK's (*Category:Detection*), with every coefficient a setting:

```
P      = fSneakPerceptionSkillMin + (Max − Min) · Perception / 10
mod    = 1 + alert·fSneakAlertMod + sleeping·fSneakSleepBonus
           + notCombatTarget·fSneakCombatMod + ambush        # ambush: fSneakAmbushTargetMod
                                                              #   (mode 1) / NonTargetMod (mode 2)
att    = max(0, (maxD − d) / maxD) ^ fSneakDistanceAttenuationExponent
         # maxD = fSneakMaxDistance, × fSneakExteriorDistanceMult in an exterior
sound  = max(0, att · [no LOS and not ambush-1: fSneakSoundLosMult] · fSneakSoundsMult
           · (action·int(fSneakActionMult)
              + moving·(fSneakBootWeightBase + fSneakBootWeightMult·weight)·[running: fSneakRunningMult]))
visual = max(0, sees · att · (1 + moving·fSneakLightMoveMult + running·fSneakLightRunMult)
           · (light + fDetectionSneakLightMod) · fSneakLightMult
           · ((100 − blindness) div 100) · size)             # integer division
           · [stealth boy: fSneakStealthBoyMult, and sees = 0]
skill  = (att · P · mod − sneaking·targetSneak) · fSneakSkillMult
value  = fSneakBaseValue + sound + visual + skill            # 0 < value < 1 returns 1
```

`size` is 1 below `fDetectionLargeActorThreshold`, otherwise 1 + (size − 1) ·
`fDetectionLargeActorSizeMult`. The blindness factor uses integer division, so any
blindness of 1 or more zeroes sight. The engine reproduces the shape and keeps the
integer division as authored. `sees` is the detector's line of sight *and* the view
cone (`0x7e6430`, `0x7e4f90`, `fDetectionViewCone` × π/180).

**New Vegas** is the FO3 shape plus the level terms in the target skill the Fallout
wiki gives (CHARAL FNV ruleset *Sneak Detection*; `stealth.rs`). The FNV capture's
constants are exactly these settings' values: base −35, exponent 2, range 2500 / 5000,
sound 1.6 and 1.6 · 0.1, boot weight 12 + 0.5·w, running 1.5, action ×2, light 1.4,
visual movement 0.01 / 0.21 (= 0.01 + 0.2), perception 10 + 8·Per, skill ×0.5.
`fnv_value` therefore reads every coefficient from `StealthRules`. One term
disagrees: the capture's `DetectorState` (0.8 asleep or fighting this target, 1.2
alert, lost or fighting another). The FO3 code instead computes 1 + alert·0.2 +
sleeping·(−0.2) + notCombatTarget·(−0.4) + ambush. Phase 2 uses the settings form,
since the code shows it, and records the capture's grouping as superseded in the
CHARAL ruleset.

**Oblivion** (`byroredux_core::stealth::oblivion`) is the CS formula
(*Category:Detection*), each term scaled by the linear Distance Factor
1 − d / range; the module docs write it out. Its sound skips movement at Sneak
mastery Expert and above and the boots' weight at Journeyman and above, adds
`fSneakTargetInCombatBonus` while the target fights, and is 0 while it swims;
sight is `fSneakLightMult` · DF · (`fDetectionSneakLightMod` + light [× night eye]) ·
(1 − Chameleon/100) · (1 − Blindness/100) [× `fSneakSwimmingLightMult`]; the
skill term is `fSneakSkillMult` · (detector Sneak · DF − target Sneak, while it
sneaks out of combat); a sleeping detector adds `fSneakSleepBonus` and has no
sight; Invisibility or 100 % Chameleon is −100. The defaults are `Oblivion.exe`'s
own (`fSneakSkillMult` 0.75, `fSneakBootWeightBase` 7 …), not `Fallout3.exe`'s.

**Skyrim** (`stealth::skyrim`) is `SkyrimSE.exe` `0x14041cd90`, read in full: the
CK shape with every coefficient a setting. The detector term is
`fSneakPerceptionSkillMin` + (Max − Min) · detector Sneak / 100 (Sneak, not
Perception) times 1 + alert · `fSneakAlertMod` + asleep · `fSneakSleepMod` +
not-combat-target · `fSneakCombatMod`. Sound is (action · int(`fSneakActionMult`) +
moving · (`fSneakEquippedWeightBase` + Mult · weight) · noise · [running]) ·
`fSneakSoundsMult` · attenuation [· `fSneakSoundLosMult` without line of sight] ·
(1 + asleep · `fSneakSleepBonus`). Light is the light level ·
(`fSneakLightExteriorMult` outdoors, `fSneakLightMult` indoors) +
`fDetectionSneakLightMod` (the Fallout shape adds the mod before multiplying);
blindness is a float factor, and a sleeping detector has no visual term; the
target's size class picks `fSneakDetectionSize{Small,Normal,Large,VeryLarge}`. The
player as target has skill `fPlayerDetectionSneakMult` · Sneak +
`fPlayerDetectionSneakBase` (0.4 · Sneak + 10 in `Skyrim.esm`). Entry points 48 and
57 run inside the formula on the target's perks with the detector on tab 1
(`0x14041d0af`, `0x14041ce35`; §9). The value is truncated to an integer, and
0 < value < 1 becomes 1. **Fallout 4 and Starfield** keep the actor-to-actor sight
test (§5) as their level source, Seen or Lost (Q2).

### 4.3 Levels

The engine's vocabulary is the four levels (PRD §2). Which ones a game produces is
a ruleset field:

- **FO3 / FNV.** `Fallout3.exe` stores level 3 when the value exceeds
  `fSneakSeenMin` (0) and 0 otherwise (`0x874393`). `GetDetectionLevel` returns that
  stored level (`0x6efd40` → `0x7db6a0`, the pair's entry `+4`). Unseen and Noticed
  are never stored, and nothing reads `fSneakUnseenMin` or `fSneakLostMin`.
  `fSneakNoticedMin` (−20) is the alert threshold (GECK). The update also reads it
  at `0x874356`–`0x87436f`, where it bounds a running maximum whose use is not
  decoded. `GetDetected` re-runs the formula and answers 1 for a value above 0
  (`0x6efc30`). The FO3 / FNV row therefore produces Seen and Lost, and the
  in-between band shows as the alert state (§4.4). Level Lost (V10) is a value at
  or below `fSneakSeenMin` at the detector's last tick.
- **Oblivion.** The classifier (`0x640900`) stores Seen above `fSneakSeenMin`,
  Noticed strictly between `fSneakNoticedMin` and `fSneakSeenMin`, and Lost
  otherwise (a value exactly 0 is Lost). Its jump table switches on a level computed
  from the same value a few instructions earlier, so the arms reading
  `fSneakUnseenMin` and `fSneakLostMin` are unreachable: there is no hysteresis (V10).
- **Skyrim.** Stealth points (CK *Stealth Points*; `0x140818570`, V11). Every 0.1 s
  the highest value any aggressive detector has toward the player drains them by
  value · `fCombatStealthPointDrainMult` per second while it is 0 or more; otherwise,
  once the alert wait (`…RegenAlertWaitTime`, or `…RegenLostWaitTime` while searching),
  the attacked wait and the detection-event wait have all run, they refill by
  max(`…RegenMin`, −value · `…RegenMult`) per second up to `fCombatStealthPointMax`. A
  value above `iCombatStealthPointSneakDetectionThreshold` (sneaking, 70) or
  `iCombatStealthPointDetectionThreshold` (25) empties them at once (`0x14081a0da`);
  an attack caps them at `…AttackedMaxValue` (`0x14081af9a`), a detection event at
  `…DetectedEventMaxValue` (`0x14081a4bb`); enemies in combat hold them at 0. The
  level: Lost at or below 0; a non-hostile detector Seen above 0 ("Stealth points
  only apply to 'aggro' detection"); a hostile one Seen once the points are gone or
  its value passes the threshold, Noticed until then. The engine keeps one pool
  against all aggressive detectors (the original keeps it per combat group's
  target), and counts an attack as the player's attack start.

### 4.4 Alert state

Normal → Alert → Combat → Lost → Normal (GECK and CK *Category:Detection*). The tick
steps it per detector, toward the player only:

- Normal → Alert when a hostile player's value crosses `fSneakNoticedMin`, or when a
  detection event it hears (SNK-DET-011) or damage from the player arrives;
- → Combat when the level is Seen (SNK-USE-005);
- Combat → Lost when the level falls to Lost;
- Lost / Alert → Normal when the event list is empty and the alert timer has run.

The timer is `fDetectionEventExpireTime` (FO3, FNV 10 s). On Skyrim an Alert or
searching detector returns to Normal once the player's stealth points are full ("all
the targets have Stealth Points = 100", CK). Oblivion has no alert timer setting: its
Alert lasts while the detector notices. The state feeds the formula's `alert` and
`notCombatTarget` flags and the indicator. An NPC that is Alert does not walk to
search (SNK-NG-002).

### 4.5 Inputs

| Input | Source in the engine |
|---|---|
| distance, exterior | `GlobalTransform`; the cell's interior flag |
| line of sight | the detection module's line test (§5): head to head, the player's body excluded |
| view cone | the detector's facing against the row's cone |
| light | §6 |
| moving / running | the player controller's velocity and Sprint |
| weight | the equipped items' catalog weight, each item once (`inventory::equipped_weight`) |
| action | the player's attack since the last tick: combat input writes the equipped weapon's `WEAP` `VNAM` rating (Loud 100 / Normal 50 / Silent 10, SNK-DET-016) to `PlayerActionNoise`; a `CreateDetectionEvent` owned by the player (0x14) is heard at its own position as that sound level (SNK-DET-011) |
| sneaking, sneak skill | `Sneaking`; the Sneak actor value |
| Perception, sleeping, levels | actor values; the detector's sit / sleep state; `CharacterLevel` (FNV only) |
| stealth boy, blindness, night eye | actor values Chameleon / Invisibility, Blindness, Night Eye |
| ambush | not modelled: no package marks an ambush target yet. The term stays 0 |

## 5. The actor-to-actor sight test (Q7)

One function, `detection::sees(world, observer, target)`: within the row's range,
inside the view cone where the row has one, and an unobstructed physics ray. It
replaces the three sight tests at HEAD (`actor_detects`, `actor_sees_corpse`, and the
hostility pass's own `in_sight`). It answers every question that does not have the
player as the target: hostility between NPCs, flee from a non-player threat,
dead-body discovery, `GetDetected` / `GetDetectionLevel` on a non-player target
(answering 3 or 0, as the original does), `IsActorDetected`, and `IsDetectedBy` on a
non-player actor. `detection::line_clear(world, from, to)` is the plain line test for
`GetLOS` (`actor_has_line_of_sight`) and attraction. A source-scan test keeps
`line_of_sight_blocked` and `DetectionConfig`'s range out of every file outside the
detection module (SNK-USE-007, PRD §7.2 scenario 4).

## 6. Light level (Q3)

Computed on the CPU at the player's chest each period: the cell's ambient
(`CellLightingRes`), the directional light where it reaches (exteriors), and each
placed light's attenuated contribution, using the renderer's own falloff from
`GpuLight` radius. The result is luminance, clamped to 0–1, then scaled to the row's
range:

- Oblivion 0–100 ("Falls in range from 0 to 100", CS *Category:Detection*);
- FO3 / FNV 0–100 (no documented range; the FNV capture clamps at 100);
- Skyrim 0–`iLightLevelMax` (300 in `Skyrim.esm`; `GetLightLevel` "between 0 and
  150", CK), plus `iLightLevelInteriorMod` (25) indoors.

It is blind to RT shadowing (PRD §6 row 1); `stealth.status` prints it.

## 7. Consumer migration

Built in Phase 2: every consumer that asks about the player reads the level,
and the count still on a sight test of its own is zero (the PRD §8 metric,
100 %). Without detection rules installed (no load order yet, unit fixtures)
there are no levels, and awareness of the player is the sight test, as on a
sight-only row (`player_seen_by`).

| Consumer | Today (HEAD `00472c4a1`) | After | Phase |
|---|---|---|---|
| Crime witnesses | `witnesses` → `actor_detects` (`crime.rs:72`) | detectors whose level toward the player is Seen (Q5) | 2 |
| `GetDetected` / `GetDetectionLevel` / `IsActorDetected` | `SightQuery` → `actor_detects`; the level answers 3 or 0 | player target: the stored level (Seen → 1 / 3); other targets: `sees` (3 / 0); `IsActorDetected` on the player answers 0 | 2 |
| Papyrus `IsDetectedBy` | unsupported native | as above | 2 |
| Ambient hostility | `in_sight` over every pair (`faction_hostility.rs`) | toward the player: alert state Combat; other pairs: `sees` | 2 |
| Guard rush | `actor_detects` toward the player | level Seen | 2 |
| Dead-body discovery | `actor_sees_corpse` | `sees` toward the body; the discoverer enters Alert (SNK-USE-008) | 2 |
| Flee | range only (`flee.rs` `nearest_threat`) | the player counts at level Seen; others by `sees` | 2 |
| `HitEvent.sneak_attack` | `false` at every producer | §8 | 3 |
| Fix catalog | no detection entry | FO3 / FNV engine-behaviour entry: `GetDetectionLevel` answers the real level when the player is the target (SNK-USE-010; the original answers 0, GECK) | 2 |

## 8. Sneak attacks

The player's strike sets `sneak_attack` when the player is in the stance and the
target's value toward the player is at or below the row's bar. The strike re-runs
the formula, as the originals do; it does not read the stored level.

Built (Phase 3): `combat::sneak_strike` is the one decision, called by the melee
swing, the projectile impact (carried on `TrapHit` to the landed `HitEvent`) and
the VATS shot; `detection::unaware_of_player` re-runs the formula;
`StealthRules::sneak_attack` is the arithmetic below, with the weapon's
`CRDT` Critical Damage now decoded (`ItemKind::Weapon::crit_damage`). Papyrus
`OnHit`'s `abSneakAttack` and the SDK hit event carry the flag (SNK-ATK-005).

- **FO3 / FNV** (`Fallout3.exe` `0x845580`). When the strike passes the gate at
  `0x845684`–`0x8456df` (an attacker virtual at `+0x35c`, an actor target, and two
  tests not yet named, `0x582760` and `0x7e4a50`), and the target's fresh detection
  value is ≤ 0 (`0x7db0e0`), the hit is a sneak attack. V12: Unseen and the Noticed
  band both count as unaware. The routine then multiplies the critical chance
  by `fCombatSneakAttackBonusMult` (100 in both masters, so the hit is a critical)
  and sets the hit's sneak flag (`0x400`). The damage
  routine (`0x845980`) then multiplies by `fCombatDamageBonusMeleeSneakingMult` for
  hand-to-hand and melee weapons (animation types 0–2; executable default 5) and by
  `fCombatDamageBonusSneakingMult` otherwise (2 in both masters). This settles V14:
  ×5 melee and unarmed, ×2 ranged, which the Fallout wiki also gives.
  `fDamageSneakAttackMult` and the `fPerkSneakAttack*` settings are present in the
  executable but have no reader. A weapon whose critical multiplier is 0 still takes
  the damage multiplier (SNK-ATK-003).
- **Oblivion** (`Oblivion.exe` `0x5477f0`, caller `0x5ff29d`). The value must be ≤ 0.
  The multiplier is `fPerkSneakAttackMelee<Mastery>Mult` for one-handed weapons and
  hand-to-hand, `fPerkSneakAttackMarksman<Mastery>Mult` for bows, and 1 for
  two-handed weapons and staves. Novice reads its own pair, so nothing withholds the
  bonus at Novice (V13). `Oblivion.esm` authors the marksman ladder 2 / 3 / 3 / 3 / 3,
  and the executable defaults are melee 4 / 6 / 6 / 6 / 6 and marksman 6 / 8 / 8 / 8 / 8.
  The CS *Sneak* page's "4x Marksman" at Apprentice matches neither. Built (Phase 5):
  the multiplier scales the strike's damage (`0x5ff51f`–`0x5ff535`) and the hit is
  flagged even at ×1. The bar (V12, `0x5ff23a`–`0x5ff278`) is the target's *stored*
  value toward the attacker at most 0, and at most `iAICombatMinDetection` (−50) while
  it fights the attacker; creatures never sneak attack. A Master-mastery flag set at
  `0x5ff2c4` (read at `0x5ff712`) is not decoded. The engine has no power attacks, so
  "not stacked with a power attack" holds trivially; spells never reach the strike.
- **Skyrim** (V15, `0x1407f1b31`–`0x1407f1b93`). The base multiplier is the
  `fCombatSneak<Class>Mult` setting the weapon's animation type indexes
  (`0x14028c410`: hand, one-hand sword, dagger, one-hand axe, mace, two-hand sword,
  two-hand axe, bow, staff, crossbow; `Skyrim.esm`: one-handed ×3, two-handed and
  hand-to-hand ×2; executable: bow and crossbow ×2, staff ×1), 1 without a weapon.
  Entry point 18 runs over it with the weapon and the target on its tabs; the damage
  is multiplied, and the hit flagged only when the result is above 1. The bar is the
  target's stored value toward the attacker at most 0 (`0x1406a2520`).
  `fCombatSneakAttackBonusMult` has no RIP-relative reader. The parser now decodes
  Skyrim's `WEAP DNAM` animation type (byte 0).

`fVATSStealthMult` (V16) is not a sneak-attack term. `Fallout3.exe` `0x5bfc16`
multiplies the player's VATS hit chance by it ("Stealth bonus: %f becomes %f") when
the VATS target's Chameleon (actor value 49) or the player's Invisibility (48) is
above 0. SNK-ATK-006 holds: it stays with the VATS model (#6350).

## 9. Perks

- **Activate perks (SNK-PERK-001, -002).** Robotics Expert needs only `IsSneaking`,
  `GetIsCreatureType`, `GetUnconscious` and `GetDetected` toward the player, all
  answered after Phase 2. Phase 3 made it live (`docs/smoke-tests/m65-robotics-expert.sh`):
  creature bases now carry their creature type (`ActorBaseTraits` read only `NPC_`
  before, so `GetIsCreatureType` answered 0 for every FO3 / FNV creature); an actor
  no quest binds is an interaction target while one of the player's activate entries
  offers on it (`interaction::add_perk_choice_actors`); and a ray hitting an actor's
  owner-unmarked body-part collider resolves to the actor through its parent chain.
  A choice whose script calls a command the host lacks is held unoffered
  (`obscript_cmd::unimplemented_commands`), which keeps Mister Sandman
  (`StartMisterSandman`) and the Cannibal family (`StartCannibal`, also gated by the
  unanswered `HasBeenEaten`) unoffered until V17. The other four wait on #6937. `HasBeenEaten` in
  `Fallout3.exe` is "dead, and extra-data `0x5F` (`ExtraDismemberedLimbs` in FOSE's
  numbering) byte `+0x1c` set" (`0x7dcd10`). `StartCannibal` calls an actor virtual
  not yet decoded. `StartMisterSandman` is New Vegas-only.
- **31 / Skyrim 15 (SNK-PERK-003).** Zeroes `running` in the value
  (`perk_entry_points::ignores_running_during_detection`, hooks FNV 31 and Skyrim 15
  Consumed at the tick).
- **Skyrim 47, 48, 57 (SNK-PERK-004; V18).** `SkyrimSE.exe`'s formula runs 48 over the
  visual movement multiplier (1 + moving · `fSneakLightMoveMult` + running ·
  `fSneakLightRunMult`) and 57 over the target's sneak skill, on the target's perks
  with the detector on tab 1. 47 scales the light level before the multipliers ("the
  Sneak and Invisibility actor values are applied after this entry point runs", CK).
  All three are Consumed at `detection/tick.rs::detection_perks`. Vanilla users: 47
  and 48 `MQ101IncreaseDetection` (×2); 57 the Stealth tree `Stealth00`–`Stealth80`
  (+20 to +40) and `VampireSkillsPerk02` (+20).
- **Skyrim 18 (SNK-PERK-005).** Consumed at `combat.rs::sneak_strike` (§8).
- **Skyrim 69 (SNK-PERK-007).** *Apply Sneaking Spell* stays Pending on the Perk Entry
  Points PRD's PERK-HOOK-009, which owns every spell-applying hook; the stance it
  reads is `Sneaking` (Phase 1).
- **FNV 69, 70, 71 (SNK-PERK-006).** The only vanilla users are companion perks, each
  Set Value 1.0 with no conditions:
  - 69 *Enhanced Sensors* (`EnhancedSensors`, `0015C60E`), from ED-E: "the player can
    detect enemies at an increased range. Additionally, enemies will appear on the
    player's compass and can be targeted in V.A.T.S. even when cloaked";
  - 70 *Spotter* (`Spotting`, `0015C60C`), from Boone: "hostile targets are
    highlighted whenever the player is actively aiming";
  - 71 *Search and Mark* (`SearchAndMark`, `0015C611`), from Rex: items "highlighted
    when you zoom the camera".

  These are the player's perception of others, not a term of the detection value, so
  M65 does not consume them. They stay Pending for the HUD and VATS layers.

## 10. The indicator

**Built** (Phase 4, #6913). One state per frame, folded by the detection tick from
every counted detector, drawn by every HUD in the game's vocabulary
(`byroredux_core::stealth::indicator`; `byroredux/src/detection/indicator.rs`).

**The Fallout 3 decision (V20, static).** `Fallout3.exe` draws the meter from
`0x937a70`, called each frame while the player's movement flags hold sneaking
(`0x400`) and not swimming (`0x800`; the caller at `0x92dff6`). Outside that it fades
the meter's alpha to 0 over 0.5 s. The routine drives the `sneak_nif` text (the HUD
menu's `+0x88`, bound under `SneakMeter` at `0x92b64e`) through four traits:
`string` (`0xfc4`), `systemcolor` (`0xff4`), `alpha` (`0xfa9`) and `user0`
(`0x1004`; ids from the registration at `0xb4ae3a`…). The first case that holds wins:

| # | Condition (`Fallout3.exe`) | Shows | `systemcolor` |
|---|---|---|---|
| 1 | A combat group fights the player (`0x8d0da0` → `0x81ab50`, player `+0x9a4`) and some member saw it within `fCombatDetectionLostTime` | `sSneakDanger`, flashing (an alpha animation, `user0` = 1) | 2 |
| 2 | Every such group lost the player longer ago (`+0x9a5`) | `sSneakCaution` | 2 |
| 3 | A detector that would attack the player has a value above 0 (`+0x584`, set at `0x874060` and `0x8d4884`) | `sSneakCaution` | 2 |
| 4 | Some high-process actor's value toward the player is above 0 (`0x8d4620` returns the highest) | `sSneakDetected` | 1 |
| 5 | Otherwise | `sSneakHidden` | 1 |

The labels are four string settings with executable defaults `[HIDDEN]`,
`[DETECTED]`, `[CAUTION]`, `[DANGER]`. The Fallout wiki's [CAUTION] ("searching") and
flashing [DANGER] ("has the player in sight") read the same cases loosely. The
`_SneakVisible` / `_SneakLevel` root traits in `hud_main_menu.xml` are never named by
the executable.

**The table (SNK-HUD-002).** The ruleset row `IndicatorModel` holds a per-detector
table on (level, alert state, hostility). The tick folds the counted detectors to the
highest state, so the cases' order is the states' order: Hidden < Detected < Caution <
Danger. The Fallout row reads the cases above per detector:

| Level | Alert state | Hostile | Shows |
|---|---|---|---|
| any | Combat | any | Danger, flashing (case 1) |
| any | Lost | any | Caution (case 2) |
| Seen | Normal or Alert | yes | Caution (case 3) |
| Seen | Normal or Alert | no | Detected (case 4) |
| otherwise | | | Hidden |

The engine's alert state goes Combat → Lost when the level falls to Lost (§4.4), not
`fCombatDetectionLostTime` after the last sighting, so [CAUTION] can arrive sooner
than in the original. Alert alone (the noticed band) stays [HIDDEN], as case 3 needs
a value above 0.

| Game | Row | Labels | Source |
|---|---|---|---|
| Fallout 3, New Vegas | Fallout | bracketed defaults; neither master authors them | `Fallout3.exe` (above). New Vegas keeps the shared HUD code; its executable is encrypted |
| Skyrim | Fallout, provisional | `Skyrim.esm` authors the four settings; `SkyrimSE.exe` carries the bracketed defaults | V20 on #6935 |
| Fallout 4 | Fallout, provisional | `Fallout4.esm` authors them; `Fallout4.exe` defaults unbracketed | V20 on #6935 |
| Starfield | Fallout, provisional, behind the `PERK` `Skill_Stealth` (`0x2CFCB2`) at rank 1 (SNK-HUD-005) | neither the master nor the executable carries them, so Fallout 4's stand in | V20 on #6935 |
| Oblivion | Eye: lit while a detector sees or fights the player | none | UESP *Oblivion:Sneak*; V20 on #6935 |
| Fallout 76 | none | — | no source |

The labels resolve per load order from the four settings, then the executable
default. A localized plugin's setting is a string-table id. The GMST parser now reads
it through the string tables, and an unresolved id counts as unauthored.

**SetNotShowOnStealthMeter (SNK-DET-014).** `ActorControlState` carries
`not_shown_on_stealth_meter`. The Papyrus native lowers to
`Effect::SetNotShowOnStealthMeter`, and the fold skips a flagged detector while it is
not hostile to the player. Its level still feeds every other consumer.

**HUDs.**

| HUD | Vanilla element (V21) | Driven |
|---|---|---|
| Native (`PanelSnapshot::stealth_indicator`) | — | Above the crosshair: the label, colored by state, with the danger state flashing (0.5 s); Oblivion's eye. Present only in the stance, not swimming, and past the gate (SNK-HUD-001, -005) |
| FO3 / FNV MenuXml | `hud_main_menu.xml` `SneakMeter` / `sneak_nif` | `string`, `systemcolor`, `alpha` (pulsed for danger, 0 outside the stance) and `user0`, as the original. The XML places neither tile and leaves the font unauthored, so the launch places the text at top centre in label font 7 (SNK-HUD-003) |
| Oblivion MenuXml | `menus\main\hud_reticle.xml` image `hudreticle_sneak` (`user8` "code set - alpha") | The reticle menu is grafted under the HUD at launch, its crosshair hidden. `user8` is 255 while lit (SNK-HUD-003) |
| Skyrim, Fallout 4 Scaleform | the meter, fed by GFx invocation (Game UI Integration PRD) | Empty; the native indicator stands in (SNK-HUD-004) |

The FNV MenuXml label draws in place, but its glyphs come out scrambled in every font
slot. That is a menuxml font decode fault, separate from the indicator (#6994).

## 11. Observability

- `stealth.status`: the indicator's state and label; per counted detector, the
  distance, line of sight, cone, each term (sound, visual, skill), the value, level,
  alert state and timer.
- Install log: `Detection indicator:` the row's style, its four labels and any gate.
- `stealth.sneak on|off`: the toggle (§2).
- Install log: the row kind and each setting with its provenance; a setting with
  neither an authored value nor a census default is an error (SNK-DX-003, scenario 3).

## 12. Saves

`Sneaking` is saved (a `FORMAT_MAJOR` bump to 130, SNK-SAVE-001), and so is
`ActorControlState.not_shown_on_stealth_meter` (SNK-DET-014, the bump to 131).
`Awareness`, `PlayerDetection` and the resolved `StealthIndicator` are not saved; the first tick after a load re-derives them
(SNK-SAVE-002, Q4). Whether the original keeps detection state is V23 (#6935).

## 13. Phase 0 results

### 13.1 Settings census (PRD §7.2 scenario 3, A3 baseline)

Run once on 2026-11-15 against the owner's installs. Master values come from the
`menu_gmst_census gmst` mode (`crates/plugin/examples/menu_gmst_census.rs`) on
`Fallout3.esm`, `FalloutNV.esm`, `Oblivion.esm` and `Skyrim.esm` (SE).
Executable defaults were read statically:

- `Fallout3.exe` and `SkyrimSE.exe` keep each setting as a static object: the
  value sits immediately before the pointer to its name, 4 bytes (32-bit) or 8
  (64-bit).
- `Oblivion.exe` builds each setting in code (`fld` constant, or `fld1` / `fldz`,
  then `push name`, `mov ecx, object`, and the constructor), with `{value, name}`
  layout.

Spot checks against the documented defaults:

- The CK settings table matches every `SkyrimSE.exe` value that `Skyrim.esm` does
  not override (`fDetectionViewCone` 190, `fDetectionUpdateTimeMin` 1,
  `fSneakCombatMod` −0.4, `fSneakLightMoveMult` 0.01,
  `iCombatStealthPointSneakDetectionThreshold` 70).
- The CS table matches `Oblivion.exe`'s melee sneak-attack ladder (4 / 6 / 6 / 6 / 6).

Cell key:

- **M**: the master authors the value.
- **E**: the executable's default; the master authors none.
- **E3**: New Vegas only. `Fallout3.exe`'s default stands in because `FalloutNV.exe`
  is encrypted, and `FalloutNV.esm` overrides none of these. §4.2 checks every E3
  value the FNV formula uses against the community capture.
- **—**: the game has no such setting.

In `Fallout3.exe`, `fSneakUnseenMin`, `fSneakLostMin`, `fDamageSneakAttackMult` and
every `fPerkSneakAttack*` have no reader (Oblivion leftovers).

| Setting | Fallout 3 | New Vegas | Oblivion | Skyrim |
|---|---|---|---|---|
| **Range and cadence** | | | | |
| `fSneakMaxDistance` | `2500` M | `2500` M | `1500` M | `2500` M |
| `fSneakExteriorDistanceMult` | `2` M | `2` M | `2` E | `2.1` M |
| `fSneakDistanceAttenuationExponent` | `2` E | `2` E3 | — | `2` E |
| `fDetectionTimerSetting` | `0.3` E | `0.3` E3 | `0.3` E | — |
| `fDetectionUpdateTimeMin` | — | — | — | `1` E |
| `fDetectionUpdateTimeMax` | — | — | — | `2` E |
| `fDetectionUpdateTimeMinComplex` | — | — | — | `2.5` E |
| `fDetectionUpdateTimeMaxComplex` | — | — | — | `5` E |
| `fDetectionViewCone` | `190` E | `190` E3 | — | `190` E |
| `fDetectionEventExpireTime` | `10` E | `10` E3 | — | `5` E |
| `fDetectionActionTimer` | `1.2` E | `1.2` E3 | `1.2` E | `1.2` E |
| `fDetectionStateExpireTime` | — | — | — | `30` E |
| **Base value and thresholds** | | | | |
| `fSneakBaseValue` | `-35` M | `-35` M | `-25` M | `-15` M |
| `fSneakSeenMin` | `0` E | `0` E3 | `0` E | — |
| `fSneakNoticedMin` | `-20` E | `-20` E3 | `-20` E | — |
| `fSneakUnseenMin` | `0` E | `0` E3 | `0` E | — |
| `fSneakLostMin` | `-20` E | `-20` E3 | `-20` E | — |
| `iAICombatMinDetection` | `-50` E | `-50` E3 | `-50` E | `-50` E |
| **Sound** | | | | |
| `fSneakSoundsMult` | `1.6` E | `1.6` E3 | `1.6` E | `1` M |
| `fSneakSoundLosMult` | `0.1` M | `0.1` M | `1` M | `0.3` M |
| `fSneakBootWeightBase` | `12` M | `12` M | `14` M | — |
| `fSneakBootWeightMult` | `0.5` M | `0.5` M | `1` M | — |
| `fSneakEquippedWeightBase` | — | — | — | `12` E |
| `fSneakEquippedWeightMult` | — | — | — | `0.5` E |
| `fSneakRunningMult` | `1.5` M | `1.5` M | `1.3` M | `2` M |
| `fSneakActionMult` | `2` M | `2` M | — | `2` M |
| `fSneakTargetInCombatBonus` | — | — | `25` M | — |
| `fSneakTargetAttackBonus` | — | — | `100` M | — |
| **Sight** | | | | |
| `fSneakLightMult` | `1.4` M | `1.4` M | `1.4` M | `0.33` M |
| `fSneakLightExteriorMult` | — | — | — | `0.5` M |
| `fDetectionSneakLightMod` | `0` E | `0` E3 | `-5` M | `15` M |
| `fSneakLightMoveMult` | `0.01` E | `0.01` E3 | — | `0.01` E |
| `fSneakLightRunMult` | `0.2` E | `0.2` E3 | — | `0.2` E |
| `fSneakSwimmingLightMult` | — | — | `0.5` E | — |
| `fSneakStealthBoyMult` | `0` E | `0` E3 | — | `0` E |
| `fDetectionNightEyeBonus` | `3` E | `3` E3 | `3` E | `3` E |
| `fDetectionLargeActorThreshold` | `135` E | `135` E3 | — | — |
| `fDetectionLargeActorSizeMult` | `1` E | `1` E3 | — | `1` E |
| `iLightLevelMax` | — | — | — | `300` M |
| `iLightLevelInteriorMod` | — | — | — | `25` M |
| `iLightLevelExteriorMod` | — | — | — | `0` E |
| **Detector skill** | | | | |
| `fSneakSkillMult` | `0.5` M | `0.5` M | `0.5` M | `0.5` M |
| `fSneakPerceptionSkillMin` | `10` E | `10` E3 | — | `0` M |
| `fSneakPerceptionSkillMax` | `90` E | `90` E3 | — | `100` M |
| `fSneakAlertMod` | `0.2` E | `0.2` E3 | — | `0` M |
| `fSneakSleepBonus` | `-0.2` E | `-0.2` E3 | `-10` M | `-0` M |
| `fSneakSleepMod` | — | — | — | `0` M |
| `fSneakCombatMod` | `-0.4` E | `-0.4` E3 | — | `-0.4` E |
| `fSneakAmbushTargetMod` | `2.5` E | `2.5` E3 | — | `2.5` E |
| `fSneakAmbushNonTargetMod` | `0` M | `0` M | — | `0` M |
| **Stealth points (Skyrim)** | | | | |
| `iCombatStealthPointDetectionThreshold` | — | — | — | `25` M |
| `iCombatStealthPointSneakDetectionThreshold` | — | — | — | `70` E |
| `fCombatStealthPointMax` | — | — | — | `100` E |
| `fCombatStealthPointDrainMult` | — | — | — | `3` M |
| `fCombatStealthPointRegenMin` | — | — | — | `5` M |
| `fCombatStealthPointRegenMult` | — | — | — | `0.2` M |
| `fCombatStealthPointRegenAlertWaitTime` | — | — | — | `10` E |
| `fCombatStealthPointRegenAttackedWaitTime` | — | — | — | `15` M |
| `fCombatStealthPointRegenDetectedEventWaitTime` | — | — | — | `10` M |
| `fCombatStealthPointRegenLostWaitTime` | — | — | — | `10` E |
| `fCombatStealthPointAttackedMaxValue` | — | — | — | `50` E |
| `fCombatStealthPointDetectedEventMaxValue` | — | — | — | `75` E |
| **Stance** | | | | |
| `fMoveSneakMult` | `0.57` M | `0.57` M | `0.6` M | — |
| **Sneak attack** | | | | |
| `fCombatSneakAttackBonusMult` | `100` M | `100` M | — | `100` M |
| `fCombatDamageBonusSneakingMult` | `2` M | `2` M | — | `2` M |
| `fCombatDamageBonusMeleeSneakingMult` | `5` E | `5` E3 | — | `5` E |
| `fDamageSneakAttackMult` | `4` E | `4` E3 | `4` E | `1` M |
| `fPerkSneakAttackMeleeNoviceMult` | `4` E | `4` E3 | `4` E | — |
| `fPerkSneakAttackMeleeApprenticeMult` | `6` E | `6` E3 | `6` E | — |
| `fPerkSneakAttackMeleeJourneymanMult` | `6` E | `6` E3 | `6` E | — |
| `fPerkSneakAttackMeleeExpertMult` | `6` E | `6` E3 | `6` E | — |
| `fPerkSneakAttackMeleeMasterMult` | `6` E | `6` E3 | `6` E | — |
| `fPerkSneakAttackMarksmanNoviceMult` | `6` E | `6` E3 | `2` M | — |
| `fPerkSneakAttackMarksmanApprenticeMult` | `8` E | `8` E3 | `3` M | — |
| `fPerkSneakAttackMarksmanJourneymanMult` | `8` E | `8` E3 | `3` M | — |
| `fPerkSneakAttackMarksmanExpertMult` | `8` E | `8` E3 | `3` M | — |
| `fPerkSneakAttackMarksmanMasterMult` | `8` E | `8` E3 | `3` M | — |
| `fCombatSneak1HDaggerMult` | — | — | — | `3` M |
| `fCombatSneak1HSwordMult` | — | — | — | `3` M |
| `fCombatSneak1HAxeMult` | — | — | — | `3` M |
| `fCombatSneak1HMaceMult` | — | — | — | `3` M |
| `fCombatSneak2HSwordMult` | — | — | — | `2` M |
| `fCombatSneak2HAxeMult` | — | — | — | `2` M |
| `fCombatSneakHandMult` | — | — | — | `2` M |
| `fCombatSneakBowMult` | — | — | — | `2` E |
| `fCombatSneakCrossbowMult` | — | — | — | `2` E |
| `fCombatSneakStaffMult` | — | — | — | `1` E |
| **VATS** | | | | |
| `fVATSStealthMult` | `0.5` E | `0.5` E3 | — | `0.5` E |

Phase 2's ignored real-data test (scenario 3) re-reads the M column from the
masters. It treats the E / E3 columns as the documented defaults it falls back to,
and it fails on a name with neither.

### 13.2 Vanilla references

- **Scenario A5 (FNV interior with cover):** `GSProspectorSaloonInterior`, the
  Prospector Saloon. It is already the FNV smoke cell most scripts boot, its NPCs
  stand behind the bar, and the bar and walls give cover.
- **Scenario A7 (FNV cell with a robot):** `Lucky38BasementFloorB2` (`001221C4`).
  It places 13 `Lucky38SecuritronMkI`, and `m47-3-obscript.sh` already boots it. The
  robot is `ACRE 0015B8C2` (base `00112070`) at (4128, −2290, 8936): persistent,
  enabled, no enable parent. Fallbacks:
  - `FreesideRobotShop` (`0011ED00`), with `VFSCRProtectron`;
  - `RepconHQ01` (`000E5A84`), with `REPCONEyebot` / `REPCONMrHandy` /
    `REPCONTourGuide`. Some of its references start disabled, so pick an enabled one.

### 13.3 Verify items (PRD §9, V1–V23)

| # | Item | Settled by | State |
|---|---|---|---|
| V1 | Disabling sneaking stands the player up? | — | #6935 |
| V2 | Skyrim `SNMV` values | `Skyrim.esm` `RACE` / `MOVT` read | closed: no playable race authors `SNMV` (only 3 dragons → `Dragon_Default_MT`, 2 sabre cats → `SabreCat_Default_MT`); sneaking uses `NPC_Sneaking_MT` (§2) |
| V3 | Running in the stance (FO3 / FNV) | archives, `Fallout3.exe` `0x57e300`, `0x6017c0` | evidence for (clips, perk 31, formula); in-game confirmation #6935 |
| V4 | Camera height | `Fallout3.exe`, `Oblivion.exe`, `SkyrimSE.exe` | no setting exists; #6935 |
| V5 | Sneak clips; Skyrim graph variable | the meshes archives, `0_master.hkx` | closed (§2) |
| V6 | FO3 / FNV detection coefficients | census | closed (§13.1): `fSneakAlertMod` 0.2, `fSneakCombatMod` −0.4, `fSneakLightMoveMult` 0.01, `fSneakLightRunMult` 0.2, `fSneakActionMult` 2, `fSneakDistanceAttenuationExponent` 2 |
| V7 | FO3 / FNV cadence | `Fallout3.exe` default and `0x874421` | closed: `fDetectionTimerSetting` 0.3 s per detector |
| V8 | Skyrim `fSneakBaseValue` | `Skyrim.esm` | closed: −15 authored (executable default −25) |
| V9 | View cone (Oblivion, FO3 / FNV) | `Fallout3.exe` `0x7e6430`, `0x7e4f90`; `Oblivion.exe` | FO3 / FNV closed: 190°, applied by the detection routines. Oblivion: no setting, and the CS formula reads line of sight only (the engine uses no cone); the sight routine is not traced, #6936 |
| V10 | Rule to level Lost | `Fallout3.exe` `0x87422d`, `0x6efd40`; `Oblivion.exe` `0x640900`; CK *Stealth Points* | closed (§4.3): FO3 / FNV Seen / Lost; Oblivion Seen / Noticed / Lost without hysteresis; Skyrim by stealth points |
| V11 | Skyrim stealth-point regeneration | `SkyrimSE.exe` `0x140818570`, `0x14081a0da`, `0x14081af9a`, `0x14081a4bb` | closed (§4.3) |
| V12 | Unseen counts as unaware? | `Fallout3.exe` `0x845580`; `Oblivion.exe` `0x5ff23a`; `SkyrimSE.exe` `0x1406a2520` | closed (§8): FO3 / FNV a fresh value ≤ 0; Oblivion the stored value ≤ 0, and ≤ `iAICombatMinDetection` while fighting the attacker; Skyrim the stored value ≤ 0 |
| V13 | Oblivion Apprentice marksman, Novice bonus | census, `Oblivion.exe` `0x5477f0` | closed (§8): Apprentice marksman 3 (`Oblivion.esm`); Novice gets its multiplier |
| V14 | FO3 / FNV sneak-attack damage | `Fallout3.exe` `0x845580`, `0x845980` | closed (§8): critical, ×5 melee and unarmed, ×2 otherwise |
| V15 | Skyrim base sneak-attack multipliers | census, `SkyrimSE.exe` `0x14028c410`, `0x1407f1b69` | closed (§8): `fCombatSneak<Class>Mult` by animation type through entry 18; `fCombatSneakAttackBonusMult` has no reader found |
| V16 | `fVATSStealthMult` | `Fallout3.exe` `0x5bfc16` | closed (§8): VATS hit chance against Chameleon / Invisibility, not sneak |
| V17 | `StartCannibal`, `StartMisterSandman`, "eaten" saved | `Fallout3.exe` `0x7aabd0`, `0x7dcd10` | partly (§9); #6937 |
| V18 | Skyrim 48 and 57 | `perk_entry_census` on `Skyrim.esm`, `SkyrimSE.exe` `0x14041d0af`, `0x14041ce35` | closed (§9): 48 the visual movement multiplier, 57 the target's sneak skill |
| V19 | FNV 69, 70, 71 | `perk_entry_census` and the perks' `DESC` on `FalloutNV.esm` | closed (§9): companion perception perks, not a detection term |
| V20 | Indicator tables | `Fallout3.exe` `0x937a70`, `0x8d0da0`, `0x81ab50`, `0x8d4620` | FO3 / FNV decoded (§10). Oblivion, Skyrim, Fallout 4, Starfield provisional; #6935 |
| V21 | Vanilla sneak element | the Misc archives, `Fallout3.exe` `0x92b64e` | closed (§10): FO3 / FNV `sneak_nif`, Oblivion `hudreticle_sneak` |
| V22 | Per-detector phase | `Fallout3.exe` `0x87445a` | closed: each detector keeps its own timer and phase |
| V23 | Detection state across a save | — | #6935 |

The Phase 0 verify-item metric (PRD §8): 12 of 23 closed (V2, V5–V8, V13–V16, V19,
V21, V22), 8 partly answered and 3 open (V1, V4, V23). Phase 5 closed V10, V11, V12,
V15 and V18 (17 of 23); V9 (Oblivion's cone) stays partly answered on #6936, the rest
on #6935 and #6937.
