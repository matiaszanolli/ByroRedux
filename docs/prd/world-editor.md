# In-Engine World Editor — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in
review round 1 (HEAD corrections for `LoadNif`, synthesized terrain LOD,
Studio results and the splat cap; plugin identity stated against ROADMAP's
"content-addressed" wording; FO4 precombine and LOD-regeneration
behaviour re-cited; selection taken out of the edit set; release opt-in
moved into Q6; VR-aware authoring and the procedural bake reconciled with
their sibling PRDs; UI acceptance added). Revision 1 was the first draft
of the M50 promise: cell, reference, lighting and terrain editing inside
the engine, live, undoable, saved as a plugin. Statements about how an original engine,
editor, format or tool behaves cite external sources (the Creation Kit,
GECK and Construction Set wikis, UESP's file-format pages, the xEdit
documentation); a claim that could not be verified is marked **Phase 0:
verify** instead of being stated as fact.

**Scope**: M50 promises an editor that runs inside the engine binary on
Linux and lets an author open a cell of any in-scope game, select, move,
place, disable and delete placed references, edit a cell's lighting and
its placed lights, and paint exterior terrain (heights, texture layers,
vertex colours), with every edit visible in the running engine at once,
undoable, drivable headlessly over the debug protocol, and saved as a
Redux-native plugin that holds only the net changes against the profile's
current data. Outside it: editing base records (the Object Window's form
editors), quests, dialogue and scripts, navmesh, distant LOD and
precombine generation, asset authoring, multi-user editing and, in this
revision's phases, VR-aware authoring (§4.2, Q8).
The plugin identity and mod-profile model are owned by the
[Legacy Mod Compatibility PRD](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)
and are cited here, never redefined.

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Edit commands, snapshots, edit history, persisted overrides (the SDK `document` module) | [SDK v0.1 plan §6.1, Phase 5](../engine/sdk-v0.1-development-plan.md#phase-5--studio-migration-and-v01-release-gate), [SDK next actions](../engine/sdk-v0.1-next-action-plan.md) |
| Wire protocol, `byro-dbg`, console-command dispatch, live field mutation | [Debug CLI](../engine/debug-cli.md) |
| Snapshot registry, `FormIdPair` remap, live load-apply | [Save/Load Round-Trip](../engine/save-load-roundtrip.md) |
| Plugin manifests, Form ID layers, `DataStore`, resolver | [Plugin Loading](../engine/plugin-loading.md) |
| Plugin identity, mod profile, fold winner and identical-override semantics | [Legacy Mod Compatibility PRD](legacy-mod-compatibility.md) |
| Front end, boot contract, profiles | [Launcher](../engine/launcher.md) |
| Terrain, distant LOD, exterior translation | [EXAL](../engine/exal.md) |
| The editor itself: cell document, edit kinds, output-plugin format, edit mode | none yet: **Phase 0 writes `docs/engine/world-editor.md`** |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `WED-` prefix.

---

## 1. Product thesis

ROADMAP files M50 in Tier 9, the "better-than-Bethesda capability stretch",
and calls it "the largest single user-visible win the project can ship"
([ROADMAP Tier 9](../../ROADMAP.md#tier-9--better-than-bethesda-capability-stretch-post-tier-4-horizon)).
The tier's gate was "no active work until M40 (streaming) and M45
(save/load) prove the underlying state model holds up". Both have closed:
M40 on 2026-05-24 and M45/M45.1 on 2026-06-21
([Completed Milestones](../../ROADMAP.md#completed-milestones)), and the
playable slice's P5 gate, a 30-minute save/transition soak, closed on
2026-10-01 behind [`p5-soak.sh`](../smoke-tests/README.md); exterior
streaming is gated by `m-exteriors.sh` in its `boundary` mode
(same index). The gate is met.

Who it is for:

1. **Authors on Linux.** In the original first-party editors
   (Construction Set, GECK, Creation Kit) references are made by dragging
   a base object from the Object Window into the Render Window ([CS: Render Window](https://cs.uesp.net/wiki/Render_Window),
   [CK: Render Window](https://ck.uesp.net/wiki/Render_Window),
   [CK: Object Window](https://ck.uesp.net/wiki/Object_Window); the GECK
   wiki's own pages are **Phase 0: verify**, settled by its *Object Window*
   and *Render Window* pages), and the
   result is saved to one active plugin
   ([CK: Data File](https://ck.uesp.net/wiki/Data_file)). Whether they run
   natively on Linux is **Phase 0: verify** (§6 row 2).
2. **The project itself.** Test cells, fixtures and repros are authored
   today by finding a vanilla cell that happens to contain the case. Four
   later milestones list M50 as a dependency: M63 OpenXR / VR (for
   VR-aware authoring, Q8), M64
   procedural exteriors, M72 decentralized mod hosting and M81 visual
   scripting (ROADMAP Tier 9 and Tier 10).
3. **Tools and automation.** Everything the editor does is a typed command
   that a test, a script or `byro-dbg` can send, the same surface the GUI
   uses (the SDK rule "UI layers only consume snapshots and emit
   commands", `crates/sdk/src/lib.rs`).

The corollary that shapes everything below: **an edit is authored data,
recorded as a typed command against a stable identity; the saved plugin is
computed from those commands against the profile's fold winner, never by
capturing the live world.** The running world holds physics-settled
clutter, AI-moved actors and script-disabled references; none of that is
an edit, and none of it may reach the output plugin. The engine therefore
will not "save what you see": it saves what you did.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Studio** | The SDK's tooling host: renderer-independent document state, snapshots and typed commands in `crates/sdk/src/studio.rs`, hosted in the engine by `byroredux/src/studio_host.rs` behind `--studio`. M50 extends it; it does not add a second tool. |
| **Document** | The SDK plan's `document` module: "Studio snapshots, object capabilities, commands, history values, and persisted overrides" ([SDK plan §6.1](../engine/sdk-v0.1-development-plan.md#61-proposed-sdk-modules)). |
| **Cell document** | The document kind M50 adds: the authored state of the cells the editor has open under one mod profile, plus the edit history. |
| **Edit command** | One typed mutation of a document, in the lineage of `StudioCommand`. Each has a typed result and an inverse (WED-DOC-003). |
| **Edit kind** | One entry of the closed vocabulary in §3.1. |
| **Fold winner** | The record the profile's load-order fold selects for a form before the output plugin is applied. "Identical" is measured against it ([LMC-REC-006](legacy-mod-compatibility.md#52-records-lmc-rec)). ROADMAP's "diff against the parsed ESM" means this. |
| **Output plugin** | The Redux-native plugin the editor writes. The original editors' equivalent is the "Active File", to which "any changes you make are automatically saved" ([CK wiki: FO4 tutorial ch. 3](https://ck.uesp.net/wiki/Fallout_4_Simple_Fetch_Quest_Tutorial_Chapter_3:_Loading_Your_Mod_As_The_Active_File)). |
| **Net edit** | For one record, the difference between the document's value and the fold winner. The output plugin holds net edits only. |
| **ITM** | xEdit's "Identical To Master edit": "a type of dirty edit made where the edited value(s) is/are identical to the value(s) without the edit" ([CK wiki: TES5Edit Cleaning Guide](https://ck.uesp.net/wiki/TES5Edit_Cleaning_Guide_-_TES5Edit)). |
| **Live apply** | An edit command reflected in the running `World` of the editing engine without a cell reload. |
| **Hot reload** | A *separate* running engine picking up a saved output plugin without a process restart. |
| **Edit mode** | The engine running a cell document. Whether simulation runs in it is decision Q4. |
| **`FormIdPair`, `PluginId`** | Stable identity: plugin plus local form ID ([Plugin Loading § Form ID System](../engine/plugin-loading.md#form-id-system)). A Redux-native plugin's `PluginId` is the UUID its manifest declares, not a hash of its content (§3.4 states the source and the ROADMAP wording it departs from). |
| **`FormRef`, `ObjectId`** | The SDK's portable form identity and document-local object identity ([SDK plan §6.2](../engine/sdk-v0.1-development-plan.md#62-stable-identity)). |

---

## 3. Core model

### 3.1 Edit kinds

The vocabulary is closed. A new kind is a revision of this document. Each
kind names the canonical decoded type it edits; per-game record dialects
stay in the parser (the read side) and, only if decision Q1 adopts legacy
export, in a per-game writer (the write side). No edit kind branches on the
game.

E1 (selection) is a document command, not an edit: it produces no net
edit, is not recorded in the undo history and never reaches the output
plugin, like a camera move (WED-UI-006). Every requirement below that says
"edit kind" means E2–E10 unless it names E1.

The inventory is the state at HEAD on 2026-11-01. The "Today" column is
hand-written; Phase 1 replaces it with the edit-kind table in the Phase 0
design and a test per kind (WED-DOC-008), after which it is removed.

| # | Edit kind | Canonical target | Today |
|---|---|---|---|
| E1 | Select a reference | `FormIdComponent` on the placement root (`cell_loader/spawn.rs`), `SelectedRef` resource | Inspection only: `prid`, `pick`, `near` set `SelectedRef` (`crates/core/src/ecs/resources/mod.rs`, an `EntityId`); `Inspect` with no entity dumps it (`crates/debug-server/src/evaluator.rs`, [debug-cli.md](../engine/debug-cli.md#picked-ref-workflow-prid--inspect)) |
| E2 | Move, rotate, scale a reference | `PlacedRef.position` / `rotation` / `scale` (`crates/plugin/src/esm/cell/placed_ref.rs`) | Live only: `SetField` on `Transform` writes the component; nothing records it as an edit or persists it |
| E3 | Place a new reference of a base object | new `PlacedRef` under the output plugin's `PluginId` | Absent: no request places a reference of a base object. `LoadNif` (`crates/debug-protocol/src/lib.rs`) queues a loose mesh import that the engine drains with `&mut World`, spawning entities with no `PlacedRef`, base form or plugin identity; `DebugDrainSystem::run` itself holds `&World` only (`crates/debug-server/src/system.rs`). Studio's `AddAsset` imports one asset into a gallery room, not a reference into a cell |
| E4 | Disable or delete a reference | `PlacedRef.initially_disabled`; deletion | Absent as an edit. `ReferenceEnableState` records script `Disable()` calls as save state, not authored data ([save-load-roundtrip.md §6](../engine/save-load-roundtrip.md#6-live-load-apply-m451)) |
| E5 | Edit a reference's fields (light radius `XRDS`, lock, ownership, enable parent, linked refs) | the matching `PlacedRef` field (`radius_override`, `lock`, `ownership`, `enable_parent`, `linked_refs`) | Live only for registered components (`LightSource.radius` via `SetField`); not persisted |
| E6 | Edit cell lighting | `CellLighting` (`crates/plugin/src/esm/cell/lighting.rs`) → `CellLightingRes` | Read only: `light.dump` prints `CellLightingRes`; resources are not in the debug `ComponentRegistry`, and no command writes it |
| E7 | Paint terrain height | `LandscapeData.heights`, recomputed `normals` (`crates/plugin/src/esm/cell/landscape.rs`) | Absent: terrain geometry is built from `LandscapeData` at cell load (`byroredux/src/cell_loader/terrain.rs`); no command edits it |
| E8 | Paint terrain texture layers | `LandscapeData.quadrants` → splat layers (`build_cell_splat_layers`) | Absent |
| E9 | Paint terrain vertex colour | `LandscapeData.vertex_colors` | Absent |
| E10 | Edit cell fields (name, ownership, water height, lighting template) | `CellData.display_name` / `ownership` / `water_height` / `lighting_template_form` (`crates/plugin/src/esm/cell/cell_data.rs`) | Absent |

### 3.2 What the debug protocol already carries

ROADMAP says "the existing debug-protocol crate already does ~60% of the
wire shape this needs". That figure has no definition and is dropped; the
table below replaces it. State at HEAD on 2026-11-01.

| Need | Today | Class |
|---|---|---|
| Framing, transport, local trust | Length-prefixed JSON, 16 MB cap (`crates/debug-protocol/src/wire.rs`); loopback bind pinned by `the_listener_binds_loopback_only` (`crates/debug-server/src/listener.rs`); release builds need `BYRO_DEBUG_SERVER=1` (`byroredux/src/main.rs`) | present |
| Find and inspect entities | `ListEntities`, `FindEntity`, `GetComponent`, `Inspect`, `WalkEntity` | present |
| Selection and camera | `prid`, `pick`, `near`, `cam.pos`, `cam.tp` through `Eval` → `CommandRegistry` | present |
| Load a cell or a loose mesh into the running engine | `LoadInteriorCell`, `LoadExteriorCell`, `LoadNif`, queued for the engine to drain between frames with `&mut World` | present |
| Screenshot | `Screenshot` | present |
| Mutate one field | `SetField`, through serde round-trip on registered components only; no whole-component replacement ([debug-cli.md § Mutation](../engine/debug-cli.md#mutation)) | partial |
| Address objects by stable identity | Requests take raw `u32` entity IDs; the SDK forbids raw `EntityId` across its boundary ([SDK plan §6.2](../engine/sdk-v0.1-development-plan.md#62-stable-identity)) | absent |
| Create and delete references | No request; `LoadNif` spawns an unidentified mesh, not a reference (E3) | absent |
| Typed edit command with a typed result and a revision check | No request; `StudioCommand` derives no `Serialize` (`crates/sdk/src/studio.rs`); the host's `apply_command` returns `()` and `apply_gallery_command` returns `Result<(), String>` (`byroredux/src/studio_host.rs`), with no revision | absent |
| Undo, redo, edit grouping | No edit history anywhere in the workspace (the only hits are `ui_input.rs`'s `Undo` / `Redo` key names) | absent |
| Change notification to a client | None: every response answers one request (`handle_client`, `listener.rs`) | absent |
| Protocol version | None in `crates/debug-protocol` | absent |

### 3.3 What the snapshot machinery gives undo

ROADMAP says "undo/redo is free off the ECS snapshot machinery". At HEAD on
2026-11-01 it is not:

| Piece | Today | Consequence for undo |
|---|---|---|
| `SaveRegistry` (`crates/save/src/registry.rs`) | A curated subset of components and resources with type-erased save/load closures ([save-load-roundtrip.md §2](../engine/save-load-roundtrip.md#2-ecs-snapshot-capture)) | Covers gameplay state, not every component an edit touches |
| `save_world` | Captures the whole registered world into a `Snapshot` (`crates/save/src/driver.rs`) | Whole-world granularity per capture |
| `apply_deltas` | Additive-only except for explicitly replacing registrations; keyed by a `FormIdPair` remap after a fresh cell reload (`driver.rs`, [§ What's not covered](../engine/save-load-roundtrip.md#whats-not-covered)) | Restoring a snapshot is a cell reload, not an undo step |
| Studio edit history | `StudioSession.original_transforms` serves `ResetTransform` only (`byroredux/src/studio_host.rs`); the SDK plan records "there is no edit history or persisted document format" ([§4.1](../engine/sdk-v0.1-development-plan.md#41-tooling-prototype)) | Undo is unbuilt |

How undo is built is decision Q2.

### 3.4 Which identity the output plugin follows

Two documents describe a Redux-native plugin's identity differently. This
PRD follows the code and the design, not the ROADMAP wording:

| Source | What it says | This PRD |
|---|---|---|
| `crates/plugin/src/manifest.rs`, `PluginId::from_uuid` (`crates/core/src/form_id.rs`), [Plugin Loading § Plugin Manifests](../engine/plugin-loading.md#plugin-manifests-redux-native) | "The manifest assigns a stable UUID that replaces the load-order-dependent slot index"; a legacy plugin's id is UUID v5 of its filename ([LMC-ID-001](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id) folds its case) | **Followed.** The output plugin's `PluginId` is the UUID in its manifest, fixed when the plugin is first created |
| [ROADMAP keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation): "Content-addressed Form IDs", kept because it "Eliminates load-order dependency + slot limits" | Stable identity independent of load order | **Followed in intent.** The stable `FormIdPair` is what removes load-order dependency and slot limits; the code realises it with manifest UUIDs, not hashes |
| ROADMAP M50 row: "saves diff against the parsed ESM into a content-addressed plugin"; M72 row: "mod-id = content hash" | A plugin named by a hash of its bytes | **Not followed.** A content hash changes on every save, so every `FormIdPair` the editor issued would change with it, breaking WED-G-007 and every save that holds a delta for an edited reference ([LMC-SAV-003](legacy-mod-compatibility.md#510-saves-and-profile-changes-lmc-sav)) |

Content hashes still have a place: the profile fingerprint covers each
plugin's content hash ([LMC-ID-007](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id)),
and a distribution address for a published plugin may be a digest of its
bytes. Neither is the `PluginId`. Reconciling the M50 and M72 rows and the
keep-list wording is a ROADMAP edit, not a decision of this PRD.

---

## 4. Goals and non-goals

### 4.1 Goals

- **WED-G-001 — Edits persist.** For each in-scope game (§7.1), every edit
  kind E2–E10 in §3.1 survives save, a fresh process, and reload of the
  edited cell, with the reloaded value equal to the edited value.
- **WED-G-002 — Edits are live.** Every edit kind except terrain painting
  (E7–E9) is visible in the editing engine on the next presented frame
  without a cell reload; terrain painting is visible without a cell
  reload, within the stroke cost §8 publishes.
- **WED-G-003 — Edits are undoable.** Every edit kind E2–E10 has an
  inverse, and an undo followed by a redo leaves the document equal, as
  serialised, to its state before the undo.
- **WED-G-004 — Output is clean.** A saved output plugin contains zero
  ITM records and declares every plugin it depends on.
- **WED-G-005 — Linux-native.** The editor runs in the Linux engine binary
  with no compatibility layer.
- **WED-G-006 — Scriptable.** Every edit the GUI can make can be made
  headlessly over the debug protocol, and the acceptance scenarios (§7.2)
  run without a person.
- **WED-G-007 — Identity holds.** A record the editor creates keeps its
  `FormIdPair` across saves, reloads and profile reorders.

### 4.2 Non-goals

- **WED-NG-001.** Editing base records: weapons, NPCs, light base forms
  (`LIGH`), statics. The Object Window's form editors
  ([CK: Object Window](https://ck.uesp.net/wiki/Object_Window)) are a later
  PRD. M50 reads base records to choose what to place (WED-UI-004); decision
  Q7 records the alternative.
- **WED-NG-002.** Quest, dialogue, package and script authoring, including
  a node-graph editor. Script logic belongs to the scripting runtime and
  M81's visual scripting ([Visual Scripting PRD](visual-scripting.md)); M50 gives
  M81 the document, command and history model (WED-DOC, WED-UNDO) to build
  its authoring surface on, not the surface itself.
- **WED-NG-003.** Navmesh editing or generation. The editor reports which
  edited cells carry navmesh (WED-DX-002).
- **WED-NG-004.** Generating prebaked distant-LOD assets: Skyrim / FO4
  `.btr` terrain meshes and `.bto` object meshes, LOD textures, Oblivion
  `.lod` placements. At HEAD on 2026-11-01 distant terrain *geometry* is
  synthesized from LAND at runtime and the prebaked sources upgrade it per
  block where present (`byroredux/src/cell_loader/terrain_lod.rs`;
  [EXAL §5](../engine/exal.md#5-lod-distance-rendering-the-must-have)), so
  a height edit reaches the synthesized ring from the same `LandscapeData`;
  what goes stale is the prebaked sources, which the editor reports
  (WED-DX-001). The [Distant LOD Completion PRD](distant-lod-completion.md)
  likewise excludes a baker (DLOD-NG-001).
- **WED-NG-005.** Precombine and previs generation. A cell with an edited
  reference renders from its placed references
  ([LMC-REC-012](legacy-mod-compatibility.md#52-records-lmc-rec)).
- **WED-NG-006.** Writing to a game's master files or to any plugin other
  than the output plugin. The Creation Kit itself "will not create master
  files" ([CK: Data File](https://ck.uesp.net/wiki/Data_file)).
- **WED-NG-007.** Multi-user editing and shared check-in servers, the role
  the Creation Kit's Version Control played for Bethesda's team
  ([CK: Version control](https://ck.uesp.net/wiki/Version_control)). Hosted
  services are anti-scope ([ROADMAP](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
- **WED-NG-008.** Mesh, texture and material authoring (M80's glTF/USD
  round-trip).
- **WED-NG-009.** Worldspace-scale heightmap import and export. Of the
  Creation Kit's Heightmap editor its wiki says "Outside of import/export,
  the features of the tool are considered obsolete and unsupported"
  ([CK: Heightmap Editing](https://ck.uesp.net/wiki/Heightmap_Editing));
  M50 paints per cell and leaves import/export to a later PRD.
- **WED-NG-010.** Builds for Windows or macOS beyond what M70 delivers, and
  VR-aware authoring in Phases 0–6. ROADMAP's M63 row puts "M50 expanded
  for VR-aware authoring" under M63; the [OpenXR / VR PRD](openxr-vr.md)
  excludes it from M63 (VRX-NG-002) and recommends moving it into M50's
  scope (its Q2). Decision Q8 records whether M50 accepts it as a later
  phase, so the item has exactly one owner.
- **WED-NG-011.** Reproducing the original editors' window layout, menus
  or key map. The editor answers the same needs (§5.8); it does not clone
  the program.

---

## 5. Requirements by area

### 5.1 Document and edit commands (`WED-DOC`)

- **WED-DOC-001.** The cell document MUST be a document kind of the SDK
  `document` module that the SDK plan's Phase 5 delivers
  ([§6.1](../engine/sdk-v0.1-development-plan.md#61-proposed-sdk-modules)),
  sharing its snapshot, command, result and history types with the Studio
  asset gallery. Today that module does not exist and Studio's types live in
  `crates/sdk/src/studio.rs`; a second command model for the world editor
  MUST NOT be introduced.
- **WED-DOC-002.** Every edit command MUST address its target by stable
  identity: a `FormRef` for a reference, cell or landscape that exists in
  the profile, or a document-local `ObjectId` for a record created in this
  session. Raw `EntityId` values MUST NOT appear in an edit command or its
  result. Today `StudioCommand` uses document-local `ObjectId` for loose
  assets; references have no stable command address.
- **WED-DOC-003.** Every edit command MUST return a typed result: applied
  with the new document revision, or rejected with a reason. Today the
  host's `apply_command` returns `()` and `apply_gallery_command` returns
  `Result<(), String>` (`byroredux/src/studio_host.rs`): an untyped string,
  no revision.
- **WED-DOC-004.** An edit command MUST carry the document revision it was
  issued against; a command issued against a stale revision MUST be
  rejected with the current revision in the result, the SDK plan's
  "revision checks" (Phase 5).
- **WED-DOC-005.** The output plugin MUST be computed from the document's
  edit set and the fold winners. The live `World` MUST NOT be read to
  produce it. A reference displaced by physics, AI or a script in edit mode
  MUST NOT appear in the output plugin.
- **WED-DOC-006.** The document's snapshot MUST be renderer-independent:
  the SDK crate depends only on `byroredux-core`, `semver`, `serde` and
  `thiserror` today (`crates/sdk/Cargo.toml`), and the cell document MUST
  NOT add a renderer or Vulkan dependency to it.
- **WED-DOC-007.** Opening a cell document MUST record the profile
  fingerprint ([LMC-ID-007](legacy-mod-compatibility.md#51-identity-profile-and-order-lmc-id))
  it was opened under, and saving under a different fingerprint MUST
  recompute every net edit against the new fold winners and report each
  record whose net edit changed. Today no profile fingerprint exists; a
  save stores only the cell, ESM path and master list (`CurrentCellContext`,
  per LMC-ID-007's own HEAD note), so this requirement waits on LMC-ID-007.
- **WED-DOC-008.** Each edit kind E2–E10 in §3.1 MUST have a headless test that
  applies it to an in-memory document, checks the snapshot, and checks
  the inverse. The tests MUST run without game data.

### 5.2 References (`WED-REF`)

- **WED-REF-001.** Moving, rotating or scaling a reference MUST update the
  document's `PlacedRef` value and the live `Transform` of its placement
  root in the same frame. Today `SetField` updates the component only.
- **WED-REF-002.** Rotation MUST be stored in the plugin's authored
  convention and converted at the boundary documented in
  [coordinate-system.md](../engine/coordinate-system.md); a reference moved
  and reloaded MUST come back with the same Euler triple it was saved with,
  to within 1e-4 radians.
- **WED-REF-003.** Placing a reference MUST create a `PlacedRef` whose
  `FormIdPair` is the output plugin's `PluginId` and the next unused local
  ID, spawn it through the same cell-loader spawn path a loaded reference
  uses, and return its `FormRef` in the result.
- **WED-REF-004.** A local ID the output plugin has issued MUST NOT be
  issued again, including after the record it named is deleted. Saves made
  against an earlier version of the plugin keep their meaning by this rule,
  because a save keys its deltas by `FormIdPair`
  ([Plugin Loading § Form ID System](../engine/plugin-loading.md#form-id-system),
  [save-load-roundtrip.md §6](../engine/save-load-roundtrip.md#6-live-load-apply-m451)).
- **WED-REF-005.** Deleting a reference that the output plugin created
  MUST remove it from the document and from the output plugin.
- **WED-REF-006.** Deleting a reference owned by another plugin MUST be
  recorded as an override with `initially_disabled` set, not as a deleted
  record, and the result MUST say so. The cleaning guide calls deleted
  references "a significant cause of crashes and game bugs" and fixes them
  by "undeleting then disabling the references and moving it to a position
  where it is no longer visible in the Creation Kit"
  ([CK wiki: TES5Edit Cleaning Guide](https://ck.uesp.net/wiki/TES5Edit_Cleaning_Guide_-_TES5Edit)).
  The move exists to hide the reference in the original editor; the
  override written here sets `initially_disabled` and leaves the position
  at the fold winner's (WED-REF-007). The engine handles both forms
  ([LMC-REC-004](legacy-mod-compatibility.md#52-records-lmc-rec)), and the
  disabled form keeps working for other plugins that edit the same
  reference.
- **WED-REF-007.** Editing a reference field in edit kind E5 MUST change
  only that field of the net edit; the other fields of the override MUST
  equal the fold winner's.
- **WED-REF-008.** Editing a reference absorbed by a Fallout 4 precombine
  MUST render that cell from its placed references under
  [LMC-REC-012](legacy-mod-compatibility.md#52-records-lmc-rec), and the
  editor MUST report the cell as precombine-bypassed (WED-DX-003). This is
  what the original game does on its own: "When we detect a change to a
  reference used in any of that process during a plugin load, we disable
  both systems for that area"; regenerating the precombined meshes and
  previs for the changed cells is the performance fix Bethesda used for
  its DLC, not a correctness requirement
  ([FO4 CK: Precombined References](https://falloutck.uesp.net/wiki/Precombined_References)).
- **WED-REF-009.** Duplicating a selection MAY be offered; when offered, it
  MUST produce one WED-REF-003 placement per selected reference, grouped as
  one undo step (WED-UNDO-002).

### 5.3 Cell lighting and lights (`WED-LIT`)

- **WED-LIT-001.** Editing any field of `CellLighting` MUST update the
  document and `CellLightingRes` in the same frame, and `light.dump` MUST
  print the new value. Today `light.dump` is read-only and no command
  writes `CellLightingRes`.
- **WED-LIT-002.** A lighting field the cell inherits from a lighting
  template (`CellLighting.inheritance_flags`) MUST be shown as inherited,
  and editing it MUST clear that field's inheritance in the net edit and
  nothing else.
- **WED-LIT-003.** Editing a placed light's radius MUST write the
  reference's `XRDS` radius override, the field UESP describes as
  "Controls the radii on objects like lights"
  ([UESP: Skyrim REFR](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/REFR)),
  decoded today as `PlacedRef.radius_override`. Oblivion's REFR lists no
  `XRDS` ([UESP: Oblivion REFR](https://en.uesp.net/wiki/Oblivion_Mod:Mod_File_Format/REFR)),
  so on Oblivion a light's radius is a base-record field and out of scope
  (WED-NG-001). **Phase 0: verify** whether the REFR of Fallout 3, Fallout:
  New Vegas, Fallout 4 and Starfield carries `XRDS`, settled by the REFR
  definitions in xEdit's `wbDefinitionsFO3.pas`, `wbDefinitionsFNV.pas`,
  `wbDefinitionsFO4.pas` and `wbDefinitionsSF1.pas`; where it does not, the
  same rule applies.
- **WED-LIT-004.** Placing, moving or deleting a light reference is edit
  kind E2 to E4 and MUST follow §5.2; the renderer's light set MUST reflect
  it on the next presented frame.

### 5.4 Terrain (`WED-TER`)

- **WED-TER-001.** A height brush MUST change `LandscapeData.heights` of
  every vertex inside its radius, rebuild the affected terrain geometry
  without a cell reload, and recompute `normals` for every changed vertex
  and its neighbours.
- **WED-TER-002.** A vertex on a shared cell edge MUST be written to both
  cells' landscape records with the same value; `terrain.seams` MUST report
  zero disagreeing shared-edge vertices across every edited cell (it reads
  `TerrainSeamStats` for resident exterior tiles,
  `byroredux/src/commands/world_info.rs`). The 33×33 grid's "first row and
  column overlaps with the north and west cell"
  ([UESP: Skyrim LAND](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/LAND)),
  and the Creation Kit warns that a brush radius above 25 "also can create large
  “tears” between cell borders" ([CK: Landscape](https://ck.uesp.net/wiki/Landscape)).
- **WED-TER-003.** A texture brush MUST write the quadrant layer and
  opacity data the parser decodes into `LandscapeData.quadrants`, and the
  splat result MUST be visible without a cell reload.
- **WED-TER-004.** A texture brush that would give a cell more distinct
  textures than the renderer's cell-global splat layer count
  (`TERRAIN_SPLAT_LAYERS`, 8 at HEAD in
  `crates/renderer/src/shader_constants_data.rs`) MUST be refused with the
  count in the result. Today `build_cell_splat_layers`
  (`byroredux/src/cell_loader/terrain.rs`) drops the excess layers of
  loaded data with a warning; the editor refuses instead of authoring data
  the renderer would drop. The Skyrim Creation Kit's own limit is different
  ("You can only use 6 different textures in a cell!",
  [CK: Landscape](https://ck.uesp.net/wiki/Landscape)); it matters only to
  legacy export (Q1).
- **WED-TER-005.** A vertex-colour brush MUST write
  `LandscapeData.vertex_colors`, creating the array for a cell that had
  none.
- **WED-TER-006.** One brush stroke (press to release) MUST be one undo
  step (WED-UNDO-002). The Creation Kit's Heightmap editor can "undo the
  last few brush edits" only
  ([CK: Heightmap Editing](https://ck.uesp.net/wiki/Heightmap_Editing)).
- **WED-TER-007.** Terrain edits MUST stay in canonical `LandscapeData`
  terms; the 33×33 grid and its per-game encoding are read by the parser
  and, if Q1 adopts legacy export, written by a per-game writer, never by
  the brush.
- **WED-TER-008.** After an edited exterior cell is saved and reloaded, the
  synthesized distant-terrain block that contains it SHOULD be built from
  the edited heights, since `terrain_lod.rs` samples the same parsed
  heightmaps (WED-NG-004); a block served by a prebaked `.btr` instead is
  reported by WED-DX-001.

### 5.5 Undo and redo (`WED-UNDO`)

- **WED-UNDO-001.** Every edit command (E2–E10) MUST be undoable and redoable. Undo
  MUST restore the prior document value exactly and re-apply it live.
- **WED-UNDO-002.** Commands MUST be groupable so that one user gesture
  (a drag, a brush stroke, a duplicate) is one undo step, the SDK plan's
  "edit grouping" (Phase 5).
- **WED-UNDO-003.** Issuing a new command after an undo MUST discard the
  redo stack. This is the original editors' rule: after another action,
  "any undone actions that you have not redone are lost"
  ([CK: Creation Kit Interface tutorial](https://ck.uesp.net/wiki/Bethesda_Tutorial_Creation_Kit_Interface)).
- **WED-UNDO-004.** History MUST be bounded by a memory budget stated in
  the Phase 0 design; when the bound evicts the oldest steps, the editor
  MUST report how many steps remain.
- **WED-UNDO-005.** Saving MUST NOT discard the undo history: after a
  save, undo still restores the edits made before it.

### 5.6 Output plugin (`WED-OUT`)

- **WED-OUT-001.** The output plugin MUST be a Redux-native plugin with a
  `PluginManifest` ([Plugin Loading § Plugin Manifests](../engine/plugin-loading.md#plugin-manifests-redux-native))
  whose dependencies list every plugin a net edit overrides or references.
  Its on-disk record format is defined in the Phase 0 design: today a
  manifest carries only identity, name, version and dependencies
  (`crates/plugin/src/manifest.rs`), `Record` holds type-erased components
  in memory with no serialisation (`crates/plugin/src/record.rs`), and
  `crates/plugin` has no record-writing function. Decision Q1 decides
  whether a legacy ESP is also written.
- **WED-OUT-002.** The output plugin MUST contain a record only where the
  document differs from the fold winner. An edit undone, or set back to the
  winner's value, MUST leave no record. The count of ITM records in a
  saved output plugin MUST be zero.
- **WED-OUT-003.** An override that sets a value equal to a master but
  different from the current fold winner MUST be kept and reported as a
  revert naming the plugin whose change it undoes
  ([LMC-REC-006](legacy-mod-compatibility.md#52-records-lmc-rec)). Today
  the merge keeps the winner only (`EsmIndex::merge_from`,
  `crates/plugin/src/esm/records/index.rs`; per-cell
  `merge_cell_override`, `crates/plugin/src/esm/cell/index.rs`), so the
  master's and earlier plugins' versions this compares against are
  discarded; the Phase 0 design names where they come from.
- **WED-OUT-004.** The output plugin MUST load into a profile through the
  same Redux-native loading path any Redux-native plugin uses (M46 and
  [LMC Phase 1](legacy-mod-compatibility.md#9-delivery-sequence)). Today that path has no
  production caller: `DataStore` and `DependencyResolver` are not
  constructed anywhere in `byroredux/src`, and the live path is
  `parse_esm_with_load_order` (#5609 reconciled the docs to say so).
- **WED-OUT-005.** Saving MUST be atomic: write to a temporary file, flush,
  verify, rename, as `write_slot` does for saves
  ([save-load-roundtrip.md §4](../engine/save-load-roundtrip.md#4-atomic-write--ring-buffer)).
  A failed save MUST leave the previous output plugin intact.
- **WED-OUT-006.** Saving the same document twice with no edit in between
  MUST produce byte-identical files.
- **WED-OUT-007.** Decoded sub-records the editor does not edit MUST be
  carried through unchanged, and undecoded sub-records of an overridden
  record MUST be preserved as opaque bytes
  ([LMC-REC-010](legacy-mod-compatibility.md#52-records-lmc-rec)); this
  depends on LMC-REC-010, because a Redux-native override written from
  canonical decoded types carries only what the parser decoded.
- **WED-OUT-008 — Writer without a session.** The output-plugin writer
  MUST be callable without an open editor session, from a set of canonical
  records whose `FormIdPair`s the caller supplies, and MUST write those
  pairs unchanged. This is the "M50 plugin writer" the
  [Procedural Exteriors PRD](procedural-exteriors.md) bakes through
  (PGEN-BAKE-001, PGEN-BAKE-003 keeps generated `FormIdPair`s); WED-REF-003's
  next-unused-ID allocation applies to editor placements only.

### 5.7 Protocol and hot reload (`WED-NET`)

- **WED-NET-001.** Every edit command, undo, redo and save MUST be
  reachable over the debug protocol and from `byro-dbg`, in the form
  decision Q3 chooses. Today the protocol has no edit request (§3.2).
- **WED-NET-002.** The protocol MUST carry a version that a client and
  server compare on connect, rejecting a mismatch with both versions in
  the error. Today there is none.
- **WED-NET-003.** The debug listener MUST stay loopback-bound
  (`the_listener_binds_loopback_only`, `crates/debug-server/src/listener.rs`).
  How a release build in edit mode starts it follows decision Q6. Today a
  release build starts it only with `BYRO_DEBUG_SERVER=1`, pinned by
  `release_debug_server_requires_explicit_opt_in` (`byroredux/src/main.rs`).
- **WED-NET-004.** A client MUST be able to subscribe to document
  revisions and receive each new revision number without polling.
- **WED-NET-005 — Hot reload.** A running engine with the output plugin
  in its profile MUST reload the current cell from the updated plugin on
  request, without a process restart, with the reloaded references equal
  to the saved document. The queued cell-load pattern exists today
  (`LoadInteriorCell`, `LoadExteriorCell`); loading a Redux-native plugin
  into it depends on WED-OUT-004.
- **WED-NET-006.** A hot reload into a play session MUST follow the
  profile-change rules of
  [LMC-SAV-003](legacy-mod-compatibility.md#510-saves-and-profile-changes-lmc-sav):
  where the session holds a saved delta for an edited reference, the delta
  wins and the reload report names the reference.

### 5.8 Editor surface (`WED-UI`)

The needs below are the original editors' main windows, answered in the
engine's debug UI, where the Studio panel already consumes snapshots and
emits commands (`crates/debug-ui/src/panels.rs`).

- **WED-UI-001 — Cell list.** The editor MUST list the profile's interior
  cells and, per worldspace, its exterior cells with their grid
  coordinates, and MUST load a chosen cell, the Cell View window's job
  ([CK: Cell View Window](https://ck.uesp.net/wiki/Cell_View_Window)).
- **WED-UI-002 — Reference list.** For the open cell the editor MUST list
  references with editor ID, form ID and base type, filterable by text, and
  selecting a row MUST select the reference (E1).
- **WED-UI-003 — Viewport editing.** The editor MUST offer pick-to-select
  and a translate / rotate / scale manipulator with axis constraint in the
  viewport, the Render Window's job
  ([CS: Render Window](https://cs.uesp.net/wiki/Render_Window)). Snapping
  is WED-UI-008.
- **WED-UI-004 — Base object browser.** The editor MUST list placeable base
  records by type with a text filter, and placing one MUST be edit kind E3,
  the Object Window's drag-to-place
  ([CK: Object Window](https://ck.uesp.net/wiki/Object_Window)).
- **WED-UI-005 — Use report.** For a selected base object the editor SHOULD
  list the references to it in loaded cells, the Object Window's "Use
  Info" (same page).
- **WED-UI-006.** The viewport camera SHOULD offer orbit around the
  selection, pan and zoom; camera moves MUST NOT be document edits.
- **WED-UI-007.** The editor UI MUST NOT mutate the world directly; it
  emits edit commands only, as the Studio panel does today.
- **WED-UI-008 — Snapping.** The manipulator MUST offer grid snapping for
  translation and angle snapping for rotation, each toggleable, as the
  Creation Kit's Render Window does ("grid snapping is on … Rotation is
  affected by angle snapping", [CK: Creation Kit Interface tutorial](https://ck.uesp.net/wiki/Bethesda_Tutorial_Creation_Kit_Interface)).

### 5.9 Diagnostics (`WED-DX`)

- **WED-DX-001.** Saving MUST report, as "LOD stale", every exterior cell
  whose terrain was edited and whose distant-LOD block draws from a
  prebaked source (a `.btr` terrain mesh or an authored LOD texture,
  WED-NG-004).
- **WED-DX-002.** Saving MUST report every edited cell that has a navmesh,
  as "navmesh not regenerated".
- **WED-DX-003.** Saving MUST report every cell that WED-REF-008 switched
  to placed-reference rendering.
- **WED-DX-004.** Every rejected edit command MUST produce one diagnostic
  naming the command, the target `FormRef` or `ObjectId`, and the reason.
- **WED-DX-005.** A console command MUST print the net-edit summary of the
  open document: records added, overridden, disabled, and reverted, per
  record type.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | The editor saves to one "Active File" plugin ([CK wiki: FO4 tutorial ch. 3](https://ck.uesp.net/wiki/Fallout_4_Simple_Fetch_Quest_Tutorial_Chapter_3:_Loading_Your_Mod_As_The_Active_File)) | A Redux-native output plugin (WED-OUT-001) | — | Authors who want CK or xEdit to open the result cannot → decision Q1 |
| 2 | The original editors are Windows programs, run on Linux through a compatibility layer (**Phase 0: verify** from each editor's published system requirements: CS, GECK, the Skyrim and Fallout 4 Creation Kits, Creation Kit 2) | Native, inside the engine binary (WED-G-005) | Wine / Proton editor setups | None |
| 3 | Undo is linear; "Holding down Ctrl-Z will rapidly undo many actions", and a new action drops the redo stack ([CK: Creation Kit Interface tutorial](https://ck.uesp.net/wiki/Bethesda_Tutorial_Creation_Kit_Interface)); the Heightmap editor undoes only "the last few brush edits" ([CK: Heightmap Editing](https://ck.uesp.net/wiki/Heightmap_Editing)) | One bounded, grouped history for every edit kind, terrain included (WED-UNDO-001…004) | — | Memory growth on long terrain sessions → WED-UNDO-004 |
| 4 | Dirty edits, "often caused by Creation Kit bugs or authors making unintended edits then undoing them incorrectly" ([CK wiki: TES5Edit Cleaning Guide](https://ck.uesp.net/wiki/TES5Edit_Cleaning_Guide_-_TES5Edit)) | Output is the net edit against the fold winner; zero ITM (WED-OUT-002) | Cleaning the editor's own output with xEdit | A deliberate revert must survive → WED-OUT-003 |
| 5 | Deleting a placed reference produces a deleted reference, "a significant cause of crashes and game bugs" when another mod edits it; cleaning undeletes, disables and moves it out of view (same source) | A reference owned by another plugin is disabled in place, not deleted or moved (WED-REF-006) | Undelete-and-disable cleaning of the editor's output | Authors expecting a hard delete → the result says it was disabled |
| 6 | A changed reference in a Fallout 4 precombined area makes the game "disable both systems for that area" at plugin load; Bethesda's DLC regenerated precombines and previs for the changed cells to recover the performance ([FO4 CK: Precombined References](https://falloutck.uesp.net/wiki/Precombined_References)) | The same fallback, rendering from placed references (WED-REF-008, LMC-REC-012), with no regeneration | Precombine / previs regeneration for editor output | Draw cost of an uncombined cell → WED-DX-003, measured in M50's metrics |
| 7 | LOD is regenerated in an explicit editor step after landscape edits: the GECK's World LOD dialog "Creates terrain meshes for the world from the local land geometry" ([GECK: World LOD](https://geck.uesp.net/wiki/World_LOD)); the Construction Set's Heightmap Editor has "File → Generate LOD" ([CS: Landscape LOD Tutorial](https://cs.uesp.net/wiki/Landscape_LOD_Tutorial)); for the Creation Kit, "After any sort of landscape editing … you will have to manually clear out all the previously generated LOD files and regenerate" ([CK: Creating a Custom Worldspace with LOD](https://ck.uesp.net/wiki/Creating_a_Custom_Worldspace_with_LOD)) | Synthesized terrain LOD follows the edited heights (WED-TER-008); prebaked LOD sources are reported stale, not regenerated (WED-NG-004, WED-DX-001) | The regenerate-LOD step for synthesized terrain | Visible mismatch where a prebaked `.btr` or LOD texture covers an edited cell → WED-DX-001 |
| 8 | The Skyrim Creation Kit allows 6 textures per cell ([CK: Landscape](https://ck.uesp.net/wiki/Landscape)) | The renderer's splat layer count governs (WED-TER-004) | — | A painted cell that legacy export could not represent → Q1 |
| 9 | Edits are tested by saving and launching the game, which "should pick up your mods" ([CK wiki: FO4 tutorial ch. 3](https://ck.uesp.net/wiki/Fallout_4_Simple_Fetch_Quest_Tutorial_Chapter_3:_Loading_Your_Mod_As_The_Active_File)) | Edits are live in the running engine (WED-G-002), and a separate session can hot-reload (WED-NET-005) | The save-launch-test loop | Runtime state leaking into the output → WED-DOC-005 |
| 10 | Light Master (`.esl`) files "cannot be edited in the Creation Kit" ([FO4 CK: Data File](https://falloutck.uesp.net/wiki/Data_File)) | Any plugin's records can be the fold winner a net edit overrides; the output plugin is never a legacy file (WED-NG-006) | — | None |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Waves follow the engine tiers in
[game-compatibility.md](../engine/game-compatibility.md#per-game-support-detail).
Because edits are phrased in canonical decoded types (§3.1), a game is in
scope wherever the engine loads and renders its cells; no requirement adds
a per-game branch to the editor.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 3. Interiors and exteriors render; listed under Tier 3 in the compatibility doc |
| Fallout 3 | yes | Wave 2. Tier 1; shares the FNV record parser |
| Fallout: New Vegas | yes | Wave 1. Tier 1 and the engine's reference title |
| Skyrim (LE / SE / AE) | yes | Wave 1 (SE; LE and AE with it). Tier 1; the largest modding ecosystem |
| Fallout 4 | yes | Wave 2. Tier 1; precombined cells handled by WED-REF-008 |
| Fallout 76 | no | No cell loader: "not yet started (no FO76 ESM stub)" (Tier 3 entry in the compatibility doc) |
| Starfield | interiors only | Wave 3. Tier 2: one walkable interior; no exterior loading, so E7–E9 are n/a |

### 7.2 Acceptance

Game data comes from the user's own install through the smoke fixtures'
data variables (`BYROREDUX_FNV_DATA`, `BYROREDUX_SKYRIMSE_DATA`, …); no
game, mod or output file is checked in. Live scenarios use `--bench-hold`
with `BYRO_DEBUG_SERVER=1` and drive `byro-dbg`, the pattern every script in
[docs/smoke-tests/README.md](../smoke-tests/README.md) uses. Edit-command
spellings below are placeholders until Q3 and the Phase 0 design fix them.

| # | Scenario | Driver | Data | Passes when |
|---|---|---|---|---|
| A1 | Edit-kind round-trip | new `cargo test -p byroredux-sdk cell_document` | none (synthetic document) | For every kind E2–E10: apply, snapshot equals the expected value, undo restores the prior snapshot, redo re-applies (WED-DOC-008, WED-UNDO-001); an E1 selection adds no history entry and no net edit |
| A2 | Net-edit output | new `cargo test -p byroredux world_editor_output` | none (synthetic `EsmIndex`) | A move, a revert-to-winner, a revert-to-master and a placement produce exactly one override, zero records, one revert record and one new record; two saves are byte-identical (WED-OUT-002, -003, -006) |
| A3 | Reference edit, FNV | new script `docs/smoke-tests/wed1-reference-edit.sh fnv` | `FalloutNV.esm` + archives; cell `GSProspectorSaloonInterior` (the `fnv.env` fixture) | Move one reference, place one, disable one owned by `FalloutNV.esm`; `inspect` shows each; undo and redo each; save; `engine.quit`; relaunch with the output plugin in the profile; `inspect` shows the same three states; the output plugin's ITM count is zero (WED-G-001, WED-G-004) |
| A4 | Reference edit, Skyrim SE | same script, `skyrim_se` | `Skyrim.esm` + archives; cell `WhiterunBanneredMare` | As A3 |
| A5 | Lighting edit | new script `docs/smoke-tests/wed2-lighting-edit.sh fnv` | as A3 | `light.dump` shows an edited ambient colour the next frame; after relaunch it shows the same; a light's radius edit is visible in `inspect` (WED-LIT-001, -003) |
| A6 | Terrain edit | new script `docs/smoke-tests/wed3-terrain-edit.sh fnv` (and `skyrim_se`) | `FalloutNV.esm` + archives, WastelandNV; `Skyrim.esm` + archives, Tamriel; the `m-exteriors.sh` profiles | A height stroke across a shared cell edge changes `terrain.height` under the camera; `terrain.seams` reports zero disagreeing vertices; the save report lists the cell as "LOD stale"; after relaunch both are unchanged (WED-TER-001, -002, WED-DX-001) |
| A7 | Live-apply latency | A3 with a frame counter | as A3 | Each edit kind's live apply lands on the next presented frame (WED-G-002) |
| A8 | Hot reload | new script `docs/smoke-tests/wed4-hot-reload.sh fnv` | as A3 | A second engine, launched with the output plugin, reloads the cell on request after the editor saves a new move, and `inspect` there shows it without a restart (WED-NET-005) |
| A9 | Editor surface | new `cargo test -p byroredux-debug-ui world_editor_panel`, headless `egui::Context` passes as the panel tests in `crates/debug-ui/src/panels.rs` already run | none (synthetic snapshot) | Scripted input on the cell list, reference list, manipulator, snapping toggles and base-object browser emits exactly the expected commands and mutates nothing else (WED-UI-001…008) |

---

## 8. Metrics

One home for the numbers: a per-game editor row group in ROADMAP's
[compatibility matrix](../../ROADMAP.md#compatibility-matrix), with each
latency figure's command in
[Repro commands](../../ROADMAP.md#repro-commands-for-every-bench-claim).

| Metric | Definition | Granularity |
|---|---|---|
| Edit-kind coverage | edit kinds passing A1 and their game's live scenario ÷ edit kinds applicable to the game | per game |
| Round-trip fidelity | edited fields equal after save, restart and reload ÷ edited fields | per game, per edit kind |
| Output cleanliness | ITM records in the output plugin ÷ records in it (target zero) | per scenario |
| Live-apply latency | frames from command receipt to the first presented frame showing it | per edit kind, per game |
| Terrain stroke cost | CPU and GPU ms to rebuild one cell's terrain after a stroke, bench methodology of ROADMAP (same-machine control, pinned camera) | per game |
| Precombine-bypass cost | frame-time difference between a precombined FO4 cell and the same cell after one edited reference, same methodology | per cell |

---

## 9. Delivery sequence

Dependencies: M45/M45.1 and M40 (met, §1); the SDK plan's Phase 5 document
model (Phase 1 builds it if it has not landed); M46 Redux-native plugin
loading, through LMC Phase 1, before Phase 3 can reload an output plugin.
ROADMAP's dependency column for M50 lists M45 and "debug protocol
expansion" only. Downstream, the procedural-exteriors bake (PGEN Phase 5)
needs WED-OUT-008 from Phase 3, and nothing else from M50.

**Phase 0 — Design, decisions, verify list (no engine code).** Write
`docs/engine/world-editor.md`: the cell document, the edit-kind table with
each kind's canonical fields and inverse, the output plugin's on-disk
format, the undo memory budget (WED-UNDO-004), and the edit-mode launch
form, which extends `--studio`, and where WED-OUT-003's pre-winner record
versions come from. Resolve Q1–Q8. Close the **Phase 0: verify** items:
the original editors' platform requirements (§6 row 2), the GECK's
drag-to-place pages (§1), and the REFR `XRDS` light-radius override on
FO3, FNV, FO4 and Starfield (WED-LIT-003). Exit: the design is checked in
and indexed, every §10 item carries **Decided**, and the three verify
items are closed with a citation or filed as issues.

**Phase 1 — Headless cell document.** The SDK `document` module with the
cell document, typed commands and results, revision checks, grouping and
bounded undo for E1–E6 and E10 (WED-DOC-001…008, WED-UNDO-001…005,
WED-REF-001…007, WED-LIT-001…002). Exit: scenario A1 passes for those
kinds with no game data.

**Phase 2 — Live editing in the engine.** Edit mode, the debug-UI surface
(WED-UI-001…008), live apply of Phase 1's kinds, the protocol request and
version (WED-NET-001…004), and the rejection diagnostics (WED-DX-004).
Exit: A9 passes, and A3 and A4 pass up to the save step, and A7 passes,
on FNV and Skyrim SE.

**Phase 3 — Output plugin.** The Redux-native output plugin
(WED-OUT-001…008), its load through the M46 path, and the save reports
(WED-DX-002, -003, -005). Depends on M46. Exit: A2, A3, A4 and A5 pass in
full, and output cleanliness is zero on all of them.

**Phase 4 — Terrain.** E7–E9 (WED-TER-001…008, WED-DX-001). Exit: A6
passes on FNV and on Skyrim SE's Tamriel, and the terrain stroke cost is
published.

**Phase 5 — Hot reload and the remaining waves.** WED-NET-005 and -006;
Wave 2 (FO3, FO4 with WED-REF-008) and Wave 3 (Oblivion, Starfield
interiors). Exit: A8 passes on FNV, A3 passes on every Wave 2 and Wave 3
game, and edit-kind coverage per game is published in ROADMAP.

**Phase 6 — Legacy export (only if Q1 adopts it).** A per-game legacy
writer behind the translation boundary. Exit: an exported plugin for each
exporting game loads in that game's xEdit with zero errors and its records
equal the Redux-native output field for field.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — Output format.** Recommended: the Redux-native plugin only
   (WED-OUT-001) for M50, with legacy ESP export left to Phase 6 or a later
   PRD. It keeps one canonical writer, keeps the editor's output on the
   keep list's stable identity, and avoids writing formats whose own
   specifications leave fields unexplained (UESP lists GRUP and record
   header fields as "Unknown" and says of group order "Whether the game
   engine expects this order is unknown",
   [UESP: Skyrim Mod File Format](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format)).
   Consequence: the output works on ByroRedux only, and authors cannot open
   it in the Creation Kit or xEdit. Alternative A: write a legacy ESP per
   game from the start. Authors keep their existing tools and the original
   games can load the result, at the cost of six per-game writers, slot and
   master-count limits on the output, Form ID allocation in a load-order
   space, and the CK's own constraints (WED-TER-004). Alternative B: both
   from Phase 3, doubling the Phase 3 test surface. Either way the
   Procedural Exteriors PRD's PGEN-BAKE-002 assumes baked heights
   round-trip through `VHGT`, whose step is "a height of 8 game units"
   per signed byte ([UESP: Skyrim LAND](https://en.uesp.net/wiki/Skyrim_Mod:Mod_File_Format/LAND));
   under the recommendation the Redux-native format stores
   `LandscapeData.heights` as decoded floats, so that requirement holds
   only for a legacy export, or if the Phase 0 design quantizes heights
   the same way.
2. **Q2 — Undo mechanism.** Recommended: command inverses recorded in the
   document history (WED-UNDO-001), the SDK plan's Phase 5 model. Undo is
   then a document operation, and live apply re-runs the inverse.
   Alternative A: capture the affected entities' rows through the
   `SaveRegistry` closures before each command; it covers registered types
   only (§3.3), so terrain, lighting resources and anything unregistered
   need a second mechanism. Alternative B: whole-world `save_world`
   snapshots restored through load-apply; each undo is a cell reload and is
   additive-only. Either alternative is what ROADMAP's "free off the ECS
   snapshot machinery" describes, and the recommendation departs from that
   sentence; adopting it means editing the M50 row.
3. **Q3 — Edit transport.** Recommended: a typed `DebugRequest` variant
   carrying the serialised SDK command and returning its typed result, with
   the protocol version of WED-NET-002. Edit commands need typed results
   and revisions (WED-DOC-003, -004), and this is a host-contract change
   (README trigger T2) that this PRD then covers. Alternative: dotted
   console commands through `Eval` (`edit.move …`), which need no protocol
   change ("New commands … automatically become reachable from `byro-dbg`
   with no protocol change", [debug-cli.md § CommandRegistry dispatch](../engine/debug-cli.md#commandregistry-dispatch-dotted-names))
   but return text, so every client parses strings.
4. **Q4 — Simulation in edit mode.** Recommended: paused by default, with a
   play toggle that runs a session from the current document and discards
   its runtime state when stopped; WED-DOC-005 holds either way.
   Alternative: simulation runs while editing, which shows AI and physics
   live but lets actors and clutter move under the author's selection.
5. **Q5 — First wave.** Recommended: Fallout: New Vegas and Skyrim SE,
   matching [LMC Q4](legacy-mod-compatibility.md#10-decisions-required). Alternative: FNV alone,
   which halves Phase 2–3 acceptance work and delays Papyrus-era cell data.
6. **Q6 — Local trust for editing.** Recommended: keep today's model,
   loopback-only and unauthenticated (#5149), and have edit mode start the
   listener in a release build as if `BYRO_DEBUG_SERVER=1` were set, so
   the editor works without a second switch. This departs from the
   release opt-in pinned by `release_debug_server_requires_explicit_opt_in`
   (`byroredux/src/main.rs`) for edit mode only, and any local process can
   then edit the open document. Alternative A: keep the opt-in; edit mode
   in release builds then needs `BYRO_DEBUG_SERVER=1` and fails to reach
   `byro-dbg` without it, as every release `--bench-hold` does today.
   Alternative B: a per-session token written to a user-only file and
   required on connect, which stops other local users and processes at
   the cost of a handshake every client must implement.
7. **Q7 — Base-record editing.** Recommended: out of M50 (WED-NG-001),
   including light base forms; "edit lighting" means cell lighting and
   placed lights. Alternative: include the `LIGH` base record's colour and
   radius, which serves lighting artists directly but opens base-record
   overrides, and with them every reference to that light in every cell.
8. **Q8 — VR-aware authoring.** Recommended: accept it into M50 as a later
   phase, added by a revision of this PRD once M63's player promise has
   shipped, matching the OpenXR / VR PRD's Q2; Phases 0–6 here stay
   desktop-only (WED-NG-010), and the M63 row's dependency on M50 is
   dropped in the ROADMAP commit that records the decision. Consequence:
   M50 owns the item, and nothing in M63 waits on the editor. Alternative:
   keep it under M63 as the ROADMAP row says; M63 then depends on M50
   shipping and the OpenXR / VR PRD needs an authoring area of its own.

---

## 11. Document map

| Need | Go to |
|---|---|
| The SDK document model, Studio, identity rules, Phase 5 gate | [sdk-v0.1-development-plan.md](../engine/sdk-v0.1-development-plan.md) |
| SDK hand-off backlog | [sdk-v0.1-next-action-plan.md](../engine/sdk-v0.1-next-action-plan.md) |
| Wire protocol, `SetField`, `CommandRegistry`, loopback trust | [debug-cli.md](../engine/debug-cli.md) |
| Snapshot registry, `apply_deltas`, atomic write | [save-load-roundtrip.md](../engine/save-load-roundtrip.md) |
| `PluginManifest`, `FormIdPair`, `DataStore` | [plugin-loading.md](../engine/plugin-loading.md) |
| Profile identity, fold winner, identical overrides, precombines, profile-change semantics | [legacy-mod-compatibility.md](legacy-mod-compatibility.md) |
| Terrain and distant LOD | [exal.md](../engine/exal.md) |
| Per-game engine tiers | [game-compatibility.md](../engine/game-compatibility.md) |
| Front end and profiles | [launcher.md](../engine/launcher.md) |
| Smoke-test index and fixtures | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |
| The procedural bake that writes through WED-OUT-008 | [procedural-exteriors.md](procedural-exteriors.md) |
| VR-aware authoring and the M63 dependency (Q8) | [openxr-vr.md](openxr-vr.md) |
| Distant-LOD scope and its no-baker non-goal | [distant-lod-completion.md](distant-lod-completion.md) |
| Coordinate convention for stored rotations | [coordinate-system.md](../engine/coordinate-system.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: HEAD corrections (`LoadNif`, synthesized terrain LOD, Studio results, cell-global splat cap, merge keeps winner only, no profile fingerprint); plugin identity source stated (§3.4); FO4 precombine and LOD-regeneration behaviour re-cited, Oblivion `XRDS` settled; E1 out of the edit set; release opt-in moved into Q6; Q8 VR-aware authoring; WED-OUT-008 writer for the procedural bake; WED-TER-008, WED-UI-008, A9; coordinator cross-PRD sync (sibling links and citations) | claude: revise |
