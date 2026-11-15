# Sneak and Detection — Product Requirements

**Status**: ACTIVE (2026-11-15). Revision 6 records the Phase 0 results.
Revision 5 filed the tracking issues. Revision 2
folded in review round 1, revision 3 recorded the §10 decisions, and the owner
approved it (§12).
The document covers the player's sneak stance, NPC detection of the player,
the consumers that read both (conditions and script functions, witnessed
crimes, hostility, sneak attacks, the sneak-gated perks) and the detection
indicator. Statements about how an original engine behaves cite external
sources: the CS, GECK, CK and Fallout 4 CK wiki dumps and UESP under
`/mnt/data/src/reference/`, xEdit's `wbDefinitions*.pas` record definitions,
and the Fallout and Starfield wikis read as raw wikitext on 2026-11-11 (the
Fallout wiki's *Sneak (Fallout 3)*, *Sneak (Fallout: New Vegas)* and *Sneak
Attack Critical* re-read on 2026-11-14). The Fallout wiki's formulas are
community-derived, not official, and are cited as such. A claim that could not
be verified is marked **Phase 0: verify** instead of being stated as fact, and
every such item is a row of the Phase 0 verify table (§9).

**Tracking**: #6908 (parent). Phases: #6909 (0), #6910 (1), #6911 (2), #6912 (3),
#6913 (4), #6914 (5), #6915 (6).

