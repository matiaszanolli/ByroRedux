# AI Packages Completion — Product Requirements

**Status**: ACTIVE (2026-11-03). Revision 5 files the tracking issues; every **Phase 0: verify** item is an acceptance criterion of the Phase 0 issue. Revision 4 recorded the owner's §10 decisions: Q2 sets Skyrim's weekday origin per edition, every other answer is the recommendation. Revision 3 folded in review round 2
(§12 lists the topics). It covers the remainder of milestone M42: the
procedures, package fields and per-game prerequisites that do not run yet.
Statements about how the original engines behave cite external sources
(UESP, the CS / GECK / Creation Kit wikis, fopdoc, xEdit, nif.xml). A
claim that could not be verified is marked **Phase 0: verify** instead of
being stated as fact.

**Tracking**: #6634 (parent). Phases: #6655 (0), #6656 (1), #6658 (2), #6660 (3),
#6662 (4), #6663 (5).

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
writes the missing design. A prerequisite that no design owns is filed as
an issue in Phase 0.

| Mechanism | Owning design |
|---|---|
| PACK decode, package selection, the FO3/FNV flat procedures, seating | [NPC Spawn → AI Packages](../engine/npc-spawn-ai-packages.md) |
| Skyrim+ tree/template packages on the ambient driver | [PACKAL](../engine/packal.md) |
| Walk-leg routing over `NAVM` | [NAVM Pathfinding](../engine/navmesh-pathfinding.md) |
| FO4 `NVNM` body decode, the prerequisite of AIP-WALK-002 | none. NAVM Pathfinding names it and disclaims it ([§9](../engine/navmesh-pathfinding.md#9-rollout) item 5: "Not this document's job"), and ROADMAP lists it open. Phase 0 files it as an issue |
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
`Skyrim.esm` and `Fallout4.esm` and got the same numbers. Review round 2
re-ran the `PACK` counts (§3.1, §5.1, §5.2, §5.6) on 2026-11-03 with a
separate uncommitted script and got the same numbers again, and added the
Starfield row and the date column of §5.6. The mesh counts of §5.4 were not
re-run. They are all **Phase 0: confirm** figures, not checked-in facts:
Phase 0 checks the tool in (AIP-DX-003) and regenerates every table from
it.

### 3.1 Flat-shape procedures

Counts are `PACK` records by `PKDT` procedure byte, per master (uncommitted
census, see above). The **Today** column is the state at HEAD on
2026-11-03. The numbering matches across the three games for 0–10.
Oblivion's enum ends at 11 = Cast magic
([UESP: Oblivion PACK](https://en.uesp.net/wiki/Oblivion_Mod:Mod_File_Format/PACK),
local dump `uesp-wiki/Oblivion Mod/Mod File Format/PACK.wiki`). FO3/FNV
continue to 16, and their 11 is "??"
([fopdoc: FNV PACK](https://tes5edit.github.io/fopdoc/FalloutNV/Records/PACK.html)).
xEdit's shared enum agrees: it names 11 Cast Magic for TES4 only and 12–16
for FO3/FNV only (`wbPackageTypeEnum`, `wbDefinitionsCommon.pas:8084-8103`).

| # | Procedure | Obl | FO3 | FNV | Class (Obl / FO3·FNV) | Today |
|---|---|---:|---:|---:|---|---|
| 0 | Find | 741 | 78 | 81 | P / P | `make_find_package_system` (`systems/find_package.rs`). Object-type targets are searched on `Fallout3NV` only, and only for Ammo 13, Alchemy 17 and Food 18. Every other type, and any object type on Oblivion, is declined with no log line (`find_behavior`, `npc_spawn/ai_package.rs:139-174`) |
| 1 | Follow | 208 | 115 | 132 | R / R | `follow_system` |
| 2 | Escort | 75 | 30 | 12 | R / R | `escort_system` |
| 3 | Eat | 829 | 296 | 317 | P / A | `systems/eat_sleep.rs` walks and sits; food is never eaten. On Oblivion the walk runs, but there is no sit clip, so the actor never sits (`sandbox_sit_enter_kf_path` → `None`) |
| 4 | Sleep | 725 | 246 | 390 | P / A | `systems/eat_sleep.rs`. A bed is preferred, with a sit-marker fallback (#5390); either way the sit pose is parked at the marker (module doc). Oblivion: walk, no seat |
| 5 | Wander | 1 820 | 42 | 31 | R / R | `wander_system` |
| 6 | Travel | 1 924 | 907 | 1 260 | R / R | `travel_system` |
| 7 | Accompany | 40 | 24 | 14 | A / A | runs as Follow (`from_package`, `is_accompany` arm) |
| 8 | UseItemAt | 751 | 20 | 26 | A / A | installs `EatBehavior`. Neither the `PTDT` item nor the *Sit Down* flag is read (§6 row 11) |
| 9 | Ambush | 80 | 25 | 38 | A / A | Guard wait plus the hostility gate (`AmbushBehavior`). Oblivion authors no `PLD2`, so its ambushers get no gate and fight by the ordinary hostility rules (`ambush_gates`, `systems/faction_hostility.rs:351-369`) |
| 10 | FleeNotCombat | 11 | 8 | 15 | R / R | `flee_package_system` |
| 11 | CastMagic | 5 | 0 | 0 | D / N | no `PROCEDURE_*` constant. A flat package falls through to `from_skyrim_procedure_tree` (`ai_package.rs:489-490`), which returns `None` on an empty tree |
| 12 | Sandbox | n/a | 681 | 753 | — / P | `systems/sandbox.rs`: nearest free sit marker. Scored activity selection is not built (§6) |
| 13 | Patrol | n/a | 180 | 505 | — / A | Wander's core. The route's linked references (`XLKR`) are decoded (`crates/plugin/src/esm/cell/walkers.rs:742-766`, FO3/FNV since #5223), but Patrol does not follow them; the `PROCEDURE_PATROL` doc's "decodes nowhere" is stale |
| 14 | Guard | n/a | 172 | 175 | — / R | `guard_system` |
| 15 | Dialogue | n/a | 386 | 332 | — / P | force-greet bridge, installed only for modelled player-targeted Conversations (#5376) |
| 16 | UseWeapon | n/a | 56 | 82 | — / P | `use_weapon_package_system`. Installs only a specific weapon (`PTDT` type 1) aimed at a specific reference (`PTD2` type 0) with fully modelled conditions (`use_weapon_behavior`, `ai_package.rs:104-128`) |

### 3.2 Tree-shape leaves (Skyrim / FO4 / FO76 / Starfield)

The Skyrim Creation Kit documents 29 procedure leaves
([CK: Procedures](https://ck.uesp.net/wiki/Category:Procedures), local dump
`ck-uesp-wiki/(main)/* (Procedure).wiki`, 29 files). The state at HEAD on
2026-11-03:

| Leaf group | Ambient driver Today | Scene driver Today |
|---|---|---|
| Sandbox, Patrol | installs (`from_skyrim_procedure_tree`, `ai_package.rs:525-562`: first leaf by name, conditions not evaluated) | Patrol as move-to |
| Travel, Escort, FollowTo | none | move-to (`resolve_command`, `crates/scripting/src/package.rs`) |
| Acquire, Activate, Shout, Sit, UseIdleMarker, UseWeapon | none | interaction (`resolve_command`) |
| ForceGreet | none. Deferred until instance conditions are modelled (comment in `from_package`, Dialogue arm) | none: `resolve_command` returns `AwaitExternal` (`package.rs:550`). `PackRecord::force_greet` resolves both dialects, but only the `dialogue.forcegreet` console command calls it (`commands/quest.rs:1183`) |
| Eat, Sleep, Find, Flee, Follow, Guard, HoldPosition, KeepAnEyeOn, LockDoors, UnlockDoors, Say, UseMagic, Wait, Wander, FlightGrab, Hover, Orbit | none | none |

The FO4, FO76 and Starfield leaf vocabularies are not documented locally
(the FO4 CK dump has only `Range (Procedure)` and `Sandbox (Procedure)`).
**Phase 0: verify** the leaf names each of those masters authors, from the
census tool's per-leaf counts (AIP-DX-003) on `Fallout4.esm`,
`SeventySix.esm` and `Starfield.esm`. The existing
`pack_ambient_shape_survey` example cannot settle it: it prints package
shapes and procedure counts, not leaf names.

### 3.3 Prerequisites by game

The state at HEAD on 2026-11-03:

| Game | Package shape | Walk source Today | Sit-enter Today | Pathing Today | Day-of-week Today |
|---|---|---|---|---|---|
| Oblivion | flat | `handtohandforward.kf` (`humanoid_walk_kf_path`, `npc_spawn/anim_clips.rs`) | none (`sandbox_sit_enter_kf_path`, "deferred") | none: no `NAVM` exists, and `PGRD` is parsed (`crates/plugin/src/esm/records/pathgrid.rs`) but has no consumer in `byroredux/src` | evaluated, plus the date byte (`package_weekday`, `package_day_of_month`, `crates/scripting/src/globals.rs:145-189`) |
| FO3 / FNV | flat | `mtforward.kf` per gender and child | `chairskirt_leftenter.kf` | `NAVM` within and across resident tiles (`path_from_resident_tiles`, `systems/navmesh_path.rs`) | evaluated (`package_weekday`) |
| Skyrim LE / SE | tree | `1hm_walkforward.hkx` (`SkyrimWalkClip`, `components/ai_anim.rs`) | none | `NAVM` (`NVNM` decoded, `decode_nvnm`) | not evaluated (`package_weekday` → `None`) |
| FO4 | tree | behaviour graph `MTBehavior.hkx` (`graph_locomotion_system`, `asset_provider/actor_graph.rs`) | none, and the sit positions authored on `FURN` `SNAM` are not read (AIP-SEAT-007) | none: the `NVNM` body never reconciles and its bytes are retained (`decode_nvnm`, `crates/plugin/src/esm/records/misc/world.rs`) | not evaluated |
| FO76 | tree | none | none | not measured | not evaluated |
| Starfield | tree | none: `humanoid_walk_kf_path` → `None`, `populate_skyrim_walk_clip` is Skyrim only (`asset_provider/animation.rs:243`), and the graph path is Fallout 4 only (`populate_archetype_idles`, `asset_provider/archetype_idle.rs:94`) | none, and `FURN` `SNAM` is not read (AIP-SEAT-007) | not measured: `parse_navm`'s coverage table (`world.rs`) lists no Starfield plugin | not evaluated |

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
  the area, with the actor as activator. Today it is declined. **Phase 0:
  verify** that a Find activates a flora. The GECK Find page has no flora
  case (an inventory object is picked up, an activator activated); the CS
  *Flora* page, which the GECK *Flora* page links to, says that "when
  activated, the harvested ingredient may be added to the actor's
  inventory". What settles it: one of FNV's seven type-10 Find packages
  observed live.
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
  actor, and one whose target is a container MUST raise one
  `ActivateEvent` on it with the actor as activator (CS: the actor "finds
  the target and 'activates' it", a container meaning "Open it"). Seating
  on Oblivion depends on AIP-SIT-001.

### 5.2 UseWeapon (`AIP-UW`)

The source is [GECK: Use Weapon Package](https://geck.uesp.net/wiki/Use_Weapon_Package).
The `PKW3` layout is from fopdoc. `PKW3`, `PTD2` and `PLD2` are decoded
today (`PackUseWeapon`, `crates/plugin/src/esm/records/misc/pack.rs`).

Vanilla UseWeapon packages with a weapon class (`PTDT` type 2; uncommitted
census, §3). The fopdoc / xEdit names for 21 and 23 are contradicted by the
EditorIDs of every vanilla package that authors them: melee and unarmed
practice at 21 "Ranged", gunfire at 23 "NONE". The name for 22 (Melee) fits
its packages: the sparring and target-dummy packages read melee, and the
four `VRRCInitiation` packages name no weapon. The order Oblivion uses
(xEdit TES4: 23 None, 24 Melee, 25 Ranged), shifted down by two, reads
21 None, 22 Melee, 23 Ranged and fits every package except
`CitInitiatePracticeMelee*`, which reads melee rather than unarmed at 21.
That fit is evidence, not a source, so 21 and 23 stay a verify item
(AIP-UW-009, decision Q1).

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
  package still installs and the flag stays unconsumed: that is a §6 row
  added at the Phase 0 exit (AIP-NG-006), not a decline.
- **AIP-UW-007.** A UseWeapon package with no `PTD2` MUST fire along the
  actor's facing direction ("fire weapon in the air in the direction they
  are facing", GECK). Today it is declined.
- **AIP-UW-008.** A UseWeapon package whose `PTDT` is object type 12
  (Weapons: Any) MUST use the actor's equipped weapon, or else its first
  inventory weapon (a Redux choice, §6 row 14). Today every type-2 weapon
  is declined (`use_weapon_behavior`). Values 21–23 follow AIP-UW-009.
- **AIP-UW-009.** Each of object types 21, 22 and 23 MUST stay declined
  with reason `unsourced-semantics` until its label is settled (22 by
  decision Q1, 21 and 23 by Phase 0), and then run by AIP-UW-008's rule
  restricted to that class. A melee class means the `WEAP` animation types
  Melee (1 Hand) and Melee (2 Hand) (xEdit `wbWeaponAnimTypeEnum`,
  `wbDefinitionsFNV.pas:2844-2847`). **Phase 0: verify** the 21 and 23
  labels on FO3 and FNV, and whether Hand to Hand counts as melee. What
  settles it: the GECK Use Weapon "Weapon type" dropdown, or one 21, one 23
  and `FortLegionarySparringPackage` (22) observed in the original game.

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
  `unsourced-semantics`, because fopdoc lists 11 as "??" and xEdit names
  it for TES4 only (`wbPackageTypeEnum`, §3.1).

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
  `SandboxSitClip`) and becomes live with AIP-SIT-002. Only 19 FO4 NIFs
  carry a marker block, all with value 0; FO4 authors its sit positions on
  the `FURN` record instead (AIP-SEAT-007).
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
- **AIP-SEAT-007.** On FO4 and Starfield, a furniture reference's sit
  positions MUST come from its `FURN` record's `SNAM` marker parameters
  (offset, Rotation Z and keyword per marker: xEdit `wbSNAMMarkerParams`,
  `wbDefinitionsFO4.pas:5314-5332`, used by `FURN` at 6915;
  `wbDefinitionsSF1.pas:6881-6892`, used by `FURN` at 12208), translated
  into `FurnitureMarker` at the plugin boundary. Today `FURN` is read only
  for its model (`extract_records_with_modl`,
  `crates/plugin/src/esm/records/dispatch_world_placement.rs:42-44`). The
  observable is a unit test that decodes a `FURN` fixture into markers.

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
  clip. Both games' `RACE` subgraph data has a Furniture role (`SRAF` role
  2: xEdit `wbDefinitionsFO4.pas:11317`, `wbDefinitionsSF1.pas:16748-16766`).
  The seat positions come from AIP-SEAT-007.
- **AIP-SIT-004.** Seating MUST resolve its clip through one
  per-game lookup at the asset boundary (today `anim_clips.rs`). Seat
  systems MUST NOT read the game kind.

### 5.6 Schedules (`AIP-SCHED`)

`PSDT` fields authored by the vanilla masters (uncommitted census, §3):

| Master | `PSDT` | Day-of-week constrained | Month byte | Date byte |
|---|---:|---:|---|---|
| `FalloutNV.esm` | 4 163 | 53 | −1 on all | 0 on all |
| `Skyrim.esm` | 5 961 | 53 | −1 on all | 0 on all |
| `Fallout4.esm` | 3 828 | 4 | −1 on 1 956 (form version < 122), 0 on 1 872 (form version ≥ 122) | 0 on all |
| `Starfield.esm` | 3 548 | 1 | 0 on all (every record form version ≥ 122) | 0 on all |

Only Oblivion authors a date (`PackSchedule::date` doc, `pack.rs:556-558`),
and it is evaluated today (§3.3), so no date requirement follows. The
Skyrim+ `PSDT` day-of-week byte uses the same enum as FO3/FNV, 0 Sunday …
6 Saturday plus four groups (xEdit `wbPackageScheduleDayOfWeekEnum`,
`wbDefinitionsCommon.pas:8007-8025`).

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
  agrees: every FO4 and Starfield month-0 record is version ≥ 122. Today
  `PackSchedule` has no month field (`pack.rs:545-561`). No vanilla master
  authors a month constraint, so the observable is a unit test on a
  synthetic record.

- **AIP-SCHED-003.** A zero-duration window (#3352) MUST stay unselectable
  until its meaning is sourced, and each such package MUST produce a decline
  line with reason `unsourced-semantics`. The GECK says only that "the
  schedule cannot be in less than one hour blocks"
  ([GECK: Packages](https://geck.uesp.net/wiki/Category:Packages)). That
  does not say what a 0 means. **Phase 0: verify** with one of the vanilla
  Travel packages `PackSchedule::active_at` names, observed in the
  original game. Today the parser logs each such schedule at debug
  (`pack.rs:1248-1256`), which names no procedure and reaches no
  `pack.ambient` field, so it is not a decline line.
- **AIP-SCHED-004.** The weekday origin MUST be one value resolved at load
  and held on the calendar resource, and `package_weekday` MUST NOT read
  the game kind (R5, the translation boundary). Today the origin is chosen
  per game: `package_weekday` matches on `ScriptGame`
  (`crates/scripting/src/globals.rs:145-152`) and `day_of_week` takes a
  `fallout` flag (`globals.rs:129-139`). This requirement retires both
  branches into the resource. Skyrim's origin depends on the edition
  (decision Q2): the loader resolves it from the master it reads, the
  2011 launch master (HEDR 0.94) giving Morndas and every later master
  (HEDR 1.71) giving Sundas; `GameKind` folds both into one kind, so the
  edition is read here and nowhere past the boundary.

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
  (`systems/archetype_idle.rs:142-162`). xEdit settles the record shape:
  Starfield's `RACE` carries the same Subgraph Data group as FO4, role 0 =
  MT (`wbDefinitionsSF1.pas:16748-16766`), which the `RACE` decode reads
  (`subgraph_data`, `crates/plugin/src/esm/records/actor/race.rs:187-193`).
  **Phase 0: verify** whether `Starfield.esm` authors `MT` entries and
  whether the graphs they name exist in Starfield's archives, by running
  `subgraph_census` (`crates/plugin/examples/`) on `Starfield.esm` and
  looking the named graphs up in the archives. If they do, the Fallout 4
  gate becomes a data test, not a second game check.
- **AIP-WALK-002.** On FO4, a package's walk legs MUST route over `NAVM`
  once the `NVNM` body decodes. No design owns that decode:
  [NAVM Pathfinding §9](../engine/navmesh-pathfinding.md#9-rollout) item 5
  names it as a prerequisite and disclaims it. This PRD depends on it and
  does not build it; Phase 0 files it as an issue.
- **AIP-WALK-003.** An Oblivion actor's walk legs MUST follow decision Q5.
  Today they are straight lines (§3.3).
- **AIP-WALK-004.** The walk-source lookup MUST stay at the asset boundary
  (`anim_clips.rs`, `asset_provider/`). The walk-animation systems
  (`npc_walk_animation_system`, `graph_locomotion_system`) MUST NOT read the
  game kind. Today this holds (`systems/walk_anim.rs` and
  `systems/graph_locomotion.rs` name no `GameKind`); the requirement keeps
  it under AIP-WALK-001.

### 5.10 Explainability and measurement (`AIP-DX`)

Today there is no `pack.ambient` command
([PACKAL §8](../engine/packal.md#8-tooling) proposes it), and the runtime
components (`AmbientPackageRuntime`, `WalkAnimation`, `NavPath`) are not in
the debug registry (`crates/debug-server/src/registration.rs`). The one
existing reader is `cell.owners [<gx> <gy>]`
(`byroredux/src/commands/world_info.rs:897-988`): for one exterior grid
cell it lists each actor with an `AmbientPackageRuntime` and its winning
package, with no decline reason and no interior form. AIP-DX-006 builds on
that query rather than copying it.

- **AIP-DX-001.** `pack.ambient <entity>` MUST print the actor's package
  list in priority order, one row per package with: the procedure (byte or
  leaf name), whether its schedule is open, whether its conditions passed
  and whether they were fully modelled, and the install or decline result
  with its reason.
- **AIP-DX-002.** Every decline MUST log one debug line with the package
  FormID, EditorID, procedure and reason. Today only the Dialogue arm logs a
  decline reason (`from_package`, `#5376` line, `ai_package.rs:475`), and
  the parser logs a zero-duration window (AIP-SCHED-003).
  Re-selection logs only the winner (`[m42-reselect]`, `ai_package.rs:1271`).
- **AIP-DX-003.** A census tool, `ambient_package_census` under
  `crates/plugin/examples/`, MUST print, per master, the counts in §3.1,
  the per-leaf counts of PKID-referenced tree packages behind §3.2, the
  per-object-type counts behind AIP-FIND and AIP-UW, the `PSDT` field
  counts of §5.6, and the number of `FURN` records that author `SNAM`
  (AIP-SEAT-007). The furniture-marker counts of §5.4 MUST come from the
  same tool or a sibling example reading the mesh archives. Run over the
  vanilla data, they regenerate those tables.
- **AIP-DX-004.** The real-data test
  `real_skyrim_esm_ambient_packages_now_resolve_for_previously_blind_npcs`
  MUST report install and decline counts per leaf. An equivalent ignored
  test MUST exist for `FalloutNV.esm`, `Fallout3.esm`, `Oblivion.esm`,
  `Fallout4.esm` and `Starfield.esm`.
- **AIP-DX-005.** `pack.ambient <entity>` MUST also print the actor's walk
  source and its cached waypoint count.
- **AIP-DX-006.** `pack.ambient` with no argument MUST list every actor that
  has an `AmbientPackageRuntime`, with its winning package or its decline
  reason, in interiors and exteriors alike.

---

## 6. Deliberate deviations and risks

No deviation below makes an existing mechanism redundant; the column is
kept for the format.

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Sandbox scores every nearby object (eat, sleep, furniture, idle markers, dialogue), with type weights that drop to zero after use and recover ([GECK: Sandbox Package](https://geck.uesp.net/wiki/Sandbox_Package)) | The nearest free sit marker (`pick_nearest_seat`) | — | Cells look less varied. Decision Q6, AIP-NG-007 |
| 2 | Packages "seem to be re-evaluated every 10 seconds" ([GECK: How Are Packages Evaluated](https://geck.uesp.net/wiki/How_Are_Packages_Evaluated)) | Once per in-game minute, and on `EvaluatePackage` (`ambient_ai_package_system`) | — | At the default Timescale of 30 ([GECK: List of Global Variables](https://geck.uesp.net/wiki/List_of_Global_Variables)) a game minute is 2 real seconds, faster than the GECK's own hedged figure. None |
| 3 | Unevaluable conditions do not exist in the original | Lists with an uncatalogued function pass (fail-open), except for procedures that take control of the actor (#5376 rule) | — | A fail-open package can win when the original would skip it. AIP-DX-001 shows when that happened |
| 4 | Accompany paths to the target's destination ([GECK: Accompany Package](https://geck.uesp.net/wiki/Accompany_Package)) | Follow's trailing runtime | — | The looser formation is not reproduced. §3.1 class A |
| 5 | Volley size and pause are random in [min, max] (GECK: Use Weapon Package) | The midpoint (AIP-UW-005) | — | Volleys are identical every time; tests and saves are deterministic |
| 6 | Eat consumes food ([GECK: Eat Package](https://geck.uesp.net/wiki/Eat_Package)) | Walk and sit only | — | Food stocks never deplete. Not addressed in this PRD |
| 7 | Sleep is in a bed ([GECK: Sleep Package](https://geck.uesp.net/wiki/Sleep_Package)) | The sit pose, parked at the bed's marker (the v0 approximation in the `systems/eat_sleep.rs` module doc), and a sit-marker fallback when no bed is in range (#5390) | — | Sleepers sit on their beds, and sit in chairs all night where no bed is in range. AIP-SEAT-004, decision Q3 |
| 8 | A zero-length schedule window's behaviour is not documented | Never selected (#3352) | — | The FNV packages `PackSchedule::active_at` lists never run. AIP-SCHED-003 |
| 9 | **Phase 0: verify** whether creatures use furniture (AIP-SEAT-005) | Never seated (#5495) | — | A creature furniture use, if one exists, is lost. AIP-SEAT-005 |
| 10 | A navmesh "tells an actor where it can walk" ([GECK: Bethsoft Tutorial Navmesh](https://geck.uesp.net/wiki/Bethsoft_Tutorial_Navmesh)) | Cross-tile joins by exact vertex match; the rest is straight-line ([NAVM Pathfinding §9](../engine/navmesh-pathfinding.md#9-rollout)) | — | Some walks hit walls across unjoined tiles. Owned by the navmesh design |
| 11 | UseItemAt uses its `PTDT` item: furniture or an activator is sat in or used, an inventory item plays its animation, and *Sit Down* seats the actor in any chair in the radius ([GECK: Use Item At Package](https://geck.uesp.net/wiki/Use_Item_At_Package)) | Eat's walk-and-sit; the `PTDT` item and the *Sit Down* flag are not read | — | An actor sits where the original would use an activator or stand. AIP-SEAT-006 covers only the inventory-item idle; the rest is not addressed in this PRD |
| 12 | The ambusher waits at its Wait Location, wanders within its radius, hides if *Hide while ambushing* is set, and detects better inside the ambush location ([GECK: Ambush Package](https://geck.uesp.net/wiki/Ambush_Package)). Oblivion's Ambush waits at its Location for the target to enter the radius, then activates it ([CS: Ambush Package](https://cs.uesp.net/wiki/Ambush_Package)) | Guard's hold at the wait location, with hostility gated to the `PLD2` zone. Oblivion authors no `PLD2`, so its ambushers fight by the ordinary hostility rules | — | Ambushers stand in the open, and Oblivion's never wait for their target. Not addressed in this PRD |
| 13 | Patrol walks the route of linked references from its starting location ([GECK: Patrol Package](https://geck.uesp.net/wiki/Patrol_Package)) | Wander's random walk in the `PLDT` radius. The `XLKR` links are decoded but not followed | — | Patrols wander instead of walking their routes. Not addressed in this PRD |
| 14 | A weapon-class package names a class, not a weapon; which weapon of the class the actor draws is not documented (GECK: Use Weapon Package, "Choose a specific weapon or weapon type") | The equipped weapon if it is of the class, else the first inventory weapon of the class | — | An actor may draw a different weapon than the original. AIP-UW-008 |

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
in, and missing data is `SKIP` (exit 77), never a pass. An ignored
real-data `cargo test` that finds no data prints its skip line and returns
green (the Skyrim test does, `ai_package.rs:3309-3312`); such a run is not
an acceptance pass.

| Scenario | Driven by | Data | Passes when |
|---|---|---|---|
| Existing FNV gates | `docs/smoke-tests/m42-package-procedures.sh`, `docs/smoke-tests/m42-eat-sleep.sh` | FNV | Both still pass after every phase |
| Find furniture / flora / actors | new `cargo test -p byroredux --bin byroredux` cases in `systems::find_package` (`a_furniture_find_seats_the_actor`, `a_flora_find_activates_it`, `an_actor_class_find_picks_the_nearest_actor`) | none (fixtures) | Each asserts `Seated`, an `ActivateEvent`, or the chosen target |
| UseWeapon place and target location | new tests in `systems::use_weapon_package` (`the_volley_waits_for_the_place_to_stand`, `the_volley_waits_for_the_target_location`, `a_blocked_shot_holds_fire`) | none | No `UseWeaponPackageStarted` before arrival or entry, and no shot while blocked |
| UseWeapon live | `m42-package-procedures.sh` gains a leg: a placed package with a `PLDT` (chosen in Phase 0 by the census) | FNV | `byro-dbg` `entities(UseWeaponPackageStarted)` lists the actor only after it is inside the location radius |
| Lean marker | new test beside `resolves_kind_from_animation_type_at_the_boundary` (`attach.rs`), whose 3 → `Lean` assertion changes with it | none | `AnimationType` 4 resolves to `Lean`, 3 to the `Sit` default |
| FO4 furniture positions | new test in `crates/plugin` (AIP-SEAT-007) on a `FURN` fixture transcribed from `Fallout4.esm` | none (fixture) | The decoded markers carry the fixture's offsets, Rotation Z and keywords |
| Decline reporting | `--bench-hold` on FNV `ULKitchen`, then `byro-dbg` `pack.ambient` with no argument | FNV | Every actor prints a winner or a decline reason; none prints an empty result |
| Census | `cargo run --release -p byroredux-plugin --example ambient_package_census -- <ESM>` (AIP-DX-003) | Oblivion, FO3, FNV, Skyrim SE, FO4, Starfield masters | Every procedure with authored packages maps to a §3 class; no "unknown" row |
| Skyrim tree | `real_skyrim_esm_ambient_packages_now_resolve_for_previously_blind_npcs` (`--ignored`), extended by AIP-DX-004 | Skyrim SE | Reports per-leaf counts; every PKID-referenced package installs or declines with a reason; the skip line is absent |
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
item in the table below. File the FO4 `NVNM` body decode as an issue
(Related). Resolve Q1–Q6, and write the Oblivion `PGRD`
section if Q5 takes it. Exit: the tool's output for the six masters is
checked into [NPC Spawn → AI Packages](../engine/npc-spawn-ai-packages.md);
every verify item is closed or filed as an issue; every §10 item carries
**Decided**.

| Verify item | Requirement | What settles it |
|---|---|---|
| FO3/FNV sit in a found chair | AIP-FIND-001 | a vanilla type-11 Find observed live, or an editor-documented furniture case |
| A Find activates a flora | AIP-FIND-002 | one of FNV's seven type-10 Finds observed live |
| Actors: Any is 28 or 29 | AIP-FIND-003 | the GECK Find "Object Type" dropdown order, or `VFSFreesideThugPackage` observed live |
| The 21 and 23 weapon-class labels, and whether Hand to Hand counts as melee | AIP-UW-009 | the GECK Use Weapon "Weapon type" dropdown, or one 21, one 23 and `FortLegionarySparringPackage` (22) observed live |
| A crouched reload pose exists | AIP-UW-006 | an archive scan for a crouched reload clip |
| CastMagic target for a self-range spell or a non-reference location | AIP-CAST-001 | one of Oblivion's five CastMagic packages observed live |
| Legacy marker refs above 20 | AIP-SEAT-002 | the census of vanilla `furnituremarkerXX.nif` files and the furniture that uses them |
| A creature furniture-enter clip exists | AIP-SEAT-005 | a scan of the vanilla creature animation directories |
| The Oblivion sit-enter KF | AIP-SIT-001 | a BSA scan of `meshes\characters\_male\` idle animations |
| Skyrim's chair-entry source (HKX clip or graph state) | AIP-SIT-002 | the Skyrim animation archives and the humanoid behaviour graph |
| What a zero-duration window does | AIP-SCHED-003 | one of the Travel packages `PackSchedule::active_at` names, observed live |
| The radius of Continue if PC near | AIP-FLG-003 | an editor or game-setting source for "nearby" |
| FO4 / FO76 / Starfield leaf names | §3.2 | AIP-DX-003's per-leaf counts on the three masters |
| Starfield `MT` subgraph entries and their graphs | AIP-WALK-001 | `subgraph_census` on `Starfield.esm`, then an archive lookup of the named graphs |
| Every census count in §3 and §5 | AIP-DX-003 | the checked-in tool's output |

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
AIP-SEAT-007, AIP-CAST-003, and the FO4 test of AIP-DX-004. It depends on
PACKAL. Exit: the Skyrim and FO4 real-data tests report an install rate
with no package outside a §3 class, `m42-skyrim-ambient.sh` passes, and the
§7.2 FO4 furniture-position test passes.

**Phase 4 — Oblivion (wave 3).** AIP-FIND-006…007, AIP-CAST-001…002,
AIP-SIT-001, AIP-WALK-003 (per Q5). Exit: `m42-oblivion-packages.sh` passes,
and Oblivion's procedure coverage is published.

**Measured, found in Phase 4 (2026-11-09, `Oblivion.esm`).** The five vanilla CastMagic packages do not carry a
spell in `PTDT`. Three author a `PTDT` of type 0 (Specific Reference) naming an actor or marker
(`E3DaedrothCast01PACK` → `E3KvatchArcherREF`, `Dark10NPCRaise01` → `Leafrot02XARef`; neither form is a `SPEL`),
`Dark10CastPack` / `Dark10CastPack02` author no `PTDT` at all and a `PLDT` of Near Reference on a trigger marker, and
`TarMeenaPracticeCasting` authors object type 29 (Spells: Range Self) with a Near Editor Location `PLDT`. AIP-CAST-001's
"cast the `PTDT` spell" therefore has no spell to name for four of the five, and the CS statement it rests on ("the
target of the package is the spell") contradicts the data; AIP-CAST-001 / -002 are not built until a source says which
spell an actor picks when the package names a reference. Object type 29's classification needs the `EFIT` range, which
the decode does not yet carry. AIP-FIND-006 / -007 shipped without them: the Oblivion enum translates to
`PackObjectClass` at the plugin decode, Armor (3) and Clothing (5) stay declined because `CLOT` and `ARMO` share one item
kind, and Soul Gems (17) are no item record.

**Measured, found in Phase 5 (2026-11-09).** All 43 Starfield `MT` subgraph entries name `AnimTextData\Tables\Graphs\*.agx`
(`NPC_MT_Wrapping.agx`, `CharGen.agx`, `1stPerson_MT_*`, ...), which exist in `Starfield - Animations.ba2`
(`meshes\animtextdata\tables\graphs\`, 347 `.agx` files) but are not Havok files. AIP-WALK-001's gate is now the data
test (`is_havok_graph`: a graph the engine can read), not a game check, and Starfield resolves nothing through it until an
`.agx` reader exists, which no design owns. The Phase 5 exit's Starfield walk source is therefore unmet.

**Phase 5 — Starfield and FO4 locomotion.** AIP-WALK-001, AIP-WALK-002,
AIP-WALK-004, and the Starfield test of AIP-DX-004. AIP-WALK-002 depends on
the FO4 `NVNM` body decode, which no design owns (Related; Phase 0 files
the issue). Exit: a Starfield Cydonia actor moved by a package shows a walk
source (`pack.ambient <id>` through `byro-dbg` names one while the actor
moves), the Starfield real-data test reports an install rate, and an FO4
interior Travel walk is routed (`pack.ambient` reports a non-zero waypoint
count).

---

## 10. Decisions required

Each decision needs the project owner. The recommendation is the first
option.

1. **Q1 — UseWeapon weapon classes.** The ROADMAP M42 row and
   `use_weapon_behavior` decline every weapon class because "the
   object-type enum numbering contradicts the vanilla data"; this decision
   is how far to reopen that. fopdoc and xEdit name 21 Ranged, 22 Melee and
   23 NONE. The vanilla EditorIDs contradict 21 and 23 (melee and unarmed
   practice at 21, gunfire at 23) and fit 22, Legion sparring and a target
   dummy (§5.2). Recommended: run 12 (Weapons: Any) and 22 (Weapons:
   Melee) now (AIP-UW-008, AIP-UW-009), whose names fit their packages, and
   keep 21 and 23 declined until Phase 0 settles their labels (AIP-UW-009).
   The 2 FO3 type-12 and 6 FNV type-22 packages then run; the 35 type-21
   and type-23 packages stay declined with a reason. Alternative A: also run 21 and 23
   by the Oblivion order shifted down by two (21 None, 23 Ranged), which
   fits every name but `CitInitiatePracticeMelee*` (melee at 21). All 43
   class packages then run, the 35 on a mapping no editor document states.
   Alternative B: run 21 and 23 by the fopdoc / xEdit names. The three
   type-21 practice packages draw ranged weapons, and the 32 type-23
   shooting packages stay declined as "NONE". Alternative C: run 12 only
   and keep 22 declined with 21 and 23 (the rev-2 recommendation). The 6 FNV
   type-22 packages keep doing nothing. Alternative D: keep every class
   declined (today). The 2 FO3 type-12 packages keep doing nothing too.
   **Decided** (2026-11-03): the recommendation — run weapon classes 12
   (Weapons: Any) and 22 (Weapons: Melee) now (AIP-UW-008, AIP-UW-009); 21
   and 23 stay declined until Phase 0 settles their labels.
2. **Q2 — Skyrim+ weekday epoch.** UESP disagrees with itself.
   *Skyrim: Statistics* says the game "starts on Morndas, the 17th of Last
   Seed" (local `uesp-wiki/Skyrim/Statistics.wiki:30`). *Skyrim: Unbound*
   says the date is fixed but the weekday "will be Sundas" when no game was
   loaded first, and otherwise whatever it was before
   (`uesp-wiki/Skyrim/Unbound.wiki:184-186`). Recommended: Sundas, which is
   Unbound's fresh-start case and the only one a new ByroRedux game can
   reproduce. Sundas is the `PSDT` Sunday, 0 (UESP *Lore: Calendar*, local
   `uesp-wiki/Lore/Calendar.wiki:55`). For FO4 and Starfield, recommended:
   the Gregorian weekday of the authored `GameYear` / `GameMonth` /
   `GameDay` start date, the rule FO3/FNV use today (`day_of_week`,
   `globals.rs:122-139`), which has no source either; the decision adopts
   it explicitly. The 53 Skyrim, 4 FO4 and 1 Starfield day-constrained
   packages then run only on their days. Alternative: Morndas for Skyrim
   (Statistics). The same packages run, each one weekday off from the
   Sundas reading. Alternative: leave Skyrim+ days unevaluated (today).
   Those packages then run every day.
   **Decided** (2026-11-03): the owner's reading, which reconciles the two
   UESP pages by edition — Skyrim starts on 17 Last Seed 4E 201, a Morndas
   on the launch edition (the 2011 master, as *Skyrim: Statistics* says)
   and a Sundas on every later edition (SE / AE, *Skyrim: Unbound*'s fresh
   start); the loader tells the editions apart by the master's HEDR version
   (AIP-SCHED-004). FO4 and Starfield take the recommendation: the
   Gregorian weekday of the authored start date. The owner's answer named
   Skyrim only; the FO4 / Starfield half is recorded as recommended.
3. **Q3 — Sleep with no bed in range.** Recommended: no sit fallback. The
   actor idles at the Sleep location, because the GECK defines Sleep as
   "find a bed and sleep in it" and, with a radius, "pick a valid bed within
   the radius". A sleeper with no bed in range then stands at its Sleep
   location through the window. Alternative: keep the #5390 sit fallback.
   Sleepers stay visibly seated in chairs through their sleep window.
   **Decided** (2026-11-03): the recommendation — no sit fallback; a
   sleeper with no bed in range idles at its Sleep location.
4. **Q4 — Ambient package flags.** Recommended: in scope are the flags that
   change selection: Once Per Day, Must Complete, Continue if PC near
   (AIP-FLG). Shopkeepers then stay while the player is in the shop, and a
   finished package does not re-run the same day. Out of scope are the
   door lock and unlock flags and Always Run / Always Sneak: the
   clip-driven games resolve a walk clip and no faster or crouched gait
   (`anim_clips.rs`, `SkyrimWalkClip`), and only FO4's behaviour graph
   blends to a run, from `Speed` (`systems/graph_locomotion.rs`). Alternative: all flags in a
   follow-up PRD. Shopkeepers then still leave with the player in the shop,
   and finished Travel packages re-run.
   **Decided** (2026-11-03): the recommendation — Once Per Day, Must
   Complete and Continue if PC near are in scope (AIP-FLG); the door lock /
   unlock flags and Always Run / Always Sneak are out.
5. **Q5 — Oblivion pathing.** Oblivion ships no `NAVM` (0 records in
   `Oblivion.esm`, `parse_navm`'s coverage table) and authors path grids
   instead ("A PGRD record defines the path grid for a cell", UESP
   *Oblivion Mod: Mod File Format/PGRD*; CS *Path Grid Generation*). `PGRD`
   is parsed today. Recommended: path Oblivion walks over `PGRD`, designed
   as a new section of [NAVM Pathfinding](../engine/navmesh-pathfinding.md)
   and delivered in Phase 4. Oblivion actors then walk around walls inside
   a cell's grid. Alternative: straight-line walks on Oblivion (today).
   Oblivion's Travel- and Wander-heavy package set (§3.1) then walks
   straight at its goals and stalls against walls, which the character
   controller blocks (`systems/locomotion.rs` module doc).
   **Decided** (2026-11-03): the recommendation — Oblivion paths over
   `PGRD`, designed as a new section of NAVM Pathfinding and delivered in
   Phase 4.
6. **Q6 — Sandbox activity scoring.** Recommended: out of this PRD
   (AIP-NG-007), and a follow-up revision once idle markers and food
   consumption have runtimes. Sandboxers keep taking the nearest free seat
   (§6 row 1). Alternative: in scope as a Phase 1 item. That makes Phase 1
   depend on an idle-marker runtime and on Eat consuming food, neither of
   which exists.
   **Decided** (2026-11-03): the recommendation — Sandbox activity scoring
   stays out of this PRD (AIP-NG-007), for a follow-up revision once idle
   markers and food consumption have runtimes.

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
| 3 | 2026-11-03 | Review round 2 folded in: HEAD re-verified at 2026-11-03 (Scene driver has no ForceGreet arm, Patrol's `XLKR` links are decoded but unused, Oblivion ambushers have no gate, `find_behavior` span, `cell.owners` as the existing reader); the FO4 `NVNM` decode named as unowned (Related, AIP-WALK-002, Phase 5); weapon class 22 fits its name, Q1 reworked to run 12 and 22; FO4/Starfield furniture positions settled from xEdit (`FURN` `SNAM`, new AIP-SEAT-007); Starfield subgraph shape settled from xEdit (AIP-WALK-001); Find-on-flora and Hand to Hand verify items; §6 rows for UseItemAt, Ambush, Patrol, the class-weapon pick and the bed sit pose; §3.2 leaf-name verify moved to the census tool (AIP-DX-003, now named); Starfield and date columns in §5.6, Starfield in AIP-DX-004; SCHED-004 states today's per-game weekday branches; AIP-UW-006 no longer a decline; real-data skip is not an acceptance pass; Phase 0 verify list as a table; Q2–Q6 consequences stated, Q4 and Q5 premises corrected | claude: revise |
| 4 | 2026-11-03 | Owner decisions Q1–Q6 recorded; Q2 amended by the owner (Skyrim's weekday origin per edition), with AIP-SCHED-004 stating the load-time edition read; the rest take the recommendation | owner: approved |
| 5 | 2026-11-03 | → ACTIVE: parent #6634 and phase issues #6655, #6656, #6658, #6660, #6662, #6663 filed; the Phase 0 verify items are acceptance criteria of #6655 | — |
