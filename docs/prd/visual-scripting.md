# Visual Scripting — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in
review round 1 (graph runs bounded in full by the frame budget, with the
ObScript VM's unbudgeted state stated; the event-marker HEAD claim
corrected for the ObScript consumer and V1 closed from code; a misquoted
Unreal sentence, the Havok Behavior quote and the "scripting is text"
claim re-cited; Q1's third-evaluator alternative cited as decided rather
than offered; one manifest entry shape shared with the LLM Dialogue
Plugin PRD; World Editor links re-pointed at WED IDs). Revision 1 was the
first draft of the M81
promise: behavior-tree-style script graphs, authored without code, that
attach to placed references and quests, run on the M47.0 event hooks and
lower onto the engine's existing ECS effect runtime rather than a new
interpreter. Statements about how an original engine, editor, paper or
external tool behaves cite external sources (the Creation Kit wikis under
`/mnt/data/src/reference/`, the published behavior-tree literature, Epic's
Unreal Engine documentation); a claim that could not be verified is marked
**Phase 0: verify** instead of being stated as fact.

**Scope**: M81 promises mod authors who do not write code a way to give a
placed reference, a base object or a quest scripted behaviour: a tree of
nodes rooted at an engine event, built from conditions, actions, waits and
control nodes, saved as a data file in a mod package, checked when it
loads, run by the scripting runtime that already runs lowered Papyrus, and
carried across saves. The node vocabulary is the surface the engine already
models for Papyrus: the delivered events, the canonical effects and the
condition functions. In scope: the graph file, its node catalog, the
load-time lowering and admission check, attachment, execution under the
interpreter's frame budget, saved state, calls into executable extensions,
the authoring surface inside the M50 editor, and diagnostics. Out of scope:
a new script language or evaluator, replacing or converting Papyrus and
ObScript content, authoring package procedure trees, Story Manager trees or
scenes as records, animation graphs, per-frame-ticked reactive trees in the
first release, and Fallout 76 (§4.2).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Event markers, frame lifecycle, condition evaluator, Papyrus VM, frame budget, reference-script delivery | [Scripting](../engine/scripting.md) |
| Event-hooks runtime (the layer M81 sits on) | [M47.0 Event Hooks Design](../engine/m47-0-design.md) |
| Recognizer chain, canonical effect lowering, decline-on-unmodeled posture | [M47.2 Scripting Runtime Design](../engine/m47-2-design.md) |
| Mod packages, content artifacts, attachments, capabilities, extension state | [Sandboxed Linked Mods](../engine/sandboxed-linked-mods.md) |
| SDK script functions published by extensions, manifests | [SDK v0.1 Development Plan](../engine/sdk-v0.1-development-plan.md) |
| Saved stores, schema fingerprint, `FORMAT_MAJOR` discipline | [Save/Load Round-Trip](../engine/save-load-roundtrip.md) |
| Authoring surface, edit commands, hot reload | [In-Engine World Editor PRD](world-editor.md) |
| Package procedure trees (precedent, not a target) | [PACKAL](../engine/packal.md) |
| Story Manager node trees (precedent, not a target) | [Story Manager](../engine/story-manager.md) |
| Mechanism dispositions and the preflight report | [Legacy Mod Compatibility PRD](legacy-mod-compatibility.md) |
| The other manifest content-artifact consumer (model artifacts), whose entry shape graph files share (VSCR-AST-008) | [LLM Dialogue Plugin PRD](llm-dialogue-plugin.md) |
| Graph file format, node catalog, lowering, attachment, run tracing | none yet: **Phase 0 writes `docs/engine/visual-scripting.md`** |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `VSCR-` prefix.

---

## 1. Product thesis

