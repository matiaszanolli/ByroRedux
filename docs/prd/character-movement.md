# Natural character movement — Product Requirements

**Status**: ACTIVE (2026-11-28). Revision 5 adds a turn rate limit and a horse
body check at the owner's request (§10 Q17) and replaces the hand-counted clip
tables with the generated census (§12). The draft was written from the owner's
research note of 2026-11-27 and from measurements of the installed game archives
and of HEAD `8a6fe63ff` taken the same day (§3).
Statements about how an original engine, format or tool behaves cite external
sources or name the shipped file they were read from; a claim that could not
be verified is marked **Phase 0: verify** instead of being stated as fact.

**Tracking**: #7435 (parent). Phases: #7436 (0), #7437 (1), #7438 (2), #7439 (3), #7440 (4), #7441 (5), #7442 (6), #7443 (7). Phase 0's verify items are listed in #7436.

**Scope**: this PRD promises how characters and creatures *move* in the five
in-scope games (§7.1): gait selection, transitions between clips, adapting
shipped clips to the capsule's real motion, feet on stairs and slopes, short
climbs, and physical reactions layered over animation. It covers the player's
body and NPCs, bipeds and creatures, on top of the Rapier kinematic controller. Outside
it: new motion data of any kind, learned controllers, motion matching, what an
NPC decides to do or where it routes, modded behaviour graphs, first-person
arm animation, swimming and riding, ladders and long climbing routes, balance
recovery and fully simulated bodies, and new clip-format readers. First-person
motion is inside it only during a traversal (MOT-TRV-011).

**Related**: this document states the *promise*; the designs below own the
*how*. Phase 0 writes the one missing design.

| Mechanism | Owning design |
|---|---|
| Clip playback, blend stack, root motion, text keys | [Animation](../engine/animation.md) |
| Rapier world, fixed physics tick, player controller | [Physics](../engine/physics.md) |
| Ragdolls, motors, active ragdoll | [PHYSAL](../engine/physal.md) |
| NPC spawn, packages, walk clips | [NPC Spawn → AI Packages](../engine/npc-spawn-ai-packages.md) |
| NPC routes | [NAVM Pathfinding](../engine/navmesh-pathfinding.md) |
| Per-game translation boundary | [NIFAL](../engine/nifal.md) |
| Locomotion sets, derived motion records, pose layers, traversal, physical reactions | `docs/engine/locomotion.md`, written in Phase 0 and linked here then |

