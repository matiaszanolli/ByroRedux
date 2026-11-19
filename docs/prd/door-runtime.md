# Door Runtime — Product Requirements

**Status**: PROPOSED (2026-11-19). Revision 1, the first draft. It is the prerequisite that decision Q8 of [AI Packages Completion](ai-packages-completion.md#10-decisions-required) split out of that PRD: no in-cell door opens for anyone today. Statements about how an original engine, format or tool behaves cite external sources; a claim that could not be verified is marked **Phase 0: verify** instead of being stated as fact.

**Scope**: a placed door that is not a load door (a `DOOR` reference with no teleport, in any cell) gets an open state, an open and close motion, collision that follows the motion, a lock gate, its authored sounds, a prompt for the player, script parity and a saved state. Actors use the same door by raising the same activation; that behaviour is [AIP-DOOR-002…006](ai-packages-completion.md#511-door-passage-aip-door), not this document. Outside it: load doors (they work: `queue_door_transition`, [Cross-cell actor travel](cross-cell-actor-travel.md)), Oblivion-gate random teleports, door damage and destruction, lockpicking policy (`lockpick.rs` owns it), and navmesh door semantics ([NAVM Pathfinding §2](../engine/navmesh-pathfinding.md#2-scope)).

**Related**: this document states the *promise*; the designs below own the *how*. Every mechanism promised here has an owner in this table, or Phase 0 writes the missing design.

| Mechanism | Owning design |
|---|---|
| Open state, `ActivateEvent`, script effects (`SetOpenState`, `GetOpenState`) | [Scripting](../engine/scripting.md) |
| Motion source per game, translated once at the model boundary | [NIFAL](../engine/nifal.md) |
| Skyrim+ door motion through a behaviour graph event | [Animation](../engine/animation.md) |
| Keyframed collision that follows an animated node | [Physics](../engine/physics.md) |
| Saved state and its restore on load | [Save / load round trip](../engine/save-load-roundtrip.md) |
| Actors opening a door in their way | [AI Packages Completion — PRD](ai-packages-completion.md) (AIP-DOOR) |
| The door state machine, the motion translation and the swing | none yet: Phase 0 writes `docs/engine/door-runtime.md` |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119 meanings, as in the related designs. Requirement IDs use the `DRT-` prefix.

---

## 1. Product thesis

A cell in these games is a set of rooms joined by doors, and the player opens them. Today the engine loads every door as a piece of scenery with collision: the load doors work and every other door is a wall. In Oblivion that is 2,156 doors across 807 of 1,855 interior cells ([census](ai-packages-completion.md#511-door-passage-aip-door)), so a large share of Oblivion's interiors cannot be walked through, and no actor can cross them either. The Oblivion live gate found it by accident: after the walk faults were fixed, the Moth priest's route to a container ended at a closed door.

The runtime is for the player first and for actors second. One door state, one activation event and one motion serve both, so an actor opening a door is an `ActivateEvent` with the actor as activator, and the door neither knows nor cares who raised it.

The corollary that shapes every requirement: **a door has one state machine, and everything else is a consumer of it.** The scripts' `SetOpenState`, the player's prompt, an actor's package, the swing, the collision and the sound read and write that one state; no second door model grows beside it, and where the games differ (a keyframed sequence, a behaviour-graph event, an engine-driven swing) the difference is chosen once at the translation boundary, never branched on in a system.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **In-cell door** | A placed `DOOR` reference with no `XTEL` teleport. It leads nowhere; it opens a way inside its cell. |
| **Load door** | A `DOOR` reference with a teleport. Out of scope; already served by `DoorTeleport`. |
| **Door state** | `closed`, `opening`, `open` or `closing`. It is the value `GetOpenState` reports: 0 none, 1 open, 2 opening, 3 closed, 4 closing (GECK *GetOpenState*; `obscript_effects/getters.rs`). |
| **Default state** | The state a door returns to when its cell is unloaded and re-entered. Set by the reference's `Open by Default` flag (CS *GetDoorDefaultOpen*; CS and GECK *Reference*). |
| **Door motion** | How a door model moves between its two states: an authored sequence, a behaviour-graph event, or an engine swing (§3). |
| **Swing** | The motion an engine drives for a model that authors none: a rotation about the model's hinge. |
| **Activator** | Whoever raises the `ActivateEvent`: the player, an actor's package or a script. |

---

## 3. Core model

A door has one state (§2) and one motion. The motion is chosen at the model boundary from what the model authors, and the three classes are the whole vocabulary.

| Motion class | What the model authors | Plays as | Reported by |
|---|---|---|---|
| **Sequence** | A `NiControllerManager` with `Open` and `Close` sequences | The sequence on the object's animation player | `door.state` names the sequence |
| **Graph event** | A behaviour graph (`BSBehaviorGraphExtraData`) with an open event | The event on the object's graph | `door.state` names the event |
| **Swing** | Neither | A rotation about the hinge, over an engine duration | `door.state` says `swing` and its angle and seconds |

The inventory below is the state at HEAD on 2026-11-19.

| # | Part | Class | Today |
|---|---|---|---|
| 1 | Prompt for an in-cell door | player | none: `InteractionKind::Door` is built from `DoorTeleport` alone (`interaction.rs:1388-1393`) |
| 2 | Open state | state | `TwoStateActivator`, installed only by the Papyrus recogniser for `default2StateActivator` or by an ObScript `SetOpenState` (`translate/recognizers/two_state_activator.rs`, `obscript_effects/world_state.rs`) |
| 3 | `TwoStateTransitionBatch`, "the presentation boundary for model animation, collision, and audio" | state | no consumer in the binary: a transition flips a flag and nothing else (`vm_state.rs`) |
| 4 | An authored `Open` / `Close` clip | motion | catalogued as `NamedSequences` and "waits for its consumer" (`animation/named_sequences.rs`) |
| 5 | Skyrim door through a behaviour graph | motion | the graph runtime plays an event a script sends to the object (`scripting/animation_graph.rs`); activating a plain door sends none |
| 6 | Collision that follows a moving door | physics | keyframed bodies track the entity's `GlobalTransform` each frame (`physics/sync.rs`, `push_kinematic`); nothing moves a door |
| 7 | Lock gate | lock | `Locked` stops a player without a key or a pick (`interaction.rs:1554-1563`); the key is read from the player's inventory only |
| 8 | `DOOR` record | decode | a model-bearing placement base: `FNAM` flags and `SNAM` / `ANAM` / `BNAM` sounds are not read (`records/dispatch_world_placement.rs:27`) |
| 9 | Reference `Open by Default` (`ONAM`) | decode | not read |
| 10 | Saved state | save | `TwoStateActivator` is in the save registry and restored on load (`save_io/registry.rs`, `cell_loader/reference_state.rs`) |

Two model measurements bear on the motion classes (`nif_stats`, 2026-11-19): Oblivion's `dungeons\caves\cdoor01.nif` and `architecture\castle\castledoor01.nif` author a collision object and no controller block; Fallout New Vegas's `dungeons\facility\doors\fachallsmdoor01.nif` and `dungeons\vaultruined\doors\vdoorslidingrr02.nif` author a `NiControllerManager` and two `NiControllerSequence`s. Two samples per game are not a census; Phase 0 takes one (DRT-REC-003).

---

## 4. Goals and non-goals

### 4.1 Goals

- **DRT-G-001 — The player opens a door.** Every in-cell door in an interior cell of a supported game opens and closes on the player's activation, unless it is locked. Counted as in-cell doors that open in the live gate ÷ in-cell doors the census lists.
- **DRT-G-002 — One state, every consumer.** The player, a script and an actor change a door's state through the one activation, and all three see the same state.
- **DRT-G-003 — The door is not a wall once open.** An open door lets the character controller through, and a closed one stops it.
- **DRT-G-004 — Authored data first.** Where a model authors its motion or a record authors its sounds, those play; the engine's own swing is the fallback for a model that authors none.
- **DRT-G-005 — No second door model.** The state is the existing `TwoStateActivator` or its direct replacement; no parallel component carries open or closed.

### 4.2 Non-goals

- **DRT-NG-001.** Load doors, including cross-space routing for actors. They work and are owned by [Cross-cell actor travel](cross-cell-actor-travel.md).
- **DRT-NG-002.** Oblivion-gate and other random-teleport destinations (`TNAM`): a load-door feature.
- **DRT-NG-003.** Door damage, destruction and the Papyrus destruction stages.
- **DRT-NG-004.** Lockpicking policy and lock levels. `lockpick.rs` owns them; DRT only calls its gate.
- **DRT-NG-005.** The `Automatic Door` flag on an in-cell door. The sources describe it for a door that moves the player into another cell (CS *Door*); what it does for a door that leads nowhere is **Phase 0: verify** and no requirement waits on it.
- **DRT-NG-006.** Which doors an actor chooses to use or avoid when routing, including `Minimal Use`: AIP-DOOR-004, which this PRD feeds with the decoded flag (DRT-REC-001).

---

## 5. Requirements by area

### 5.1 Door state (`DRT-STATE`)

- **DRT-STATE-001.** Every in-cell door MUST carry the door state from spawn, not only after a script has touched it. Today the component appears only for a `default2StateActivator` script or an ObScript `SetOpenState` (row 2). Observable: a test that spawns a plain `DOOR` placement and finds the component, `closed`.
- **DRT-STATE-002.** The state MUST be four values, `closed`, `opening`, `open` and `closing`, and `GetOpenState` MUST read them as 3, 4, 1 and 2 (GECK *GetOpenState*, as `getters.rs` already maps `is_open` and `is_animating`). The existing two booleans MAY stay as the representation. Observable: the existing getter tests plus one per transition state.
- **DRT-STATE-003.** A door whose reference carries `Open by Default` MUST spawn `open`, and a door MUST return to its default state when its cell is unloaded and re-entered, whatever state it was left in (CS *GetDoorDefaultOpen*). Today `ONAM` is not read (row 9). Whether the return applies to a cell re-entered from a saved game, and to exteriors, is **Phase 0: verify**; decision Q1 sets the promise. Observable: a test that opens a door, unloads and reloads the cell, and reads `closed`.
- **DRT-STATE-004.** `SetOpenState`, `SetDefaultOpen`'s runtime read, and Papyrus `SetOpen` MUST drive the same transition as an activation: the motion plays and the collision follows (DRT-MOVE-001, -004). Today `SetOpen` changes the flag only (row 3). Observable: a test that raises `Effect::SetOpen` and sees the transition begin.

### 5.2 Activation and locks (`DRT-ACT`)

- **DRT-ACT-001.** An `ActivateEvent` on an unlocked in-cell door MUST start the opposite transition, from any activator; an event during a transition MUST be ignored rather than queued. Observable: a test with a player activator and one with an actor activator over the same door fixture.
- **DRT-ACT-002.** The player's prompt for an in-cell door MUST read `Open` while it is closed and `Close` while it is open (`ui_labels.rs` defines both). Today the door prompt exists only for a load door (row 1). Observable: `InteractionState::prompt_verb` over a door fixture in each state.
- **DRT-ACT-003.** A locked door MUST refuse an activator that carries no key and has no pick, as `activation_is_blocked` does for the player today, and the key MUST be read from the *activator's* inventory (row 7 reads the player's only). Observable: a test with a keyless actor refused and a keyed one admitted. Whether the original game lets an actor through a locked in-cell door by ownership or faction is **Phase 0: verify**; the CK says so for load doors only.
- **DRT-ACT-004.** A script's `OnActivate` MUST still run first and a script that blocks the default activation MUST keep the door shut, as the load-door path does (`interaction.rs:1722`). Observable: the existing blocked-activation test over a door fixture.

### 5.3 Motion and collision (`DRT-MOVE`)

- **DRT-MOVE-001.** On a transition the door MUST play the motion its model authors: the `Open` or `Close` sequence of its `NamedSequences`, or the open event of its behaviour graph. Observable: a test per class that the animation player or the graph receives the clip or the event.
- **DRT-MOVE-002.** A door model that authors neither MUST swing about its hinge. The hinge axis, the angle, the duration and the direction relative to the activator are **Phase 0: verify** (CS and GECK say only that animating doors "run their animation when activated"; a hinged door with no authored animation has no source). Until Phase 0 settles them the swing uses one labelled approximation (§6 row 1, decision Q2). Observable: a test that a swing door's rotation reaches the open pose over its duration and returns.
- **DRT-MOVE-003.** The motion class MUST be chosen once, at the model translation boundary, and the door systems MUST NOT read the game kind to pick it. Observable: a text-scan guard over the door systems, as AIP-WALK-004's guard over the walk systems.
- **DRT-MOVE-004.** A door's collision MUST follow its motion, so that the character controller passes an open door and stops at a closed one. The keyframed-body path exists (row 6); this requirement is that the door's collider is on the moving node. Observable: a physics test that walks a controller into a closed door fixture (stopped) and through the same fixture opened (passes).
- **DRT-MOVE-005.** A door MUST NOT close onto a body standing in its swept volume: while a capsule overlaps it the close MUST hold, and no body may be left embedded in a closed door. What the original game does is **Phase 0: verify**. Observable: a test that holds a capsule in the doorway during `closing` and finds the door `open` and the capsule free.

### 5.4 Sounds (`DRT-SND`)

- **DRT-SND-001.** The door's open sound MUST play when it starts opening and its close sound when it starts closing, through the M44 audio path at the door's position. The `SNAM` (open), `ANAM` (close) and `BNAM` (loop) sounds are decoded under DRT-REC-002. When the loop sound plays is **Phase 0: verify** (CS names it "Selects the looping sound" and nothing more). Observable: a test that a transition queues one sound at the door's position.

### 5.5 Record decode (`DRT-REC`)

- **DRT-REC-001.** The `DOOR` `FNAM` flags MUST decode per game onto the door component: Oblivion `Oblivion Gate`, `Automatic Door`, `Hidden`, `Minimal Use`; Fallout 3 and Fallout New Vegas `Automatic Door`, `Hidden`, `Minimal Use`, `Sliding Door`; Skyrim the same four as Fallout 3 (xEdit `wbDefinitionsTES4.pas:2170-2176`, `wbDefinitionsFO3.pas:4090-4098`, `wbDefinitionsFNV.pas:4584-4590`, `wbDefinitionsTES5.pas:4845-4852`). The bit positions come from those definitions, translated in `crates/plugin`. Observable: a fixture test per game that each flag lands in the component.
- **DRT-REC-002.** The `SNAM`, `ANAM` and `BNAM` sound references MUST decode onto the door component (xEdit `wbDefinitionsTES4.pas:2167-2169`, `wbDefinitionsFO3.pas:4087-4089`, `wbDefinitionsFNV.pas:4581-4583`, and `wbDefinitionsTES5.pas:4842-4844`, where they name `SNDR` sound descriptors, not `SOUN` records). Observable: a fixture test that the three form IDs reach the component.
- **DRT-REC-003.** A census MUST report, per game and over the in-cell door bases, how many author a `Sequence`, how many a `Graph event` and how many neither, from checked-in code, not from a script (the standard AIP-DX-003 sets). Observable: the example under `crates/nif/examples` and the table it prints, linked from §8.
- **DRT-REC-004.** The reference's `Open by Default` (`ONAM`, an empty subrecord in the Oblivion, Fallout 3, Fallout New Vegas, Skyrim and Fallout 4 definitions: `wbDefinitionsTES4.pas:3230` and the siblings) MUST decode onto the placement. Observable: a fixture test that a placement carrying `ONAM` reads open by default.

### 5.6 Saves (`DRT-SAVE`)

- **DRT-SAVE-001.** A door's state MUST be saved and restored, and a restored `open` door MUST appear in its open pose with its collision open, without replaying the swing. The state is already saved (row 10); the pose is not. Observable: a round-trip test that saves an open door, loads it, and finds the door node at the open pose and the controller passing.
- **DRT-SAVE-002.** Adding the door state MUST NOT change the saved shape if the existing `TwoStateActivator` carries it; if a field is added, `FORMAT_MAJOR` and the registry shape guard change in the same commit. Observable: the registry-completeness and shape-guard tests stay green.

### 5.7 Explainability (`DRT-DX`)

- **DRT-DX-001.** `door.state [<entity>]` MUST print, for the entity or for every in-cell door in the cell, its base form, state, default state, lock, flags, motion class with its sequence or event or swing parameters, and its sounds. Observable: a command test over a fixture of each motion class.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | The hinge, angle, speed and direction of a door with no authored motion are not documented in the sources read | One labelled approximation, replaced when Phase 0 sources the values (DRT-MOVE-002, Q2) | None | A door that swings the wrong way or at the wrong speed; DRT-MOVE-002 names the constants in one place and the Phase 0 verify item closes them |
| 2 | A door returns to its default state when its cell is unloaded and re-entered (CS *GetDoorDefaultOpen*) | The same, unless Q1 is decided the other way | None | A quest that left a door open loses it on re-entry; DRT-STATE-003 and Q1 |
| 3 | An actor may pass a locked door by ownership, faction or key (CK, for load doors) | The activator needs the key or a pick until Phase 0 sources the in-cell rule (DRT-ACT-003) | None | An NPC refused a door the original would open; the give-up path keeps it from freezing (AIP-DOOR-003) |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 1. The largest affected share (807 of 1,855 interior cells hold an in-cell door) and the live gate that found the gap; plain doors author no motion, so it exercises the swing |
| Fallout 3 | yes | Wave 1. Same record shape as Oblivion's plus `Sliding Door`; authored sequences on the animated doors |
| Fallout: New Vegas | yes | Wave 1. The reference title; its authored sequences exercise the `Sequence` class |
| Skyrim (LE / SE / AE) | yes | Wave 2. Doors move through a behaviour graph; the graph runtime exists (row 5) |
| Fallout 4 | yes | Wave 3, if decision Q3 holds. Graph doors; no census yet |
| Fallout 76 | no | Out of the sibling PRD's scope ([AIP-NG-005](ai-packages-completion.md#42-non-goals)) |
| Starfield | no | Census only (Q3): no door motion source has been read, and its animation text tables have no reader |

### 7.2 Acceptance

A scenario passes when:

1. **Fixtures:** the `cargo test -p byroredux --bin byroredux` cases named in §5 pass, with no game data.
2. **Oblivion live:** `docs/smoke-tests/door-runtime.sh oblivion` (written in Phase 1; data: Oblivion) boots `TempleMothCatacombs02`, activates the door at the Moth priest's route and finds it `open` with the controller passing, and `m42-oblivion-packages.sh` with `STRICT_FIND=1` sees the priest reach and activate a container.
3. **FNV live:** the same script with `fnv` opens one authored-sequence door and one swing door.
4. **Skyrim live:** the same script with `skyrim_se` opens one graph door (Phase 2).
5. **Saves:** the round-trip test of DRT-SAVE-001 passes, and `Locked` doors stay shut in each live run.

---

## 8. Metrics

The numbers live in ROADMAP's [M42 row](../../ROADMAP.md#tier-7--deep-gameplay-systems) while this PRD serves M42, and in a new row of its own if the milestone changes (decision to take at approval).

| Metric | Definition | Granularity |
|---|---|---|
| Door motion coverage | in-cell door bases by motion class (`Sequence`, `Graph event`, none) ÷ in-cell door bases, from the DRT-REC-003 census | per game |
| Doors opened live | in-cell doors a live gate finds `open` ÷ in-cell doors the gate activates | per game, per gate |
| Actor passage | `m42-oblivion-packages.sh` runs that end with a container activation ÷ runs | per run |

---

## 9. Delivery sequence

**Phase 0 — Census, sources and the design (no engine code).** The DRT-REC-003 census over the five games; the verify items named in §5 (the hinge and its timing, the closing rule, the loop sound, the default state against a saved game, the locked in-cell door, the in-cell `Automatic Door`), each settled from a CS, GECK or CK page, a UESP page or an observation of the original game, or filed as an issue; `docs/engine/door-runtime.md` written, with the state, motion and swing and the choice between `TwoStateActivator` and a replacement (DRT-G-005). Decisions Q1 to Q3 taken. Exit: the census is checked in and its table is linked from §8, every verify item is closed or filed, and the design links from §3.

**Phase 1 — State, activation, scripts and saves (wave 1).** DRT-STATE-001…004, DRT-ACT-001…004, DRT-REC-001, -002, -004 and DRT-SAVE-001, -002, with the swing (DRT-MOVE-002…005) and the sound (DRT-SND-001); Oblivion, Fallout 3 and Fallout New Vegas. Milestone dependencies: M28.5 (the character controller), M44 (audio), M47.2 (scripts and the activation event). Exit: `door-runtime.sh` passes on Oblivion and FNV, `m42-oblivion-packages.sh` passes with `STRICT_FIND=1`, and every fixture test in §5.1 to §5.6 passes.

**Phase 2 — Sequences and Skyrim (wave 2).** DRT-MOVE-001 for the `Sequence` and `Graph event` classes and DRT-DX-001. Exit: `door-runtime.sh skyrim_se` opens a graph door and the FNV leg opens a sequence door.

**Phase 3 — Fallout 4 (wave 3).** The Fallout 4 graph doors, if decision Q3 holds. Exit: a Fallout 4 live leg opens a graph door, or the phase is withdrawn with the census figure that shows why.

---

## 10. Decisions required

Each decision needs the project owner. The recommendation is the first option.

1. **Q1 — Door state across a cell unload.** The CS says a door returns to its default state when its cell is unloaded and re-entered, regardless of the state it was left in (*GetDoorDefaultOpen*). Recommended: follow it for a cell re-entered in the same session, and restore the saved state when the cell comes back from a save (saved games keep what the player did). A door a quest opened then closes again on re-entry, as in the original. Alternative: keep the state across unloads. Doors stay as left, which matches how containers and other reference state already persist, and which no source says the original does.
2. **Q2 — The swing before the original's timing is sourced.** Oblivion's plain doors author no motion (§3), and no source read gives the hinge, angle or speed. Recommended: ship one labelled approximation now (DRT-MOVE-002, §6 row 1) and replace it when Phase 0 settles the values; the doors then open and close with a visible swing. Alternative A: swap a door between its closed and open poses instantly, with no swing. It needs no invented numbers, and costs the visible motion and the sound's timing. Alternative B: defer plain doors until the values are sourced. Oblivion's actors and player stay behind closed doors until then.
3. **Q3 — Fallout 4 and Starfield.** Recommended: Fallout 4 in wave 3 (its graph doors reuse the Skyrim path) and Starfield census-only, as the cross-cell travel PRD treated it. Alternative: both out for now. The wave-3 phase disappears and Fallout 4 doors stay as they are.

---

## 11. Document map

| Need | Go to |
|---|---|
| What actors do at a door | [AI Packages Completion — PRD](ai-packages-completion.md#511-door-passage-aip-door) |
| The census of in-cell doors by master | [AI Packages Completion — PRD](ai-packages-completion.md#511-door-passage-aip-door) |
| Activation, script effects and the open state | [Scripting](../engine/scripting.md) |
| Keyframed collision | [Physics](../engine/physics.md) |
| How a model's motion is translated | [NIFAL](../engine/nifal.md) |
| Saved state | [Save / load round trip](../engine/save-load-roundtrip.md) |
| Load doors and cross-space routes | [Cross-cell actor travel](cross-cell-actor-travel.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-19 | First draft, from decision Q8 of the AI packages PRD: the HEAD state of in-cell doors, the three motion classes, seven requirement areas, three decisions | — |