The ROADMAP row is the owner's framing: a "behavior-tree node graph layered
on M47.0 event hooks, for modders who don't write code", with the "same
surface as Papyrus would expose, but with no language to learn", that
"complements rather than replaces M47.0 / M47.2"
([ROADMAP Tier 10](../../ROADMAP.md#tier-10--ecosystem-unlock-post-tier-4-horizon)).
Tier 10 carries its own gate: "No active work until the engine ships
something playable for the existing community to mod." This PRD is Phase 0
work only until that gate opens (§9).

Who it is for:

1. **Authors who already build logic without code.** The Creation Kit's
   package system is a node tree authored in a GUI: "A Procedure Tree is a
   stack of Branches (or 'Nodes') that contain procedures", and "Because
   Branches and Procedures can all have Conditions, you can create very
   complex switches to route AI behavior through"
   ([CK: Packages](https://ck.uesp.net/wiki/Category:Packages), local dump
   `ck-uesp-wiki/Category/Packages.wiki`). Its branch types, Random,
   Sequence, Simultaneous and Stacked, are behavior-tree composites under
   other names ([CK: Package Templates § Branches](https://ck.uesp.net/wiki/Category:Package_Templates),
   `ck-uesp-wiki/Category/Package Templates.wiki`). The Story Manager is a
   second such tree, of "Branch nodes … and Quest nodes", each "either
   stacked … or random"
   ([CK: Bethesda Tutorial Story Manager](https://ck.uesp.net/wiki/Bethesda_Tutorial_Story_Manager)).
   Gameplay scripting, by contrast, is authored as text in both editor
   generations: "A papyrus script is essentially a plain text source
   document ('''.psc''' file) that you can use any text editor to write and
   compile" ([CK: Papyrus Introduction](https://ck.uesp.net/wiki/Papyrus_Introduction),
   `ck-uesp-wiki/(main)/Papyrus Introduction.wiki`), and a GECK script has
   "source code" whose comments are "ignored by the compiler"
   ([GECK: Scripting for Beginners](https://geck.uesp.net/wiki/Scripting_for_Beginners),
   `geck-uesp-wiki/(main)/Scripting for Beginners.wiki`). The engine's own
   account of the three generations is [Scripting § Three Generations](../engine/scripting.md#three-generations-of-bethesda-scripting).
2. **Authors of small behaviours.** A lever that opens a gate once the
   quest is at stage 20, a container that gives a note on first open, a
   trigger that shows a message: on Skyrim and later each is a `.psc`
   source compiled into a `.pex` (same CK page).
3. **The project.** A graph is checked whole when it loads and runs
   through the same effect runtime as lowered Papyrus, so a graph that is
   accepted is also a test of that runtime.

Behavior trees fit because they are "a way to structure the switching
between different tasks in an autonomous agent" that is "both modular and
reactive" (Colledanchise and Ögren, *Behavior Trees in Robotics and AI: An
Introduction*, CRC Press 2018, [arXiv:1709.00084](https://arxiv.org/abs/1709.00084)),
and commercial engines ship them as designer tools: Unreal's Blueprints use
"a node-based interface to create gameplay elements", which "provides the
ability for designers to use virtually the full range of concepts and tools
generally only available to programmers" ([Unreal: Blueprints](https://dev.epicgames.com/documentation/en-us/unreal-engine/blueprints-visual-scripting-in-unreal-engine)),
and its behavior trees "are event-driven to avoid doing unnecessary work
every frame" ([Unreal: Behavior Tree Overview](https://dev.epicgames.com/documentation/en-us/unreal-engine/behavior-tree-in-unreal-engine---overview)).

The corollary that shapes everything below: **a graph is data that lowers
to the effects the engine already runs; it is never a program the engine
interprets.** The keep list fixes scripting as "ECS-native; bounded
interpreter fallback under a per-frame statement budget", and the
anti-scope rules out cloning the Papyrus VM
([ROADMAP § Architecture Decisions](../../ROADMAP.md#architecture-decisions),
[anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)). So the
engine will not add a third evaluator for graphs, will not run a graph it
cannot lower whole, will not run a graph outside the per-frame statement
budget, will not tick idle graphs every frame, and will not
offer a node that the effect runtime cannot already perform or that the
Phase 0 design does not add to it.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Script graph** | One authored tree of nodes with one event root, stored in a graph file. "Behavior graph" is not used: the Creation Kit uses it for an actor's Havok animation graph ([CK: Race](https://ck.uesp.net/wiki/Race), "Behavior Graph: points to the actor's behavior graph HKX file"). |
| **Graph file** | The on-disk document holding one or more script graphs, their properties and their attachments (VSCR-AST). |
| **Node** | One element of a graph: an event root, a control node, a condition, an action or a wait (§3.1). |
| **Event root** | The node a run starts from: one event the runtime delivers (VSCR-EVT-002). |
| **Run** | One execution of a graph from its event root, started by one delivered event. A run that reaches a wait suspends and resumes on a later frame. |
| **Lowering** | The load-time translation of a graph into a list of `Effect` values (`crates/scripting/src/translate/effects/model.rs`), the canonical effect IR lowered Papyrus already produces. |
| **Admission** | The load-time check that a graph lowers whole; a graph that fails is refused, never run in part (VSCR-LOW). |
| **Node catalog** | The machine-readable list of nodes the engine offers, per game, each tied to the IR element it lowers to (VSCR-CAT). |
| **Graph property** | A named, typed input a graph declares (a form, a quest, a number), given a value where the graph is attached. Resolved the way a VMAD property is (VSCR-AST-006). |
| **Graph variable** | A named value a run writes and a later run reads; saved (VSCR-SAV). A *run local* lives for one run only (`Effect::SetLocal`). |
| **Attachment** | The declaration that binds a graph to a target: a placed reference, a base object or a quest. The sandbox design's *attachment* ([§3](../engine/sandboxed-linked-mods.md#3-terminology)). |
| **Frame budget** | `VmFrameBudget`, the interpreter's per-frame statement allowance ([Scripting § The frame budget](../engine/scripting.md#the-frame-budget-m472-5763)). |

---

## 3. Core model

### 3.1 The node vocabulary

The vocabulary is closed. Behavior-tree semantics are Colledanchise and
Ögren's classical formulation: a node "is executed if and only if it
receives ticks" and returns *Running*, *Success* or *Failure*; a Sequence
"returns Success if and only if all its children return Success"; a
Fallback "returns Failure if and only if all its children return Failure";
a Condition "never returns a status of Running" (section "Classical
Formulation of BTs", [arXiv:1709.00084](https://arxiv.org/html/1709.00084v6)).

| Node kind | BT meaning | Creation Kit counterpart ([Package Templates § Branches](https://ck.uesp.net/wiki/Category:Package_Templates)) | Lowers to (at HEAD on 2026-11-01) | In v1 |
|---|---|---|---|---|
| Event root | Starts a run | Package Begin/End/Change fragments, script `Event` blocks | A delivered event (VSCR-EVT-002) | yes |
| Sequence | All children in order; stops at the first that fails | **Sequence**: "do all branches and procedures in a sequence" | Ordered effects; a failing condition child becomes the guard of an `Effect::Conditional` over the rest | yes |
| Fallback | First child that does not fail | **Stacked**: "picks the first valid branch or procedure" | `Effect::Conditional` with an else chain | yes |
| Random | One valid child, chosen at random | **Random**: "pick one and only one valid branch or procedure" | `ValueExpr::RandomInt` into a run local, then `Effect::Conditional` on `FragmentGuard::Compare`, whose own doc names `Utility.RandomInt(1, 3) == 1` (`model.rs:128`) | SHOULD |
| Parallel | Children together; threshold *M* | **Simultaneous**: "finished when any of its child procedures or branches is completed" | Nothing: the effect IR has no concurrent branch | no (VSCR-NG-006) |
| Condition | Succeeds or fails, never runs | Conditions on a branch or procedure | A guard (decision Q3) | yes |
| Action | Does one thing | A procedure | One `Effect` variant, or `Effect::ProviderCall` into an extension (VSCR-EXT) | yes |
| Wait | Runs until a time passes or a condition stops holding | The **Wait** procedure ([CK: Wait (Procedure)](https://ck.uesp.net/wiki/Wait_(Procedure))) | `Effect::Wait`, `Effect::WaitWhile` | yes |
| Set variable | Writes a value | Package data | `Effect::SetLocal` (run local); `Effect::SetQuestVariable` / `ModQuestVariable` (saved) | yes |
| Subgraph | A named tree reused by reference | Package Template | Inlined at lowering; no call at run time | SHOULD |
| Repeat / loop decorator | Re-ticks a child | — | Nothing: the effect IR has no loop | no (VSCR-NG-006) |

### 3.2 Status semantics in v1

In the effect IR an action reports nothing back: effects are applied and
logged, never failed (`apply_effects`, `crates/scripting/src/fragment/effects.rs:295`,
at HEAD on 2026-11-01). A v1 graph is therefore a behavior tree whose
actions always succeed or run; only conditions fail. It is close to the
subset the book uses in section 11.2, "How BTs Generalize Decision Trees"
(chapter 2), whose leaves are "Action nodes always returning Running",
and it shares that construction's cost: "no information flows out of the
actions". Two consequences are part of the promise:

| Consequence | Stated as |
|---|---|
| A Fallback whose child begins with an action never tries a later child | VSCR-LOW-006 reports it as an authoring warning |
| A failed action (an item the container does not hold) does not divert the tree | The run trace shows the effect's log line (VSCR-DX-003) |

Reactivity in the classical formulation comes from "the continual
generation of ticks" (section 12.1, "Advantages", the *Reactivity*
paragraph). v1 runs are started by events, as
Unreal's are; continual ticking is decision Q2.

### 3.3 What the graph lowers onto

The inventory below is the state at HEAD on 2026-11-01. It is
hand-written; Phase 0's catalog generator (VSCR-CAT-001) replaces the
"Today" column for rows 2–4, and the column is not maintained by hand after
that.

| # | Piece | Role for M81 | Today |
|---|---|---|---|
| 1 | Event markers and delivery | Event roots | Markers in `crates/scripting/src/events.rs`, raised with no game gate (the player's activation: `emit_activate_event`, `byroredux/src/interaction.rs:1585`). Two consumers read them: `reference_script_event_system` (`fragment/reference.rs:254`) turns them into the named Papyrus events listed in `DELIVERED_EVENTS` (`fragment/reference.rs:63`) for `PapyrusReferenceScripts`, and `obscript_object_system` (`obscript_objects.rs:247`) runs the matching ObScript blocks (`OnActivate` and the rest) on FO3, FNV and Oblivion references |
| 2 | Canonical effect IR | Actions, waits, control | `pub enum Effect` (`translate/effects/model.rs:337`); serde-derived and saved, so "a shape change is a `FORMAT_MAJOR` question" (same file, module comment) |
| 3 | Guards | Conditions | `FragmentGuard` (same file) reads quest, objective, global, quest-variable and a set of actor and reference state; the M47.1 CTDA catalog is a separate enum, `ConditionFunction` (`condition/catalog.rs:23`), with `is_modeled` (`condition/eval.rs:95`); `Effect::Conditional` takes `FragmentGuard`s only |
| 4 | Object naming in effects | Graph properties, the owner, event arguments | `ObjectRef` is `Property(name)` resolved through a VMAD, or `Linked` (`translate/compose.rs:73`); `Self` as an object is declined at lowering (`translate/effects/receivers.rs:348`), and only a quest has a self reference (`QuestRef::SelfRef`, `compose.rs:48`); event arguments (`akActionRef`) reach only an `Effect::Interpret` run's `args` (`model.rs:566`) |
| 5 | Effect runtime | Runs a lowered graph | `apply_effects` walks nested `Conditional`s with an explicit cursor stack (`fragment/effects.rs:295`); `MAX_CONDITIONAL_DEPTH` bounds nesting at lowering (`translate/effects/lower.rs:411`) |
| 6 | Latent continuation | Waits | `FragmentExecutionQueue` (`fragment/state.rs:800`), saved (`byroredux/src/save_io/registry.rs:556`), capped at `MAX_FRAGMENT_CONTINUATIONS` |
| 7 | Frame budget | Bounds graph cost | `VmFrameBudget` (`fragment/budget.rs`), renewed by `event_cleanup_system` (`cleanup.rs:102`); only Papyrus VM runs charge it (`budget.charge(vm.spent)`, `fragment/vm/interp.rs:146`). Lowered effect lists are not charged, and neither is the ObScript VM (`obscript_vm.rs` reads no `VmFrameBudget`; its bound is per-run nesting, `MAX_OBSCRIPT_VM_NESTING`, `obscript_vm.rs:126`) |
| 8 | Admission precedent | Whole-graph check | `check_reference_script` (`fragment/vm/checker.rs:72`): a script runs only if every handler of the delivered events runs whole; `Err` carries the first reason |
| 9 | Owner-keyed saved variables | Graph variables | `QuestScriptVariables` and `QuestScriptObjects` (`quest_variables.rs:175`), keyed by the owning quest's or reference's FormID, saved (`save_io/registry.rs:425`, `:456`); read live by `script.var` (`byroredux/src/commands/script_var.rs:213`) |
| 10 | Extension calls | Action nodes owned by mods | `FragmentProviderCall { route, arguments, principal }` (`model.rs:301`) carrying SDK `ScriptValue`s (`crates/sdk/src/script_function.rs`) |
| 11 | Data-only packages | Shipping a graph | `ExtensionManifest` has no content-artifact field and refuses a package with no component (`ManifestError::MissingComponent`, `crates/sdk/src/manifest.rs:304`) |
| 12 | Portable form identity | Form references in a graph | `FormRef { source, local }`, load-order independent (`crates/sdk/src/identity.rs:238`) |
| 13 | Authoring surface | The editor | None for graphs. The debug UI's Studio tab edits transforms and materials through `StudioCommand`s (`crates/debug-ui/src/panels/studio.rs:70`); M50 is PROPOSED ([World Editor PRD](world-editor.md)) |
| 14 | Graph file, loader, catalog | — | None: no graph or behavior-tree code exists in `crates/` or `byroredux/` |
| 15 | Original procedure-tree structure | Precedent only | `PackRecord` decodes leaf procedures; "Structural Sequence/Simultaneous/Stacked branches remain implicit" (`crates/plugin/src/esm/records/misc/pack.rs:124`) |

### 3.4 How a graph is reported

Every graph a profile names is reported in one of four states, in the load
log and by `graph.list` (VSCR-DX-001):

| State | Meaning |
|---|---|
| **active** | Admitted and attached to at least one target |
| **refused** | Failed admission; the report names the node and the reason |
| **unattached** | Admitted, but no attachment target exists in the profile |
| **unavailable** | Uses a node the game in the profile does not offer (VSCR-CAT-004) |

Graphs are one mechanism in the legacy-mod catalog sense: a graph file is
Redux-native content, disposition D1 ([LMC § 3](legacy-mod-compatibility.md#3-the-disposition-model)),
and the four states above are its per-mechanism report.

---

## 4. Goals and non-goals

### 4.1 Goals

- **VSCR-G-001 — Behaviour without code.** Each graph in the Phase 1
  fixture corpus (§7.2), built only from catalog nodes, produces in a
  vanilla cell the quest, inventory and message effects its expectation
  file names, with no Papyrus or Rust written for it.
- **VSCR-G-002 — No third evaluator.** Every admitted graph runs as `Effect`
  lists through `apply_effects`; the count of graph-specific interpreters
  is zero.
- **VSCR-G-003 — Explained refusal.** Every refused graph in the refusal
  corpus names the node and the reason.
- **VSCR-G-004 — Saved like scripts.** Every suspended run in the save
  fixtures resumes after a save and load with the same result as an
  uninterrupted run.
- **VSCR-G-005 — Bounded.** Every graph run in the A4 fixtures is charged
  to `VmFrameBudget`, and with the budget spent no graph run starts in that
  frame (VSCR-RUN-002, VSCR-RUN-003, VSCR-RUN-008).
- **VSCR-G-006 — Cross-game.** One graph file whose nodes are all offered
  on two games runs on both unchanged.

### 4.2 Non-goals

- **VSCR-NG-001.** A new script language, a new bytecode or a graph
  interpreter. The keep list and the anti-scope decided this
  ([ROADMAP § Architecture Decisions](../../ROADMAP.md#architecture-decisions));
  decision Q1 records the alternatives.
- **VSCR-NG-002.** Replacing or retiring Papyrus, ObScript or the M47.2 /
  M47.3 runtimes. Legacy scripts keep running through the recognizer chain
  and the VMs; a graph attached beside one runs in addition (VSCR-EVT-005).
- **VSCR-NG-003.** Converting existing `.pex` or `SCTX` scripts into graphs.
  The VM runs loops and arrays the effect IR cannot hold
  ([Scripting § The Papyrus VM](../engine/scripting.md#the-papyrus-vm-m472)), so a conversion
  would be partial.
- **VSCR-NG-004.** Authoring `PACK` procedure trees, Story Manager node
  trees or `SCEN` scenes. Those are records with their own runtimes
  ([PACKAL](../engine/packal.md#2-two-drivers-one-core), [Story Manager](../engine/story-manager.md));
  a graph may start or stop them through the effects that already do so
  (`StartScene`, `EvaluatePackage`, `SendStoryEvent`).
- **VSCR-NG-005.** Animation graphs. Havok Behavior graphs drive animation,
  and the animation events scripts receive are, in the CK's words, "set up
  by the artist in Havok Behavior"
  ([CK: Differences from Previous Scripting](https://ck.uesp.net/wiki/Differences_from_Previous_Scripting),
  `ck-uesp-wiki/(main)/Differences from Previous Scripting.wiki`);
  a graph reacts to their events only through `OnAnimationEvent`, which the
  runtime already delivers.
- **VSCR-NG-006.** Parallel nodes and loops in v1. The effect IR has no
  concurrent branch and no loop (§3.1); adding either is a change to the
  runtime the keep list protects, and would need its own decision. Unreal
  makes a similar cut for the same event-driven reason: "Instead of complex
  parallel nodes, Unreal Engine Behavior Trees use Simple Parallel nodes"
  ([Unreal: Behavior Tree Overview](https://dev.epicgames.com/documentation/en-us/unreal-engine/behavior-tree-in-unreal-engine---overview)).
- **VSCR-NG-007.** Native or WebAssembly code inside a graph. Code ships in
  an extension component and is called through a script function
  (VSCR-EXT); a graph file is data.
- **VSCR-NG-008.** Per-frame-ticked reactive trees in v1 (decision Q2).
- **VSCR-NG-009.** Hosted graph sharing, a marketplace or telemetry. Hosted
  services are anti-scope ([ROADMAP](../../ROADMAP.md#what-we-are-not-doing-anti-scope));
  graph files travel in mod packages like any content.
- **VSCR-NG-010.** Fallout 76 (§7.1).

---

## 5. Requirements by area

### 5.1 Graph file and identity (`VSCR-AST`)

- **VSCR-AST-001.** A graph file MUST be a text document in the format
  decision Q4 chooses, carrying a schema version; the loader MUST refuse a
  schema version it does not know with both versions in the message.
  Today no graph file exists (§3.3 row 14).
- **VSCR-AST-002.** A graph file MUST name forms by `FormRef`
  (`crates/sdk/src/identity.rs:238`) or by editor ID resolved through it,
  never by a load-order-relative FormID, so a graph's meaning does not
  change when the load order does (the keep list's "Content-addressed Form
  IDs" row, which the code realises with manifest UUIDs, as the
  [World Editor PRD § 3.4](world-editor.md#34-which-identity-the-output-plugin-follows)
  records).
- **VSCR-AST-003.** Each graph MUST have an identity, derived from its
  package's `ModId` (sandbox rule ARC-001) and a graph name as the Phase 0
  design specifies, that is unchanged when the file is moved or renamed
  inside its package or the load order changes, asserted by a test; every
  report and saved value MUST carry that identity.
- **VSCR-AST-004.** Saving an unchanged graph document twice MUST produce
  byte-identical files (stable node, key and property order), so graph
  files diff cleanly under version control.
- **VSCR-AST-005.** A graph file MUST ship in a mod package as a content
  artifact with a separate attachment declaration
  ([ARC-002](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment)).
  Today the extension manifest has no content-artifact field and refuses a
  package with no executable component (`crates/sdk/src/manifest.rs:304`);
  decision Q5 chooses how a data-only package is admitted.
- **VSCR-AST-006.** Graph properties MUST be resolved through the same
  `ScriptInstanceData` path that resolves a VMAD property for lowered
  effects (`ObjectRef::Property`, `translate/compose.rs:73`), so an effect
  lowered from a graph and one lowered from Papyrus name objects the same
  way.
- **VSCR-AST-007.** The loader MUST refuse a graph above the node-count and
  depth limits the Phase 0 design sets, naming the limit; depth MUST NOT
  exceed `MAX_CONDITIONAL_DEPTH` (`translate/effects/lower.rs:411`).
- **VSCR-AST-008.** A graph file's manifest entry MUST use the one
  content-artifact entry shape the package manifest gains for ARC-002,
  carrying a package-relative path and a content hash
  ([ARC-006](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment)),
  the shape the [LLM Dialogue Plugin PRD](llm-dialogue-plugin.md)'s model
  artifacts also use (its LLMD-INF-002 and LLMD-INF-003). M81 MUST NOT add a
  graph-only manifest field, and the two PRDs' manifest additions MUST land
  under one `EXTENSION_MANIFEST_VERSION` step (`crates/sdk/src/manifest.rs:24`).

### 5.2 Node catalog (`VSCR-CAT`)

- **VSCR-CAT-001.** The node catalog MUST be generated from the engine's
  own vocabularies, the `Effect` variants, the guards decision Q3 admits,
  and the delivered event names, and checked in; a test MUST fail when one
  of those vocabularies gains or loses an element the catalog does not
  account for (offered, or excluded with a reason).
- **VSCR-CAT-002.** Each catalog entry MUST name the IR element it lowers
  to and the Papyrus function or event it corresponds to (or, on FO3, FNV
  and Oblivion, the ObScript function), with the CK, GECK or CS wiki page
  that defines it, so "the same surface as Papyrus" is checkable entry by
  entry.
- **VSCR-CAT-003.** Each catalog entry MUST declare its typed inputs
  (form kind, quest, number, boolean, text) and the graph editor and the
  loader MUST refuse a connection of the wrong type.
- **VSCR-CAT-004.** Per-game availability MUST be decided in the catalog's
  translate step from the profile's game, the way `translate_script`
  already takes a `GameKind` (`translate/mod.rs:66`); no node's run-time
  behaviour MAY branch on the game.
- **VSCR-CAT-005.** An `Effect` variant whose runtime arm only records that
  a call ran (`Effect::is_placeholder`, `model.rs:1052`, at HEAD on
  2026-11-01) MUST NOT be offered as an action node, so authors are not
  promised an effect the engine does not perform.

### 5.3 Lowering and admission (`VSCR-LOW`)

- **VSCR-LOW-001.** A graph MUST be lowered once, when its package
  activates, to `Effect` lists; nothing MAY walk graph nodes at run time.
- **VSCR-LOW-002.** A graph with any node that does not lower MUST be
  refused whole; no part of it MAY run. This is the decline-on-unmodeled
  rule of the M47.2 design ([recognizer contract](../engine/m47-2-design.md#the-recognizer-contract-unchanged-from-the-r5-shape))
  applied to graphs.
- **VSCR-LOW-003.** A refusal MUST name the graph identity, the node
  identity and one reason from a closed list the design defines (unknown
  node, wrong input type, unresolved form, unmodeled condition, limit
  exceeded, cycle, unavailable on this game).
- **VSCR-LOW-004.** A condition node whose CTDA function is not modeled
  end to end (`is_modeled`, `condition/eval.rs:95`) MUST refuse the graph.
  The evaluator's fail-open default for an unknown function, which
  evaluates it to 0.0 ([Scripting § Condition evaluator](../engine/scripting.md#condition-evaluator-m471)),
  MUST NOT apply to graphs; `condition_form.rs` already refuses a
  Starfield condition form on the same rule.
- **VSCR-LOW-005.** A subgraph reference that forms a cycle MUST refuse
  every graph in the cycle.
- **VSCR-LOW-006.** The loader SHOULD warn, without refusing, on a
  Fallback child that begins with an action (§3.2), naming the unreachable
  siblings.
- **VSCR-LOW-007.** Lowering MUST be deterministic: the same graph file and
  profile MUST produce equal `Effect` lists, asserted by a test.

### 5.4 Attachment and events (`VSCR-EVT`)

- **VSCR-EVT-001.** A graph MUST be attachable to a placed reference, to
  every reference of a base object, and to a quest. Attachment targets are
  named by `FormRef` (VSCR-AST-002).
- **VSCR-EVT-002.** An event root MUST name one event the runtime delivers:
  a name in `DELIVERED_EVENTS` (`fragment/reference.rs:63`), an update or
  timer registration event, or, for a quest attachment, the stage-set
  event behind `QuestStageAdvanced`. A root naming any other event MUST
  refuse the graph.
- **VSCR-EVT-003.** Graph runs MUST be started from the same delivery
  systems that deliver events to reference scripts and quest fragments
  (`reference_script_event_system`, `quest_fragment_dispatch_system`);
  M81 MUST NOT add a consumer of the markers beside the two that exist.
  Today `reference_script_event_system` delivers to
  `PapyrusReferenceScripts` only, and FO3, FNV and Oblivion references are
  served by `obscript_object_system` (§3.3 row 1); the design states which
  of the two starts a graph run on each game.
- **VSCR-EVT-004.** An event argument a root exposes (the activator, the
  aggressor) MUST be readable by the run's nodes. Today an event argument
  reaches only an `Effect::Interpret` run (§3.3 row 4); the design adds the
  IR element that names it, which is a saved-shape change (VSCR-SAV-004).
- **VSCR-EVT-005.** A graph and a legacy script attached to the same
  reference MUST both run for one delivered event, in the order decision
  Q6 fixes, and a test MUST assert that order.
- **VSCR-EVT-006.** One delivered event MUST start at most one run per
  attached graph root. Overlapping runs of the same graph on the same owner
  (a second activation during a wait) MUST follow one rule the design
  states, asserted by a test, because Papyrus authors guard that case by
  hand: "a 'queue' forms of all of those threads"
  ([CK: Threading Notes (Papyrus)](https://ck.uesp.net/wiki/Threading_Notes_(Papyrus))).
- **VSCR-EVT-007.** A graph attached to a reference MUST be able to name
  that reference (Papyrus's `Self`) as the object of an action. Today the
  lowering declines `Self` as an object (§3.3 row 4); the design adds the
  IR element, under VSCR-SAV-004.

### 5.5 Execution and budget (`VSCR-RUN`)

- **VSCR-RUN-001.** A graph run MUST execute through `apply_effects`, with
  the attachment's quest or reference as its context and the graph's
  resolved properties as its `ScriptInstanceData`.
- **VSCR-RUN-002.** A graph run MUST charge `VmFrameBudget`: the run's
  start cost (`RUN_START_COST`, `fragment/budget.rs:54`) plus a per-effect
  cost the design sets. Today lowered effect lists are not charged (§3.3
  row 7).
- **VSCR-RUN-003.** A graph run that cannot start because the budget is
  spent MUST be deferred unchanged onto `FragmentExecutionQueue` and start
  on a later frame, as an unstarted `Effect::Interpret` run is
  ([Scripting § The frame budget](../engine/scripting.md#the-frame-budget-m472-5763)).
- **VSCR-RUN-004.** A wait node MUST suspend the run onto
  `FragmentExecutionQueue` with the rest of its program, as `Effect::Wait`
  and `Effect::WaitWhile` do today.
- **VSCR-RUN-005.** A condition wait MUST end after an authored maximum
  duration even if its condition still holds, and the run trace MUST say
  which ended it. Today `Effect::WaitWhile` has no maximum (`model.rs`,
  variant `WaitWhile`).
- **VSCR-RUN-006.** The same delivered events in the same order MUST
  produce the same effects in the same order, asserted by a test that runs
  a fixture twice.
- **VSCR-RUN-007.** Graph runs MUST count toward `MAX_FRAGMENT_CONTINUATIONS`
  (`fragment/state.rs:788`) like any other tail, and the overflow warning
  MUST name the graph.
- **VSCR-RUN-008.** A graph run that has started MUST be charged its whole
  cost in the frame it runs and MUST NOT be cut short by the budget, the
  rule a VM run that cannot be sliced follows today
  ([Scripting § The frame budget](../engine/scripting.md#the-frame-budget-m472-5763));
  its worst-case cost MUST be bounded by the VSCR-AST-007 limits, and the
  design MUST state that bound in statements.
- **VSCR-RUN-009.** Each re-check of a condition wait (`Effect::WaitWhile`
  polling) MUST be charged to `VmFrameBudget`, so a graph that waits on a
  condition costs budget on every frame it polls, not only when it starts.

### 5.6 State and saves (`VSCR-SAV`)

- **VSCR-SAV-001.** Graph variables MUST be saved in the owner-keyed stores
  that hold script variables today (`QuestScriptVariables`,
  `QuestScriptObjects`), keyed by the owner's FormID and namespaced by
  graph identity, so two graphs and a Papyrus script on one reference do
  not collide; `script.var` MUST read them.
- **VSCR-SAV-002.** A run suspended at a wait MUST survive a save and load
  and complete after it with the effects of an uninterrupted run.
- **VSCR-SAV-003.** On load, a suspended run whose graph is no longer in
  the profile MUST be dropped, and the load report MUST count the dropped
  runs per graph identity. Graph variables of an absent graph MUST be kept
  and written back unchanged, the way absent-package extension state is
  ([sandbox STATE-008](../engine/sandboxed-linked-mods.md#13-persistent-state-and-saves)).
- **VSCR-SAV-004.** Any IR element M81 adds (an event-argument reference,
  the owner reference, a CTDA guard, a wait maximum, a run's graph
  identity) MUST land with a
  `FORMAT_MAJOR` bump that `saved_type_shape_changes_require_format_major_bump`
  (`byroredux/src/save_io/serde_default_guard_tests.rs:478`) enforces, and
  the design MUST list every such element in one table.
- **VSCR-SAV-005.** M81 SHOULD add no new saved resource; a new one MUST be
  justified in the design against VSCR-SAV-001.

### 5.7 Extension calls (`VSCR-EXT`)

- **VSCR-EXT-001.** A script function an extension publishes
  (`ScriptFunctionDeclaration`, `crates/sdk/src/script_function.rs`) MUST
  appear in the catalog as an action node, with the function's declared
  parameter types as its inputs, and MUST lower to `Effect::ProviderCall`.
- **VSCR-EXT-002.** A graph that uses an extension's function MUST declare
  the extension as a dependency; activation MUST refuse the graph, naming
  the extension, when that extension is absent or not granted.
- **VSCR-EXT-003.** A graph MUST NOT gain any capability by calling an
  extension: the call runs with the extension's grants, attributed to its
  principal (`FragmentProviderCall::principal`), as for Papyrus calls today.

### 5.8 Authoring surface (`VSCR-UI`)

- **VSCR-UI-001.** Graph editing MUST be available inside the M50 editor
  surface and MUST only emit typed edit commands, never mutate the world
  directly (World Editor PRD WED-UI-007, [§ 5.8](world-editor.md#58-editor-surface-wed-ui)).
- **VSCR-UI-002.** Every graph edit command, validation and save MUST be
  reachable from `byro-dbg` so a graph can be built and checked headless
  (World Editor PRD WED-NET-001, [§ 5.7](world-editor.md#57-protocol-and-hot-reload-wed-net)).
- **VSCR-UI-003.** The editor MUST offer only catalog nodes valid for the
  profile's game, with each node's wiki page from VSCR-CAT-002.
- **VSCR-UI-004.** The editor MUST show the admission result live: a
  refused graph highlights the node VSCR-LOW-003 names.
- **VSCR-UI-005.** Saving a graph in a running engine SHOULD re-admit and
  re-attach it without a process restart; runs already suspended MUST
  finish on the program they started with.
- **VSCR-UI-006.** Undo and redo of graph edits SHOULD use the editor's
  history (World Editor PRD WED-UNDO-001,
  [§ 5.5](world-editor.md#55-undo-and-redo-wed-undo)), not a second one.
- **VSCR-UI-007.** The graph document MUST be a document kind of the SDK
  `document` module, sharing its command, result and history types, with
  no second command model (World Editor PRD WED-DOC-001,
  [§ 5.1](world-editor.md#51-document-and-edit-commands-wed-doc)).

### 5.9 Diagnostics (`VSCR-DX`)

- **VSCR-DX-001.** A console command `graph.list` MUST print every graph in
  the profile with its identity, state (§3.4) and attachment count.
- **VSCR-DX-002.** A console command `graph.check <file>` MUST run
  admission on a graph file without attaching it and print the result in
  the VSCR-LOW-003 shape.
- **VSCR-DX-003.** A console command `graph.trace <entity>` MUST print, for
  the last runs on that owner, the event, the nodes taken, the effects
  applied with their log lines, and whether the run completed, waited or
  was deferred.
- **VSCR-DX-004.** The budget diagnostics (`VmFrameUse`) MUST report graph
  statements separately from VM statements.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Package procedure trees run "again as long as it remains the top-most package" and Simultaneous branches run procedures "at the same time" ([CK: Package Templates](https://ck.uesp.net/wiki/Category:Package_Templates)) | Graph runs start on events and have no Parallel node (VSCR-NG-006, VSCR-NG-008) | — | Authors expecting procedure-tree habits find no Simultaneous → the catalog says so per node (VSCR-CAT-002); packages stay in PACKAL (VSCR-NG-004) |
| 2 | Papyrus threads on one script instance queue behind a lock ([CK: Threading Notes](https://ck.uesp.net/wiki/Threading_Notes_(Papyrus))) | Runs are synchronous in the scheduler; overlap follows one stated rule | Hand-written "busy" flags and `GoToState` guards for re-entry | An author's mental model of overlap differs → VSCR-EVT-006 |
| 3 | An unknown condition function evaluates to 0.0 at run time ([Scripting § Condition evaluator](../engine/scripting.md#condition-evaluator-m471)) | A graph with an unmodeled condition is refused at load (VSCR-LOW-004) | — | Fewer graphs run on less-modeled games → reported as "refused" with the function named (VSCR-LOW-003) |
| 4 | A Papyrus `While` with `Utility.Wait` may poll forever | A condition wait has an authored maximum (VSCR-RUN-005) | — | A wait that should be open-ended ends → the trace says the maximum ended it |
| 5 | Papyrus saves stacks; a removed script leaves orphaned stacks ([Scripting § Save Serialization](../engine/scripting.md#save-serialization)) | Suspended graph runs of an absent graph are dropped and counted; variables are kept (VSCR-SAV-003) | Save cleaners for orphaned script instances | A run in progress when its mod is removed never finishes → counted in the load report |
| 6 | A text script is the only way to add gameplay logic | A graph needs no compiler, archive or `.pex` | The Papyrus compiler step for small behaviours | Graphs cannot express loops or arrays → those behaviours stay Papyrus or an extension (VSCR-NG-003, VSCR-EXT) |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Graphs target the canonical events and effects, so a graph is game-neutral
by construction and per-game difference lives only in the catalog's
availability step (VSCR-CAT-004). Waves follow the engine tiers
([Game Compatibility](../engine/game-compatibility.md#per-game-support-detail))
and the legacy-mod waves
([LMC § 7.1](legacy-mod-compatibility.md#71-waves)).

| Game | In scope | Why / wave |
|---|---|---|
| Fallout: New Vegas | yes | Wave 1. Engine reference title; its placed references carry ObScript, run by M47.3. The event markers are raised with no game gate and already reach FNV references through `obscript_object_system` (§3.3 row 1, at HEAD on 2026-11-01); A6 proves a graph run live |
| Skyrim (LE / SE / AE) | yes | Wave 1. The Papyrus VM's main corpus; the Creation Kit procedure-tree precedent (§1) |
| Fallout 3 | yes | Wave 2. Shares FNV's machinery |
| Fallout 4 | yes | Wave 2. Papyrus with FO4 events; the FO4 Creation Kit has the same Procedure Tree on packages ([FO4 CK: Package](https://falloutck.uesp.net/wiki/Package)) and a Simultaneous node that "completes when any of its children complete" ([FO4 CK: Sandbox (Procedure)](https://falloutck.uesp.net/wiki/Sandbox_(Procedure))); the rest of its branch list is **Phase 0: verify** (V2) |
| Starfield | yes | Wave 3. Tier 2 engine support; condition forms already refuse unmodeled functions (VSCR-LOW-004) |
| Oblivion | yes | Wave 3. Tier 3 engine support; ObScript runtime present |
| Fallout 76 | no | No official mod support and no corpus, as in the [Legacy Mod Compatibility PRD](legacy-mod-compatibility.md) scope |

### 7.2 Acceptance

Graph fixtures are authored by the project and checked in under
`docs/smoke-tests/fixtures/`; they reference vanilla forms by `FormRef`
and contain no game data. Game data is the user's own install.

| # | Scenario | Driven by | Data | Passes when |
|---|---|---|---|---|
| A1 | Lowering of every fixture graph | new `cargo test -p byroredux-scripting graph_lowering_fixtures` | none | Each fixture's lowered `Effect` list equals its checked-in expectation |
| A2 | Refusal corpus | new `cargo test -p byroredux-scripting graph_admission_refusals` | none | Each refusal fixture is refused with the node and reason its expectation names (VSCR-LOW-003) |
| A3 | Catalog drift | new `cargo test -p byroredux-scripting graph_catalog_covers_ir` | none | Fails when an `Effect` variant, admitted guard or delivered event is unaccounted for (VSCR-CAT-001) |
| A4 | Budget charge | new `cargo test -p byroredux-scripting graph_runs_charge_frame_budget` | none | A graph run reduces `VmFrameBudget::remaining` by its cost; with the budget spent it is deferred (VSCR-RUN-002/003) |
| A5 | Save shape | existing `saved_type_shape_changes_require_format_major_bump` | none | Green after every IR addition, with the bump (VSCR-SAV-004) |
| A6 | Activator graph live, FNV | new script `docs/smoke-tests/m81-graph-activate.sh`: `--bench-hold` on a vanilla FNV interior with a fixture package, then `byro-dbg`: `graph.list`, `script.activate <ref>`, `quest.show <quest>`, `script.var <ref> <var>`, `graph.trace <ref>` | user's FNV install | The graph is *active*, the stage and variable match the fixture, the trace lists the expected nodes |
| A7 | Same graph, Skyrim SE | the same script with a Skyrim SE profile and the same graph file | user's Skyrim SE install | Same assertions as A6 (VSCR-G-006) |
| A8 | Wait across save | new script `docs/smoke-tests/m81-graph-save.sh`: activate a fixture whose graph waits, quicksave mid-wait, quickload, wait | user's FNV install | The post-wait effect is applied exactly once (VSCR-SAV-002) |
| A9 | Removed graph | the A8 script with the fixture package removed before quickload | user's FNV install | Load report counts one dropped run for the graph; no effect applied (VSCR-SAV-003) |
| A10 | Headless authoring | new script `docs/smoke-tests/m81-graph-author.sh`: build the A6 graph from empty through `byro-dbg` edit commands only, save, `graph.check` | user's FNV install | The saved file is byte-identical to the A6 fixture (VSCR-AST-004, VSCR-UI-002) |

Scripts follow the shared procedure in
[docs/smoke-tests/README.md](../smoke-tests/README.md) and are indexed
there when written.

---

## 8. Metrics

Numbers are published in the M81 row of
[ROADMAP Tier 10](../../ROADMAP.md#tier-10--ecosystem-unlock-post-tier-4-horizon)
once Phase 1 lands; this document holds definitions only.

| Metric | Definition | Granularity |
|---|---|---|
| Catalog coverage | catalog entries offered ÷ (`Effect` variants + admitted guards + delivered events) | per game |
| Admission rate | fixture graphs admitted ÷ fixture graphs expected to admit | per phase |
| Refusal attribution | refusals naming a node and a closed-list reason ÷ refusals | per phase |
| Graph budget share | graph statements ÷ all interpreter statements, per frame, on the A6 cell | per game |
| Save round-trip | A8 and A9 passes ÷ runs | per game |
| Cross-game reuse | fixture graphs passing A6 and A7 unchanged ÷ fixture graphs offered on both | per wave |

---

## 9. Delivery sequence

Dependencies: M47.0 and M47.2 are closed; M50 is PROPOSED and gates Phase
3. Tier 10's "no active work" gate (§1) applies to Phases 1–4 and is the
owner's to lift; Phase 0 is documentation only.

**Phase 0 — Design, catalog draft and decisions (no engine code).** Write
`docs/engine/visual-scripting.md`: file format, identity rule, lowering
table per node kind, the IR additions list (VSCR-SAV-004), overlap rule
(VSCR-EVT-006), limits. Draft the node catalog by hand from the IR for
review. Author the fixture and refusal corpora as files with expectations.
Take decisions Q1–Q6 and close the verify list below. Exit: the design is
merged and registered in `docs/engine/index.md`; the fixture and refusal
corpora are checked in; every V item is closed or filed as an issue; every
§10 item carries **Decided**.

**Phase 1 — Graph file, catalog and lowering (headless).** Loader, catalog
generator, lowering, admission and `graph.check`. Satisfies VSCR-AST
(except VSCR-AST-005 and VSCR-AST-008, which land with Phase 2's package
admission), VSCR-CAT, VSCR-LOW, VSCR-DX-002. Exit: A1, A2 and A3 pass; catalog coverage
published for Wave 1 games.

**Phase 2 — Attachment, runs and saves.** Data-only package admission
(Q5, VSCR-AST-005, VSCR-AST-008), event delivery to graphs, budget charge, waits, variables, IR
additions with their `FORMAT_MAJOR` bump, `graph.list` and `graph.trace`.
Satisfies VSCR-EVT, VSCR-RUN, VSCR-SAV, VSCR-DX-001/003/004. Exit: A4, A5,
A6, A7, A8 and A9 pass on FNV and Skyrim SE.

**Phase 3 — Authoring in the editor.** Graph editing on the M50 surface and
protocol. Depends on M50's editor surface and protocol phases. Satisfies
VSCR-UI. Exit: A10 passes.

**Phase 4 — Extension calls and later waves.** Script-function nodes and
the Wave 2 and Wave 3 games. Satisfies VSCR-EXT. Exit: one fixture calling
a test extension's function passes A6; catalog coverage and A6 published
for every in-scope game.

Phase 0 closes these **Phase 0: verify** items:

| # | Question | What settles it |
|---|---|---|
| V1 | (closed in rev 2 from code) Whether the event markers are raised for FO3, FNV and Oblivion references | Settled at HEAD on 2026-11-01: `emit_activate_event` has no game gate (`byroredux/src/interaction.rs:1585`) and `obscript_object_system` consumes the markers for ObScript references (`obscript_objects.rs:247`). What remains is VSCR-EVT-003's design choice, not a verify item |
| V2 | The branch types of the Fallout 4 and Starfield procedure trees besides Simultaneous (§7.1) | The FO4 Creation Kit's Package Templates page (the local dump's `Category/Package Templates.wiki` is an empty stub), or a census of `PACK` branch records over `Fallout4.esm` and `Starfield.esm` |

---

## 10. Decisions required

1. **Q1 — What a graph lowers onto.** Recommended: the canonical `Effect`
   IR, lowered once at load (VSCR-LOW-001). It keeps one runtime, inherits
   saves and budget, and makes refusal explainable per node; the cost is a
   v1 vocabulary limited to what the IR expresses (no loops, no parallel),
   a handful of IR additions (VSCR-SAV-004), and new budget charging for
   lowered lists (VSCR-RUN-002). Alternative: emit a Papyrus AST and feed
   the existing recognizer chain and VM. Consequence: loops and arrays
   become available and the VM's statement budget applies unchanged, but a
   graph inherits Papyrus semantics, its errors surface as VM decline
   reasons rather than nodes, and run traces no longer map to nodes. Not
   offered: a dedicated behavior-tree tick evaluator. It would be a third
   interpreter beside the Papyrus and ObScript VMs, which the keep list
   ("ECS-native; bounded interpreter fallback under a per-frame statement
   budget") and VSCR-NG-001 already rule out; this PRD does not reopen it.
2. **Q2 — Tick model.** Recommended: event-started runs with latent waits,
   as Unreal's event-driven trees do (§1), so an idle graph costs nothing.
   Alternative: per-frame ticking for full classical reactivity (the book's
   Reactivity subsection). Consequence: cost scales with attached graphs rather than with
   events, against the "zero overhead for idle scripts" property
   ([Scripting](../engine/scripting.md#byroredux-this-engine)), and every
   tick needs a budget charge.
3. **Q3 — Condition vocabulary.** Recommended: the M47.1 CTDA catalog
   (`ConditionFunction`), reached through one new guard element carrying a
   `ConditionList`, so the conditions authors already build in the
   Creation Kit's Conditions tabs carry over, refused when unmodeled
   (VSCR-LOW-004). Consequence: a saved-shape change and `FORMAT_MAJOR`
   bump. Alternative: `FragmentGuard` only. Consequence: no format change,
   but a smaller vocabulary that differs from what the CK offers authors.
4. **Q4 — Graph file format.** Recommended: JSON with a schema version and
   canonical key order; `serde_json` is already a workspace dependency and
   the debug protocol speaks JSON, so `byro-dbg` edits and files share one
   shape. Alternative: TOML, as extension manifests use
   (`byroredux/src/extensions/install.rs:700`). Consequence: matches
   manifests but nests trees awkwardly. Alternative: records inside the M50
   output plugin. Consequence: one artifact for authors, but M81 would wait
   on a record format M50 has not defined
   ([WED-OUT-001](world-editor.md#56-output-plugin-wed-out)).
5. **Q5 — How a data-only package carries a graph.** Recommended: extend
   the extension manifest with content artifacts and attachments
   ([ARC-002](../engine/sandboxed-linked-mods.md#71-package-content-and-attachment))
   and admit packages with no executable component. Consequence: a change
   to the SDK manifest contract (`EXTENSION_MANIFEST_VERSION`), shared with
   the LLM Dialogue Plugin PRD's model-artifact fields, so the two land as
   one manifest step with one entry shape (VSCR-AST-008); the
   `MissingComponent` refusal is relaxed only for packages that declare
   content artifacts.
   Alternative: a separate graph-package manifest. Consequence: no SDK
   change, but two package kinds to resolve, order and report.
   Alternative: attach through a VMAD-like field in a Redux-native plugin
   record. Consequence: attachments conflict by plugin order like any
   record, and it depends on M50's record format.
6. **Q6 — Order beside a legacy script on the same reference.**
   Recommended: the graph runs after the reference's legacy handlers for
   the same event, in the same frame, so a graph observes what the script
   did. On FO3, FNV and Oblivion those handlers run in
   `obscript_object_system`, so the order is also a schedule order between
   that system and the one VSCR-EVT-003 picks. Alternative: before. Consequence: a graph can prepare state a
   script reads. Alternative: an opt-in "replaces the script" flag, as an
   ObScript `OnActivate` block "will prevent normal activation of the
   object" ([GECK: OnActivate](https://geck.uesp.net/wiki/OnActivate)). Consequence:
   a graph can patch vanilla behaviour, and the preflight must report each
   replaced script.

---

## 11. Document map

| Need | Go to |
|---|---|
| Event markers, delivered events, the VM, the frame budget, saved script variables | [Scripting](../engine/scripting.md) |
| The event-hooks layer M81 builds on | [M47.0 Event Hooks Design](../engine/m47-0-design.md) |
| Lowering and the decline-on-unmodeled rule | [M47.2 Scripting Runtime Design](../engine/m47-2-design.md) |
| Packages, attachments, capabilities, extension state | [Sandboxed Linked Mods](../engine/sandboxed-linked-mods.md) |
| Script functions and manifests | [SDK v0.1 Development Plan](../engine/sdk-v0.1-development-plan.md) |
| Saved stores and `FORMAT_MAJOR` | [Save/Load Round-Trip](../engine/save-load-roundtrip.md) |
| The editor surface, protocol, undo | [In-Engine World Editor PRD](world-editor.md) |
| Mechanism dispositions, waves | [Legacy Mod Compatibility PRD](legacy-mod-compatibility.md) |
| The shared manifest content-artifact shape | [Sandboxed Linked Mods](../engine/sandboxed-linked-mods.md) ARC-002 / ARC-006, [LLM Dialogue Plugin PRD](llm-dialogue-plugin.md) |
| Procedure trees and Story Manager trees in this engine | [PACKAL](../engine/packal.md), [Story Manager](../engine/story-manager.md) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md) |
| Smoke-test procedure and index | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: graph runs bounded in full by the frame budget (G-005, RUN-008, RUN-009) and the ObScript VM's unbudgeted state stated; marker-delivery HEAD claim corrected and V1 closed from code; Unreal, Havok Behavior and text-scripting citations fixed; Q1 third evaluator cited as decided; shared manifest entry shape with LLMD (AST-008); WED links cite IDs; UI-007 document kind | claude: revise |
