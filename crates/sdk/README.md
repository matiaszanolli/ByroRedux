# ByroRedux SDK

`byroredux-sdk` is the renderer- and UI-independent foundation for tools built
on ByroRedux. It deliberately contains contracts and algorithms, not Vulkan,
egui, archive IO, or the game executable.

The current public surface includes:

- `ObjectId` is a stable document-local identity that never exposes the host's
  ECS entity IDs.
- `EntityRef` is a non-zero `(world_generation, object)` runtime handle that
  lets hosts reject stale references without exposing ECS slots.
- `FormRef` is the load-order-independent, save-safe identity used to rebind
  authored entities after a world replacement.
- `ContentCatalog` is a bounded snapshot of loaded regular/light plugins,
  their declared master edges, and portable record-existence/type metadata.
- `EntityProjection` and `WorldTransform` are finite, bounded, renderer-neutral
  snapshots of callback-visible entities; they expose no ECS slot or pointer.
  Actor-value projections use the canonical base/permanent/temporary/damage
  layers and portable AVIF identities.
- `ActorValueCommand` is the validated deferred mutation contract for setting
  or modifying canonical actor-value layers without exposing native storage or
  numeric global FormIDs.
- `InventorySnapshot` exposes a bounded, sorted summary by portable base form,
  with 64-bit counts, biped-slot occupancy, weapon equip state, and an explicit
  truncation bit. Optional validated metadata carries the resolved item name,
  semantic category, value, and non-negative finite weight. The contract
  deliberately does not invent stable per-instance handles.
- `FactionSnapshot` exposes deterministic callback-local FACT memberships and
  signed ranks through portable identities, with explicit truncation. It does
  not conflate membership with REPU fame/infamy or faction relationships.
- `PerkSnapshot` exposes bounded, deterministic owned perk ranks through
  portable identities, with explicit truncation for invalid or unresolved
  live entries. Perk mutation remains closed pending rank-limit and progression
  semantics.
- `PackageSnapshot` preserves ordered ambient and active scene package stacks,
  their current winners, scene/action provenance, and template identities.
  `EvaluatePackageCommand` requests semantic reevaluation through the shared
  engine marker instead of manipulating AI state directly.
- `SpatialSnapshot` provides bounded, deterministic radius queries over live
  authored references. Results carry portable `FormRef` identities and finite
  world positions, never ECS IDs; truncation remains explicit.
- Validated extension, principal, capability, service, event, and schema IDs
  are stable namespaced values shared by package loaders and sandbox hosts.
- `ExtensionManifest` declares SDK/dependency ranges, executable components,
  requested capabilities, subscriptions, and state schema versions without
  granting authority or performing IO.
- `ServiceCatalog` provides pre-compilation SDK/capability compatibility checks
  and validates that effective grants are supported and were actually
  requested.
- `ExtensionComponentStore` registers typed schemas under authenticated
  principals and applies bounded command batches atomically. Identical schema
  IDs owned by different principals cannot observe or mutate one another.
- `PrincipalStorageStore` provides bounded, schema-versioned key/value state
  for data that is not attached to an entity. Reads are principal-private and
  deferred writes can commit atomically with component edits.
- `ExtensionStateSnapshot` and `PersistedComponentRow` define the versioned,
  schema-tagged entity and principal-storage payload embedded by the engine in
  its normal save container.
- `ActivationEvent`, `CellLoadEvent`, `EquipmentEvent`, `HitEvent`,
  `InputActionEvent`, `SessionEvent`, `UpdateEvent`, and `ExtensionCommand` form
  the first canonical event-to-deferred-mutation contracts used by sandboxed components. Equipment
  events identify the wearer with an opaque `EntityRef` and the inventory item
  with a stable `FormRef`. Input events expose rebinding-independent gameplay
  actions and press/release edges, never physical key codes. Recurring update
  subscriptions carry a validated 16 ms–1 hour interval and the engine owns
  their cadence. Session events report new-game, committed-save, and
  completed-load transitions with an optional numeric slot, never a host path.
- `AssetBounds`, `BoundSphere`, and `CornellFit` provide deterministic,
  testable scene fitting.
- `StudioSnapshot` and `ObjectSnapshot` are immutable projections for any UI.
- `StudioCommand` is the mutation protocol shared by GUI, CLI automation, and
  other trusted tool clients.
- `pick_spheres` is a host-neutral center-ray selector.

The `byroredux` binary is the first host adapter. It imports NIF/SPT assets from
loose files or the existing BSA/BA2 and game-profile providers, constructs the
room, turns ECS state into snapshots, and applies commands after UI rendering.
It privately owns the `ObjectId` to ECS entity mapping; the egui crate never
sees that mapping or mutates the world directly.

The binary also owns the first executable-extension adapter. It resolves an
explicit manifest set, applies explicit capability grants, initializes
sandboxed components, snapshots activation, cell-load, combat-hit, ordered
equipment-change, normalized-input, and committed session-lifecycle events
outside ECS guards, advances manifest-declared recurring callbacks in the
late-stage scheduler, commits deferred state atomically, invalidates transient
handles on world replacement,
persists form-backed extension rows in normal ByroRedux saves, retains rows for
missing packages/forms, persists private principal storage, and performs
orderly shutdown. It also snapshots names, stable form identities, world
transforms, and bounded actor values before guest delivery, with separate
read/write grants. It also projects capability-gated inventory and worn
equipment summaries by portable base form, faction memberships, and the latest
bounded spatial snapshot of live authored references. Ranked actor perks are
also projected read-only from the canonical live character component.
Ambient and scene package state is projected together, while capability-gated
reevaluation is deferred to the existing selectors used by native scripting.
Actor-value mutations resolve transient entity handles and portable AVIF
identities, then stage and validate the full batch before touching ECS state.
Inventory and perk mutation wait for their stable validation contracts. Schema
migrators and the broader service surface are still planned work.

Run the current tool with:

```bash
cargo run --release -- --studio path/to/asset.nif
cargo run --release -- --game skyrim_se --studio \
  --mesh 'meshes\\clutter\\ingredients\\sweetroll01.nif'
```

This is intentionally an initial SDK surface, not a stability promise for the
entire engine. New inspectors, gizmos, asset browsers, undo/redo, serialization,
and engine-native extension services should extend the typed SDK boundary
instead of coupling to the executable, egui callbacks, ECS IDs, or process
memory.

The proposed path from this prototype to a supported SDK and a capability-gated
replacement for script-extender facilities is tracked in the
[SDK and extension platform development plan](../../docs/engine/sdk-v0.1-development-plan.md).