**Scope**: The player's sneak stance and how the actors around the player
become aware of them, in Oblivion, Fallout 3, Fallout: New Vegas and Skyrim:
entering and leaving the stance and what it changes for the player; a
detection value per detector and target, computed by each game's own formula
from its own game settings; the detection level and alert state that value
produces; the indicator that shows the player the worst level around them;
and every consumer that reads the stance or the level (the sneak and
detection conditions and script functions, crime witnesses, ambient
hostility, dead-body discovery, guard rush, sneak attacks, and the perk entry
points tied to sneaking, the FO3 / FNV *Activate* perks among them). The
promise is to the player of those four titles and to the mods that re-tune
stealth through game settings, perks and weapon records. Fallout 4 and
Starfield get the stance and its conditions; their detection formula waits on
§10 Q2. Outside it: actors other than the player sneaking (the AI Packages PRD
decided *Always Sneak* is out), NPC search movement, pickpocketing,
killmoves, and Fallout 76.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| FO3 / FNV detection formula and its sourced coefficients | [CHARAL FNV/FO3 ruleset § Sneak Detection](../engine/charal-fnv-fo3-ruleset.md#sneak-detection-fnv--locked-but-an-aistealth-subsystem-not-a-charal-stat) |
| Skyrim detection formula | [CHARAL Skyrim ruleset § Sneak Detection](../engine/charal-skyrim-ruleset.md#sneak-detection-skyrim--locked-sounddistance-halves-pending-visualskill-halves--out-of-charal-scope) |
| Oblivion sneak-attack multiplier inside the damage formula | [CHARAL Oblivion ruleset § The Complete Damage Formula](../engine/charal-oblivion-ruleset.md#the-complete-damage-formula--closes-marksmanhand-to-hand-adds-luck-chained-skill--armor-rating--all-now-built) |
| Skyrim sneak-attack multipliers by weapon class | [CHARAL Skyrim ruleset § Sneak Detection](../engine/charal-skyrim-ruleset.md#sneak-detection-skyrim--locked-sounddistance-halves-pending-visualskill-halves--out-of-charal-scope) (its sneak-attack paragraph) |
| Crime witnesses and fines | [CHARAL Oblivion ruleset § Crime fines](../engine/charal-oblivion-ruleset.md#crime-fines--theft-assault-and-murder-built-the-rest-sourced-and-unbuilt) |
| Fallout 4 detection ("dead end" at HEAD) | [CHARAL FO4 ruleset](../engine/charal-fo4-ruleset.md) |
| Stance, detection tick, per-pair level and alert state, the actor-to-actor sight test (§10 Q7), the light input, consumer migration, FO3 / FNV sneak-attack damage | [Sneak and Detection design](../engine/sneak-detection.md) (Phase 0, #6909) |
| `Control::Sneaking` input-enable domain | [Scripting § Input enable layers](../engine/scripting.md#input-enable-layers-m472) |
| Indicator on MenuXml (Oblivion, FO3, FNV) and Scaleform (Skyrim, FO4) | [UI System § MenuXml](../engine/ui.md#menuxml-legacy-ui-m484-oblivion--m485-fo3), [UI System § Scaleform HUD](../engine/ui.md#scaleform-hud-m486-skyrim--m487-fallout-4) |
| Menu framework and HUD meters the Scaleform games cannot feed | [Game UI Integration PRD](game-ui-integration.md) |
| Perk entry-point hooks the stealth values compose through | [Perk Entry Points PRD](perk-entry-points.md) (`PERK-HOOK`) |
| NPC sneaking (*Always Sneak*, Crouch To Reload) | [AI Packages Completion PRD](ai-packages-completion.md) (Q4, AIP-UW-006) |
| VATS's stealth multiplier (`fVATSStealthMult`), read with no consumer | [CHARAL FNV/FO3 ruleset § VATS tuning](../engine/charal-fnv-fo3-ruleset.md#vats-tuning-action-points-regen-and-the-radiation-gauge--built-6350); SNK-ATK-006 holds it unconsumed |
| The original engines' detection bugs the engine does not reproduce | [Built-in fixes § Fix catalog](../engine/builtin-fixes.md#3-fix-catalog) |
| Saved stance and its `FORMAT_MAJOR` bump | [Save/Load Round-Trip § 2](../engine/save-load-roundtrip.md#2-ecs-snapshot-capture) |
| Schedule placement of the detection tick | [Game Loop § schedule](../engine/game-loop.md#the-system-schedule-m27-declared-access) |
| `stealth.*` console commands | [Debug CLI](../engine/debug-cli.md) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `SNK-` prefix.

---

## 1. Product thesis

Sneaking is how a player avoids a fight, steals, and opens one on their own
terms, in every target game, and the content assumes it. Every vanilla FO3 /
FNV *Activate* perk (Cannibal, Mister Sandman, Robotics Expert, Ghastly
Scavenger, Dine and Dash) requires the player to be sneaking: tab 0 of each
carries `IsSneaking` (`perk_entry_census`, `CENSUS_ENTRY_POINT=27`, on
`FalloutNV.esm` and `Fallout3.esm`). A crime counts only when someone sees it:
"Crimes are only reported if they were witnessed by an Actor who reports
crimes" (CK *Crime*), and in Oblivion "Successfully sneaking while committing a
crime will prevent you from being detected" (UESP *Oblivion:Crime*). Sneak
attacks are the stealth build's damage: in Skyrim "You perform a sneak attack
when you physically strike someone who is unaware of your presence" (UESP
*Skyrim:Sneak*).

At HEAD the engine has the detection math for one game and nothing that uses
it. `crates/core/src/stealth.rs` transcribes the FNV formula and has no caller.
No actor can sneak, so `IsSneaking` answers 0 for everyone. Every check of
awareness (crime witnesses, ambient hostility, guard rush, dead-body
discovery, `GetDetected`) is a range-and-line-of-sight test, one of the sight
tests of §2, and flee checks range alone (§3.1 row 8). The activate-choice
runtime that shipped on 2026-11-10 (`683d9a92b`) therefore cannot offer a
single vanilla FO3 / FNV perk in play.

The owner's framing is the roadmap's priority: "shortest path to a playable
cell … the bottlenecks are *consumers* — systems that make parsed data do
something on screen, at the speakers, or in play" (ROADMAP, Active Roadmap),
and the instruction of 2026-11-10 to "go for the gameplay layer".

**Corollary.** Awareness of the player has one producer. One detection tick
computes the level of every detector toward the player, and every consumer
that asks about the player reads that level; none runs a sight test of its
own. Awareness between other actors (an NPC of an NPC, an actor of a corpse)
follows §10 Q7. What differs between games
(the formula, its game settings, the level thresholds, the indicator's
vocabulary) is a row of the game's character ruleset, read at the plugin
boundary, never a branch in a consumer. The engine will not make NPCs sneak,
will not move a searching NPC toward the player, and will not invent a
formula for a game whose formula no source gives.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Sneak stance** | The player's crouched mode: "Returns 1 if the actor is sneaking (crouched)" (GECK *IsSneaking*). Entered and left by a toggle (§5.1). |
| **Detector** | An actor whose awareness of the player is computed: alive, placed, and counted by the ruleset (SNK-DET-014). |
| **Detection value** | The number one game's formula produces for one detector and the player. The FO3 / FNV GECK form: "Detection Value = fSneakBaseValue + (Sound factor * Distance attenuation ) + (Visual factor * Distance attenuation) + (Actor Skill factor * Distance attenuation) - ( Target Skill factor )" (GECK *Category:Detection*). |
| **Detection level** | The detector's awareness of the player, one of the four the original engines name: "0 = Lost :1 = Unseen :2 = Noticed :3 = Seen" (CS *GetDetectionLevel*); the GECK pairs them with HUD words: "0 = Lost ("Undetected") … 1 = Unseen ("Caution") … 2 = Noticed ("Danger") … 3 = Seen (flashing "Danger")" (GECK *GetDetectionLevel*), a pairing another source contradicts (§3.2). Where it could be confused with the alert state, this document writes *level Lost*. |
| **Alert state** | The detector's own state: "Actors pass through 4 detection states: Normal->Alert->Combat->Lost->Normal" (GECK and CK *Category:Detection*). It scales the detector's skill term (`stealth::DetectorState`). *Alert state Lost* (searching after combat) is not *level Lost* (not detected); this document always qualifies the word. |
| **Detection indicator** | What the HUD shows the player while sneaking: the worst level over the counted detectors ("it reports the worst detection state towards the player character of all potential targets", Fallout wiki *Sneak (Fallout 3)*), in the game's vocabulary (§3.2). |
| **Detection tick** | The one system that computes values and levels, at the game's cadence (SNK-DET-006). |
| **Sneak attack** | A strike by the sneaking player on a detector whose level toward the player is below the ruleset's attack threshold (SNK-ATK-001). |
| **Sight test** | Any of the range-and-ray tests at HEAD: range (`fSneakMaxDistance`, scaled outdoors, `DetectionConfig::range`) and an unobstructed physics ray (`PhysicsWorld::line_of_sight_blocked`), nothing else. Three exist: `actor_detects` for one actor toward another (`byroredux/src/systems/faction_hostility.rs:163`), `actor_sees_corpse` for an actor toward a body (`:183`), and the hostility pass's own `in_sight` over every actor pair (`:526`, called at `:593`). |
| **Actor-to-actor sight test** | The one sight test §10 Q7 keeps, if it keeps one, for awareness where the player is not the target. |

---

## 3. Core model

### 3.1 Inventory

The inventory below is the state at HEAD on 2026-11-14 (`922072af8`). Phase 0's
design doc replaces the Today column with a test-backed table.

| # | Mechanism | Class | Today |
|---|---|---|---|
| 1 | Sneak stance | Stance | Absent. `InputAction` has no sneak action (`byroredux/src/interaction.rs:54`); in `ActionBindings::default` Left Ctrl is bound to nothing and C is Block (`:171`). The only other use of Ctrl is as a text-editing modifier in menus (`byroredux/src/ui_input.rs:213`). No stance component exists. |
| 2 | "Sneaking allowed" control | Stance | `PlayerControlState.sneaking_enabled`, saved; read only by `GetPlayerControlsDisabled` and the input layers (`crates/scripting/src/player_control.rs:73`, `input_layers.rs:150`). |
| 3 | `IsSneaking` (ObScript `0x111e`, CTDA 286) | Condition | Answers 0 for every actor: "The engine has no sneak stance" (`crates/scripting/src/obscript_effects/getters.rs:350`). |
| 4 | Papyrus `IsSneaking`, `StartSneaking`, `IsDetectedBy` | Script | No native; the call stops the fragment as unsupported (`crates/scripting/src/fragment/vm/interp.rs:1166`). |
| 5 | `GetDetected` (CTDA 45), `GetDetectionLevel` (180), `IsActorDetected` | Condition | `actor_detects` as 1 / 0, through the `SightQuery` resource (`byroredux/src/boot/world.rs:509`), for any target; `GetDetectionLevel` answers 3 or 0 (`getters.rs:337`, `:370`; `crates/scripting/src/obscript_more/state.rs:651`). |
| 6 | Detection formula | Detection | The FNV formula, pure math, no caller (`crates/core/src/stealth.rs:249` `detection_score`, `:236` `classify`). |
| 7 | Crime witnesses | Consumer | Every living non-creature actor `actor_detects` passes (`witnesses`, `byroredux/src/crime.rs:72`, the call at `:79`); "Not modelled yet: sneaking undetected" (`:40`). |
| 8 | Ambient hostility, guard rush, dead-body discovery, flee range | Consumer | Four different tests. Ambient hostility: its own range filter and `in_sight` ray over every actor pair, NPC against NPC and creature included, every 0.5 s (`byroredux/src/systems/faction_hostility.rs:562-593`, period `:71`). Guard rush: `actor_detects` toward the player (`crime.rs:831`). Dead-body discovery: `actor_sees_corpse`, an actor toward a body, not toward the player (`byroredux/src/systems/dead_body.rs:87`). Flee: range alone, no line of sight, from any threat (`byroredux/src/systems/flee.rs:57`, `nearest_threat`). |
| 9 | Sound events (`CreateDetectionEvent`) | Detection | Queued in `PendingDetectionEvents` and never read (`crates/scripting/src/detection_events.rs:7`). |
| 10 | Sneak attack | Consumer | `HitEvent.sneak_attack` is `false` at every producer (`byroredux/src/combat.rs:354` among them: "no power-attack charge, sneak-attack, or bash input exists", `:366`); only the SDK, Papyrus `OnHit` and the mod runtime read it, no damage code. |
| 11 | Sneak perk entry points | Perk | FO3 / FNV 31, 69, 70, 71, Skyrim 15, 18, 47, 48, 57 and Fallout 4 15, 18, 47, 48, 57, 69 (`FALLOUT4_HOOKS`) are Pending (`UNCLASSIFIED`); Skyrim 69 is Pending under PERK-HOOK-009 (`crates/scripting/src/perk_entry_points/catalog.rs`). |
| 12 | FO3 / FNV *Activate* perks | Perk | The runtime is built (`byroredux/src/perk_activate.rs`); every vanilla entry fails closed on `IsSneaking` (the `683d9a92b` commit body). |
| 13 | Detection indicator | Presentation | None on the native HUD (`crates/debug-ui/src/panels/hud.rs`), MenuXml (`byroredux/src/hud.rs`) or Scaleform (`byroredux/src/scaleform_hud.rs`). |
| 14 | Sneak locomotion and crouch height | Stance | No sneak clip, graph variable or capsule change (`byroredux/src/npc_spawn/anim_clips.rs`, `systems/graph_locomotion.rs:192`, `crates/physics/src/components.rs:191`). |
| 15 | Sneak skill actor value | Input | Populated on Oblivion, FO3, FNV and Skyrim (`crates/core/src/character/skill.rs`); no skill-use experience for sneaking. |
| 16 | VATS stealth multiplier | Consumer | `fVATSStealthMult` is read into `VatsModel::stealth_mult` and nothing reads that field (`crates/core/src/character/vats.rs:256`). |

### 3.2 Detection level and its indicator per game

The four levels are the engine's closed vocabulary; each game's indicator is
a rendering of the worst level. `stealth::DetectionState` (Undetected /
Suspicious / Detected) is the instantaneous class of one value; the level adds
the history that "Lost" needs.

| Game | Indicator, as sourced | Source |
|---|---|---|
| Oblivion | An eye that lights when anyone is aware: "If the eye lights up, that means somewhere an NPC or creature knows you're there" | UESP *Oblivion:Sneak* |
| Fallout 3 / New Vegas | [HIDDEN] ("being undetected"); [CAUTION] ("unfriendly targets are suspicious of presence and are searching"); [DANGER] ("hostile NPCs are aware of one's presence, but they do not know the current location"), flashing when "the enemy has the player character in sight and will engage them"; [DETECTED] ("being in sight by one or more targets … shown instead of [CAUTION] because they haven't identified one as hostile") | Fallout wiki *Sneak (Fallout 3)*, *Sneak (Fallout: New Vegas)* |
| Skyrim | "A flat horizontal line means you are completely hidden", "A slightly open eye means an enemy thinks a target may be near", "A half-open eye means the enemy is actively searching for you", "An open eye means the enemy knows where you are or can see you" | UESP *Skyrim:Sneak* |
| Fallout 4 | A CAUTION state exists ("When the player enters CAUTION detection state while in sneak mode"); the other words **Phase 0: verify** | FO4 CK *OnTutorialEvent* |
| Starfield | A meter only with the Stealth skill: "Rank 1 - Adds a Stealth Meter" | Starfield wiki *Stealth* |

No game's mapping from level to indicator state is fully sourced. On FO3 /
FNV the GECK pairs the four levels with "Undetected", "Caution", "Danger" and
flashing "Danger" (GECK *GetDetectionLevel*, §2), while the Fallout wiki names
[HIDDEN] and a fifth state, [DETECTED], that a target shows instead of
[CAUTION] when it has not identified the player as hostile. The state on
screen therefore depends on the detector's hostility to the player as well as
on the level. **Phase 0: verify** which level and hostility render [DETECTED]
versus flashing [DANGER] on FO3 and FNV (GECK *GetDetectionLevel* and Fallout
wiki *Sneak (Fallout 3)* disagree), and the Oblivion, Skyrim, Fallout 4 and
Starfield tables (SNK-HUD-002).

---

## 4. Goals and non-goals

### 4.1 Goals

- **SNK-G-001 — The stance.** The player can enter and leave the sneak stance
  on every in-scope game, and `IsSneaking` reads it.
- **SNK-G-002 — One producer of awareness of the player.** Every consumer in
  §3.1 rows 5, 7 and 8 that asks about the player reads the detection level;
  the count of those still on a sight test of their own is zero. Awareness
  where the player is not the target follows §10 Q7.
- **SNK-G-003 — Each game's formula.** FO3, FNV, Oblivion and Skyrim compute
  the detection value from their own sourced formula and their load order's
  game settings.
- **SNK-G-004 — The perks that need it fire.** The vanilla FO3 / FNV
  *Activate* perks and the sneak perk entry points compose in play.
- **SNK-G-005 — Sneak attacks.** A strike on an unaware detector is a sneak
  attack and takes its game's multiplier.
- **SNK-G-006 — The player can see it.** The detection indicator shows the
  worst level while sneaking.

### 4.2 Non-goals

- **SNK-NG-001.** Actors other than the player in the sneak stance. The AI
  Packages PRD decided *Always Sneak* is out (its Q4) and owns Crouch To
  Reload (AIP-UW-006).
- **SNK-NG-002.** NPC search movement toward a last-known position. The level
  and alert state are in scope; walking an actor to investigate is AI
  package behaviour the AI Packages PRD does not promise.
- **SNK-NG-003.** Pickpocketing. Its formulas are their own (the CHARAL
  rulesets' pickpocket sections) and its menu is the Game UI Integration PRD's.
- **SNK-NG-004.** Killmoves and paired sneak-kill animations.
- **SNK-NG-005.** Fallout 76: the engine has no Fallout 76 gameplay route.
- **SNK-NG-006.** Reproducing the original engines' detection bugs where a fix
  is recorded instead (§6 row 5).

---

## 5. Requirements by area

Every per-game rule below (a formula, a game setting, a threshold, whether
walls block sound, the indicator's table and gate) is a field of the
character ruleset row SNK-DET-002 selects, read at the plugin boundary; none
is a branch on the game in the tick, a consumer or the HUD.

### 5.1 The stance (`SNK-STN`)

- **SNK-STN-001.** An `InputAction::Sneak` bound by default to Left Ctrl MUST
  toggle the player's stance: "Ctrl … Toggle crouch/sneak mode" (UESP
  *Oblivion:Controls*, *Skyrim:Controls*); "Left Ctrl … Sneak / crouch"
  (Fallout wiki *Fallout: New Vegas (PC)*). Today there is no action and Left
  Ctrl is unbound (§3.1 row 1).
- **SNK-STN-002.** The stance MUST be one component on the player, and
  `IsSneaking` (ObScript `0x111e`, CTDA 286) MUST answer 1 while it is present
  and 0 otherwise. Today it answers 0 (§3.1 row 3).
- **SNK-STN-003.** Papyrus `Actor.IsSneaking` MUST return the stance, and
  `Actor.StartSneaking` MUST enter it, or leave it when already sneaking ("If
  the actor was already sneaking, cancels sneaking", CK *StartSneaking -
  Actor*). Today neither is a native (§3.1 row 4).
- **SNK-STN-004.** The stance MUST NOT be entered while the `Control::Sneaking`
  layer or `DisablePlayerControls`' sneaking flag disables sneaking ("Enables
  or disables the player's ability to sneak on this layer", FO4 CK
  *EnableSneaking - InputEnableLayer*). **Phase 0: verify** whether disabling
  sneaking stands an already sneaking player up.
- **SNK-STN-005.** The player's movement speed in the stance MUST be scaled by
  the game's sneak multiplier read from the load order: Oblivion
  `fMoveSneakMult` ("Speed multiplier for being in sneak mode", CS *Movement
  Game Settings*), FO3 / FNV `fMoveSneakMult` ("Movement speed is multiplied
  by this when the actor is sneaking", GECK *Movement Settings*), Skyrim the
  race's sneak movement type (xEdit TES5 `RACE` `SNMV`, "Base Movement Default -
  Sneak"). **Phase 0: verify** the Skyrim `MOVT` values the vanilla races name.
- **SNK-STN-006.** On FO3 and FNV, Sprint MUST work in the stance and MUST
  count as running in the detection value (the GECK formula's "Running *
  fSneakLightRunMult" term, GECK *Category:Detection*). The vanilla Silent
  Running perk of both masters is one *Ignore Running During Detection* (31)
  entry (`perk_entry_census`, `CENSUS_ENTRY_POINT=31`), which has an effect
  only if the player can run while sneaking. The sources disagree: the in-game
  Sneak skill description says "You cannot run and sneak at the same time"
  while the Fallout wiki says the player "can both sneak and run … However,
  without the Silent Running perk, they will suffer a detection penalty while
  running" (both on Fallout wiki *Sneak (Fallout 3)*). **Phase 0: verify** in
  game which holds on FO3 and FNV; the requirement changes if the game forbids
  it.
- **SNK-STN-007.** The first-person camera SHOULD be lower in the stance (UESP
  *Skyrim:Sneak*, a bug note: "in first-person mode your character's view will
  be lower as if sneaking"). **Phase 0: verify** the height on each game; no
  source gives it.
- **SNK-STN-008.** The player's locomotion in the stance SHOULD play the game's
  own sneak animations. **Phase 0: verify** the clip names (FO3 / FNV / Oblivion
  KF) and the behaviour-graph variable (Skyrim) from the archives.

### 5.2 Detection (`SNK-DET`)

- **SNK-DET-001.** One detection tick MUST compute, for every detector within
  the game's maximum distance of the player, a detection value and a detection
  level toward the player, and MUST be the only producer of levels. Which other
  pairs it computes is §10 Q7. Today detection is a sight test and
  `stealth.rs` has no caller (§3.1 rows 5, 6).
- **SNK-DET-002.** Which formula the tick runs MUST be a property of the
  character ruleset row the load order selects (as
  `CharacterRulesProfile::perk_entry_points` is for perks), with no game named
  in the tick.
- **SNK-DET-003.** On FO3 the value MUST be the GECK formula (§2, *Detection
  value*) over FO3's game settings. FO3 has no level terms: "unlike in Fallout
  3, sneak is determined by the character levels of the actor sneaking as well
  as the actor attempting to detect them" (Fallout wiki *Sneak (Fallout: New
  Vegas)*).
- **SNK-DET-004.** On FNV the value MUST be the formula `stealth.rs` transcribes
  (the CHARAL FNV ruleset's capture, community-derived from the Fallout wiki),
  with the coefficients the GECK names as game settings read from the load
  order. **Phase 0: verify** the values of `fSneakAlertMod`, `fSneakCombatMod`,
  `fSneakLightMoveMult`, `fSneakLightRunMult`, `fSneakActionMult` and
  `fSneakDistanceAttenuationExponent` on `FalloutNV.esm` and `Fallout3.esm`
  (the GECK names them without values).
- **SNK-DET-005.** On Oblivion the value MUST be the CS formula (CS
  *Category:Detection*: sight, sound and sneak-skill terms, each scaled by
  "Distance Factor … 1 - (distance between detector and target) /
  MaxDetectionRange"), its game settings read from `Oblivion.esm`, with the CS
  defaults (CS per-setting pages) as the fallback when a load order authors
  none.
- **SNK-DET-006.** The tick MUST re-evaluate each detector at the game's
  cadence: Oblivion "every .3 seconds" (`fDetectionTimerSetting`, CS
  *Category:Detection*), Skyrim between `fDetectionUpdateTimeMin` and
  `fDetectionUpdateTimeMax` (CK *Category:Settings*). **Phase 0: verify** the FO3
  / FNV cadence.
- **SNK-DET-007.** On Skyrim the value MUST be the CK formula (CK
  *Category:Detection*) over the CK's game settings. **Phase 0: verify**
  `fSneakBaseValue`: UESP gives −15 and the CK settings list has no row for it;
  read `Skyrim.esm` or the executable's default table.
- **SNK-DET-008.** A detector MUST see the player only through an unobstructed
  ray, as the sight test does today, and only inside its view cone where the
  game authors one: Skyrim "All seeing creatures and NPCs have a defined cone of
  vision" (UESP *Skyrim:Sneak*), `fDetectionViewCone` (CK). **Phase 0: verify**
  whether Oblivion and FO3 / FNV have a view cone.
- **SNK-DET-009.** The light term MUST read a light level at the player computed
  by the engine from the lights it renders, by the method §10 Q3 decides. No
  source gives the original's units: light levels "are not well-understood"
  (Fallout wiki *Sneak (Fallout: New Vegas)*); Skyrim's *GetLightLevel*
  "Returns a number between 0 and 150" while `iLightLevelMax` is 300 (CK).
- **SNK-DET-010.** The sound term MUST include the movement sound from equipped
  weight.
- **SNK-DET-016.** The sound term MUST include the action sound of the player's
  attacks from the weapon's sound level (xEdit FO3 / FNV `WEAP` `VNAM` "Sound
  Level": Loud / Normal / Silent).
- **SNK-DET-017.** Walls MUST block sound by the game's rule: on Oblivion they
  MUST NOT ("Sound is not blocked by walls, or even a dozen feet of solid
  rock", UESP *Oblivion:Sneak*); on FO3 / FNV and Skyrim sound without line of
  sight MUST be attenuated by `fSneakSoundLosMult` (GECK *Category:Detection*;
  CK *Category:Settings*).
- **SNK-DET-011.** A `CreateDetectionEvent` (GECK: "iSoundLevel = 0-100"; FO4 CK
  *CreateDetectionEvent - ObjectReference*) SHOULD enter the tick as an action
  sound at its position. Today the events are queued and never read (§3.1 row 9).
- **SNK-DET-012.** The level MUST follow from the value by the game's thresholds:
  Oblivion `fSneakUnseenMin`, `fSneakNoticedMin`, `fSneakLostMin` and
  `iAICombatMinDetection` (CS *Category:Detection*); FO3 / FNV the "Alert
  Threshold (fSneakNoticedMin = -20)" (GECK *Category:Detection*). **Phase 0:
  verify** the rule that moves a level to Lost on each game.
- **SNK-DET-013.** Each detector MUST carry an alert state (Normal, Alert,
  Combat, Lost; §2), and the formula's detector-state factor MUST read it.
  Today `stealth::DetectorState` exists with no producer.
- **SNK-DET-014.** An actor flagged by `SetNotShowOnStealthMeter` MUST NOT count
  toward the indicator while it is not hostile to the player ("Sets this actor
  to not affect the detection level on the stealth meter if he is not hostile
  to the player", CK *SetNotShowOnStealthMeter - Actor*).
- **SNK-DET-015.** On Skyrim the combat search SHOULD follow stealth points:
  "when your Stealth Points go to 0, you are detected" and "Stealth Points
  always = 0 when enemies are in combat with you" (CK *Stealth Points*), with
  the `iCombatStealthPoint*` settings (CK *Category:Settings*). **Phase 0:
  verify** the regeneration rule beyond `fCombatStealthPointRegenAlertWaitTime`.

### 5.3 Consumers (`SNK-USE`)

- **SNK-USE-001.** A crime MUST be witnessed only by an actor whose level toward
  the player at the crime is at or above the level §10 Q5 decides. Today every
  actor `actor_detects` passes is a witness (§3.1 row 7).
- **SNK-USE-002.** When the target is the player, `GetDetected` (CTDA 45) MUST
  answer 1 when the run-on actor's level toward the player is Seen ("Seen:
  Player has been detected (i.e. GetDetected player > 0)", CS *Detection
  Tab*), and `GetDetectionLevel` (180) MUST answer the level. For any other
  target both MUST answer from the awareness §10 Q7 decides. Today both read
  `actor_detects` (§3.1 row 5).
- **SNK-USE-003.** `IsActorDetected` MUST answer 0 when called on the player
  ("Will always return 0 when called on the player", GECK *IsActorDetected*).
  On any other actor it MUST answer 1 when another actor is aware of it by the
  awareness §10 Q7 decides: the calling actor is never the player, so no level
  toward the player answers it.
- **SNK-USE-004.** Papyrus `Actor.IsDetectedBy(akOther)` MUST answer whether
  `akOther`'s level toward the actor is Seen when the actor is the player, and
  by §10 Q7's awareness otherwise. Today it is not a native.
- **SNK-USE-005.** Ambient hostility MUST start combat with the player only from
  a detector whose level reaches Seen (Oblivion: the value reaches
  `iAICombatMinDetection`, CS *Category:Detection*); combat between other
  actors starts by §10 Q7's awareness. Today the nearest hostile in sight is
  engaged, NPC against NPC included (`faction_hostility.rs:589`).
- **SNK-USE-006.** Guard rush MUST rush the player only from a guard whose level
  toward the player is Seen. Today it reads `actor_detects` (`crime.rs:831`).
- **SNK-USE-007.** Outside the detection module, code MUST NOT call
  `PhysicsWorld::line_of_sight_blocked` or `DetectionConfig::range`, held by a
  source-scan test. Callers that need a plain line test with no awareness
  (ObScript `GetLOS` through `actor_has_line_of_sight`, attraction's line test
  at `byroredux/src/systems/attraction.rs:205`) call the line test the module
  exports. Today the two are called from the hostility pass, the shared sight
  helpers and flee (§3.1 row 8).
- **SNK-USE-008.** Dead-body discovery MUST stay a test of an actor toward the
  body, not a level toward the player, run by the actor-to-actor sight test of
  §10 Q7; an actor that discovers a body SHOULD enter alert state Alert ("When
  an enemy spots a dead body, it will often enter the [CAUTION] state and move
  toward the dead body", Fallout wiki *Sneak (Fallout 3)*).
- **SNK-USE-009.** Flee MUST count the player as a threat only when the fleeing
  actor's level toward the player is Seen, the level SNK-USE-005 starts combat
  at; other threats count by §10 Q7's awareness. Today flee checks range alone,
  with no line of sight (`byroredux/src/systems/flee.rs:57`).
- **SNK-USE-010.** The Fallout 3 and Fallout: New Vegas fix catalogs MUST carry
  an implemented engine-behaviour entry, with its test, for `GetDetectionLevel`
  answering the real level when the player is the target (§6 row 5). At HEAD
  on 2026-11-14 the built-in fixes design has no detection entry.

### 5.4 Sneak attacks (`SNK-ATK`)

- **SNK-ATK-001.** `HitEvent.sneak_attack` MUST be true for a strike the player
  makes in the stance on a detector whose level toward the player is below
  Noticed, and false otherwise: Oblivion "SneakMultiplier is 1 unless your
  character is sneaking and undetected by the target" (UESP *Oblivion:The
  Complete Damage Formula*); Skyrim "someone who is unaware of your presence"
  (UESP *Skyrim:Sneak*); FO3 / FNV "as long as the target has not detected the
  attacker" (Fallout wiki *Sneak Attack Critical*). **Phase 0: verify** whether
  Unseen counts as unaware on each game. Today it is false everywhere (§3.1 row
  10).
- **SNK-ATK-002.** On Oblivion a sneak attack MUST multiply damage by the
  `fPerkSneakAttackMelee<Mastery>Mult` or `fPerkSneakAttackMarksman<Mastery>Mult`
  setting for the player's Sneak mastery (CS *FPerkSneakAttackXXMult*), MUST NOT
  apply to two-handed weapons or spells ("Two-handed weapons do not get the
  damage bonus", "Spells do not get any sneak attack bonus", UESP
  *Oblivion:Sneak*), and MUST NOT stack with a power attack ("max(
  SneakMultiplier, PowerAttackMultiplier )", UESP *Oblivion:The Complete Damage
  Formula*). **Phase 0: verify** the Apprentice marksman value and the Novice
  bonus, each sourced two ways (§6 row 6).
- **SNK-ATK-003.** On FO3 / FNV a sneak attack MUST be a critical hit when the
  weapon allows criticals ("If set to zero, the weapon will not cause any
  critical hits, even for sneak attacks", GECK *Weapons*). **Phase 0: verify**
  the damage multiplier: the Fallout wiki gives "doubling (ranged) or
  quintupling (melee/unarmed)", the GECK says "It is currently unknown how
  sneak damage modifiers are applied" and names `fCombatDamageBonusSneakingMult`,
  `fCombatDamageBonusMeleeSneakingMult` and `fDamageSneakAttackMult`.
- **SNK-ATK-004.** On Skyrim a sneak attack MUST take the weapon class's
  multiplier from the damage-multiplier table of UESP *Skyrim:Sneak* (captured
  in the CHARAL Skyrim ruleset, Related) composed through *Mod Sneak Attack
  Mult* (18, "Adjusts damage multiplier
  applied to a successful sneak attack", CK *Perk Entry Point*). **Phase 0:
  verify** where the base multipliers live (`fCombatSneakAttackBonusMult` is
  100 with no description, CK *Category:Settings*).
- **SNK-ATK-005.** Papyrus `OnHit`'s `abSneakAttack` and the SDK hit event MUST
  carry the value SNK-ATK-001 sets; they already carry the field.
- **SNK-ATK-006.** On FO3 / FNV, `VatsModel::stealth_mult` (`fVATSStealthMult`)
  MUST stay unconsumed until **Phase 0: verify** says what it scales: GECK
  *VATS Settings* gives the default 0.5 and an empty description. Today it is
  read and unused (§3.1 row 16).

### 5.5 Perks (`SNK-PERK`)

- **SNK-PERK-001.** Robotics Expert MUST be offered when its tabs pass with the
  player in the stance: the A8 live leg of the Perk Entry Points PRD. Its other
  conditions are answered at HEAD (`b73899734` bridged `GetUnconscious` and
  `GetIsCreatureType`); its target tab also asks `GetDetected` toward the player
  (CTDA 45, `perk_entry_census`, `CENSUS_ENTRY_POINT=27`), so it reads
  SNK-USE-002. The other four *Activate* perks follow SNK-PERK-002.
- **SNK-PERK-002.** Cannibal, Ghastly Scavenger and Dine and Dash read
  `HasBeenEaten` (CTDA 127: "Returns 1 if the calling actor has been eaten by
  the player", GECK *HasBeenEaten*) and run `StartCannibal` ("The victim gets
  cannibalized by the cannibal, which regains some health", GECK
  *StartCannibal*); Mister Sandman runs `StartMisterSandman`, whose GECK page
  is empty. **Phase 0: verify** what each command changes beyond what the perk
  descriptions state (Mister Sandman: "silently kill any human or Ghoul while
  they're sleeping"; Cannibal: "eat a corpse to regain Health"), and whether
  "eaten" is saved; until then these four perks SHOULD stay unoffered rather
  than run half their script.
- **SNK-PERK-003.** *Ignore Running During Detection* (FO3 / FNV 31, Skyrim 15)
  MUST remove the running term when it composes non-zero ("If the value is
  modified to be non-zero, running will be ignored", GECK *Entry Point*).
- **SNK-PERK-004.** Skyrim *Mod Detection Light* (47) MUST scale the light term
  before the Sneak and Invisibility values apply ("The Sneak and Invisibility
  actor values are applied after this entry point runs", CK *Perk Entry
  Point*). **Phase 0: verify** which terms *Mod Detection Movement* (48) and
  *Mod Detection Sneak Skill* (57) scale; the CK leaves both blank.
- **SNK-PERK-005.** *Mod Sneak Attack Mult* (Skyrim 18) MUST be consumed at
  SNK-ATK-004's call site and its catalog row become Consumed.
- **SNK-PERK-006.** FNV *Has Improved Detection* (69), *Has Improved Spotting*
  (70) and *Has Improved Item Detection* (71) MUST stay Pending in the hook
  catalog until **Phase 0: verify** says what each does (the GECK description
  cells are empty).
- **SNK-PERK-007.** *Apply Sneaking Spell* (Skyrim 69) is owned by the Perk Entry
  Points PRD's PERK-HOOK-009; this PRD MUST supply the "while sneaking" state it
  reads.
- **SNK-PERK-008.** Fallout 4's sneak hooks (15, 18, 47, 48, 57; §3.1 row 11)
  MUST stay Pending until §10 Q2 gives Fallout 4 a detection value. Its *Apply
  Sneaking Spell* (69) reads only the stance, which this PRD MUST supply as
  SNK-PERK-007 does for Skyrim.

### 5.6 The indicator (`SNK-HUD`)

- **SNK-HUD-001.** The native HUD MUST show the detection indicator while the
  player is in the stance, in the game's vocabulary (§3.2), and hide it
  otherwise. Today there is none (§3.1 row 13).
- **SNK-HUD-002.** The indicator MUST map each pair of (worst level, whether
  that detector is hostile to the player) to the game's state by a table in
  the ruleset row. Every game's table is **Phase 0: verify** (§3.2): on FO3 /
  FNV which pair renders [DETECTED] versus flashing [DANGER]; the Oblivion,
  Skyrim, Fallout 4 and Starfield tables whole.
- **SNK-HUD-003.** The FO3 / FNV and Oblivion MenuXml HUDs SHOULD drive their
  vanilla sneak element. **Phase 0: verify** the element and trait the vanilla
  `hud_main_menu.xml` uses for it.
- **SNK-HUD-004.** On Skyrim and Fallout 4 the vanilla Scaleform meter MAY stay
  empty: the HUD meters are fed by GFx object-path invocation (the UI System
  design's Scaleform HUD section), which the Game UI Integration PRD owns.
- **SNK-HUD-005.** On Starfield the indicator MUST show only while the player
  holds rank 1 of the Stealth skill ("Rank 1 - Adds a Stealth Meter",
  Starfield wiki *Stealth*), a gate in Starfield's ruleset row.

### 5.7 Persistence (`SNK-SAVE`)

- **SNK-SAVE-001.** The player's stance MUST survive a save and a load, with the
  `FORMAT_MAJOR` bump that adds it (§10 Q4).
- **SNK-SAVE-002.** Detection levels and alert states MUST NOT be saved: the
  first tick after a load re-derives them (§6 row 4).

### 5.8 Observability (`SNK-DX`)

- **SNK-DX-001.** A `stealth.status` console command MUST list, for every
  counted detector, its distance, line of sight, each formula term, the value,
  the level and the alert state.
- **SNK-DX-002.** A `stealth.sneak <on|off>` console command MUST set the stance
  through the same path the key does, so a smoke can drive it headless.
- **SNK-DX-003.** The install log MUST name the ruleset row the tick runs and
  every game setting it read, with the ones that fell back to a documented
  default.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | The light level is the engine's own measure, in units no source documents (CK *GetLightLevel*: "0 and 150"; `iLightLevelMax` 300) | A canonical light level from the engine's lights, by §10 Q3 | — | A spot that hides the player in the original may not here → SNK-DET-009, SNK-DX-001 (every term visible) |
| 2 | Detectors re-evaluate on a timer ("every .3 seconds", CS *Category:Detection*); whether each detector's timer runs on its own phase is **Phase 0: verify** | The sourced cadence, phase not reproduced | — | Detection a few frames earlier or later than the original → SNK-DET-006 |
| 3 | A searching NPC walks toward the player: "Alerted actors will search towards detection events" (GECK *Category:Detection*) | The level and alert state only | — | [CAUTION] never resolves into a visible search → SNK-NG-002; the AI Packages PRD would own it |
| 4 | The detection state survives a save (**Phase 0: verify**) | Re-derived on the first tick | — | An NPC searching before a save forgets on load → SNK-SAVE-002 |
| 5 | "it does not return any value other than 0 when the player is the target" (GECK *GetDetectionLevel*) | The real level, recorded as an engine-behaviour fix in the FO3 / FNV fix catalogs by SNK-USE-010 | Mods that work around the bug | A script that relied on the 0 sees a level → SNK-USE-002 |
| 6 | Oblivion's sneak multipliers, sourced two ways (CS *Sneak* "4x Marksman" at Apprentice against 3.0 in the GMST pages; a loading screen's "A Novice of Sneak does not get the critical damage bonus") | The settings `Oblivion.esm` authors | — | The wrong bonus at low mastery if the load order's settings are not the ones the game uses → SNK-ATK-002, SNK-DX-003 |
| 7 | Fallout 4 and Starfield detect by formulas no source gives | Per §10 Q2 | — | Either no detection on those games or an unverified one → SNK-DET-002 |
| 8 | Actors detect each other through the same detection states as they detect the player ("Actors pass through 4 detection states", GECK *Category:Detection*) | Per §10 Q7; recommended, levels toward the player only and the actor-to-actor sight test for every other pair | — | NPCs notice each other by sight alone, blind to light, sound and sneak skill; `IsActorDetected` answers by sight → SNK-USE-003, SNK-USE-005, SNK-USE-008, SNK-USE-009 |
| 9 | Skyrim shows detection in its vanilla HUD (the eye, UESP *Skyrim:Sneak*, §3.2); Fallout 4 names a CAUTION detection state (FO4 CK *OnTutorialEvent*), its HUD rendering **Phase 0: verify** | The vanilla Scaleform meter MAY stay empty until the Game UI Integration PRD feeds HUD meters; the native indicator stands in | — | A Skyrim or Fallout 4 player sees no vanilla eye or meter → SNK-HUD-004, SNK-HUD-001 |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 2. The CS documents the whole formula and every game setting's default |
| Fallout 3 | yes | Wave 1. The GECK formula; shares the FNV runtime without the level terms |
| Fallout: New Vegas | yes | Wave 1. The reference title, the formula `stealth.rs` already transcribes, and the activate perks |
| Skyrim (LE / SE / AE) | yes | Wave 2. The CK formula and settings; stealth points; the Scaleform meter per SNK-HUD-004 |
| Fallout 4 | partly | Stance and conditions in Wave 1; detection per §10 Q2 (the CHARAL FO4 ruleset records the formula as a dead end); sneak hooks per SNK-PERK-008 |
| Fallout 76 | no | SNK-NG-005 |
| Starfield | partly | Stance and conditions in Wave 1; detection per §10 Q2 (game settings sourced, formula not) |

### 7.2 Acceptance

A scenario passes when:

1. **Stance.** `cargo test` cases show the Sneak action toggling the stance,
   `IsSneaking` answering 1 then 0, a disabled `Control::Sneaking` layer
   refusing it, and Sprint in the stance counting as running on the FO3 / FNV
   rows (SNK-STN-001, -002, -004, -006).
2. **Formula.** A `cargo test` per ruleset row computes a fixture pair's value
   and level and matches the transcription test of that row's sourced formula
   (SNK-DET-003, -004, -005, -007).
3. **Settings census.** An ignored real-data test reads every game setting the
   rows name from `Fallout3.esm`, `FalloutNV.esm`, `Oblivion.esm` and
   `Skyrim.esm`, reports each one's value or its fallback, and fails on a name
   with neither an authored value nor a documented default (SNK-DX-003).
4. **One producer.** A source-scan test holds the calls of
   `PhysicsWorld::line_of_sight_blocked` and `DetectionConfig::range` outside
   the detection module at zero (SNK-USE-007).
5. **FNV stealth.** A `docs/smoke-tests/` script holds the engine on a vanilla
   FNV interior Phase 0 names, sets the stance with `stealth.sneak on`, and reads
   through `stealth.status` a detector at Lost from behind cover and at Seen in
   its sight (SNK-DET-001, SNK-DX-001, -002).
6. **Unseen theft.** A `cargo test` in the shape of
   `a_witnessed_theft_adds_the_bounty_and_marks_the_faction`
   (`byroredux/src/inventory/tests.rs`): with every detector below §10 Q5's
   level a theft has no witness and adds no bounty, and with one detector at
   that level the same theft adds the authored bounty (SNK-USE-001).
7. **Activate perk, live.** A `docs/smoke-tests/` script holds FNV
   (`--bench-hold`) on a cell with a robot that Phase 0 names, gives the player
   Robotics Expert with `obscript.cmd` (`AddPerk`), places the player behind
   the robot with `cam.pos`, sets the stance with `stealth.sneak on`, presses
   `input.press activate`, answers the "Deactivate" choice with `ui.answer`,
   and reads `GetUnconscious` 1 on the robot through `obscript.cmd`
   (SNK-PERK-001; the Perk Entry Points PRD's A8 live leg).
8. **Sneak attack.** A `cargo test` per game shows a strike on an unaware
   detector setting `sneak_attack` and taking the game's multiplier, and the
   same strike on an aware detector doing neither (SNK-ATK-001 to -004).
9. **Indicator.** A `cargo test` builds the native HUD's `PanelSnapshot`
   (`crates/debug-ui/src/panels/snapshots.rs`) for each game's ruleset row and
   asserts its indicator field for every (level, hostility) pair, and its
   absence outside the stance; no image is compared (SNK-HUD-001, -002).

No game file is checked in; the scenarios read the owner's installs through
the existing `BYROREDUX_*_DATA` paths.

---

## 8. Metrics

The numbers live in the ROADMAP row this PRD's milestone gets (§10 Q6).

| Metric | Definition | Granularity |
|---|---|---|
| Consumers on the level | Consumers in §3.1 rows 5, 7, 8 that ask about the player and read the level ÷ all consumers there that ask about the player | Per phase |
| Formula coverage | Formula terms computed from a sourced expression ÷ all terms of the row | Per game |
| Settings resolved | Game settings the row reads that the master authors ÷ all it reads | Per game |
| Sneak-gated perks fireable | Vanilla entries whose tabs can pass in play ÷ vanilla entries gated on `IsSneaking` | Per game |
| Verify items closed | Rows of the §9 Phase 0 verify table answered ÷ all its rows | Per phase |

---

## 9. Delivery sequence

**Phase 0 — Design and verify list (no engine code).** Write
`docs/engine/sneak-detection.md` (stance, tick, level, alert state, the
actor-to-actor sight test §10 Q7 decides, light input, consumer migration,
FO3 / FNV sneak-attack damage, schedule placement) and register it; answer
every row of the verify table below or file an issue for it; run the settings
census of §7.2 scenario 3 as a one-off example and check in its table; name the
vanilla references scenarios 5 and 7 run on. Needs §10 Q1 to Q7 decided. Exit:
the design is merged, the verify table has no unanswered row without an issue,
and the census table is checked in.

| # | Verify item | IDs | What settles it | Issue |
|---|---|---|---|---|
| V1 | Whether disabling sneaking stands an already sneaking player up | SNK-STN-004 | An in-game test per game with the sneak control disabled | #6935 |
| V2 | The `MOVT` values vanilla Skyrim races name in `SNMV` | SNK-STN-005 | xEdit read of `Skyrim.esm` `RACE` and `MOVT` | closed (design §13.3) |
| V3 | Whether FO3 and FNV let the player run in the stance | SNK-STN-006 | An in-game test on FO3 and FNV | #6935 |
| V4 | First-person camera height in the stance | SNK-STN-007 | A camera-height game setting found by the census, or an in-game measurement | #6935 |
| V5 | Sneak clip names (KF games) and Skyrim's behaviour-graph variable | SNK-STN-008 | A scan of each game's archives | closed (design §2) |
| V6 | Values of the FO3 / FNV detection coefficients | SNK-DET-004 | The census (§7.2 scenario 3) | closed (design §13.1) |
| V7 | The FO3 / FNV detection cadence | SNK-DET-006 | A timer setting found by the census, or the executable's default table | closed (design §4.1) |
| V8 | Skyrim's `fSneakBaseValue` | SNK-DET-007 | `Skyrim.esm` through the census, or the executable's default table | closed (design §13.1) |
| V9 | Whether Oblivion and FO3 / FNV have a view cone | SNK-DET-008 | A view-cone setting found by the census, or an in-game test | FO3 / FNV closed; Oblivion #6936 |
| V10 | The rule that moves a detector to level Lost | SNK-DET-012 | The *Category:Detection* timers through the census, then an in-game test | FO3 / FNV closed (design §4.3); Oblivion, Skyrim #6936 |
| V11 | Skyrim's stealth-point regeneration | SNK-DET-015 | The CK settings through the census and an in-game test | #6936 |
| V12 | Whether level Unseen counts as unaware for a sneak attack | SNK-ATK-001 | An in-game test per game | FO3 / FNV closed (design §8); Oblivion, Skyrim #6936 |
| V13 | Oblivion's Apprentice marksman multiplier and Novice bonus | SNK-ATK-002 | `Oblivion.esm` through the census and an in-game test | closed (design §8) |
| V14 | The FO3 / FNV sneak-attack damage multiplier | SNK-ATK-003 | The three GECK-named settings through the census and an in-game test | closed (design §8) |
| V15 | Where Skyrim's base sneak-attack multipliers live | SNK-ATK-004 | `Skyrim.esm` settings and weapon records, and an in-game test | closed (design §8); bonus-mult role #6936 |
| V16 | What `fVATSStealthMult` scales | SNK-ATK-006 | An in-game VATS test | closed (design §8) |
| V17 | What `StartCannibal` and `StartMisterSandman` change, and whether "eaten" is saved | SNK-PERK-002 | An in-game test across a save and load | #6937 |
| V18 | Which terms Skyrim 48 and 57 scale | SNK-PERK-004 | An in-game test with a vanilla perk using each, named by the census | #6936 |
| V19 | What FNV 69, 70 and 71 do | SNK-PERK-006 | The vanilla perks using each, named by the census, and an in-game test | closed (design §9): companion perception perks |
| V20 | Each game's indicator table: FO3 / FNV [DETECTED] versus flashing [DANGER], the rest whole | SNK-HUD-002, §3.2, §6 row 9 | In-game captures of each state with its detector's level and hostility | #6935 |
| V21 | The sneak element and trait of the vanilla `hud_main_menu.xml` | SNK-HUD-003 | Reading the XML from each MenuXml game's archives | closed (design §10) |
| V22 | Whether each detector's timer runs on its own phase | §6 row 2 | An in-game timing test or the executable | closed (design §4.1) |
| V23 | Whether detection state survives a save | §6 row 4 | An in-game test: save while an NPC searches, then load | #6935 |

**Phase 1 — The stance.** SNK-STN-001 to -007, SNK-SAVE-001, SNK-DX-002, on
every game. Exit: §7.2 scenario 1 passes, and `IsSneaking` reads 1 in a held
FNV session after `stealth.sneak on`.

**Phase 2 — Detection, Wave 1 (FO3, FNV).** SNK-DET-001 to -004, -006, -008 to
-014, -016, -017, SNK-USE-001 to -010, SNK-SAVE-002, SNK-DX-001, -003. Depends
on the CHARAL FNV ruleset's capture. Exit: §7.2 scenarios 2 to 6 pass on FO3
and FNV, and the consumers-on-the-level metric is 100 % in ROADMAP.

**Phase 3 — Sneak attacks and perks, Wave 1.** SNK-ATK-001, -003, -005, -006,
SNK-PERK-001 to -003, -006, -008. Depends on the Perk Entry Points PRD's
activate runtime (`683d9a92b`). Exit: §7.2 scenarios 7 and 8 pass on FNV, and
the sneak-gated-perks metric is published for FO3 and FNV.

**Phase 4 — The indicator.** SNK-HUD-001 to -005. Exit: §7.2 scenario 9 passes
for FO3, FNV, Oblivion and Skyrim.

**Phase 5 — Wave 2 (Oblivion, Skyrim).** SNK-DET-005, -007, -015, SNK-ATK-002,
-004, SNK-PERK-004, -005, -007, SNK-STN-008. Exit: §7.2 scenarios 2, 3 and 8
pass on Oblivion and Skyrim, and formula coverage is published for both.

**Phase 6 — Fallout 4 and Starfield detection.** Per §10 Q2. Exit: the
decision's outcome is in ROADMAP with its formula-coverage number, or the
games are recorded out with the reason.

---

## 10. Decisions required

1. **Q1 — The stance input.** Recommended: a toggle on Left Ctrl, as Oblivion,
   Skyrim and FNV ship it (SNK-STN-001's sources); it collides with nothing at
   HEAD. Alternative: hold to sneak; consequence: a departure from every
   sourced default, and a §6 row.
   **Decided** (2026-11-14): the recommendation — a toggle on Left Ctrl.
2. **Q2 — Fallout 4 and Starfield detection.** Recommended: ship their stance
   and conditions, and keep their detection on the sight test until a source
   gives their formula; consequence: those games keep today's awareness, with
   the reason recorded. Alternative A: run the Skyrim CK formula over each
   game's own game settings (Starfield authors `fSneakMaxDistance`,
   `fSneakSoundLosMult`, `fSneakRunningMult`, `fDetectionViewCone` and the rest
   under the same names, Starfield wiki *Mod File Format/GMST*); consequence:
   plausible stealth now, but unverified behaviour and a §6 row per game.
   Alternative B: drop both games from this PRD; consequence: `IsSneaking`
   stays 0 there too.
   **Decided** (2026-11-14): the recommendation — Fallout 4 and Starfield ship the stance and its
   conditions, and their detection stays on the sight test until a source gives
   their formula.
3. **Q3 — The light level.** Recommended: a light level computed on the CPU from
   the lights the renderer is given (the cell's ambient and directional light
   plus each point light's attenuated contribution at the player), scaled to
   each game's documented range; consequence: deterministic, testable, no
   frame of latency, but blind to RT shadowing. Alternative A: read back the
   lit luminance at the player from the GPU; consequence: matches what is on
   screen, costs a readback and a frame of latency, and ties gameplay to the
   renderer. Alternative B: ignore light (a constant); consequence: darkness
   never helps the player.
   **Decided** (2026-11-14): the recommendation — a light level computed on the CPU from the
   lights the renderer is given, scaled to each game's documented range.
4. **Q4 — What a save keeps.** Recommended: the stance only (SNK-SAVE-001,
   -002); consequence: one `FORMAT_MAJOR` bump, and awareness re-derived on the
   first tick. Alternative A: also save levels and alert states; consequence:
   a searching NPC keeps searching across a load, at the cost of per-actor
   state in every save. Alternative B: save nothing; consequence: the player
   stands up on every load.
   **Decided** (2026-11-14): the recommendation — the save keeps the stance only.
5. **Q5 — The witness level.** Recommended: Seen, the level the CS equates with
   `GetDetected` ("Seen: Player has been detected", CS *Detection Tab*), so a
   witness is exactly an actor for whom `GetDetected` is 1. Alternative:
   Noticed; consequence: stricter for the player (a suspicious NPC reports the
   crime), with no source that says so.
   **Decided** (2026-11-14): the recommendation — a witness is an actor whose level toward the
   player is Seen.
6. **Q6 — The milestone.** Recommended: a new Tier 7 milestone, M65 "Sneak and
   detection", depending on M41 (actors), M42 (hostility) and M24.2 (perks);
   no M number is in use from M65 to M69. Alternative: fold it into M42;
   consequence: the AI Packages PRD's scope grows by a player-side system it
   does not describe.
   **Decided** (2026-11-14): the recommendation — a new Tier 7 milestone, M65 "Sneak and
   detection", depending on M41, M42 and M24.2.
7. **Q7 — Whose awareness the tick computes.** Recommended: levels toward the
   player only (SNK-DET-001), and one actor-to-actor sight test, owned by the
   detection module, for every case where the player is not the target:
   hostility between NPCs and creatures, flee from a non-player threat,
   dead-body discovery, `GetDetected` / `GetDetectionLevel` on a non-player
   target, `IsActorDetected` and `IsDetectedBy` on a non-player actor.
   Consequence: the tick's cost stays linear in the detectors near the player,
   and `GetDetectionLevel` on a non-player target keeps answering 3 or 0, which
   is what the original answers there too ("It will only return a value of 3
   or 0 for all other cases where the player is not the target", GECK
   *GetDetectionLevel*); NPCs notice each other by sight alone (§6 row 8).
   Alternative: run the formula for every detector–target pair in range;
   consequence: NPC ambushes and NPC-on-NPC stealth follow the original's
   detection states, at a per-tick cost quadratic in the actors in range and a
   level to keep for every pair.
   **Decided** (2026-11-14): the recommendation — levels toward the player only, and one
   actor-to-actor sight test, owned by the detection module, for every other
   case.

---

## 11. Document map

| Need | Go to |
|---|---|
| The FO3 / FNV formula and its sources | [CHARAL FNV/FO3 ruleset § Sneak Detection](../engine/charal-fnv-fo3-ruleset.md#sneak-detection-fnv--locked-but-an-aistealth-subsystem-not-a-charal-stat) |
| `fVATSStealthMult` and the other VATS settings | [CHARAL FNV/FO3 ruleset § VATS tuning](../engine/charal-fnv-fo3-ruleset.md#vats-tuning-action-points-regen-and-the-radiation-gauge--built-6350) |
| The Skyrim formula and sneak-attack multipliers | [CHARAL Skyrim ruleset § Sneak Detection](../engine/charal-skyrim-ruleset.md#sneak-detection-skyrim--locked-sounddistance-halves-pending-visualskill-halves--out-of-charal-scope) |
| Oblivion damage and crime | [CHARAL Oblivion ruleset](../engine/charal-oblivion-ruleset.md) |
| Fallout 4 detection, recorded as a dead end | [CHARAL FO4 ruleset](../engine/charal-fo4-ruleset.md) |
| The activate perks and the sneak entry points | [Perk Entry Points PRD](perk-entry-points.md) |
| NPC sneaking, Crouch To Reload | [AI Packages Completion PRD](ai-packages-completion.md) |
| HUD surfaces | [UI System](../engine/ui.md), [Game UI Integration PRD](game-ui-integration.md) |
| Engine-behaviour fixes | [Built-in fixes](../engine/builtin-fixes.md) |
| Input enable layers | [Scripting § Input enable layers](../engine/scripting.md#input-enable-layers-m472) |
| Save format | [Save/Load Round-Trip](../engine/save-load-roundtrip.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-11 | First draft | — |
| 2 | 2026-11-14 | Review round 1 folded in: inventory re-measured at `922072af8` (§3.1 row 8 per consumer: hostility's own `in_sight` over every pair, guard rush on `actor_detects`, dead-body discovery on `actor_sees_corpse`, flee on range alone; anchors re-pointed); §2 *Sight test* as the family, *level Lost* versus *alert state Lost*; `fVATSStealthMult` owner moved to the CHARAL FNV/FO3 ruleset (row 16, SNK-ATK-006); player-only detection scope as new Q7 with §6 row 8 (corollary, SNK-G-002, SNK-DET-001, SNK-USE-002 to -005 reworded, SNK-USE-006 split into -006, -008, -009); one-producer test re-aimed at `line_of_sight_blocked` / `DetectionConfig::range` (SNK-USE-007); FO3 / FNV indicator mapping keyed on level and hostility, sourced conflict marked verify (§3.2, SNK-HUD-002); §6 rows 2 and 3 cited, row 9 added; light-level quote cited at its wiki page; SNK-PERK-001 scoped to Robotics Expert, SNK-PERK-002 counts four; SNK-STN-006 given a provisional default and moved to Phase 1; fix-catalog entry as SNK-USE-010; FO4 sneak hooks as SNK-PERK-008; SNK-DET-010 split into -010, -016, -017; acceptance drivers named (scenarios 6, 7, 9) and the census fail rule matched to the fallback; SNK-SAVE-002 and SNK-HUD-004 scheduled; Phase 0 verify table (V1–V23); sneak-attack owners and document map rows; per-game rules stated as ruleset fields | claude: revise |
| 3 | 2026-11-14 | Decisions Q1–Q7 recorded: every recommendation taken | — |
| 4 | 2026-11-14 | PROPOSED → APPROVED | owner: approved |
| 5 | 2026-11-14 | → ACTIVE: parent #6908 and phase issues #6909 to #6915 filed; the Phase 0 verify items V1–V23 are acceptance criteria of #6909; ROADMAP gains the M65 row (Q6) | — |
| 6 | 2026-11-15 | Phase 0 (#6909): the design [`sneak-detection.md`](../engine/sneak-detection.md) owns the mechanisms in *Related*; the §9 verify table carries each row's state (12 of 23 closed, the rest on #6935, #6936, #6937). Findings that refine requirements: no playable Skyrim race authors `SNMV`, so SNK-STN-005's Skyrim sneak speed comes from the `NPC_Sneaking_MT` movement type (design §2); FNV 69–71 are companion perception perks (SNK-PERK-006 stays Pending); FO3 / FNV store only levels Seen and Lost, so SNK-HUD-002's indicator table keys on the alert state as well as the level and hostility (design §10); `fVATSStealthMult` scales the VATS hit chance against Chameleon / Invisibility, not a sneak attack (SNK-ATK-006 holds) | — |