Four PRDs border this one. [AI Packages Completion](ai-packages-completion.md)
owns where a package-driven NPC gets a walk source on Starfield (AIP-WALK-001,
AIP-WALK-005) and the rule that keeps that lookup at the asset boundary
(AIP-WALK-004).
[Legacy Mod Compatibility](legacy-mod-compatibility.md) owns modded behaviour
graphs and conditional animations (`LMC-ANIM`).
[Sneak and Detection](sneak-and-detection.md) owns the sneak stance
(`SNK-STN`).
[P2P Co-op § 3.2](p2p-coop.md#32-determinism-inventory) inventories what makes
two runs of one session differ.

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `MOT-` prefix.

---

## 1. Product thesis

The owner's framing (2026-11-27):

> The goal is to redo movement across all seven games: walking that feels
> real, stairs where each foot lands where it should, and reaching high places
> by climbing instead of floating. All of it with each game's own animation
> and techniques that run on top of the kinematic controller over Rapier.

And the earlier direction on NPC motion (2026-11-20):

> While we parse havok movement, we want to ease it out to make it look more
> natural and stick to the directives rather than making a carbon copy of the
> movement. Our NPCs are meant to feel more natural.

The feature is for the person playing, who steers one body and watches many.
The seven clip libraries are very uneven (§3.4), so a technique that needs a
rich library cannot be the base, and one that fills gaps from the clips that
do ship pays off most.

The corollary that shapes every requirement: **motion is promised per need and
per game, from that game's own data. Where the data for a need is missing, the
engine adapts what ships or reports that it does not serve the need. It never
invents or borrows a clip, and never buys looks with control response or with
determinism.**

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Clip** | One shipped animation: a KF sequence, a Havok HKX animation, or a Starfield `.af` file |
| **Locomotion need** | One entry of the closed list in §3.2 |
| **Locomotion set** | For one skeleton family, the resolved source and disposition (§3.1) of every locomotion need |
| **Gait cycle** | A looping clip that carries a body across the ground at one speed and heading |
| **Pose layer** | One step of §3.3 that changes a skeleton's pose after clip selection |
| **Transition layer** | The pose layer that makes a clip switch continuous. Inertialization and dead blending are the two known techniques (§11) |
| **Stride warping** | Changing step length or playback rate so a clip covers ground at the capsule's speed |
| **Orientation warping** | Turning the legs toward the movement heading while the torso keeps its facing |
| **Distance curve** | For one clip, cumulative root distance against clip time |
| **Distance matching** | Choosing a start or stop clip's time from the distance left to the stop point instead of from elapsed time |
| **Displacement source** | The authored data that says how far a clip carries its actor |
| **Foot event** | An authored, timed label in a clip that names a footfall |
| **Plant window** | The span of a clip during which one leg's foot is on the ground |
| **Step phase** | One scalar per cyclic clip that locates a time inside the gait cycle |
| **Leg chain** | The ordered bones of one leg of a skeleton, from its root joint to its ground contact |
| **Derived motion record** | What the engine computes from a clip: plant windows, step phase, distance curve, capsule-driving spans (MOT-DATA-001) |
| **Locomotion tick** | One fixed 1/60 s simulation step on which movement state advances |
| **Traversal move** | A continuous motion that carries the body onto or over an obstacle too tall for the controller's step |
| **Need variant** | One locomotion need at one speed band and heading; a band or heading that does not apply is left out |
| **Contact point** | The point a leg-chain record declares where that leg's foot meets the ground |
| **Blocking collider** | An actor collider that capsule sweeps and contact solving read |
| **Hit collider** | An actor collider that only queries for hits read |

---

## 3. Core model

### 3.1 Dispositions

Every need variant of every skeleton family has exactly one disposition.
`anim.locomotion` (MOT-DX-001) reports it with its source clip or its reason.

| Disposition | Meaning |
|---|---|
| **Authored** | The game ships a clip for the need and the engine plays it |
| **Adapted** | No clip fits the need; the engine derives the pose from a shipped clip (transition, warping, distance matching, IK, anchoring a clip to a world point) |
| **Simulated** | No clip is used; the pose comes from the physics simulation |
| **Unserved** | Neither; the actor keeps its fallback (MOT-SUP-003). The reason is one of `no clip`, `no reader`, `no displacement source`, `no leg chain`, `no joint data`, `not built` |

Every reason but `not built` states a fact about the data or what can read it,
which the census or the named design shows. §9 limits what `not built` may
close.

### 3.2 Locomotion needs

| Need | Covers |
|---|---|
| idle | Standing still |
| gait | Moving, by speed band and by heading relative to the body |
| start | Leaving idle |
| stop | Arriving at idle |
| reversal | Changing heading sharply while moving |
| turn | Turning in place |
| airborne | Jump, fall, land |
| stance | The sneak variants of the needs above |
| grade | Stairs and slopes |
| traversal | Getting onto or over a tall obstacle |
| reaction | Responding to a shove or a hit without going down |

### 3.3 Pose layers

Layers apply in this order. A *feedback* layer changes where the capsule is or
what it does next; a *pose-only* layer changes only bones.

| Order | Layer | Kind |
|---|---|---|
| 1 | Clip selection | feedback |
| 2 | Clip sampling | feedback: the sample poses the blocking colliders (MOT-SIM-007) and, in marked spans, moves the capsule (MOT-GAIT-007) |
| 3 | Transition | pose-only |
| 4 | Warping | pose-only |
| 5 | Feet: leg IK and pelvis | pose-only |
| 6 | Hands | pose-only |
| 7 | Physical reaction | pose-only |

Playback rate and clip time are decided in layer 1 and advance on the
locomotion tick, including where warping or distance matching chooses them.
Layers 3 to 7 read tick state and write bones, from which the hit colliders are
posed. The Phase 0 design says which of that work runs once per rendered frame;
whatever hit detection reads is posed on the tick (MOT-SIM-007).

The shipped scripted head look (`byroredux/src/systems/head_look.rs`) and
transform propagation follow the last layer.

### 3.4 What each game ships

The counts are generated:
[census § Base set by role](../engine/locomotion-census.md#base-set-by-role)
(MOT-DX-005), with the archives it reads. It counts by file name in the vanilla
archives, mod archives excluded. The *base set* is the adult, third-person, unarmed or relaxed
locomotion set that the census names per game; weapon stances, personality
folders and the female, child and first-person variants are not counted. A role
comes from the file name, so the counts say what is named, not what a behaviour
graph plays. What the requirements lean on:

- The base sets of Oblivion, Fallout 3 and New Vegas hold gaits, turns and
  airborne clips, and no start, stop or reversal.
- Skyrim's holds starts, and no stop or reversal. Skyrim LE's counts match SE's.
- Fallout 4's holds forward gaits only, with starts, stops and reversals. Where
  its unarmed backward and strafing motion comes from is not visible there (V9).
- Fallout 76 and Starfield ship large sets that the engine cannot read.
- No jump start is named among the airborne clips of Fallout 3 and New Vegas.

### 3.5 Clip data and readers

What the engine reads and what each base-set clip carries are generated too:
[gait cycles](../engine/locomotion-census.md#gait-cycles-what-each-carries),
[each base-set clip](../engine/locomotion-census.md#base-set-clips-one-by-one) with its gait speed, and
[reader coverage](../engine/locomotion-census.md#every-actor-clip-what-the-engine-reads).

- **Readers.** The engine reads KF and HKX packfile clips. Fallout 76's Havok
  tagfile (`TAG0`) clips and Starfield's `.af` clips have no reader.
- **Footfall labels.** Every base-set gait the engine reads carries one: the text
  keys `Enum: Left` and `Enum: Right` in KF; `FootLeft` and `FootRight` in
  Skyrim and Fallout 4, with `FootSprintLeft` and `FootSprintRight` on sprints.
  Fallout 4 gaits also carry `SyncLeft` and `SyncRight`.
- **Displacement.** KF gaits carry accumulation-root (`Bip01`) translation, and
  Fallout 4 gaits Havok extracted motion. No Skyrim base-set gait has a source
  the engine reads. Some Skyrim and Fallout 4 clips that are not gaits travel on
  a root or centre-of-mass track.

Outside what the census covers, read by hand on 2026-11-27: Skyrim's
`1hm_walkforward` has no foot event; the horse walk carries `FootFront`,
`FootBack`, `SyncLeft` and `SyncRight`; Fallout 76 clips hold the four Fallout 4
strings; Skyrim ships text files of timed translation and rotation keys under
`meshes\animationdata\boundanims\`, which the engine does not read; and
`clip_census_ba2` flags 1,117 of the 8,330 character clips in Fallout 4's base
animation archive for a rotation jump between neighbouring samples (V17).

For Fallout 3 and New Vegas, the GECK wiki's *Sounds Tab* ties footdown sounds to "any
given footdown enum note in the animation for each corresponding limb". For
Skyrim, the CK wiki's *Animation Events* sends `FootLeft` "when the left foot
is in motion", which does not say at which instant (V1, V2). No lift-off event
was seen in any sampled clip. The Oblivion meaning of its keys is part of V1.

Every base-set gait of Fallout 3 and New Vegas carries a `Blend` text key with a
number, and none of Oblivion's or Skyrim's does. Clips of the three KF games also
carry `m:L` and `m:R` keys
([other labels](../engine/locomotion-census.md#other-labels-on-base-set-locomotion-clips)). No engine code
was found that reads either. The CS wiki's *Animation Tab* names `Blend: 9` and
calls its meaning unknown (V10).

### 3.6 Stair, slope and climbing clips

No actor clip in any of the seven games, or in Skyrim LE, has a slope, uphill,
downhill, incline or stair name
([census](../engine/locomotion-census.md#slope-and-stair-names)). Clips with climbing names do exist
([census](../engine/locomotion-census.md#climbing-names)); a name is a candidate, not proof.

- Oblivion has none, and Skyrim only the dragon's flight.
- Fallout 3 and New Vegas ship one-off scripted climbing idles, and creature
  climbs.
- Fallout 4 and Fallout 76 ship kneeling-cover vaults at three heights, and
  creature ambushes over walls.
- Starfield ships mantle, ladder and traversal clips.

Two Fallout 4 vault clips were probed: each carries the root forward through
extracted motion while the centre of mass rises from a kneeling height. What
graph state plays them is open (V7).

### 3.7 Engine behaviour

State at HEAD on 2026-11-27 (`8a6fe63ff`); rows 4, 6, 8, 10, 14 and 17 were
re-read on 2026-11-28 at HEAD `4f3c8088d`, and row 18 was read that day at HEAD
`a174e4ebe`. Files under `byroredux/src/`
unless a crate is named. MOT-DX-001 replaces the hand-written rows about
selection and sources; the rest are replaced by the tests their requirements
add.

| # | Capability | Today |
|---|---|---|
| 1 | Gait selection | `systems/walk_anim.rs` swaps one walk clip in when an actor's position changes and restores the earlier clip when it stops, at playback rate 1.0 |
| 2 | KF walk clips | `npc_spawn/anim_clips.rs`: forward walk only. Oblivion plays `handtohandforward.kf`, and the comment there takes it for the closest forward walk, although `_male\walkforward.kf` is archived |
| 3 | Skyrim walk clip | `asset_provider/animation.rs`: `1hm_walkforward.hkx`, and the comment says no unarmed walk ships standalone, although `male\mt_walkforward.hkx` is archived |
| 4 | Fallout 4 | `systems/graph_locomotion.rs` feeds the behaviour graph `Speed` and `Direction`; the graph's blenders and transition fades choose the clips, and a synced blend group shares one phase taken from each clip's `SyncLeft` and `SyncRight` (`crates/scripting/src/animation_graph.rs`). AI movers move the actor. A graph actor gets no `WalkSpeed`, since only `attach_walk_animation` inserts one and no walk clip resolves for Fallout 4, so movers use the 100 units/s default; root motion moves the actor only while the graph sets `bGraphDriven` or `bAnimationDriven` (read from code, not measured in a run) |
| 5 | Fallout 76, Starfield | no walk-clip path |
| 6 | Player's third-person body | `player_body.rs` attaches a forward walk and a sneak walk and idle; its walk lookup resolves nothing for Fallout 4. The capsule moves at 220 units/s, doubled when sprinting (`CharacterController::HUMAN`), whatever the clip covers (the New Vegas forward walk travels about 102 units in 1.2 s). No run, strafe, backward, turn or jump clip path appears in `byroredux/src/`. A jump rises about 105 units, on a capsule 128 units tall |
| 7 | NPC speed | `walk_speed_for` takes the walk clip's accumulation-root speed inside fixed bounds, else a default |
| 8 | Transitions | `AnimationStack` ramps weights linearly; the classic paths hard-cut. [Animation § State machine](../engine/animation.md#state-machine-the-consumer-anchored-slice-ar-09--4128) gives as the reason that those sequences carry no authored blend duration, which the `Blend` keys of §3.5 put in doubt |
| 9 | Foot IK, warping, inertialization | none found in `crates/` or `byroredux/src/` |
| 10 | Foot events | text keys surface as `AnimationTextKeyEvents`; no consumer of a foot-event label was found (the sync points of row 4 are read), and footsteps fire by accumulated distance (`systems/audio.rs`) from emitters that only the fly camera and the player body are given (`scene.rs`); NPCs have none |
| 11 | Stairs | the capsule auto-steps and snaps to ground ([Physics § Controller tuning](../engine/physics.md#controller-tuning)); the body does not adjust |
| 12 | Climbing | no traversal or ladder code found in `byroredux/src/` or `crates/physics/` |
| 13 | Reactions | a knockdown is a full ragdoll followed by a get-up (`systems/knockdown.rs`); ragdoll motors are parsed and skipped ([PHYSAL](../engine/physal.md#active-motorised-ragdoll--data-captured-simulation-deferred)) |
| 14 | Simulation step | of the movement systems, only `PhysicsWorld::step` runs on a fixed accumulator; it drops backlog past a substep cap or a wall-clock budget, and its static-scene fast path zeroes the accumulator (`crates/physics/src/world/mod.rs`). The player controller steps once per frame with a clamped `dt` (`boot/schedule/early.rs`, `systems/character.rs`); NPC movers step once per frame (`boot/schedule/post_update/ambient_ai.rs`) |
| 15 | Determinism setting | `rapier3d` is declared without features (`Cargo.toml`), so `enhanced-determinism` is off |
| 16 | NPC facing | At `8a6fe63ff`, `step_toward_detailed` (`systems/locomotion.rs`) turned the actor's local +Z to its target, so walkers moved backwards. Commit `fc766e09f`, of the same date, turns local −Z and drops the player body's compensating half turn (test `a_walker_faces_where_it_is_going`). No baseline is taken at an earlier commit |
| 17 | Actor bone colliders | the player's capsule sweep is blocked by other actors' bone colliders (`systems/character.rs`, `filter_groups: None`, chosen deliberately); NPC sweeps mask them (`actor_move_interaction_groups`) |
| 18 | Turning | an NPC mover closes a fraction of the remaining angle each frame (`LOCOMOTION_TURN_RATE`, `systems/locomotion.rs`), so a larger turn is faster and no rate is capped; the third-person player body takes the camera's yaw at once (`player_body.rs`); a held conversation partner is set facing the speaker at once (`systems/random_conversation.rs`) |

---

## 4. Goals and non-goals

### 4.1 Goals

- **MOT-G-001 — Planted feet stay put.** Planted-foot slip (§8) is inside its
  bound on every in-scope game and gait.
- **MOT-G-002 — No pops.** Switch discontinuity (§8) stays inside its bound on
  every switch between locomotion clips, whether locomotion selection or an
  actor's behaviour graph makes it (§10 Q15).
- **MOT-G-003 — Feet meet the ground.** Foot height error and swing-foot
  penetration (§8) stay inside their bounds on the stair and slope fixtures.
- **MOT-G-004 — Every game gets what its data allows, and says so.** Every
  locomotion need of every in-scope skeleton family has a reported
  disposition.
- **MOT-G-005 — Control stays responsive.** Input response (§8) is no worse
  than its bound against the baseline recorded before the tick change.
- **MOT-G-006 — A ledge is climbed.** The ledge scenarios (§7.2) are completed
  by a continuous traversal move.
- **MOT-G-007 — Same ticks, same result.** The determinism scenario (§7.2)
  passes with every feedback layer enabled.

### 4.2 Non-goals

- **MOT-NG-001.** New motion data: no motion capture, no clips authored by the
  project, no clips moved from one game to another, and no third-party motion
  data in the repository or the binary. Research datasets such as LaFAN1,
  published under CC BY-NC-ND 4.0 (§11), serve lab comparison only.
- **MOT-NG-002.** Learned controllers and trained policies (PFNN, DeepPhase,
  AMP, DReCon policies, learned motion matching). They need capture data or
  large-scale training, and PFNN's published code and data are for academic,
  non-commercial use (§11). The step phase they motivate is in scope.
- **MOT-NG-003.** Modded behaviour graphs and conditional animation
  replacement: `LMC-ANIM` in
  [Legacy Mod Compatibility](legacy-mod-compatibility.md).
- **MOT-NG-004.** What an NPC does and where it routes, including NPC use of
  traversal moves. [AI Packages Completion](ai-packages-completion.md) and
  [NAVM Pathfinding](../engine/navmesh-pathfinding.md) own those, as does
  AIP-WALK-005 for Starfield's `.agx` tables.
- **MOT-NG-005.** First-person arm and weapon animation, other than the
  traversal motion of MOT-TRV-011. The first-person camera is otherwise covered
  only where a movement layer could disturb it.
- **MOT-NG-006.** Frame-exact reproduction of the original engines' movement.
  The owner's 2026-11-20 direction (§1) decides this; decoders stay exact
  (MOT-GAIT-011).
- **MOT-NG-007.** Swimming animation, mounting and riding. A horse moving on
  its own legs is a creature and is in scope.
- **MOT-NG-008.** Ladders, and physics-synthesised climbing of long routes.
  Climbing here means short traversal (§10 Q5); long routes wait for a later
  PRD.
- **MOT-NG-009.** A motion-matching search (§10 Q8). The phase-data limits of
  §5.11 still apply.
- **MOT-NG-010.** Balance recovery and a fully simulated body. Reactions here
  are partial ones (§10 Q9).
- **MOT-NG-011.** Readers for Fallout 76's `TAG0` tagfile clips and
  Starfield's `.af` clips (§10 Q7). Each needs its own design; this PRD's
  requirements reach those games through a revision once a reader lands.

---

## 5. Requirements by area

### 5.1 Clip supply (`MOT-SUP`)

- **MOT-SUP-001.** Each actor skeleton family MUST resolve a locomotion set
  that gives every need of §3.2 a disposition and, unless it is Unserved or
  Simulated, a source clip.
- **MOT-SUP-002.** Per-game differences MUST be resolved at the asset
  translation boundary; no movement or pose-layer system may read the game
  kind. This extends AIP-WALK-004 beyond the walk-animation systems it names.
- **MOT-SUP-003.** An Unserved need MUST leave the actor on the fallback that
  the design's per-need table names, with the reason reported. Phase 0 writes
  the table; each entry is behaviour the actor has today or has from authored
  data.
- **MOT-SUP-004.** Every clip in a locomotion set MUST come from the loaded
  game's own archives or the user's loaded content.
- **MOT-SUP-005.** An unarmed Oblivion biped's forward walk MUST resolve to
  `_male\walkforward.kf` when it is archived (§3.7 row 2).
- **MOT-SUP-006.** An unarmed Skyrim biped's gaits MUST resolve to the unarmed
  `mt_` clips when they are archived (§3.7 row 3). A clip whose displacement
  source is unread stays subject to MOT-DATA-004.
- **MOT-SUP-007.** The repository and the shipped binary MUST contain no clip,
  derived motion record or other motion data taken from game files; records are
  derived from the user's installation.

### 5.2 Tick and determinism (`MOT-SIM`)

- **MOT-SIM-001.** Given one load, one set of random seeds, one load and
  streaming schedule counted in ticks and one sequence of per-tick inputs,
  every actor's capsule position and the state of MOT-SIM-002 at tick N MUST be
  bit-identical however the ticks are grouped into rendered frames, on the same
  machine and binary (§10 Q2; cross-platform identity is not promised). A
  replay MUST be able to set the seeds and the schedule. Today this does not
  hold (§3.7 row 14), and
  [P2P Co-op § 3.2](p2p-coop.md#32-determinism-inventory) lists further sources
  of divergence; any that reaches this state in S9 is fixed here.
- **MOT-SIM-002.** The player controller, the NPC movers and all state that
  feeds back into capsule motion (clip selection, step phase, marked root
  motion, traversal progress, springs) MUST advance only on the locomotion
  tick.
- **MOT-SIM-003.** Rendered actor and camera positions MUST be interpolated
  between ticks, so a display faster than the tick rate shows movement on every
  frame.
- **MOT-SIM-004.** Simulation time dropped under load MUST be counted and
  printed, so a determinism run can tell that it lost ticks.
- **MOT-SIM-005.** Loading a save MUST resume each actor in an authored pose
  with no transition, foot lock or reaction offset carried over; pose-only
  layer state MUST NOT be saved.
- **MOT-SIM-006.** A pose layer from 3 to 7 MUST NOT write anything that a
  feedback layer, or a physics query that moves a capsule, reads.
- **MOT-SIM-007.** Before any layer after sampling ships, each actor MUST have
  blocking colliders posed from the layer-2 sample and separate hit colliders
  posed from the final pose, both on the locomotion tick (§10 Q13). Today the
  player's sweep is blocked by actor bone colliders (§3.7 row 17).
- **MOT-SIM-008.** Simulation time dropped under load MUST be dropped as whole
  ticks, for every tick-driven system together, so that dropping never changes
  what a tick computes.

### 5.3 Derived motion data (`MOT-DATA`)

- **MOT-DATA-001.** Every clip in a locomotion set MUST have a derived motion
  record: plant windows per leg chain; a step phase if it is cyclic; a distance
  curve if it has a displacement source; and the spans in which it drives the
  capsule (MOT-GAIT-007), each naming its source: the authored graph, an
  authored label, or a labelled engine choice. **Phase 0: verify** what marks
  such spans in a KF clip (V19).
- **MOT-DATA-002.** A touchdown time MUST come from the clip's authored foot
  event where one exists and Phase 0 has confirmed its timing (V1, V2);
  otherwise it MUST be derived from the foot bone's height and speed. The end
  of a plant window is derived from bone motion. Both ends MUST agree with the
  reference windows of MOT-DATA-007 within the window tolerance (§8.1).
- **MOT-DATA-003.** Each plant window MUST record whether it is authored or
  derived.
- **MOT-DATA-004.** A clip's displacement MUST come from that game's authored
  displacement source. A clip with none, or with one the engine cannot read,
  MUST be reported `no displacement source`, and no speed is made up and
  presented as authored. Where Skyrim's source stays unread (V4 ends in a
  declared gap), the actor MUST keep walking with its set's walk clip at a
  speed labelled as an engine choice (§10 Q14). Today the Skyrim samples have
  no source that the engine reads (§3.5), and `walk_speed_for` falls back to a
  default. **Phase 0: verify** the Skyrim bound-animation format and how a
  block names its clip (V4).
- **MOT-DATA-005.** Deriving a record twice from the same clip bytes MUST give
  identical records.
- **MOT-DATA-006.** An actor that fires footsteps today (§3.7 row 10) SHOULD
  fire them at plant-window touchdowns in place of accumulated distance when
  its gait clip has plant windows. This promises no NPC footsteps.
- **MOT-DATA-007.** A reference derivation of plant windows that shares no code
  with the runtime's records MUST compute them from the user's installation, by
  a rule the Phase 0 design fixes, and MUST be checked against a synthetic bone
  trace with known contact times. Contact metrics (§8) are taken over its
  windows; a gait clip for which it or the runtime record has no window fails
  the run and never scores zero.

### 5.4 Gaits and displacement (`MOT-GAIT`)

- **MOT-GAIT-001.** A moving actor MUST play the gait clip of its movement
  heading relative to its body, among the headings its set holds as Authored.
- **MOT-GAIT-002.** An actor's speed band MUST be chosen by comparing capsule
  speed with the authored speeds of its set's gait clips, by the band rule and
  hysteresis of §8.1.
- **MOT-GAIT-003.** The player's third-person body MUST resolve gaits through
  the same locomotion set and selection as an NPC of that skeleton.
- **MOT-GAIT-004.** An airborne actor MUST play its set's jump, fall and land
  clips where they are Authored.
- **MOT-GAIT-005.** A standing actor that changes heading SHOULD play its
  set's turn clip.
- **MOT-GAIT-006.** Authored start, stop and reversal clips SHOULD be played
  when the matching change of motion happens.
- **MOT-GAIT-007.** Which spans of a clip drive the capsule MUST be data in
  the clip's record or its authored graph (MOT-DATA-001), not a rule inside a
  state machine. Outside those spans the capsule's motion is the authority
  (§10 Q3).
- **MOT-GAIT-008.** Positional response and rotational response MUST be
  separate settings, each one entry in the settings registry.
- **MOT-GAIT-009.** Capsule acceleration and braking SHOULD follow springs
  fitted per gait from that game's start and stop clips; where a game has none,
  the spring values MUST be labelled as an engine choice.
- **MOT-GAIT-010.** Easing MUST NOT change where an actor ends up: a package
  destination is reached within today's arrival tolerance, and a seat or
  furniture entry starts from the same authored pose as today.
- **MOT-GAIT-011.** Clip and graph decoders (`crates/nif`, `crates/hkx`) MUST
  keep returning the authored values; easing MUST live in the runtime.
- **MOT-GAIT-012.** Locomotion selection MUST NOT replace playback that another
  owner holds: scripted idles, cinematics, combat takes, furniture, death.
- **MOT-GAIT-013.** On an actor whose behaviour graph chooses its clips, this
  PRD's selection MUST act through the graph's variables and events, and MUST
  NOT override the graph's chosen clip. On such an actor MOT-GAIT-001 and -002
  are met by the variables fed to the graph, and `anim.locomotion` names the
  clips the graph chose.
- **MOT-GAIT-014.** A sneaking actor SHOULD resolve each need through its set's
  sneak clips where they are Authored. Which clips the player's stance uses is
  SNK-STN-008's.
- **MOT-GAIT-015.** A graph actor's AI walking speed MUST come from the
  extracted motion of its walk gait clip, in place of the default it moves at
  today (§3.7 row 4).
- **MOT-GAIT-016.** The body's horizontal offset from its capsule MUST be
  damped, speed-capped and never larger than the body offset limit (§10 Q3,
  §8.1).
- **MOT-GAIT-017.** The facing of the third-person player body and of an NPC
  MUST change on the locomotion tick, no faster than the turn rate limit
  (§8.1, §10 Q17). Exempt are a facing that a script, a teleport, a load, a
  seat or furniture entry, or another owner of playback sets (MOT-GAIT-012),
  and the turn an authored span drives (MOT-GAIT-007). A conversation partner
  turning to the speaker is not exempt (§3.7 row 18).
- **MOT-GAIT-018.** The turn rate limit MUST NOT delay the camera's look or the
  capsule's movement direction.

### 5.5 Transitions (`MOT-TRN`)

- **MOT-TRN-001.** A switch between locomotion clips, whether locomotion
  selection or an actor's behaviour graph makes it, MUST carry each bone's pose
  and velocity from the outgoing motion, within the switch discontinuity
  bound.
- **MOT-TRN-002.** A skeleton in transition SHOULD sample no more clips per
  tick than the same skeleton out of transition.
- **MOT-TRN-003.** The transition layer MUST work on any pose source: KF clip,
  HKX clip or behaviour-graph output.
- **MOT-TRN-004.** An authored transition duration or an authored "no blend"
  MUST be used where the game supplies one; any other duration MUST be labelled
  an engine choice (§10 Q4). Today the HKX transition effect's duration is
  decoded (`crates/hkx/src/behavior.rs`), and the `Blend` keys of Fallout 3 and
  New Vegas are unread (§3.5). **Phase 0: verify** what those keys mean and
  what a KF clip without one uses (V10).
- **MOT-TRN-005.** A switch between two cyclic gaits SHOULD enter the new clip
  at the matching step phase. Today only a behaviour graph's synced blend group
  shares a phase (§3.7 row 4).

### 5.6 Warping and distance matching (`MOT-WRP`)

- **MOT-WRP-001.** At every capsule speed the scripted routes produce, an
  actor's stride, playback rate or both MUST adapt so that planted-foot slip
  stays inside its bound, leaving out the ticks counted under MOT-WRP-004,
  whose share of planted ticks MUST stay within the sliding allowance (§8.1,
  §10 Q12).
- **MOT-WRP-002.** A heading with no Authored gait clip MUST be served as
  Adapted by turning the legs toward the heading while the torso keeps its
  facing.
- **MOT-WRP-003.** Where a start or stop clip and its distance curve exist, the
  clip's time SHOULD be chosen by distance matching.
- **MOT-WRP-004.** Every pose correction in layers 4 to 6 MUST have a limit;
  past it the engine MUST leave the foot to slide or miss and not distort the
  pose further, and MUST count the event.
- **MOT-WRP-005.** A game with no start or stop clips MUST still start and stop
  inside the switch discontinuity bound, using the transition layer and the
  springs.

### 5.7 Feet on the ground (`MOT-FT`)

- **MOT-FT-001.** During a plant window a foot MUST stay at its world contact
  point, within the slip bound.
- **MOT-FT-002.** A swinging foot's landing point MUST be found on collision
  geometry before touchdown, so that on the stair fixture the foot clears the
  riser within the penetration bound.
- **MOT-FT-003.** A leg MUST reach its target by IK, lowering the pelvis when
  the leg cannot reach, without bending a joint past the range the skeleton's
  own clips use.
- **MOT-FT-004.** A planted foot SHOULD align to the surface normal, up to a
  limit.
- **MOT-FT-005.** The feet layer MUST NOT move the capsule or the camera.
  Step height and slope limit stay the controller's.
- **MOT-FT-006.** A foot with no surface within reach MUST keep its authored
  pose.
- **MOT-FT-007.** The feet layer MUST be off while an actor swims, is
  ragdolled, is seated or is in a cinematic.
- **MOT-FT-008.** Skinning and the actor's hit colliders MUST see the adjusted
  pose. Blocking colliders do not (MOT-SIM-007).
- **MOT-FT-009.** The feet layer MAY be skipped for actors that are off screen
  or beyond a distance setting. A skip chosen from the camera MUST NOT change
  anything simulation reads, hit colliders included.
- **MOT-FT-010.** Pelvis height and foot targets MUST change continuously across
  steps, and as a foot enters or leaves surface reach, within the stair
  smoothness bound (§8).

### 5.8 Creatures and leg chains (`MOT-LEG`)

- **MOT-LEG-001.** Leg chains, with each leg's contact point, MUST be data per
  skeleton family; the feet and warping layers MUST hold no code for a
  particular creature.
- **MOT-LEG-002.** The feet layer MUST accept any number of legs.
- **MOT-LEG-003.** A skeleton with no leg-chain data MUST play unadjusted and
  report `no leg chain`.
- **MOT-LEG-004.** A body with front and hind legs SHOULD pitch and roll to
  the slope under its contacts; the Skyrim horse MUST, within the body tilt
  limit (§8.1).
- **MOT-LEG-005.** The Skyrim horse MUST pass the stair and slope fixtures on
  all four feet.
- **MOT-LEG-006.** An event that names a leg group instead of one leg MUST be
  resolved to per-leg plant windows from bone motion. The horse's `FootFront`
  and `FootBack` are taken for such events from their names alone;
  **Phase 0: verify** what they mark (V2).
- **MOT-LEG-007.** On the stair and slope fixtures, every vertex of the Skyrim
  horse's skinned body mesh MUST stay no deeper below the collision surface
  than the body penetration limit (§8.1, §10 Q17).

### 5.9 Traversal (`MOT-TRV`)

- **MOT-TRV-001.** When the player gives the traversal input at an obstacle
  inside the reachable envelope, the body MUST move onto or over it with no
  tick's displacement above the traversal tick bound (§8.1).
- **MOT-TRV-002.** The obstacle's height, depth and the room at the far end
  MUST be measured against collision geometry, and a request with no room to
  stand MUST be refused with its reason logged.
- **MOT-TRV-003.** Traversal poses MUST be Authored from a shipped climbing
  clip where one is readable and fits, else Adapted from the game's own jump,
  land or crouch clips anchored to the obstacle; `anim.locomotion` MUST name
  which. **Phase 0: verify** which clips can serve (V7, V8, V11).
- **MOT-TRV-004.** During a hand-contact span the hands SHOULD reach the edge
  by arm IK within the anchor error bound.
- **MOT-TRV-005.** A knockdown, a death or a lost obstacle during a traversal
  MUST end it into a ragdoll or a fall.
- **MOT-TRV-006.** Traversal MUST be a setting that stays off by default until
  Phase 6 has published a check of quest routes against reachable ledges
  (§10 Q6), run to a quest set, method and default rule the owner fixes
  beforehand (§8.1).
- **MOT-TRV-007.** Trigger volumes that the capsule crosses during a traversal
  MUST fire as they do when it walks through them.
- **MOT-TRV-008.** A save made during a traversal MUST load with the capsule
  standing on a supported position.
- **MOT-TRV-009.** Scripted climbing idles MUST keep playing as their scripts
  ask (MOT-GAIT-012).
- **MOT-TRV-010.** In first person, the camera SHOULD follow the traversal
  path without a jump between ticks.
- **MOT-TRV-011.** In first person, a traversal MUST show motion taken from the
  clip that serves it (MOT-TRV-003): the camera, a visible hand or both follow
  that clip's path, anchored to the obstacle, within the anchor error bound. A
  glide with an unmoving pose does not count (§10 Q11). **Phase 0: verify**
  which wave 1 and wave 2 games ship first-person clips that can serve
  (V18).

### 5.10 Physical reactions (`MOT-PHY`)

- **MOT-PHY-001.** Where MOT-PHY-003 permits it, a shove or a non-lethal hit
  MUST move the torso and arms physically over the animated locomotion and
  return them to the animated pose within the reaction return time (§8.1).
- **MOT-PHY-002.** A partial reaction MUST NOT move the capsule or change what
  the actor is doing.
- **MOT-PHY-003.** Reactions MUST drive joints only on ragdolls whose joint
  limits and motors [PHYSAL](../engine/physal.md#active-motorised-ragdoll--data-captured-simulation-deferred)
  supports; any other actor reports the need Unserved, `no joint data`. PHYSAL
  records that Oblivion's constraints carry no motor data;
  **Phase 0: verify** whether its ragdolls can be driven all the same (V20).
- **MOT-PHY-004.** The same ticks and the same hit MUST give the same reaction
  pose (MOT-SIM-001).
- **MOT-PHY-005.** A death or knockdown during a reaction MUST take over into
  the full ragdoll, which behaves as it does today.

### 5.11 Phase data and motion matching (`MOT-MM`)

This PRD builds no matcher (MOT-NG-009). These keep its phase data clear of
the shape the claims in §10 Q8 describe, for any later one.

- **MOT-MM-001.** A derived motion record MUST hold at most one step phase per
  clip, and no per-leg, per-bone or per-contact phase channel.
- **MOT-MM-002.** Clip or frame selection MUST NOT use a per-leg or
  per-contact phase value, or plant-window state, as a match criterion. Plant
  windows serve foot placement and audio.

### 5.12 Diagnostics and cost (`MOT-DX`)

- **MOT-DX-001.** `anim.locomotion <entity>` MUST print, per need variant, the
  disposition, the source clip or the reason, and the clip playing now.
- **MOT-DX-002.** `anim.feet <entity>` MUST print, per leg, the plant state,
  the target and the current error.
- **MOT-DX-003.** Each pose layer MUST have a runtime switch, for an A/B
  comparison on one running scene.
- **MOT-DX-004.** A headless scripted route MUST print, in a machine-readable
  form with the commit it ran at, every §8 metric the build can measure, and
  name those it cannot.
- **MOT-DX-005.** The clip census behind §3.4 to §3.6 MUST be a checked-in
  tool whose report regenerates from the installed games.
- **MOT-DX-006.** Telemetry MUST report CPU time per pose layer and the number
  of skeletons it ran on.
- **MOT-DX-007.** In steady state the pose layers MUST add no heap allocation
  per tick.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | On Fallout 3 and New Vegas, both animations play during a transition between anim groups (GECK wiki, *Animation Tab*). **Phase 0: verify** Oblivion, Skyrim and Fallout 4 (V21) | A transition layer that need not play both (MOT-TRN-002) | two-clip cross-fades on classic-path switches; a behaviour graph's own blends stay | Flattened detail or sliding feet in the new clip → MOT-FT-001, MOT-DX-003 |
| 2 | **Phase 0: verify** whether each original adjusts feet to the ground, and how (V12) | Feet and pelvis adjusted off the authored pose (MOT-FT-*) | none established | A distorted pose → MOT-FT-003, MOT-WRP-004 |
| 3 | **Phase 0: verify** what each original does when the player jumps at a ledge (V13) | A traversal move (MOT-TRV-001) | none established | Reaching places level design kept out of reach, or skipping triggers → MOT-TRV-002, -006, -007 |
| 4 | NPCs follow the authored movement | Eased motion that keeps the directives (owner, 2026-11-20) | none | An actor ending off its mark or a misaligned seat → MOT-GAIT-010, MOT-GAIT-012 |
| 5 | **Phase 0: verify** each original's player acceleration (V14) | Springs fitted from clips (MOT-GAIT-009) | none established | Heavy-feeling control → MOT-G-005, MOT-GAIT-008 |
| 6 | **Phase 0: verify** each original's non-lethal hit reaction (V15) | Physical partial reactions (MOT-PHY-001) | none established | Combat feel and timing change → MOT-PHY-002 |
| 7 | **Phase 0: verify** what each original shows in first person during such a move (V13, V18) | Camera or hand motion from the serving clip (MOT-TRV-011) | none established | A disorienting view, or hands out of frame → MOT-TRV-010, MOT-TRV-011 |
| 8 | **Phase 0: verify** how fast each original turns the player's body and NPCs (V22) | Bodies turn no faster than a limit (MOT-GAIT-017) | none established | A body that trails its movement direction and plays a side or backward gait while it turns → MOT-GAIT-001, MOT-GAIT-018, MOT-G-005 |

Engineering risks, which depart from today's engine rather than from an
original:

| Risk | Requirement or scenario |
|---|---|
| Moving onto the tick changes routes that count rendered frames | S7 |
| Interpolation delays what is on screen | MOT-G-005; input response is measured as presented |
| Pose layers and a second collider set cost CPU | MOT-DX-006, MOT-DX-007, MOT-FT-009 |
| What blocks the player differs from what is drawn | MOT-SIM-007; the difference is the layers' correction, held by MOT-WRP-004 |
| Warped legs fight an authored directional clip | MOT-GAIT-001 selects Authored first; MOT-WRP-002 applies only where there is none |
| Traversal overlaps the existing jump (§3.7 row 6) | The traversal input and envelope of §8.1; MOT-TRV-006 |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow what the engine can read today. Wave 1 is the KF games, whose
sampled clips carry both events and displacement. Wave 2 is the Havok games
whose clips decode.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 1. Readable, and a small base set with no starts or stops: the test that adaptation works |
| Fallout 3 | yes | Wave 1. Same clip layout as New Vegas |
| Fallout: New Vegas | yes | Wave 1, the reference title |
| Skyrim (LE / SE / AE) | yes | Wave 2. Clips decode; displacement waits on V4. LE's base set matches SE's |
| Fallout 4 | yes | Wave 2. The behaviour graph already chooses clips; the layers compose with it (MOT-GAIT-013) |
| Fallout 76 | no (§10 Q7) | The clips have no reader. Every need reports Unserved, `no reader` |
| Starfield | no (§10 Q7) | `.af` has no reader, and its walk source is AI Packages Completion's. Every need reports Unserved, `no reader` |

### 7.2 Acceptance

Scenarios name their data. No game or mod file is checked in. `cargo test`
names and `docs/smoke-tests/mot-locomotion.sh` are new; Phase 0 writes the
script and later phases add the tests.

| # | Scenario | Driver and data |
|---|---|---|
| S1 | **Determinism.** One input log with fixed seeds is run with ticks grouped at 30, 60 and 144 frames per second and unevenly; capsule positions and MOT-SIM-002 state match at every tick | `cargo test`, synthetic physics world; no game data |
| S2 | **Reference walk.** The player and an NPC follow a scripted route with a speed sweep, diagonals, turns and stops. The gait observable is that `anim.locomotion` names the clip for each heading and band; the facing observable is that no body turns faster than the turn rate limit, outside the changes MOT-GAIT-017 exempts; the §8 metrics print | `mot-locomotion.sh`; `FalloutNV.esm` and its meshes and textures archives |
| S3 | **Degradation.** The same route on Oblivion: diagonals report Adapted (from Phase 3), starts and stops report Adapted through MOT-WRP-005, and no source lies outside the Oblivion archives | `mot-locomotion.sh`; `Oblivion.esm` and its archives |
| S4 | **Stairs and slopes.** A biped walks a generated staircase and ramp, up and down; foot height error, penetration and stair smoothness stay inside their bounds | ignored real-data test: generated collision, the New Vegas skeleton and walk clips |
| S5 | **Quadruped.** S4 with the Skyrim horse, whose body pitch also stays within the body tilt limit on the ramp, and whose drawn body stays within the body penetration limit on both fixtures | ignored real-data test: the Skyrim horse skeleton, body mesh and clips |
| S6 | **Ledges.** Obstacles inside the envelope are traversed; those too tall or with no room are refused with a reason. On each traversal the clip `anim.locomotion` names is the one playing, anchor error at each contact span (and hand error where arm IK runs) is inside its bound, and in first person the motion of MOT-TRV-011 is present | `cargo test`, synthetic collision; a smoke leg on New Vegas and one on Skyrim |
| S7 | **Nothing regresses.** A headless traversal route, driven by simulated time and checked against positions within stated tolerances, which stands in for the frame-counted `p1-character-traversal.sh`; and `p3-player-body.sh`, `m42-package-procedures.sh`, `m42-eat-sleep.sh`, `m42-oblivion-packages.sh` and `m42-skyrim-ambient.sh`. Each matches its recorded baseline | `docs/smoke-tests/`; the data each script names |
| S8 | **Behaviour graph.** An NPC walks, turns and stops: `anim.locomotion` names the clips the graph chose, graph switches are inside the discontinuity bound, and its walking speed is the clip's extracted speed | `mot-locomotion.sh`; `Fallout4.esm` and its archives |
| S9 | **Replay in the engine.** Two runs of one input log, seed set and load schedule, at two frame rates the route names, give equal hashes of capsule positions and MOT-SIM-002 state at every tick, with no dropped tick | `mot-locomotion.sh`; New Vegas and Fallout 4 data |
| S10 | **Reaction.** A scripted shove and a non-lethal hit on a walking actor move its torso and arms, leave its capsule path equal to that of a run without them, and return the pose within the return time; an actor without joint data reports Unserved, `no joint data` | `mot-locomotion.sh`; each wave 1 and wave 2 game |

A phase runs its observables on every game of the waves it names, on the cell
and route that Phase 0 names for each game and checks in with
`mot-locomotion.sh`; S2 to S10 name where a scenario's full check runs. An
ignored test counts only when run explicitly with `-- --ignored` where the data
is present: a run that skips, or finds data missing, is not a pass, and the
phase issue records the command and the commit. Baselines for S7 are recorded
over at least five runs; a leg that varies between them is judged by its
recorded spread, or is left out with the reason written down.

A scenario passes when:

1. Every observable in its row that the phase's exit names holds on a run at a
   named commit.
2. Every metric the phase's exit holds to a bound is inside it (§8.1).
3. No need variant of an actor in it lacks a disposition.

---

## 8. Metrics

Published in [ROADMAP.md § Compatibility matrix](../../ROADMAP.md#compatibility-matrix):
one movement entry per game, with pose cost and determinism as engine-wide
entries. Contact metrics are taken at each leg's contact point over the
reference windows of MOT-DATA-007.

| Metric | Definition | Granularity |
|---|---|---|
| Planted-foot slip | Horizontal distance the contact point moves during a plant window, per second planted, leaving out ticks counted under MOT-WRP-004; 95th percentile and maximum | per game, gait and skeleton family |
| Limited-tick share | Planted ticks counted under MOT-WRP-004 ÷ planted ticks | per game and gait |
| Switch discontinuity | Over the whole transition, the largest tick-to-tick change in any bone's angular velocity, and separately in any translated bone's linear velocity, each divided by the largest such change inside either clip alone, with a floor on the divisor. Switches with an authored "no blend" are counted apart. Rotation jumps that V17 finds to be decode faults are fixed first | per game and kind of switch |
| Foot height error | Vertical distance between a planted contact point and the collision surface; 95th percentile and maximum | per fixture and skeleton family |
| Swing penetration | Greatest depth of a swinging foot below the collision surface | per fixture and skeleton family |
| Stair smoothness | Largest tick-to-tick change in vertical velocity of the pelvis and knees on the stair fixture, beyond that of the clip on flat ground | per fixture and skeleton family |
| Warping error | Torso yaw error and leg twist on a diagonal route; stop-point error where distance matching runs | per game |
| Pose distortion | Largest joint-angle change that pose corrections add to the sampled pose | per layer and game |
| Body offset | Horizontal distance between the body and its capsule; maximum | per game |
| Facing turn rate | Largest change in a body's yaw over one tick, as degrees a second, leaving out the changes MOT-GAIT-017 exempts; maximum | per game, player body and NPCs apart |
| Body tilt error | Difference between body pitch and roll and the slope under the contacts | per fixture, for front-and-hind-legged families |
| Body penetration | Greatest depth of any vertex of the drawn body below the fixture's collision surface | per fixture, for the Skyrim horse |
| Anchor error | Distance between an anchored point (hand, foot or first-person camera track) and its obstacle anchor during a contact span | per game and traversal move |
| Input response | Ticks from an input change until capsule speed reaches 90 % of its new target, and until the presented pose, after interpolation, differs from the pose without the input by more than the response threshold; 95th percentile and maximum | per game, against the baseline |
| Disposition coverage | Need variants in each disposition ÷ all need variants, with Unserved split by reason | per game and skeleton family |
| Derived-record coverage | Locomotion-set clips with a record, split by authored or derived plant windows | per game |
| Pose cost | CPU time per skeleton, per evaluation and per second of play, at a stated actor count; mean and 95th percentile | per layer |
| Determinism | S1 and S9 pass or fail | engine |

### 8.1 Limits and when they are fixed

Each value is the owner's, recorded as a revision. Phase 0 fixes what offline
data supports. Any other limit is fixed after the phase that builds its
instrument has recorded a baseline, and before that phase tunes against it
(§10 Q10). A phase whose exit needs a limit that is not fixed does not close.

| Limit | Instrument built in | Fixed before tuning in |
|---|---|---|
| Input response bound and pose-change threshold | Phase 1 | Phase 1 |
| Window tolerance (MOT-DATA-002) | Phase 1 | Phase 1 |
| Switch discontinuity bound and divisor floor | Phase 2 | Phase 2 |
| Gait band rule and hysteresis (MOT-GAIT-002) | Phase 2 | Phase 2 |
| Turn rate limit, which may differ by speed band and for turning in place | Phase 2 | Phase 2 |
| Planted-foot slip bound | Phase 1 | Phase 3 |
| Sliding allowance on the limited-tick share | Phase 3 | Phase 3 |
| Warping error and pose distortion bounds; body offset limit | Phase 3 | Phase 3 |
| Pose correction limits (MOT-WRP-004) | the layer's phase | Phases 3, 4 and 6 |
| Foot height error, swing penetration and stair smoothness bounds | Phase 4 | Phase 4 |
| Body tilt limit | Phase 5 | Phase 5 |
| Body penetration limit | Phase 5 | Phase 5 |
| Traversal input, reachable envelope, traversal tick bound, anchor error bound | Phase 6 | Phase 6 |
| Quest-route check: quest set, method, and what a found sequence break does to the default | Phase 6 | before the check runs |
| Reaction return time | Phase 7 | Phase 7 |

---

## 9. Delivery sequence

A phase closes when its exit is measured on every in-scope game of the waves it
names. In place of a measurement, a need may be reported Unserved only for a
reason other than `not built` (§3.1): `not built` never closes a phase for a
need that one of the phase's MUST requirements serves. A SHOULD left unbuilt is
recorded with its reason in the phase issue. Every phase from 1 on also passes
S7.

**Phase 0 — Census, verification and baseline (no engine code).** Writes
`docs/engine/locomotion.md`, with the per-need fallback table (MOT-SUP-003);
checks in the census tool and its report (MOT-DX-005); closes the verify list
below or files each open item; writes `mot-locomotion.sh` and names each game's
cell and route; specifies the S7 route and records what S7 baselines today's
build can give; records the movement baseline at `fc766e09f` or later (§3.7
row 16), from existing `byro-dbg` queries or offline from clip data, and says
so for any metric that must wait; corrects what
[Animation § State machine](../engine/animation.md#state-machine-the-consumer-anchored-slice-ar-09--4128)
says about authored blend durations (§3.5); adds the ROADMAP milestone entry
with its milestone dependencies; files the issue for PHYSAL's active-ragdoll
slice, which Phase 7 needs; and has the owner fix the limits offline data
supports (§8.1). The derived motion data the later phases consume is engine
code and belongs to Phase 1. Exit: the generated census is checked in, every
verify item is closed or filed, the baseline table names its commit, and each
§8.1 limit is either in this PRD as a revision or listed with the phase that
must record its baseline first.

| # | **Phase 0: verify** | Blocks | Settled by |
|---|---|---|---|
| V1 | When `Enum: Left` / `Enum: Right` fire against foot contact, per KF game, and what share of locomotion clips carry them | MOT-DATA-002 | The census, comparing event times with foot-bone height; CS and GECK wiki |
| V2 | The same for `FootLeft` / `FootRight` on Skyrim and Fallout 4, and what the Skyrim horse's `FootFront` / `FootBack` mark | MOT-DATA-002, MOT-LEG-006 | As V1; CK and Fallout 4 CK wiki |
| V3 | The displacement source of every base-set clip | MOT-DATA-004 | The census; closed 2026-11-28 for every clip the engine can read |
| V4 | Skyrim's bound-animation text format, and how a block names its clip | MOT-DATA-004 | An external format description checked against the shipped files; else a declared gap |
| V5 | Skyrim LE's base set, events and displacement | §7.1 | The census on the LE archives; closed 2026-11-28: matches SE |
| V6 | Whether any slope or stair motion ships under another name (graph blend children, a pitch variable) | MOT-FT-002 | Behaviour-graph dumps per Havok game; the census per KF game |
| V7 | What the Fallout 4 vault clips are for, and which state plays them | MOT-TRV-003 | Graph dumps; Fallout 4 CK wiki |
| V8 | Whether the Fallout 3 and New Vegas climbing idles survive being re-anchored to other geometry | MOT-TRV-003 | Root and hand paths from the clips |
| V9 | Where Fallout 4 gets unarmed backward and strafing motion | MOT-GAIT-001 | Graph dumps and the census |
| V10 | What carries a transition duration for KF sequences: the units and meaning of the `Blend` keys and any related ones, what `m:L` / `m:R` are, and what a clip without a key uses | MOT-TRN-004 | CS and GECK wiki; the text keys and records in the data |
| V11 | Which jump, fall, land and crouch clips have contact spans an anchor can use | MOT-TRV-003 | The census |
| V12 | Whether each original adjusts feet to the ground | §6 row 2 | External sources (UESP, CK wikis) |
| V13 | What each original does at a jump toward a ledge, and what it shows in first person during such a move | §6 rows 3 and 7 | External sources |
| V14 | Each original's player acceleration and braking | §6 row 5 | External sources and the games' settings |
| V15 | Each original's non-lethal hit reaction | §6 row 6 | External sources and the games' settings |
| V16 | Which bones form the leg chains of each creature family | MOT-LEG-001 | The shipped skeletons |
| V17 | Whether the rotation jumps the Fallout 4 clip census flags are decode faults or authored motion | MOT-TRN-001 | The flagged tracks compared with their source control points |
| V18 | Which wave 1 and wave 2 games ship first-person clips a traversal can use | MOT-TRV-011 | The census on the first-person directories |
| V19 | What marks the capsule-driving spans of a KF clip | MOT-DATA-001, MOT-GAIT-007 | The census; CS and GECK wiki; else a labelled engine choice |
| V20 | Whether Oblivion ragdolls, whose constraints carry no motors, can be driven | MOT-PHY-003 | PHYSAL's decoder and nif.xml; else `no joint data` |
| V21 | Whether both clips play during a transition on Oblivion, Skyrim and Fallout 4 | §6 row 1 | External sources (UESP, CK wikis) |
| V22 | How fast each original turns the player's body and NPCs, in place and while moving | §6 row 8 | External sources and the games' settings |

**Phase 1 — Tick and motion data.** MOT-SIM-001 to -006 and -008,
MOT-SUP-001 to -004 and -007, MOT-DATA-001 to -005 and -007, MOT-LEG-001, -003
and -006 for bipeds and the Skyrim horse, MOT-MM-*, MOT-DX-001, MOT-DX-004.
Waves 1 and 2. Depends on Q1, Q2, V1 to V5, V19, and V16 for those skeletons;
Skyrim records depend on V4. Its first slice builds whatever S7 and the
instruments need that Phase 0 could not, and records their baselines on
per-frame movement; the tick follows. Exit: S1 and S9 pass; S7 matches; and
derived-record coverage and disposition coverage are published for both waves.

**Phase 2 — Gaits and transitions.** MOT-GAIT-001 to -008, -010 to -015, -017
and -018, MOT-TRN-*, MOT-SUP-005, -006, MOT-SIM-007, MOT-DATA-006,
MOT-DX-003, -006, -007. Waves 1 and 2. Depends on Q3, Q4, Q13, Q15, Q17, V9,
V10, V17, V21, V22; Skyrim gaits depend on V4. Exit: the gait-selection, facing
and switch-discontinuity observables of S2 and S3 pass, and S8 passes; S7
matches; and discontinuity,
slip (not yet held to a bound), input response and pose cost are published
against the baseline.

**Phase 3 — Warping and response.** MOT-WRP-001 to -003 and -005, the layer-4
part of MOT-WRP-004, MOT-GAIT-009 and -016. Waves 1 and 2. Depends on Q12,
V14; Skyrim warping depends on V4. Exit: S2 and S3 pass in full: across the
speed sweep, slip and the limited-tick share are inside their bounds; torso yaw
error and leg twist on the diagonal route, stop-point error where distance
matching runs, discontinuity at starts and stops, pose distortion and body
offset are inside theirs; and S7 matches.

**Phase 4 — Feet on stairs and slopes, bipeds.** MOT-FT-*, MOT-DX-002, and the
layer-5 part of MOT-WRP-004. Waves 1 and 2. Depends on Q16, V6, V12. Exit: S4
passes; foot height error, penetration and stair smoothness are inside their
bounds and published with pose cost; and S7 matches.

**Phase 5 — Creatures.** MOT-LEG-002, -004, -005, -007, and MOT-LEG-001, -003
and -006 for the other creature families. Waves 1 and 2. Depends on Q17, V16;
Skyrim creatures depend on V4. Exit: S5 passes; every creature family the census lists
passes its leg-chain check (each chain resolves on the shipped skeleton, and
each contact point lies within the foot height error bound of the ground plane
in the idle pose) or is Unserved for a reason other than `not built`; and S7
matches.

**Phase 6 — Short traversal.** MOT-TRV-*, and the layer-6 part of
MOT-WRP-004. Waves 1 and 2. Depends on Q5, Q6, Q11, V7, V8, V11, V13, V18.
Exit: S6 passes; each game's traversal source is published, and so is the check
of quest routes against reachable ledges (MOT-TRV-006); and S7 matches.

**Phase 7 — Partial physical reactions.** MOT-PHY-*. Waves 1 and 2. Depends on
Q9, V15, V20 and on PHYSAL's active-ragdoll slice (motors and joint limits).
Exit: S1 passes with reactions firing; S10 passes; and S7 matches.

---

## 10. Decisions required

1. **Q1 — Which clock moves characters?** Recommended: move the player
   controller, the NPC movers and every feedback layer onto the 60 Hz
   locomotion tick, with interpolated rendering, in Phase 1. It is what makes
   MOT-G-007 true and what every later measurement stands on. Alternative: keep
   per-frame movement and drop MOT-SIM-001, so trajectories keep depending on
   frame rate.
   **Decided** (2026-11-27): the recommendation. Movement runs on the 60 Hz tick with interpolated rendering (MOT-SIM-002, -003).

2. **Q2 — How strong a determinism promise?** Recommended: same machine and
   same binary. Rapier is locally deterministic by default when structures are
   built and inserted in the same order (Rapier user guide, *Determinism*,
   §11). Alternative: cross-platform, which the same page ties to the
   `enhanced-determinism` feature (it excludes `simd8`), IEEE 754-2008
   targets and restricted float functions during setup; the engine's own
   movement math would need the same audit.
   **Decided** (2026-11-27): the recommendation. Same machine and same binary (MOT-SIM-001).

3. **Q3 — Who owns displacement?** Recommended: the capsule. The body follows
   it through warping and damped, speed-capped correction, and prefers a little
   sliding to a distorted pose; marked spans (turns, traversal, graph-driven
   moves) let the clip drive. Alternative: clips drive the capsule, which looks
   grounded and makes control slower (Holden, *Code vs Data Driven
   Displacement*, §11).
   **Decided** (2026-11-27): the recommendation. The capsule owns displacement outside marked spans (MOT-GAIT-007).

4. **Q4 — May the engine choose a transition duration?**
   [Animation § State machine](../engine/animation.md#state-machine-the-consumer-anchored-slice-ar-09--4128)
   keeps classic-path switches as hard cuts because choosing a duration would
   be guessing. Recommended: reverse that for runtime feel, as the 2026-11-20
   direction implies; an authored value wins, and an engine value is labelled
   (MOT-TRN-004). Alternative: keep hard cuts on KF paths, which leaves
   MOT-G-002 unmet on the three wave 1 games.
   **Decided** (2026-11-27): the recommendation. An engine-chosen duration is allowed and labelled; an authored one wins (MOT-TRN-004). Confirmed by the owner on 2026-11-28 after review round 1 found the `Blend` keys of §3.5, which that stance did not allow for: the decision stands, and V10 settles what they author.

5. **Q5 — How far does climbing go?** Recommended: short traversal only (a
   waist or chest obstacle, a ledge in reach), from shipped clips and IK.
   Physics-synthesised climbing of long routes (Naderi and others, §11) waits
   for a later PRD: it is the most experimental technique in the research and
   the published planner runs offline. Alternative: include it as a last
   phase, with its own determinism and cost exit. Ladders are in neither
   option unless the owner adds them.
   **Decided** (2026-11-27): the recommendation. Short traversal only; ladders and long routes are out (MOT-NG-008).

6. **Q6 — Is traversal on by default?** Recommended: off by default until
   Phase 6 publishes a check of quest routes against reachable ledges, since a
   new way up can skip content. Alternative: on by default, accepting sequence
   breaks until reports find them.
   **Decided** (2026-11-27): the recommendation. Off by default until the Phase 6 check is published (MOT-TRV-006).

7. **Q7 — Fallout 76 and Starfield.** Between them they ship much of the
   richest clip material (§3.4, §3.6), and the engine reads neither format.
   Recommended: keep both readers out of this PRD; each gets its own design,
   and this PRD's requirements apply once a reader lands, through a revision.
   This also keeps compatibility ahead of polish on Starfield. Alternative:
   build the readers here, which adds two format projects ahead of any
   movement work for those games.
   **Decided** (2026-11-27): the recommendation. Both readers are out of this PRD (MOT-NG-011).

8. **Q8 — Motion matching and EA's patents.** Recommended: build no matcher in
   this PRD. The readable base sets hold few gait cycles (§3.4), so a search
   would have little to choose from, and warping serves the same gaps. MOT-MM-001 and
   MOT-MM-002 keep the phase data clear of the claimed shape. Alternative: add
   a locomotion-only matcher over the one scalar step phase, filed only after a
   legal review is recorded.
   **Decided** (2026-11-27): the recommendation. No matcher is built (MOT-NG-009); MOT-MM-001 and MOT-MM-002 apply.

   The independent claims were read from Google Patents on 2026-11-27 through
   an automated fetch; the US11995754B2 row was corrected on 2026-11-28 against
   its claim 1. **This is a reading of claim text, not a legal
   opinion**: claim construction, equivalents, most dependent claims,
   prosecution history, other EA families and other jurisdictions were not
   assessed. Anyone deciding Alternative should read the claims directly.

   | Patent | Each independent claim requires | Trained network | Recites a single global phase |
   |---|---|---|---|
   | [US11562523B1](https://patents.google.com/patent/US11562523B1/en) | several local bone phases tied to contacts; a local motion-matching process for each | no | no |
   | [US11995754B2](https://patents.google.com/patent/US11995754B2/en) (continuation) | several local bone phases tied to contacts are accessed; a local bone phase, encoded with position, orientation, velocity and acceleration, is matched to local pose data | no | no |
   | [US12403400B2](https://patents.google.com/patent/US12403400) | motion matching over a manifold of several local phase channels, one per body segment | no | no |
   | [US11670030B2](https://patents.google.com/patent/US11670030B2/en) | two neural networks over motion-capture video that predict a local phase | yes | no |

   None of the independent claims recites a single global gait phase, and none
   was read as expressly excluding one. In the first three, the element a
   one-scalar-phase matcher lacks on its face is the *plurality* of local
   phases. Per-foot contact features in a search are the nearest thing a
   classic matcher has to it, which MOT-MM-002 rules out.

9. **Q9 — How far do physical reactions go?** Recommended: partial reactions
   of torso and arms over animation (Phase 7); balance recovery and a fully
   simulated body wait for a later PRD. Alternative A: add balance recovery
   (SIMBICON, §11) as a further phase. Alternative B: move all reactions to a
   PRD of their own beside PHYSAL's active-ragdoll slice, which would withdraw
   §5.10, Phase 7 and the reaction need from this one.
   **Decided** (2026-11-27): the recommendation. Partial reactions only (MOT-NG-010).

10. **Q10 — How are metric bounds set?** Recommended: the owner fixes each §8
    bound at the Phase 0 exit, from the measured baseline and, for slip, from
    what the clip itself does when played at its authored speed; the bounds go
    into this PRD as a revision. Alternative: require only improvement over the
    baseline, which lets a small gain pass a visibly poor result.
   **Decided** (2026-11-27): the recommendation. The owner fixes the bounds at the Phase 0 exit, recorded as a revision. Amended by the owner on 2026-11-28, since some instruments are built later: Phase 0 fixes what offline data supports, and each other limit is fixed once its instrument has a baseline and before tuning (§8.1).

11. **Q11 — What does first-person climbing show?** Recommended: camera
    movement, hand movement or both, taken from the game's own clips. The
    default view hides the body (`player_body.rs`), so without it a climb looks
    like a gliding camera. Alternative: no first-person motion, recorded in §6
    as a limitation.
    **Decided** (2026-11-28): the recommendation (MOT-TRV-011, MOT-NG-005).

12. **Q12 — Do player speeds change to match the clips?** Recommended: keep
    today's capsule speeds until Phase 0 has measured every gait's authored
    speed, and limit how often a foot may be left to slide. Alternative: change
    the speeds now, which changes how control feels before anything is
    measured.
    **Decided** (2026-11-28): the recommendation (MOT-WRP-001). The owner revisits the speeds once Phase 0 has published the gait speeds; a change is a revision.

13. **Q13 — Which pose do actor colliders follow?** Recommended: blocking
    colliders follow the sampled pose, before layers 3 to 7, and separate hit
    colliders follow the final pose; the cost is a second collider set.
    Alternative: actor bone colliders stop blocking the player, and actor
    capsules block instead, which changes gameplay.
    **Decided** (2026-11-28): the recommendation (MOT-SIM-007, MOT-FT-008).

14. **Q14 — What if Skyrim's motion data cannot be read?** Recommended: keep
    walking at a speed labelled as an engine choice, with the need reported
    `no displacement source`. Alternative: serve no Skyrim gait until the data
    is read.
    **Decided** (2026-11-28): the recommendation (MOT-DATA-004).

15. **Q15 — Does the transition goal cover Fallout 4's behaviour graph?**
    Recommended: yes, measured without overriding the clips the graph chooses.
    Alternative: leave graph switches out of MOT-G-002.
    **Decided** (2026-11-28): the recommendation (MOT-G-002, MOT-TRN-001).

16. **Q16 — How strong are the stair, horse, reaction and slip promises?**
    Recommended: require smooth stairs, the horse's body tilt and partial
    reactions where ragdolls support them, and hold slip to its bound.
    Alternative: leave them optional or improvement-only, which lets stairs
    snap and a small gain pass.
    **Decided** (2026-11-28): the recommendation (MOT-FT-010, MOT-LEG-004, MOT-PHY-001, MOT-G-001).

17. **Q17 — Are turning speed and horse clipping part of the promise?**
    Recommended: yes. Limit how fast a body turns, never the camera or the
    movement direction, and check that the horse's drawn body stays out of the
    stairs and ramp. The research note names over-fast turning as a main cause
    of unrealism, and not clipping through stairs as part of the horse's
    minimum. Alternative: leave both out, with separate rotation settings
    (MOT-GAIT-008) and a feet-only horse check, which lets a body spin and lets
    a horse pass with its belly in a step.
    **Decided** (2026-11-28): the recommendation (MOT-GAIT-017, -018, MOT-LEG-007).

---

## 11. Document map

| Need | Go to |
|---|---|
| Clip runtime, blend stack, root motion, the hard-cut stance Q4 reopens | [Animation](../engine/animation.md) |
| Physics tick and player controller | [Physics](../engine/physics.md) |
| Ragdolls and the deferred motor slice | [PHYSAL](../engine/physal.md) |
| NPC movers, packages, walk clips | [NPC Spawn → AI Packages](../engine/npc-spawn-ai-packages.md) |
| NPC routes | [NAVM Pathfinding](../engine/navmesh-pathfinding.md) |
| Walk sources owned elsewhere | [AI Packages Completion](ai-packages-completion.md) |
| The sneak stance | [Sneak and Detection](sneak-and-detection.md) |
| Other sources of divergence between two runs | [P2P Co-op](p2p-coop.md#32-determinism-inventory) |
| Modded animation | [Legacy Mod Compatibility](legacy-mod-compatibility.md) |
| What each game ships and what the engine reads of it (generated) | [Locomotion clip census](../engine/locomotion-census.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

### Technique references

From the owner's research note of 2026-11-27. They inform the design and bind
no requirement to a method. Apart from the rows marked *read*, and the patents
in Q8, this draft did not re-fetch them.

| Area | References |
|---|---|
| Transitions | Bollo, [Inertialization](https://gdcvault.com/play/1025165/Inertialization-High-Performance-Animation-Transitions) (GDC 2018) and [Gears of War 4 talk](https://history.siggraph.org/wp-content/uploads/2022/09/2017-Talks-Bollo_High-Performance-Animation-in-Gears-of-War-4.pdf) (also foot-phase sync and anchor points); Holden, [Dead Blending](https://theorangeduck.com/page/dead-blending) |
| Warping, distance matching | Unreal, [Pose Warping](https://dev.epicgames.com/documentation/en-us/unreal-engine/pose-warping-in-unreal-engine); Delayen, [Paragon techniques](https://forums.unrealengine.com/t/animation-techniques-used-in-paragon-and-january-game-jam-theme-announcement-live-from-epic-hq/52339/28); Rosen, [procedural animation with few poses](https://gdcvault.com/play/1020049/Animation-Bootcamp-An-Indie-Approach) |
| Feet and stairs | Guerrilla, [Horizon traversal](https://www2.slideshare.net/guerrillagames/player-traversal-mechanics-in-the-vast-world-of-horizon-zero-dawn); Roche and Torres-Cros, [Fitting the World](https://gdcvault.com/play/1023009/Fitting-the-World-A-Biomechanical) (predictive IK); Hagdahl, [virtual ground plane](https://sites.google.com/view/realtime-animation-tips/tutorials/animation-stepping-stones); [procedural foot placement tutorial](https://docs.vulkan.org/tutorial/latest/Advanced_glTF/Procedural_Animation_IK/04_foot_placement.html) |
| Controller | Holden, [Code vs Data Driven Displacement](https://theorangeduck.com/page/code-vs-data-driven-displacement), [springs](https://theorangeduck.com/page/spring-roll-call), [fitting them](https://theorangeduck.com/page/fitting-code-driven-displacement); Rapier [character controller](https://rapier.rs/docs/user_guides/rust/character_controller/) and [determinism](https://rapier.rs/docs/user_guides/rust/determinism/) (*read*) |
| Climbing | van Grinsven, [Horizon traversal video](https://gdcvault.com/play/1024699/Player-Traversal-Mechanics-in-the); Ubisoft, [Assassin's Creed III](https://gdcvault.com/play/1018058); Techland, [Dying Light](https://gamedeveloper.com/design/video-how-techland-built-i-dying-light-i-s-remarkable-parkour-system); Naderi and others, [offline synthesis](https://history.siggraph.org/?p=102339), [learned climbing](https://diglib.eg.org/items/50f223e5-b9d3-4b7f-b596-b3b8640d0aa3/full), [reinforcement learning](https://acris.aalto.fi/ws/portalfiles/portal/38172752/SCI_Naderi_A_Reinforcement.2019.pdf) |
| Reactions | [Rockstar interview](https://wccftech.com/rockstar-euphoria-evolved-rdr2/); [Euphoria](https://en.wikipedia.org/wiki/Euphoria_(software)) (behaviour reference only); [SIMBICON](https://cs.ubc.ca/~van/papers/Simbicon.htm); [DReCon](https://theorangeduck.com/media/uploads/other_stuff/DReCon.pdf) |
| Motion matching | Clavet, [For Honor](https://www.gdcvault.com/play/1022985/Motion-Matching-and-The-Road); Naughty Dog, [The Last of Us Part II](https://gdcvault.com/play/1027118/Motion-Matching-in-The-Last); [Learned Motion Matching](https://www.theorangeduck.com/page/learned-motion-matching); [LaFAN1](https://github.com/ubisoft/ubisoft-laforge-animation-dataset) (lab only, MOT-NG-001) |
| Learned controllers (MOT-NG-002) | [PFNN](https://theorangeduck.com/page/phase-functioned-neural-networks-character-control) and its [licence](https://www.github.com/PapyKent/PFNN/blob/master/LICENSE.md); [DeepPhase](https://i.cs.hku.hk/~taku/deepphase.pdf); [AMP](https://arxiv.org/abs/2104.02180v2) |
| Creatures | Hecker and others, [retargeting to varied bodies](https://chrishecker.com/Real-time_Motion_Retargeting_to_Highly_Varied_User-Created_Morphologies); Zhang and others, [quadruped networks](https://history.siggraph.org/?p=100804); Frey, [quadrupeds on a budget](https://gamedeveloper.com/art/video-animating-pups-boars-other-quadrupeds-in-i-the-flame-in-the-flood-i-) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-27 | First draft: research note folded in; clip, event, displacement and climbing census; HEAD inventory; EA patent claim reading | — |
| 2 | 2026-11-27 | Q1 to Q10 decided as recommended and folded in (MOT-SIM-001 to -003, MOT-TRV-006, MOT-NG-008 to -011, §5.11, Phase 0 and Phase 6); facing row updated to `fc766e09f`. No separate rubric review round was run | owner: approved |
| 3 | 2026-11-27 | APPROVED → ACTIVE: tracking issues #7435 to #7443 filed, with Phase 0's verify items in #7436; §9 assigns the layer-5 and layer-6 parts of MOT-WRP-004 to Phases 4 and 6; no requirement changed | — |
| 4 | 2026-11-28 | Review round 1 folded in: KF `Blend` keys, Fallout 4 mover speed, census and engine-state corrections, the US11995754B2 summary; `not built` can no longer close a phase; metric definitions and the §8.1 limits table; traversal, stair, reaction, behaviour-graph and replay checks; collider, pose-layer, disposition and goal conflicts; phase order; adds MOT-SIM-007, -008, MOT-DATA-007, MOT-GAIT-014 to -016, MOT-FT-010, MOT-TRV-011, V18 to V21, S8 to S10. Promise changes follow the owner's 2026-11-28 acceptance of the review's recommendations (Q11 to Q16; Q4 and Q10), given before this wording was written. Left out as not covered by it: a turn-rate bound, toe and ankle foot pivots, a horse body-penetration check, and NPC footsteps | claude: revise |
| 5 | 2026-11-28 | At the owner's request, a turn rate limit (MOT-GAIT-017, -018) and a horse body-penetration check (MOT-LEG-007), recorded as Q17, with V22. The §3.4 to §3.6 tables give way to the generated census (MOT-DX-005), which adds the sprint footfall labels; V3 and V5 are marked closed. This wording, like revision 4's, has not been approved by the owner | claude: revise |
