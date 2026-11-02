# AI Packages Completion — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (census provenance, the FO3/FNV object-type enum and its xEdit
disagreement, the Skyrim Lean value settled by a mesh census, the FO4
month sentinel settled from xEdit, Q1 reworked, HEAD answers for the
Starfield graph gate and `input_destination`, `pack.ambient` split into
one observable per ID). It covers the remainder of milestone M42: the
procedures, package fields and per-game prerequisites that do not run yet.
Statements about how the original engines behave cite external sources
(UESP, the CS / GECK / Creation Kit wikis, fopdoc, xEdit, nif.xml). A
claim that could not be verified is marked **Phase 0: verify** instead of
being stated as fact.

**Scope**: What it means for an actor's own AI package list (`PKID`, the
*ambient* packages) to run on ByroRedux in every target game. This covers
procedure coverage, how far each procedure consumes its authored data,
schedules, furniture use, and the per-game sources that let an actor walk
and sit. The scope is the remainder of the
[M42 row](../../ROADMAP.md#tier-7--deep-gameplay-systems), which lists what
already ships. Out of scope: the Scene-driven package executor (PACKAL
keeps it unchanged), combat AI, the dialogue runtime behind force-greet,
navmesh door and cover semantics, and Fallout 76 (§7.1).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| PACK decode, package selection, the FO3/FNV flat procedures, seating | [NPC Spawn → AI Packages](../engine/npc-spawn-ai-packages.md) |
| Skyrim+ tree/template packages on the ambient driver | [PACKAL](../engine/packal.md) |
| Walk-leg routing over `NAVM`, FO4 `NVNM` decode as a prerequisite | [NAVM Pathfinding](../engine/navmesh-pathfinding.md) |
| Package state across tile eviction and respawn | [Stream-Boundary State Continuity](../engine/stream-boundary-state-continuity.md) |
| Conditions (the M47.1 evaluator), the caster runtime, ObScript package commands | [Scripting](../engine/scripting.md) |
| Oblivion `PGRD` pathing | none yet. Phase 0 writes it into [NAVM Pathfinding](../engine/navmesh-pathfinding.md) if decision Q5 takes it |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md#per-game-support-detail) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `AIP-` prefix.

---

## 1. Product thesis

Ambient AI is what makes a loaded cell look inhabited. A bartender works
the bar, a settler sleeps at night, a guard holds a post, a sniper fires
from a wall. Every one of those behaviours is authored data in the
`PACK` record. Following the keep list's "parse data, don't emulate
engine" rule
([ROADMAP keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)),
ByroRedux runs that data through ECS systems. It does not reproduce the
original AI process.

Most of M42 has shipped. On FO3 and FNV every authored procedure has a
runtime; CastMagic has none, and FO3/FNV author no CastMagic package
(§3.1). What is left is of three kinds:

1. **Data a running procedure still drops.** Find skips most object types.
   UseWeapon fires from wherever the actor stands. Weapon classes are
   declined.
2. **Per-game prerequisites.** Skyrim+ ambient packages reach two leaf
   types. Only FO3/FNV have a sit clip. Skyrim+ ignores the schedule's day
   of the week. Starfield has no walk source.
3. **Things that silently do nothing.** A package that is declined says
   nothing about why.

The corollary that shapes every requirement below: **every authored package
either runs or is declined for a reason someone can read**. Each procedure,
in each game, is in exactly one class of §3. A decline is a reported
state, not a silent `None`. Coverage grows by moving rows between classes,
not by guessing semantics. When a source cannot settle what a field means,
the package stays declined and the gap is a **Phase 0: verify** item.

The engine will **not**:

- reproduce the original engines' internal sub-procedures (GECK
  [GetCurrentAIProcedure](https://geck.uesp.net/wiki/GetCurrentAIProcedure)
  lists about fifty of them);
- invent a meaning for an unsourced field to raise coverage;
- add per-game branches past the parser boundary. A per-game numbering
  becomes one canonical value inside `crates/plugin`.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Package** | One `PACK` record. Vocabulary from [NPC Spawn → AI Packages §3](../engine/npc-spawn-ai-packages.md#3-pack-record-parsing). |
| **Flat shape** | The Oblivion / FO3 / FNV package: `PKDT` procedure byte plus `PSDT` / `PLDT` / `PTDT`. `PackRecord::procedure_type` holds the byte. |
| **Tree shape** | The Skyrim / FO4 / FO76 / Starfield package: a `PKCU` template plus a procedure tree (`PackRecord::procedures`, `data_inputs`). Defined in [PACKAL §1](../engine/packal.md#1-whats-already-unified-vs-what-isnt). |
| **Procedure** | A flat-shape `procedure_type` value, or a tree-shape leaf's `procedure_type` string (`"Sandbox"`, `"Travel"`, …). |
| **Ambient driver** | `ambient_ai_package_system` plus `AmbientBehavior::from_package` (`byroredux/src/npc_spawn/ai_package.rs`). It selects an actor's own package and installs a behaviour component. |
| **Scene driver** | `scene_package_system` (`crates/scripting/src/package.rs`). It runs packages that a `SCEN` action names. Unchanged by this PRD. |
| **Install** | The ambient driver attaches a behaviour component (`SandboxBehavior`, `FindPackageBehavior`, …) for the winning package. |
| **Decline** | The ambient driver refuses a package. Either selection skips it (`selectable_by_installer`), or `from_package` returns `None`. |
| **Decline line** | One debug log line plus one `pack.ambient` field naming the package, its procedure and one reason from the closed set in §3. |
| **Fully modelled** | Every condition function in the package's `CTDA` list is in the M47.1 catalog (`package_conditions_fully_modeled`). Procedures that take control of the actor install only from a fully modelled condition pass. This is the #5376 rule, recorded in [NPC Spawn → AI Packages](../engine/npc-spawn-ai-packages.md#whats-not-covered--honest-state). |
| **Object type** | The `PTDT` / `PLDT` type-2 / type-5 enum that names a class of objects ("Furniture", "Weapons: Ranged"). Each game numbers it its own way. |
| **Marker kind** | `FurnitureMarkerKind` (`Sit` / `Sleep` / `Lean`, `crates/core/src/ecs/components/furniture.rs`), resolved at import from the furniture NIF. |
| **Seat procedure** | A procedure whose runtime ends in `seat_at_marker` or the Sandbox seat step: Sandbox, Eat, Sleep, UseItemAt. |
| **Walk source** | The archive data a walking actor animates from: a KF clip, an HKX clip or a behaviour graph. |
| **Sit-enter clip** | The clip whose last frame is the seated pose that seating holds (`sandbox_sit_enter_kf_path`). |
| **Census** | A count over the vanilla masters or archives. Round-1 counts came from uncommitted Python scripts; Phase 0 replaces them with a checked-in tool (AIP-DX-003) that is re-run on demand. |

---

## 3. Core model

Every procedure, per game, is in exactly one class:

| Class | Name | Meaning | Reported as |
|---|---|---|---|
| **R** | Runs | Installs from authored data. Every decoded field is consumed or listed in §6 | the installed component |
| **A** | Approximated | Runs on another procedure's runtime. The difference is listed in §6 | the installed component, plus §6 |
| **P** | Partial | Runs for some data shapes and declines the rest | the component, or a decline line naming the shape |
| **D** | Declined | No runtime. The reason is catalogued | a decline line naming the reason |
| **N** | Not authored | No vanilla package in the game uses it | none |

The decline reasons are a closed set: `unmodelled-conditions`,
`unsupported-shape`, `unsupported-object-type`, `unsourced-semantics`,
`no-runtime`.

**Census provenance.** Every count in §3 and §5 was taken on 2026-11-01
with uncommitted Python scripts that walk the ESM GRUP tree (PACK counts,
PSDT fields) or the BSA / BA2 mesh archives (furniture markers). Review
round 1 re-ran them on `FalloutNV.esm`, `Fallout3.esm`, `Oblivion.esm`,
`Skyrim.esm` and `Fallout4.esm` and got the same numbers. They are
**Phase 0: confirm** figures, not checked-in facts: Phase 0 checks the tool
in (AIP-DX-003) and regenerates every table from it.

### 3.1 Flat-shape procedures

Counts are `PACK` records by `PKDT` procedure byte, per master (uncommitted
census, see above). The **Today** column is the state at HEAD on
2026-11-01. The numbering matches across the three games for 0–10.
Oblivion's enum ends at 11 = Cast magic
([UESP: Oblivion PACK](https://en.uesp.net/wiki/Oblivion_Mod:Mod_File_Format/PACK),
local dump `uesp-wiki/Oblivion Mod/Mod File Format/PACK.wiki`). FO3/FNV
continue to 16, and their 11 is "??"
([fopdoc: FNV PACK](https://tes5edit.github.io/fopdoc/FalloutNV/Records/PACK.html)).

| # | Procedure | Obl | FO3 | FNV | Class (Obl / FO3·FNV) | Today |
|---|---|---:|---:|---:|---|---|
| 0 | Find | 741 | 78 | 81 | P / P | `find_package_system` (`systems/find_package.rs`). Object-type targets are searched on `Fallout3NV` only, and only for Ammo 13, Alchemy 17 and Food 18. Every other type, and any object type on Oblivion, is declined with no log line (`find_behavior`, `npc_spawn/ai_package.rs:139-167`) |
| 1 | Follow | 208 | 115 | 132 | R / R | `follow_system` |
| 2 | Escort | 75 | 30 | 12 | R / R | `escort_system` |
| 3 | Eat | 829 | 296 | 317 | P / A | `systems/eat_sleep.rs` walks and sits; food is never eaten. On Oblivion the walk runs, but there is no sit clip, so the actor never sits (`sandbox_sit_enter_kf_path` → `None`) |
| 4 | Sleep | 725 | 246 | 390 | P / A | `systems/eat_sleep.rs`. A bed is preferred, with a sit-marker fallback (#5390). Oblivion: walk, no seat |
| 5 | Wander | 1 820 | 42 | 31 | R / R | `wander_system` |
| 6 | Travel | 1 924 | 907 | 1 260 | R / R | `travel_system` |
| 7 | Accompany | 40 | 24 | 14 | A / A | runs as Follow (`from_package`, `is_accompany` arm) |
| 8 | UseItemAt | 751 | 20 | 26 | A / A | installs `EatBehavior`. The `PTDT` item is not read |
| 9 | Ambush | 80 | 25 | 38 | A / A | Guard wait plus the hostility gate (`AmbushBehavior`) |
| 10 | FleeNotCombat | 11 | 8 | 15 | R / R | `flee_package_system` |
| 11 | CastMagic | 5 | 0 | 0 | D / N | no `PROCEDURE_*` constant. A flat package falls through to `from_skyrim_procedure_tree` (`ai_package.rs:489-490`), which returns `None` on an empty tree |
| 12 | Sandbox | n/a | 681 | 753 | — / P | `systems/sandbox.rs`: nearest free sit marker. Scored activity selection is not built (§6) |
| 13 | Patrol | n/a | 180 | 505 | — / A | Wander's core; patrol routes are not decoded (`PROCEDURE_PATROL` doc) |
| 14 | Guard | n/a | 172 | 175 | — / R | `guard_system` |
| 15 | Dialogue | n/a | 386 | 332 | — / P | force-greet bridge, installed only for modelled player-targeted Conversations (#5376) |
| 16 | UseWeapon | n/a | 56 | 82 | — / P | `use_weapon_package_system`. Installs only a specific weapon (`PTDT` type 1) aimed at a specific reference (`PTD2` type 0) with fully modelled conditions (`use_weapon_behavior`, `ai_package.rs:104-128`) |

### 3.2 Tree-shape leaves (Skyrim / FO4 / FO76 / Starfield)

The Skyrim Creation Kit documents 29 procedure leaves
([CK: Procedures](https://ck.uesp.net/wiki/Category:Procedures), local dump
`ck-uesp-wiki/(main)/* (Procedure).wiki`, 29 files). The state at HEAD on
2026-11-01:

| Leaf group | Ambient driver Today | Scene driver Today |
|---|---|---|
| Sandbox, Patrol | installs (`from_skyrim_procedure_tree`, `ai_package.rs:525-562`: first leaf by name, conditions not evaluated) | Patrol as move-to |
| Travel, Escort, FollowTo | none | move-to (`resolve_command`, `crates/scripting/src/package.rs`) |
| Acquire, Activate, Shout, Sit, UseIdleMarker, UseWeapon | none | interaction (`resolve_command`) |
| ForceGreet | none. Deferred until instance conditions are modelled (comment in `from_package`, Dialogue arm) | the force-greet bridge resolves both dialects (`PackRecord::force_greet`) |
| Eat, Sleep, Find, Flee, Follow, Guard, HoldPosition, KeepAnEyeOn, LockDoors, UnlockDoors, Say, UseMagic, Wait, Wander, FlightGrab, Hover, Orbit | none | none |

The FO4, FO76 and Starfield leaf vocabularies are not documented locally
(the FO4 CK dump has only `Range (Procedure)` and `Sandbox (Procedure)`).
**Phase 0: verify** the leaf names each of those masters authors, by running
`pack_ambient_shape_survey` (`crates/plugin/examples/`) on `Fallout4.esm`,
`SeventySix.esm` and `Starfield.esm`.

### 3.3 Prerequisites by game

The state at HEAD on 2026-11-01:

| Game | Package shape | Walk source Today | Sit-enter Today | Pathing Today | Day-of-week Today |
|---|---|---|---|---|---|
| Oblivion | flat | `handtohandforward.kf` (`humanoid_walk_kf_path`, `npc_spawn/anim_clips.rs`) | none (`sandbox_sit_enter_kf_path`, "deferred") | none: no `NAVM` exists, and `PGRD` is parsed (`crates/plugin/src/esm/records/pathgrid.rs`) but has no consumer in `byroredux/src` | evaluated, plus the date byte (`package_weekday`, `package_day_of_month`, `crates/scripting/src/globals.rs:145-189`) |
| FO3 / FNV | flat | `mtforward.kf` per gender and child | `chairskirt_leftenter.kf` | `NAVM` within and across resident tiles (`path_from_resident_tiles`, `systems/navmesh_path.rs`) | evaluated (`package_weekday`) |
| Skyrim LE / SE | tree | `1hm_walkforward.hkx` (`SkyrimWalkClip`, `components/ai_anim.rs`) | none | `NAVM` (`NVNM` decoded, `decode_nvnm`) | not evaluated (`package_weekday` → `None`) |
| FO4 | tree | behaviour graph `MTBehavior.hkx` (`graph_locomotion_system`, `asset_provider/actor_graph.rs`) | none | none: the `NVNM` body never reconciles and its bytes are retained (`decode_nvnm`, `crates/plugin/src/esm/records/misc/world.rs`) | not evaluated |
| FO76 | tree | none | none | not measured | not evaluated |
| Starfield | tree | none: `humanoid_walk_kf_path` → `None`, `populate_skyrim_walk_clip` is Skyrim only (`asset_provider/animation.rs:243`), and the graph path is Fallout 4 only (`populate_archetype_idles`, `asset_provider/archetype_idle.rs:94`) | none | not measured: `parse_navm`'s coverage table (`world.rs`) lists no Starfield plugin | not evaluated |

These hand-written tables are replaced by the census tool (AIP-DX-003) and
`pack.ambient` (AIP-DX-001) and regenerated at each phase exit.

---

## 4. Goals and non-goals

### 4.1 Goals

- **AIP-G-001 — No silent decline.** In every in-scope game, every vanilla
  package referenced from an `NPC_` / `CREA` package list either installs or
  produces a decline line with a reason from the closed set in §3. Measured
  by the census tool and the real-data tests.
- **AIP-G-002 — FO3/FNV data fully consumed.** On FO3 and FNV, every
  procedure with authored packages is class R or A. Each remaining
  approximation has a §6 row, and each P row shrinks to declines with
  `unsourced-semantics` as the only reason.
- **AIP-G-003 — One ambient driver for both shapes.** Skyrim+ tree-shape
  packages install the same behaviour components as flat-shape ones,
  through the one ambient driver ([PACKAL goal](../engine/packal.md)). No
  system reads a game kind to decide behaviour.
- **AIP-G-004 — Every walker animates from archive data.** In every
  in-scope game, an actor that a package moves plays a walk source from
  that game's own archives.
- **AIP-G-005 — Schedules honour every authored constraint.** Each `PSDT`
  field a game authors (hour, minute, duration, day of week, date, month)
  is evaluated, or is declined as `unsourced-semantics` with a verify item.
- **AIP-G-006 — Explainable.** For any actor, a console command shows which
  package won, which procedure ran, and why each higher-priority package
  was skipped.

### 4.2 Non-goals

- **AIP-NG-001.** Resolving `NearReference` targets beyond what resolves
  today. The M42 row deprioritised it.
- **AIP-NG-002.** The Scene driver. PACKAL keeps it unchanged
  ([PACKAL §6](../engine/packal.md#6-what-stays-out-of-scope)).
- **AIP-NG-003.** Reproducing the original AI's internal sub-procedures or
  its evaluation timer. ROADMAP keep list: "parse data, don't emulate
  engine".
- **AIP-NG-004.** Navmesh door and cover semantics, triangle-flag path cost
  and dynamic obstacle avoidance. These are owned and excluded by
  [NAVM Pathfinding §2](../engine/navmesh-pathfinding.md#2-scope).
- **AIP-NG-005.** Fallout 76 (§7.1).
- **AIP-NG-006.** New animation content. Every walk, sit and cast pose comes
  from the game's own archives. A game whose archives have no source for a
  pose keeps the posture it has today, and the gap is reported.
- **AIP-NG-007.** Sandbox's scored multi-activity selection (eat, sleep,
  idle markers and conversation, weighted by type), unless decision Q6
  takes it in.

---

## 5. Requirements by area

### 5.1 Find object types (`AIP-FIND`)

What a Find does on arrival comes from the editor wikis. The GECK: an
inventory object is picked up, an activator activated, a creature attacked,
a hostile NPC attacked, a non-hostile NPC talked to
([GECK: Find Package](https://geck.uesp.net/wiki/Find_Package)). The CS adds
chair (sit in it), container (open it) and door (open it, go through a load
door) ([CS: Find Package](https://cs.uesp.net/wiki/Find_Package)).

The FO3/FNV object-type names come from fopdoc, which lists 0–27, 28 = "??"
and 29 = Actors: Any. xEdit does not agree with itself at the top of the
enum: `wbDefinitionsFO3.pas` matches fopdoc (sparse `29, 'Actors: Any'`),
while `wbDefinitionsFNV.pas` places Actors: Any at 28 (a commented-out `''`
entry removes the 28 slot). Oblivion's enum is in xEdit
`wbDefinitionsTES4.pas` (`PTDT` "Object type", 0–35, with Apparatus 2 and
Soul Gems 17 shifting every later value), which agrees with the comment in
`find_behavior`.

Vanilla `PTDT` type-2 targets in Find packages (uncommitted census, §3):

| Object type (fopdoc) | FO3 | FNV | Requirement |
|---|---:|---:|---|
| 0 None | 1 | 3 | AIP-FIND-005 |
| 5 Containers | 0 | 2 | AIP-FIND-005 |
| 10 Flora | 0 | 7 | AIP-FIND-002 |
| 11 Furniture | 17 | 16 | AIP-FIND-001 |
| 12 Weapons: Any | 3 | 2 | AIP-FIND-004 |
| 13 Ammo, 17 Alchemy, 18 Food | 5 | 14 | runs today |
| 14 NPCs | 4 | 4 | AIP-FIND-003 |
| 28 (fopdoc "??", xEdit FNV Actors: Any) | 0 | 1 | AIP-FIND-003 / -005 |

- **AIP-FIND-001.** A FO3/FNV Find package whose target is object type 11
  (Furniture) MUST leave the actor `Seated` on a sit marker of the nearest
  furniture reference inside the `PLDT` area, through the existing seat path
  (`seat_at_marker`). Today it is declined (`find_behavior` accepts 13, 17
  and 18 only). **Phase 0: verify** that FO3/FNV sit in a found chair. The
  GECK Find page has no furniture case; the CS page's "Chair: Sit in it" is
  the Oblivion statement. What settles it: a vanilla FO3/FNV type-11 Find
  package observed live, or an editor-documented furniture case.
- **AIP-FIND-002.** A FO3/FNV Find package whose target is object type 10
  (Flora) MUST raise one `ActivateEvent` on the nearest flora reference in
  the area, with the actor as activator. Today it is declined.
- **AIP-FIND-003.** A FO3/FNV Find package whose target is object type 14
  (NPCs), 15 (Creatures) or Actors: Any MUST select the nearest loaded live
  actor of that class in the area, then apply the GECK arrival rule for
  that actor (attack, or greet). Today it is declined. **Phase 0: verify**
  whether Actors: Any is 28 or 29 on each game. What settles it: the GECK
  Find "Object Type" dropdown order, or `VFSFreesideThugPackage` (FNV's one
  type-28 Find) observed live.
- **AIP-FIND-004.** A FO3/FNV Find package whose target is an item class
  (2 Armor, 3 Books, 4 Clothing, 7 Ingredients, 9 Misc, 12 Weapons: Any,
  16 Keys, 19 All: Combat Wearable, 20 All: Wearable) MUST pick up the
  nearest matching placed item through `inventory::take_placed_item`, once
  the item records can classify that class. Each class SHOULD be enabled
  as soon as its classifier lands.
- **AIP-FIND-005.** A Find package whose object type AIP-FIND-001…004 do
  not cover (among them 0 None, 1 Activators, 5 Containers, 6 Doors,
  8 Lights, 24–27 Actor Effects, whichever of 28 / 29 is not Actors: Any,
  and any number outside the enum) MUST produce a decline line with reason
  `unsupported-object-type` and the raw number.
- **AIP-FIND-006.** Oblivion Find packages with an object-type target MUST
  be classified through Oblivion's own numbering (xEdit
  `wbDefinitionsTES4.pas`), translated into one canonical object class
  inside `crates/plugin`, so that `find_package_system` never reads the
  game. Today every Oblivion object-type Find is declined by a game check
  in the driver (`find_behavior`, the `Fallout3NV` gate,
  `ai_package.rs:152-157`), which this requirement removes.
- **AIP-FIND-007.** An Oblivion Find whose target is a chair MUST seat the
  actor, and one whose target is a container MUST open it, per the CS
  table. Seating on Oblivion depends on AIP-SIT-001.

### 5.2 UseWeapon (`AIP-UW`)

The source is [GECK: Use Weapon Package](https://geck.uesp.net/wiki/Use_Weapon_Package).
The `PKW3` layout is from fopdoc. `PKW3`, `PTD2` and `PLD2` are decoded
today (`PackUseWeapon`, `crates/plugin/src/esm/records/misc/pack.rs`).

Vanilla UseWeapon packages with a weapon class (`PTDT` type 2; uncommitted
census, §3). The fopdoc / xEdit names for 21–23 are contradicted by the
EditorIDs of every vanilla package that authors them; the order Oblivion
uses (xEdit TES4: 23 None, 24 Melee, 25 Ranged), shifted down by two, reads
21 None, 22 Melee, 23 Ranged and fits them, except that
`CitInitiatePracticeMelee*` reads melee rather than unarmed. That fit is evidence, not a source, so the classes stay a
verify item (AIP-UW-009, decision Q1).

| Value | fopdoc / xEdit name | FO3 | FNV | Vanilla EditorIDs |
|---:|---|---:|---:|---|
| 12 | Weapons: Any | 2 | 0 | `MS08HarknessConfrontZimmerShoot`, `MQ01DefendCheckpoint` |
| 21 | Weapons: Ranged | 2 | 1 | `CitInitiatePracticeMeleeA0x24` / `B0x24`, `LittleBusterUnarmedPractice8x4` |
| 22 | Weapons: Melee | 0 | 6 | `VRRCInitiation1PACKAGE`…`4`, `FortLegionarySparringPackage`, `VHDLegionLegateCampAttackTargetDummyPackage` |
| 23 | Weapons: NONE | 19 | 13 | `MisfitsShootGood` / `Bad`, `VOVNVAliceHostetlerShootsBecky`, `TenpennyGuardTargetPracticePackage1`, `BobPRacticeRifle18x4` |

- **AIP-UW-001.** A UseWeapon actor whose package authors a `PLDT` location
  MUST be inside that location's radius before its volley is raised
  (`UseWeaponPackageStarted`). Today it fires from where it stands
  (`systems/use_weapon_package.rs`, "v0 scope").
- **AIP-UW-002.** When the package authors a `PLD2` target location, the
  actor MUST hold fire until its target is inside that location (GECK
  *Target Location*). Today `PLD2` is ignored for UseWeapon.
- **AIP-UW-003.** With Hold Fire When Blocked set, an actor MUST NOT fire
  while a movable object blocks the line of fire to its target (GECK), as
  tested by a physics ray query.
- **AIP-UW-004.** With Always Hit set, every shot MUST resolve as a hit on
  the package target, whatever it strikes first.
- **AIP-UW-005.** Between volleys, the wait MUST come from the `PKW3` pause
  range, not from the weapon's attack interval (today). The midpoint of the
  range is the deterministic stand-in (§6).
- **AIP-UW-006.** Crouch To Reload SHOULD put the actor in its sneak
  stance for the length of each reload. **Phase 0: verify** that the
  game's archives have a crouched reload pose to play. If none exists, the
  flag is reported as `unsourced-semantics` (AIP-NG-006).
- **AIP-UW-007.** A UseWeapon package with no `PTD2` MUST fire along the
  actor's facing direction ("fire weapon in the air in the direction they
  are facing", GECK). Today it is declined.
- **AIP-UW-008.** A UseWeapon package whose `PTDT` is object type 12
  (Weapons: Any) MUST use the actor's equipped weapon, or else its first
  inventory weapon. Today every type-2 weapon is declined
  (`use_weapon_behavior`). Values 21–23 follow AIP-UW-009.
- **AIP-UW-009.** Object types 21, 22 and 23 MUST stay declined with reason
  `unsourced-semantics` until Phase 0 settles their labels, then run by
  AIP-UW-008's rule restricted to the settled class (decision Q1).
  **Phase 0: verify** the 21–23 labels on FO3 and FNV. What settles it: the
  GECK Use Weapon "Weapon type" dropdown order, or one 21, one 22 and one 23
  package from the table above observed in the original game.

### 5.3 CastMagic and UseMagic (`AIP-CAST`)

- **AIP-CAST-001.** An Oblivion actor whose winning package is CastMagic
  (procedure 11) MUST cast the `PTDT` spell through
  `byroredux_scripting::magic_effects::cast_spell` (`magic_effects.rs:227`),
  observed as an `ActiveMagicEffects` entry on the target. The target is
  the `PLDT` reference when the spell is a touch or target spell ("only
  valid if casting a touch or target spell",
  [CS: Cast Magic Package](https://cs.uesp.net/wiki/Cast_Magic_Package)).
  **Phase 0: verify** what a self-range spell, or a touch spell with a
  non-reference location, targets: the CS gives no target, and
  `cast_spell` needs one. Today there is no runtime (§3.1).
- **AIP-CAST-002.** When a CastMagic target names a spell by object type,
  the actor MUST pick only from its own spell list. A specific spell MUST be
  cast even when the actor's list lacks it (CS: Cast Magic Package).
- **AIP-CAST-003.** A Skyrim+ `UseMagic` leaf SHOULD install the same
  runtime, with its `CastTimeMin/Max` and `CoolDownTimeMin/Max` inputs
  bounding the cadence and `HoldWhenBlocked` withholding the cast while "an
  actor other than the target is in the way"
  ([CK: UseMagic](https://ck.uesp.net/wiki/UseMagic_(Procedure))). The CK's
  blocker is an actor, not AIP-UW-003's movable object.
- **AIP-CAST-004.** On FO3 and FNV, procedure 11 MUST stay class N. No
  runtime is built for it, because no vanilla package authors it (§3.1).
  A modded package that uses it produces a decline line with reason
  `unsourced-semantics`, because fopdoc lists 11 as "??".

### 5.4 Furniture markers and seating (`AIP-SEAT`)

Skyrim+ `BSFurnitureMarker` / `BSFurnitureMarkerNode` positions carry an
`AnimationType`; nif.xml (`docs/legacy/nif.xml:2221-2226`) documents 1 Sit,
2 Sleep and "4 = Lean … Used for lean animations?". Round 1's mesh census
(uncommitted, §3) read every position in the vanilla mesh archives:

| Archive | NIFs with markers | `AnimationType` 0 | 1 | 2 | 3 | 4 |
|---|---:|---:|---:|---:|---:|---:|
| `Skyrim - Meshes0.bsa` | 197 | 0 | 208 | 49 | 0 | 2 (`railleanmarker.nif`, `wallleanmarker.nif`) |
| `Skyrim - Meshes1.bsa` | 44 | 0 | 72 | 10 | 0 | 0 |
| `Fallout4 - Meshes.ba2` | 19 | 71 | 0 | 0 | 0 | 0 |

- **AIP-SEAT-001.** The Skyrim+ `AnimationType` value 4 MUST resolve to
  `FurnitureMarkerKind::Lean`. Today the code maps 3 to `Lean` and 4 to the
  `Sit` default (`furniture_component`,
  `byroredux/src/cell_loader/references/attach.rs:65`, mirrored by the test
  helper `systems/sandbox.rs:590` and the doc comments in `furniture.rs:65`
  and `crates/nif/src/import/mod.rs:372`), so Skyrim's two lean markers read
  as seats. The fault is latent while Skyrim has no sit-enter clip
  (`seat_at_marker` and the Sandbox seat step return early without
  `SandboxSitClip`) and becomes live with AIP-SIT-002. **Phase 0: verify**
  where FO4 authors furniture positions: only 19 FO4 NIFs carry a marker
  block, all with value 0.
- **AIP-SEAT-002.** Legacy (Oblivion / FO3 / FNV) markers MUST resolve to a
  marker kind from the `furnituremarkerXX` they refer to, through a table
  checked in from a census. Today refs 1–10 resolve to Sleep and every
  other ref to Sit (`imported_furniture_marker`, `crates/nif/src/import/mod.rs`).
  The CS documents only "1 - 10 for beds, and 11 - 20 for chairs"
  ([CS: GetFurnitureMarkerID](https://cs.uesp.net/wiki/GetFurnitureMarkerID)).
  **Phase 0: verify** what the refs above 20 are (lean, work and
  floor-sit markers are candidates). The census of vanilla
  `furnituremarkerXX.nif` files and the furniture that uses them settles it.
- **AIP-SEAT-003.** Sandbox, Eat and UseItemAt MUST NOT seat an actor on a
  `Lean` or `Sleep` marker. Today this holds by kind (`is_sit_marker`
  accepts `Sit` only, `systems/sandbox.rs:76`); it is breached only through
  a misclassified marker (AIP-SEAT-001, AIP-SEAT-002).
- **AIP-SEAT-004.** A Sleep package with no bed marker in range MUST behave
  as decision Q3 sets. Today it seats the actor on the nearest sit marker
  and logs at debug (#5390, `seat_at_marker`, `systems/eat_sleep.rs`).
- **AIP-SEAT-005.** A creature MUST NOT be posed with a clip authored for
  another skeleton. Today this holds: creatures never take furniture
  (#5495, test `a_creature_actor_never_takes_furniture`, gate
  `docs/smoke-tests/m42-package-procedures.sh`). A creature MAY take a
  marker once a sit clip for its own skeleton is shown to exist.
  **Phase 0: verify** whether any vanilla creature directory ships a
  furniture-enter clip.
- **AIP-SEAT-006.** A UseItemAt package whose `PTDT` names a
  non-furniture item MAY play an authored idle for that item, if one
  resolves from the package's idle collection. Otherwise the actor stands
  at the location (today).

### 5.5 Sit-enter beyond FO3/FNV (`AIP-SIT`)

- **AIP-SIT-001.** Oblivion seating MUST hold a sit-enter clip from
  `Oblivion - Meshes.bsa`, with a verified path. Today
  `sandbox_sit_enter_kf_path(Oblivion)` returns `None`, marked "deferred".
  **Phase 0: verify** which Oblivion KF lowers the accumulation root onto
  the seat, by a BSA scan of `meshes\characters\_male\` idle animations.
- **AIP-SIT-002.** Skyrim SE seating MUST produce a seated pose from the
  game's own animation data: a furniture-enter HKX clip, or the behaviour
  graph's furniture state. Today Skyrim has no sit-enter source, so a
  Skyrim sandboxer stands at its location. **Phase 0: verify** which of the
  two Skyrim ships for chair entry.
- **AIP-SIT-003.** FO4 and Starfield seating SHOULD drive the actor's
  behaviour graph into its furniture state, through the same path
  `graph_locomotion_system` uses for walking, rather than through a parked
  clip.
- **AIP-SIT-004.** Seating MUST resolve its clip through one
  per-game lookup at the asset boundary (today `anim_clips.rs`). Seat
  systems MUST NOT read the game kind.

### 5.6 Schedules (`AIP-SCHED`)

`PSDT` fields authored by the vanilla masters (uncommitted census, §3):

| Master | `PSDT` | Day-of-week constrained | Month byte |
|---|---:|---:|---|
| `FalloutNV.esm` | 4 163 | 53 | −1 on all |
| `Skyrim.esm` | 5 961 | 53 | −1 on all |
| `Fallout4.esm` | 3 828 | 4 | −1 on 1 956 (form version < 122), 0 on 1 872 (form version ≥ 122) |

- **AIP-SCHED-001.** On Skyrim, FO4 and Starfield, the `PSDT` day of week
  MUST be tested against the game calendar once decision Q2 fixes the
  weekday of a new game's first day. Today `package_weekday` returns `None`
  outside Oblivion, FO3 and FNV (`crates/scripting/src/globals.rs:145-152`),
  so day-constrained packages run every day.
- **AIP-SCHED-002.** The `PSDT` month byte MUST be decoded into
  `PackSchedule` as one canonical value (a month 0–11, a season 12–15, or
  "any"), with the record-version offset applied inside `crates/plugin`:
  xEdit stores month + 1 on records of form version ≥ 122, so a stored 0
  there is "any" (`wbPackagePSDTMonthValueToStr` /
  `wbPackageScheduleMonthEnum`, `Core/wbDefinitionsCommon.pas`). The census
  agrees: every FO4 month-0 record is version ≥ 122. Today `PackSchedule`
  has no month field (`pack.rs:545-561`). No vanilla master authors a
  month constraint, so the observable is a unit test on a synthetic record.

- **AIP-SCHED-003.** A zero-duration window (#3352) MUST stay unselectable
  until its meaning is sourced, and each such package MUST produce a decline
  line with reason `unsourced-semantics`. The GECK says only that "the
  schedule cannot be in less than one hour blocks"
  ([GECK: Packages](https://geck.uesp.net/wiki/Category:Packages)). That
  does not say what a 0 means. **Phase 0: verify** with one of the vanilla
  Travel packages `PackSchedule::active_at` names, observed in the
  original game.
- **AIP-SCHED-004.** The weekday origin MUST be one value resolved at load
  and held on the calendar resource; `package_weekday` MUST NOT gain a
  per-game arm for it (R5, the translation boundary).

### 5.7 Package flags (`AIP-FLG`)

The flag names and meanings are from the GECK Packages page and UESP's
Oblivion PACK page. Today the ambient driver reads none of the `PKDT`
flags. Only the script-package slot reads Must Complete and Must Reach
Location (`crates/scripting/src/script_packages.rs:39-68`). This area
applies if decision Q4 takes it.

- **AIP-FLG-001.** An ambient package with Once Per Day set MUST NOT win
  selection again within 24 game hours of the moment it was first selected
  ("a 24 hour block beginning when the package was first selected", GECK).
- **AIP-FLG-002.** An ambient package with Must Complete set MUST keep
  winning, even after its schedule window closes, until its procedure
  reports done: Travel arrived, Find count met, UseWeapon bursts fired.
- **AIP-FLG-003.** An ambient package with Continue if PC near set MUST
  keep winning past its window while the player is in the actor's cell
  (GECK: "will not terminate … so long as the PC is nearby. Primarily used
  to keep shops open"). **Phase 0: verify** the radius "nearby" means. The
  GECK gives none; the cell is this PRD's stand-in.

### 5.8 Skyrim+ ambient tree (`AIP-TREE`)

- **AIP-TREE-001.** The ambient driver MUST choose a tree-shape package's
  leaf by evaluating each candidate leaf's own conditions, in tree order.
  Today `from_skyrim_procedure_tree` takes the first leaf named `Sandbox`,
  then the first named `Patrol`, without evaluating conditions
  ([PACKAL §5](../engine/packal.md#5-second-slice-skyrim-ambient-patrol-shipped-2026-08-19)).
- **AIP-TREE-002.** A `Travel` leaf MUST install `TravelBehavior`, with
  its destination resolved through the Scene driver's `input_destination`.
  Today that function is private to `crates/scripting/src/package.rs:403`;
  this requirement makes it shared rather than duplicated.
- **AIP-TREE-003.** `Sleep` and `Eat` leaves MUST install `SleepBehavior` and
  `EatBehavior`, with the location from the leaf's `Location` input.
- **AIP-TREE-004.** `Sit` and `UseIdleMarker` leaves SHOULD install the
  seat runtime. Seating needs AIP-SIT-002 / 003.
- **AIP-TREE-005.** A `ForceGreet` leaf on an actor's own package list MUST
  install the Dialogue behaviour under the #5376 rule (fully modelled
  conditions only). Today it is deferred (the Dialogue arm of `from_package`).
- **AIP-TREE-006.** `Follow`, `Escort`, `Guard`, `Wander` and `Flee` leaves
  SHOULD install their FO3/FNV counterparts' components. Wander installs
  only when it is the root leaf (PACKAL §5 found every vanilla Wander
  nested inside another template).
- **AIP-TREE-007.** Any other leaf MUST produce a decline line with reason
  `no-runtime` and the leaf name.
- **AIP-TREE-008.** FO4 and Starfield tree packages MUST go through the same
  ambient driver code path as Skyrim. A per-game difference in leaf names or
  input layout MUST be resolved at the `PackRecord` decode, never in the
  driver.

### 5.9 Walk sources and pathing (`AIP-WALK`)

- **AIP-WALK-001.** A Starfield actor moved by a package MUST animate from
  Starfield's own archive data. Today the graph path cannot reach it:
  `populate_archetype_idles` returns early outside Fallout 4
  (`asset_provider/archetype_idle.rs:94`), and `archetype_idle_system`
  queues an actor on `PendingActorGraphs` only after an idle clip resolved
  (`systems/archetype_idle.rs:142-162`). **Phase 0: verify** whether
  `Starfield.esm` `RACE` records carry `MT` subgraph entries that the
  `RACE` decode reads (`subgraph_data`, `race.rs:187-193`, documented
  "FO4+"), and whether the graphs they name exist in Starfield's archives.
  If they do, the Fallout 4 gate becomes a data test, not a second game
  check.
- **AIP-WALK-002.** On FO4, a package's walk legs MUST route over `NAVM`
  once the `NVNM` body decodes. The decode is owned by
  [NAVM Pathfinding §9](../engine/navmesh-pathfinding.md#9-rollout) item 5.
  This PRD depends on it and does not build it.
- **AIP-WALK-003.** An Oblivion actor's walk legs MUST follow decision Q5.
  Today they are straight lines (§3.3).
- **AIP-WALK-004.** The walk-source lookup MUST stay at the asset boundary
  (`anim_clips.rs`, `asset_provider/`). The walk-animation systems
  (`npc_walk_animation_system`, `graph_locomotion_system`) MUST NOT read the
  game kind.

### 5.10 Explainability and measurement (`AIP-DX`)

Today there is no `pack.ambient` command
([PACKAL §8](../engine/packal.md#8-tooling) proposes it), and the runtime
components (`AmbientPackageRuntime`, `WalkAnimation`, `NavPath`) are not in
the debug registry (`crates/debug-server/src/registration.rs`).

- **AIP-DX-001.** `pack.ambient <entity>` MUST print the actor's package
  list in priority order, one row per package with: the procedure (byte or
  leaf name), whether its schedule is open, whether its conditions passed
  and whether they were fully modelled, and the install or decline result
  with its reason.
- **AIP-DX-002.** Every decline MUST log one debug line with the package
  FormID, EditorID, procedure and reason. Today only the Dialogue arm logs a
  decline reason (`from_package`, `#5376` line, `ai_package.rs:475`).
  Re-selection logs only the winner (`[m42-reselect]`, `ai_package.rs:1271`).
- **AIP-DX-003.** A census tool under `crates/plugin/examples/` MUST print,
  per master, the counts in §3.1 and §3.2, the per-object-type counts
  behind AIP-FIND and AIP-UW, and the `PSDT` field counts of §5.6. The
  furniture-marker counts of §5.4 MUST come from the same tool or a sibling
  example reading the mesh archives. Run over the vanilla data, they
  regenerate those tables.
- **AIP-DX-004.** The real-data test
  `real_skyrim_esm_ambient_packages_now_resolve_for_previously_blind_npcs`
  MUST report install and decline counts per leaf. An equivalent ignored
  test MUST exist for `FalloutNV.esm`, `Fallout3.esm`, `Oblivion.esm` and
  `Fallout4.esm`.
- **AIP-DX-005.** `pack.ambient <entity>` MUST also print the actor's walk
  source and its cached waypoint count.
- **AIP-DX-006.** `pack.ambient` with no argument MUST list every actor that
  has an `AmbientPackageRuntime`, with its winning package or its decline
  reason.

---

## 6. Deliberate deviations and risks

No deviation below makes an existing mechanism redundant; the column is
kept for the format.

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Sandbox scores every nearby object (eat, sleep, furniture, idle markers, dialogue), with type weights that drop to zero after use and recover ([GECK: Sandbox Package](https://geck.uesp.net/wiki/Sandbox_Package)) | The nearest free sit marker (`pick_nearest_seat`) | — | Cells look less varied. Decision Q6, AIP-NG-007 |
| 2 | Packages "seem to be re-evaluated every 10 seconds" ([GECK: How Are Packages Evaluated](https://geck.uesp.net/wiki/How_Are_Packages_Evaluated)) | Once per in-game minute, and on `EvaluatePackage` (`ambient_ai_package_system`) | — | At the default time scale this is faster than the GECK's own hedged figure. None |
| 3 | Unevaluable conditions do not exist in the original | Lists with an uncatalogued function pass (fail-open), except for procedures that take control of the actor (#5376 rule) | — | A fail-open package can win when the original would skip it. AIP-DX-001 shows when that happened |
| 4 | Accompany paths to the target's destination ([GECK: Accompany Package](https://geck.uesp.net/wiki/Accompany_Package)) | Follow's trailing runtime | — | The looser formation is not reproduced. §3.1 class A |
| 5 | Volley size and pause are random in [min, max] (GECK: Use Weapon Package) | The midpoint (AIP-UW-005) | — | Volleys are identical every time; tests and saves are deterministic |
| 6 | Eat consumes food ([GECK: Eat Package](https://geck.uesp.net/wiki/Eat_Package)) | Walk and sit only | — | Food stocks never deplete. Not addressed in this PRD |
| 7 | Sleep is in a bed ([GECK: Sleep Package](https://geck.uesp.net/wiki/Sleep_Package)) | A sit-marker fallback when no bed is in range (#5390) | — | Actors sit in chairs all night. AIP-SEAT-004, decision Q3 |
| 8 | A zero-length schedule window's behaviour is not documented | Never selected (#3352) | — | The FNV packages `PackSchedule::active_at` lists never run. AIP-SCHED-003 |
| 9 | **Phase 0: verify** whether creatures use furniture (AIP-SEAT-005) | Never seated (#5495) | — | A creature furniture use, if one exists, is lost. AIP-SEAT-005 |
| 10 | A navmesh "tells an actor where it can walk" ([GECK: Bethsoft Tutorial Navmesh](https://geck.uesp.net/wiki/Bethsoft_Tutorial_Navmesh)) | Cross-tile joins by exact vertex match; the rest is straight-line ([NAVM Pathfinding §9](../engine/navmesh-pathfinding.md#9-rollout)) | — | Some walks hit walls across unjoined tiles. Owned by the navmesh design |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail).
FNV leads as the engine's reference title.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 3. Tier 3. Same flat shape, so the ambient driver already runs it. Its remainder is CastMagic, its own object-type enum, sit-enter and pathing (Q5) |
| Fallout 3 | yes | Wave 1. Shares FNV's procedures and runtime |
| Fallout: New Vegas | yes | Wave 1. Reference title. Has the live gates `m42-package-procedures.sh` and `m42-eat-sleep.sh` |
| Skyrim (LE / SE / AE) | yes | Wave 2. Tree shape (AIP-TREE), sit (AIP-SIT-002), weekday (AIP-SCHED-001). LE shares SE's records and is counted with it |
| Fallout 4 | yes | Wave 2. The same tree driver. Walking already works through the behaviour graph. Pathing waits on the `NVNM` decode (AIP-WALK-002) |
| Fallout 76 | no | Tier 3, parse only: no cell loads with actors to run packages on ([game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail)). Revisit when FO76 cells load |
| Starfield | yes | Wave 3. Tier 2 walkable interior. Needs a walk source (AIP-WALK-001) and the tree driver |

### 7.2 Acceptance

Game data comes from local installs only (`BYROREDUX_FNV_DATA` and the
other per-game variables that the smoke scripts read). Nothing is checked
in, and missing data is `SKIP` (exit 77), never a pass.

| Scenario | Driven by | Data | Passes when |
|---|---|---|---|
| Existing FNV gates | `docs/smoke-tests/m42-package-procedures.sh`, `docs/smoke-tests/m42-eat-sleep.sh` | FNV | Both still pass after every phase |
| Find furniture / flora / actors | new `cargo test -p byroredux --bin byroredux` cases in `systems::find_package` (`a_furniture_find_seats_the_actor`, `a_flora_find_activates_it`, `an_actor_class_find_picks_the_nearest_actor`) | none (fixtures) | Each asserts `Seated`, an `ActivateEvent`, or the chosen target |
| UseWeapon place and target location | new tests in `systems::use_weapon_package` (`the_volley_waits_for_the_place_to_stand`, `the_volley_waits_for_the_target_location`, `a_blocked_shot_holds_fire`) | none | No `UseWeaponPackageStarted` before arrival or entry, and no shot while blocked |
| UseWeapon live | `m42-package-procedures.sh` gains a leg: a placed package with a `PLDT` (chosen in Phase 0 by the census) | FNV | `byro-dbg` `entities(UseWeaponPackageStarted)` lists the actor only after it is inside the location radius |
| Lean marker | new test beside `resolves_kind_from_animation_type_at_the_boundary` (`attach.rs`) | none | `AnimationType` 4 resolves to `Lean`, 3 to the `Sit` default |
| Decline reporting | `--bench-hold` on FNV `ULKitchen`, then `byro-dbg` `pack.ambient` with no argument | FNV | Every actor prints a winner or a decline reason; none prints an empty result |
| Census | `cargo run --release -p byroredux-plugin --example <census tool> -- <ESM>` (AIP-DX-003) | Oblivion, FO3, FNV, Skyrim SE, FO4, Starfield masters | Every procedure with authored packages maps to a §3 class; no "unknown" row |
| Skyrim tree | `real_skyrim_esm_ambient_packages_now_resolve_for_previously_blind_npcs` (`--ignored`), extended by AIP-DX-004 | Skyrim SE | Reports per-leaf counts; every PKID-referenced package installs or declines with a reason |
| Skyrim sit live | new script `docs/smoke-tests/m42-skyrim-ambient.sh`: `--bench-hold` into `WhiterunBanneredMare` at a sandbox hour (`time.set`) | Skyrim SE | `entities(Seated)` is non-empty |
| Oblivion live | new script `docs/smoke-tests/m42-oblivion-packages.sh`: a cell with a placed CastMagic actor (chosen in Phase 0 from the five vanilla packages) | Oblivion | The target gains an `ActiveMagicEffects` entry; a seated sandboxer appears after AIP-SIT-001 |
| Schedules | new tests in `crates/plugin` / `npc_spawn::ai_package` (`a_skyrim_weekday_package_selects_on_its_day`, `psdt_month_gates_the_schedule`, `a_v122_month_zero_is_any`) | none | A day- or month-constrained package wins only on its day or month; a stored 0 on a version-122 record is "any" |

---

## 8. Metrics

The numbers live in ROADMAP's [M42 row](../../ROADMAP.md#tier-7--deep-gameplay-systems)
(one home). This section defines what is counted.

| Metric | Definition | Granularity |
|---|---|---|
| Procedure coverage | Procedures of class R or A ÷ procedures with at least one PKID-referenced vanilla package | per game |
| Install rate | PKID-referenced packages that install ÷ PKID-referenced packages, from the AIP-DX-004 tests | per game |
| Decline breakdown | Declined packages per reason ÷ PKID-referenced packages | per game, per reason |
| Seat rate | Actors whose winning package is a seat procedure and who have a free marker in range that end up `Seated` ÷ all such actors, in the acceptance cells | per acceptance cell |

---

## 9. Delivery sequence

The phases go in order of what they unblock. Wave 1 finishes FO3/FNV
first because they have live gates and the most decoded data. Every phase
depends on M42's shipped runtime and the M47.1 evaluator.

**Phase 0 — Census, sources and decisions (no engine code).** Check in the
census tool (AIP-DX-003; tooling under `examples/`, not engine code) and
regenerate §3.1–§3.3, §5.1, §5.2, §5.4 and §5.6 from it. Close every **Phase 0: verify**
item: FO3/FNV furniture Find, Actors: Any at 28 or 29,
the 21–23 weapon classes, Crouch To Reload, the CastMagic self-range
target, FO4 furniture positions, legacy refs above 20, creature furniture
clips, Oblivion and Skyrim sit sources, the zero-duration window, the
Continue if PC near radius, the FO4 / FO76 / Starfield leaf names, and the
Starfield subgraph data. Resolve Q1–Q6, and write the Oblivion `PGRD`
section if Q5 takes it. Exit: the tool's output for the six masters is
checked into [NPC Spawn → AI Packages](../engine/npc-spawn-ai-packages.md);
every verify item is closed or filed as an issue; every §10 item carries
**Decided**.

**Phase 1 — FO3/FNV completion (wave 1).** AIP-FIND-001…005, AIP-UW-001…009,
AIP-SEAT-001…004, AIP-FLG-* (per Q4), AIP-DX-001, AIP-DX-002, AIP-DX-005,
AIP-DX-006, and the FNV, FO3 and Oblivion tests of AIP-DX-004. Exit: on FO3
and FNV, procedure coverage is 100% of authored procedures, every P row's
remaining declines carry `unsourced-semantics`, and the §7.2 FNV and
Lean-marker scenarios pass.

**Phase 2 — Schedules.** AIP-SCHED-001…004 (Skyrim+ weekday per Q2, month
decode). Exit: the schedule tests pass, and the census shows zero
day- or month-constrained packages left unevaluated on any game whose
calendar is settled.

**Phase 3 — Skyrim+ ambient tree (wave 2).** AIP-TREE-001…008, AIP-SIT-002…004,
AIP-CAST-003, and the FO4 test of AIP-DX-004. It depends on PACKAL. Exit:
the Skyrim and FO4 real-data tests report an install rate with no package
outside a §3 class, and `m42-skyrim-ambient.sh` passes.

**Phase 4 — Oblivion (wave 3).** AIP-FIND-006…007, AIP-CAST-001…002,
AIP-SIT-001, AIP-WALK-003 (per Q5). Exit: `m42-oblivion-packages.sh` passes,
and Oblivion's procedure coverage is published.

**Phase 5 — Starfield and FO4 locomotion.** AIP-WALK-001, AIP-WALK-002,
AIP-WALK-004. AIP-WALK-002 depends on the FO4 `NVNM` body decode
([NAVM Pathfinding §9](../engine/navmesh-pathfinding.md#9-rollout) item 5).
Exit: a Starfield Cydonia actor moved by a package shows a walk source
(`pack.ambient <id>` through `byro-dbg` names one while the actor moves),
and an FO4 interior Travel walk is routed (`pack.ambient` reports a
non-zero waypoint count).

---

## 10. Decisions required

Each decision needs the project owner. The recommendation is the first
option.

1. **Q1 — UseWeapon weapon classes.** The ROADMAP M42 row and
   `use_weapon_behavior` already decline the classes because "the
   object-type enum numbering contradicts the vanilla data"; this decision
   is whether, and how far, to reopen that. Recommended: run 12 (Weapons:
   Any) now (AIP-UW-008), whose name fits its vanilla packages, and keep
   21–23 declined until Phase 0 settles their labels (AIP-UW-009).
   Alternative A: adopt the Oblivion-order reading (21 None, 22 Melee,
   23 Ranged), which fits the vanilla FO3/FNV EditorIDs in the §5.2 table (one name,
   `CitInitiatePracticeMelee*`, reads melee at 21).
   The 32 type-23 and 9 type-21/22 packages then run, on a mapping no
   editor document states. Alternative B: run 21–23 by the fopdoc / xEdit
   names. The three type-21 packages, named for melee and unarmed practice,
   would draw ranged weapons, and the 32 shooting packages stay declined as "NONE".
   Alternative C: keep every class declined (today). The 2 FO3 type-12
   packages keep doing nothing.
2. **Q2 — Skyrim+ weekday epoch.** UESP disagrees with itself.
   *Skyrim: Statistics* says the game "starts on Morndas, the 17th of Last
   Seed" (local `uesp-wiki/Skyrim/Statistics.wiki:30`). *Skyrim: Unbound*
   says the date is fixed but the weekday "will be Sundas" when no game was
   loaded first, and otherwise whatever it was before
   (`uesp-wiki/Skyrim/Unbound.wiki:184-186`). Recommended: Sundas, which is
   Unbound's fresh-start case and the only one a new ByroRedux game can
   reproduce. For FO4 and Starfield, recommended: the Gregorian weekday of
   the authored `GameYear` / `GameMonth` / `GameDay` start date, the rule
   FO3/FNV use today (`day_of_week`, `globals.rs:121-139`), which has no
   source either; the decision adopts it explicitly. Alternative: Morndas
   for Skyrim (Statistics). Alternative: leave Skyrim+ days unevaluated
   (today). The 53 Skyrim and 4 FO4 day-constrained packages then run every
   day.
3. **Q3 — Sleep with no bed in range.** Recommended: no sit fallback. The
   actor idles at the Sleep location, because the GECK defines Sleep as
   "find a bed and sleep in it" and, with a radius, "pick a valid bed within
   the radius". Alternative: keep the #5390 sit fallback. Sleepers stay
   visibly seated in chairs through their sleep window.
4. **Q4 — Ambient package flags.** Recommended: in scope are the flags that
   change selection: Once Per Day, Must Complete, Continue if PC near
   (AIP-FLG). Out of scope are the door lock and unlock flags and Always
   Run / Always Sneak, until run and sneak gaits exist for NPCs.
   Alternative: all flags in a follow-up PRD. Shopkeepers then still leave
   with the player in the shop, and finished Travel packages re-run.
5. **Q5 — Oblivion pathing.** Oblivion ships no `NAVM` and ships `PGRD`,
   which is parsed today. Recommended: path Oblivion walks over `PGRD`,
   designed as a new section of
   [NAVM Pathfinding](../engine/navmesh-pathfinding.md) and delivered in
   Phase 4. Alternative: straight-line walks on Oblivion (today). Oblivion's
   Travel- and Wander-heavy package set (§3.1) then walks through walls.
6. **Q6 — Sandbox activity scoring.** Recommended: out of this PRD
   (AIP-NG-007), and a follow-up revision once idle markers and food
   consumption have runtimes. Alternative: in scope as a Phase 1 item. That
   makes Phase 1 depend on an idle-marker runtime and on Eat consuming
   food, neither of which exists.

---

## 11. Document map

| Need | Go to |
|---|---|
| Per-procedure runtime detail, FO3/FNV selection and seating | [npc-spawn-ai-packages.md](../engine/npc-spawn-ai-packages.md) |
| What the ambient driver does with tree-shape packages | [packal.md](../engine/packal.md) |
| How walk legs are routed, and the FO4 `NVNM` prerequisite | [navmesh-pathfinding.md](../engine/navmesh-pathfinding.md) |
| Package state across tile eviction | [stream-boundary-state-continuity.md](../engine/stream-boundary-state-continuity.md) |
| Conditions, caster runtime, ObScript package commands | [scripting.md](../engine/scripting.md) |
| Per-game engine tiers | [game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail) |
| The live gates and how to run them | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Live milestone state and the published numbers | [ROADMAP.md](../../ROADMAP.md#tier-7--deep-gameplay-systems) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: census provenance (Phase 0: confirm), FO3/FNV object-type enum and the xEdit FO3/FNV split at 28/29, weapon classes 21–23 and Q1 reworked against the ROADMAP rationale, Skyrim Lean value settled by a mesh census (AIP-SEAT-001 now a fix), FO4 month sentinel settled from xEdit (AIP-SCHED-002), weekday origin at the boundary (new AIP-SCHED-004), Starfield graph gate and `input_destination` stated from HEAD, `pack.ambient` split (AIP-DX-005/006), CastMagic self-range and UseMagic blocker, deviation citations | claude: revise |
