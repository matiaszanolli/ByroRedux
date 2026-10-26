# Save/Load Round-Trip: Snapshot to Live Reload

Third in the cross-cutting series alongside [Pipeline Overview](pipeline-overview.md)
(interior cell load) and [Exterior Grid Streaming](exterior-grid-streaming.md).
This one traces the M45/M45.1 save system: what a save actually captures,
how it's written safely to disk, and — the part that makes this engine's
save/load different from a typical "restart and reload a level" design —
how a save gets applied to a *running* engine without a process restart.

> **Currency note.** Refreshed 2026-08-25 after #3162, #3163 and #3280.
> Save/load now reports player-visible success/failure notifications, quickload
> skips corrupt newest slots in recency order, every typed snapshot column is
> decoded before destructive teardown, and exterior reload waits for the full
> saved radius before applying FormID-keyed deltas. §2/§3/§6 were also
> re-checked after #3028:
> the registered component/resource count roughly doubled since the
> 2026-08-18 pass (M42 AI-procedure state, cinematic/quest-fragment
> resources, `Material`, `RigidBodyData`, `RumbleOnActivate`, …),
> `validate_world` gained three more invariant checks, and §6's death
> handling changed from "additive-only, nothing removes state" to a
> marker-plus-reconciler pattern (#3022). Sections below are written to
> point at the registration/column-list source rather than hand-enumerate
> it, since that enumeration is what went stale last time — see each
> section's own note. Also reconciles #3021 (an exterior save taken after
> visiting an interior no longer masquerades as an interior save).

## 1. Save trigger

`F5`, the pause-menu **Quicksave** button, and the `save` console command
all call the same `save_io::quicksave`/`SaveCommand` implementation. It resolves the target slot
(empty args → `SaveRing::advance()`; explicit `u32` → that slot), runs
the validation gates (§3), then calls `save_world` → `encode` →
`disk::write_slot`. It only needs `&World`, so it's a plain
`ConsoleCommand`, not a deferred/queued action. `SaveInfoCommand`
(`byroredux/src/save_io/commands.rs`, `"save.info"`) is the read-only companion — decode +
verify a slot without touching the live world.

## 2. ECS snapshot capture

Despite "full-ECS snapshot" shorthand elsewhere, this is a **curated
subset by design** — only types that carry player-visible game state;
derived data, GPU handles, and transient event markers are reconstructed
on load, never serialised (`crates/save/src/registry.rs`'s module doc).
`SaveRegistry` (`registry.rs:74`) is a type-erased registry — the same
shape as debug-server's `ComponentDescriptor` (per CLAUDE.md) — storing
a boxed save/load closure pair per component or resource, keyed by a
stable string name. `build_save_registry()` (`byroredux/src/save_io/registry.rs`)
is the binary-side population point and the authoritative list — don't
hand-copy its contents here, it has grown fast (roughly two dozen
components — actor/AI/inventory/equipment/animation/scripting state,
NPC vitals, cinematic and rigid-body data — plus about a dozen resources
including `ItemInstancePool`, `CurrentCellContext`/
`CurrentExteriorContext`, `PlayerPose`, `GameTimeRes`, and the quest/
scripting-fragment resources). Each registration site carries its own
comment explaining *why* that type is saved, which is more durable than
a count. `save_world` (`crates/save/src/driver.rs:28`) walks the
registry's component/resource entries and a `StringPool` dump (symbol
order) into a `Snapshot` (`crates/save/src/snapshot.rs:78`); component
rows are sorted by entity id, and the outer column/resource names are
ordered. Resource values keep each resource's own serialization order:
`HashMap`/`HashSet` backed resources can therefore vary between saves,
so the payload CRC detects byte corruption but does not promise a
reproducible CRC for equal logical state.

Entity ids round-trip **exactly** — load doesn't remap ids from scratch
the way a delta-log system would; `World::set_next_entity` +
batch-insert at the saved sparse ids keeps `Parent`/`Children`/
`root_entity` references valid with no separate remap pass for
structural data. (The `FormIdPair`-keyed remap in §6 is a different,
additional step — for reconciling saved state against a *freshly
reloaded* cell's newly-spawned entities, which get new session-local ids
even though they're logically the same game objects.)

## 3. Validation gates

`validate_world` (`crates/save/src/validate.rs:67`) has grown to seven
invariants, each its own `ValidationKind`: Hierarchy (`Parent`⇄`Children`
agreement, dangling refs), Equipment (`EquipmentSlots` occupant indexes
resolve into `Inventory`), DanglingEntity (saved `FollowState`/
`EscortState`/`Seated` entity references point at a spawned id —
`validate_saved_entity_references`), AnimationClip
(`AnimationPlayer.clip_handle` resolves in the registry), ItemInstance
(`ItemStack.instance` resolves in `ItemInstancePool`), and NonFiniteMaterial (a `Material` scalar is NaN/Inf — #2687,
probes a clone via `Material::sanitize_finite` rather than a second field
list) — plus a binary-side `validate_form_ids` (`byroredux/src/save_io.rs`,
needs `FormIdPool`, which the save crate doesn't own) and
`validate_cinematic_entity_refs` (#2535, needs `byroredux-scripting`
types). New invariants get added here often enough that, as with §2,
treat `validate.rs`'s `ValidationKind` enum as the source of truth.

The three (`validate_world` + `validate_form_ids` +
`validate_cinematic_entity_refs`) run **before** writing
(`save_io.rs`, `SaveCommand::execute`): a non-empty result aborts the save
outright — `save_world`/`write_slot` are never called, and both the command
surface and `SaveLoadNotifications` report failure. On load,
`validate_snapshot_types` first decodes every registered component/resource
column against the snapshot alone. A decoding failure aborts while the current
session is still intact. Referential and world invariants still run as a
post-load diagnostic because they depend on the rebuilt live world.

## 4. Atomic write + ring buffer

`write_slot` (`crates/save/src/disk.rs:34`): write to
`save_<slot>.ess.tmp` → flush + `sync_all` → re-read and byte-compare
(catches a lying/short-write filesystem) → `fs::rename` over the live
`save_<slot>.ess` → `fsync` the parent directory itself (a bare rename
isn't durable until the directory entry is synced too; Unix-only, this
last step is skipped on Windows).

`SaveRing` (`disk.rs:143`) is a fixed-size round-robin cursor — size
`10`, directory `saves/` relative to the working directory (both set in
`boot::registries::install_runtime_registries`, `byroredux/src/boot/registries.rs`;
the directory comes from `save_io::discover_save_dir`), filename scheme `save_<n>.ess`. `BYROREDUX_SAVE_DIR` overrides the
directory (#3009), mirroring `BYROREDUX_SETTINGS_PATH`: the vertical-slice
gate-5 smoke check saves and reloads for real, and the smoke harness runs the
engine from the repository root, so without the override it would write into
the operator's own ring. `SaveRing::resume` (`disk.rs:166`) scans on-disk
mtimes at boot and starts one slot *past* the newest, so a post-restart
quicksave can't clobber the most recent good save.

## 5. Load trigger

`F9` and the pause-menu **Quickload** button scan slots newest-first and select
the first decodable one, reporting every corrupt slot they skip;
`--load <slot>` queues a specific slot during startup; and the diagnostic
console retains `load <slot>`. All three enter through
`save_io::queue_load_slot`/`LoadCommand`. Being read-only against
`&World`, it can only decode + verify the slot and check it carries exactly one
reload identity: `CurrentCellContext` for an interior or
`CurrentExteriorContext` for a streamed worldspace. A loose-NIF save has
neither and is rejected; it then pushes the decoded
`Snapshot` into a `PendingSaveLoadSlot` resource for the next frame to
drain, because actually applying a load needs `&mut World` **and**
`&mut VulkanContext`, which a console command can't hold. Every load in
this engine — whether queued at boot or mid-session — goes
through the same live load-apply path in §6; there's nothing else to
distinguish, since a fresh process simply has no world state to overlay
onto yet.

## 6. Live load-apply (M45.1)

Orchestrator: `execute_pending_save_loads` (`byroredux/src/save_io.rs`),
drained once per frame by `App::step_save_loads` (`byroredux/src/app_step.rs`),
called from `app_events.rs`'s per-frame driver in tick order
`step_streaming → step_debug_loads → step_save_loads →
step_cell_transition`. The sequence below is that function's real body
order — several steps are order-sensitive against each other (#5060
tracks the invariant); cites are by function name, not line, so they
survive edits:

1. **Preflight (abort before any teardown)**: `validate_snapshot_types`
   decodes every typed column from the snapshot alone, and
   `extensions::preflight_extension_state` snapshots the sandboxed
   extension state that must survive the teardown (see
   [Engine-native extension state](#engine-native-extension-state),
   #4145). A corrupt/stale save aborts with the current session intact.
2. **Pre-reload resource restore** (#3789):
   `restore_resources_subset(..., PRE_RELOAD_RESOURCES)` — today
   `ReferenceEnableState`, `ReferenceLockState`, `ReferenceScriptState` —
   installs the saved ledgers that *spawn-time* consumers read while the
   reload is building the new cell (`cell_loader::spawn::
   placement_is_disabled`, `scripted_lock_override`). Deliberately a
   subset, never the wholesale `restore_resources`: the reload's
   `unload_cell` still needs the **live** `ItemInstancePool` to release
   against (#4135 — a wholesale restore here frees slots in the save
   being loaded). Failure aborts before the irreversible teardown.
3. **Reload, under `without_parked_state`**: the snapshot's cell/exterior
   context picks `reload_interior_session` or `reload_exterior_session`.
   Each helper preflights its target *before* the destructive teardown
   (`validate_cell_loadable` / the exterior world-context build;
   SAVE-D6-02 — a missing cell strands no one in the void), then purges
   the live cinematic/tether retention rows (#5056 — the convoy despawns
   with the cell instead of duplicating the reload's fresh spawns; after
   the preflight, since an aborted load keeps the live session), drains
   the streaming state and unloads the current interior/exterior
   (`CurrentCellRoot` + `CurrentCellContext` cleared together, #3021).
   Interiors then run the same `cell_loader::load_cell_with_masters`
   [Pipeline Overview](pipeline-overview.md) traces; exteriors rebuild
   `WorldStreamingState` through `assemble_exterior_streaming` in
   `FullRadius` bootstrap mode so every cell in the saved radius settles
   before delta remap (#3280). `without_parked_state` itself suppresses
   ordinary eviction capture/respawn restoration so the outgoing
   session's parked state cannot contaminate the save (#4695).
4. **Story-manager reset** (#5435): `reset_story_location_cursor` — the
   CLOC cursor still describes the outgoing session — and
   `discard_pending_story_events`, whose markers carry session-local
   `EntityId`s the reload may have reused for unrelated entities.
5. **Extensions restore**: `extensions::restore_extension_state` rebinds
   transient SDK entity handles through stable FormRef identity
   (#4145/#4139 context below).
6. **Wholesale resource restore**: `restore_resources`
   (`crates/save/src/driver.rs`) replaces resources like
   `ItemInstancePool` and `GameTimeRes` wholesale — safe now that the
   teardown already released against the live pool — and re-asserts
   every saved value over what the reload itself rebuilt
   (`CurrentCellContext`, `PlayerPose`; idempotent over step 2).
7. **Resident reference rows** (#4695): `cell_loader::reference_state::
   restore_resident` re-runs the per-placement restore the suppressed
   store skipped during the reload — the pickup tombstone of an item
   taken in this very cell first among them. Rows whose reference is not
   resident stay parked for their next eviction.
8. **Drop entity-bound continuations** (#4139):
   `PapyrusProviderContinuationQueue::drop_entity_bound_continuations`
   purges suspended scripts whose locals hold session-local `EntityRef`
   handles that could otherwise resolve to a *different live entity*.
9. **Re-seat ambient packages** (#4815):
   `npc_spawn::ai_package::reseat_ambient_packages_after_restore` — the
   reload picked every NPC's ambient package against the outgoing
   session's clock; re-seat against the restored one **before** the
   overlay, so the saved procedure state lands on the saved winner and
   the first Update tick does not clear it as a package change.
10. **Reconcile entity identity**: `build_form_id_remap`
    (`crates/save/src/driver.rs`) matches each saved `FormIdPair` against
    the freshly-reloaded cell's live `FormIdComponent`s, building a
    saved-entity → live-entity map. (This is the piece §2's "entity ids
    round-trip exactly" doesn't cover — those ids are stable *within* a
    snapshot, but the reload just spawned brand-new session-local ids
    for the same logical objects.)
11. **Park non-resident rows** (#5054): `unresolved_form_id_pairs` →
    `cell_loader::reference_state::park_unresolved_snapshot_rows`. An
    exterior save holds resident cells out to the hysteresis ring
    (`radius_load + 1`) while the reload streams only `radius_load`, so
    the ring's rows cannot overlay; parked, they return with their saved
    state when streamed in.
12. **Overlay deltas**: `apply_deltas` (`crates/save/src/driver.rs`) —
    additive-only, over `MUTABLE_DELTA_COLUMNS` (`save_io.rs`), a curated
    mutable-column set of 24 entries (`Transform`, `Inventory`,
    `EquipmentSlots`, `ActorValues`, `Dead`, the M42 AI-procedure states,
    `CharacterController`, `RigidBodyData` — registered *preserving* so
    its `serde(skip)` `part_surface_materials` survives the overlay,
    #5686 — …) — see that constant's own doc for the session-stability
    checklist a new entry must pass, and step 13 for why *this* list
    can't just be "everything registered." Structural columns and
    `AnimationPlayer`/`AnimationStack` are deliberately excluded — their
    values embed session-local entity/registry-handle fields a key-based
    remap can't fix.
13. **Reconcile derived state** — four reconcilers after the overlay,
    all on the player-success arm except where noted:
    - `combat::reconcile_dead_actor_runtime_state` runs on **both** the
      success and apply-failure arms. `apply_deltas` can only
      insert/update a row, never remove one, so a runtime removal that's
      a *consequence* of overlaid state has to be rebuilt explicitly
      afterward. `Dead` is overlaid as a normal delta, then this removes
      the respawned AI/animation state and reactivates ragdoll — the
      same reconciler the live combat-kill path uses, so save/load and
      in-session death can't drift apart (#3022).
    - `inventory::reconcile_player_equipped_weapon` (#3488) re-derives
      the player's `EquippedWeapon` from the `EquipmentSlots` +
      `Inventory` rows just overlaid (the overlay is additive-only, so a
      saved *absence* — unarmed — cannot clear the live one off the
      process-lifetime player body).
    - `clear_player_gear_handoff_scratch` + `loot_appearance::
      reconcile_worn_gear` (#5034/#5255) — the visual sibling: drop the
      player's mid-life gear handoff scratch, then diff the surviving
      body roots against the just-overlaid slots so worn gear agrees
      with the restored state.
    - `inventory::reset_player_factions_to_record` (#5058) — the ledger
      reset cannot name the player's stale alias-injected memberships,
      so strip them before the first refresh reads them as authored.

    The weapon/gear/factions reconcilers deliberately skip the
    apply-failure arm: it aborts on an admittedly partial overlay, and
    re-deriving equipment state from data already known to be incomplete
    would be worse than stopping.
14. **Post-load diagnostics** (#1844/SAVE-01): `validate_world` +
    `validate_form_ids` + `validate_cinematic_entity_refs`, mirroring the
    save path's pairing. Diagnostic only — a load can't cleanly fall
    back to the previous cell.
15. **Player pose, last**: `apply_player_pose`, from the `PlayerPose`
    resource refreshed every frame post-scheduler by
    `capture_player_pose` — position plus yaw/pitch restored onto
    `InputState` (both camera systems rebuild rotation from that each
    frame, so writing `Transform.rotation` directly wouldn't survive the
    next tick), with a Character-vs-FlyCam branch (Character re-pins the
    camera next frame via `camera_follow_system` and re-syncs the
    kinematic Rapier body; FlyCam just repositions the camera directly).
    A `SessionPhase::LoadComplete` extension event is queued after it.

## What's not covered

Delta application is still structurally **additive-only**
(`crates/save/src/driver.rs`, `apply_deltas`'s doc comment) — it can
insert or update a row, never remove one. That was an unqualified latent
gap as of the 2026-08-18 pass ("an entity despawned mid-session and then
loaded-over just comes back"); it no longer is, for the cases that
started depending on removal semantics. §6 step 13 above is the general
shape the fix established, and it now has **four instances**: persist a
marker fact via the normal additive overlay (`Dead`, or the overlaid
`EquipmentSlots`/`Inventory` rows), then run the *same* reconciler the
live-session path already uses to rebuild whatever runtime state that
fact implies removing — `combat::reconcile_dead_actor`, shared between
the in-session kill branch and this load path (#3022);
`inventory::reconcile_player_equipped_weapon`, shared with the runtime
equip/unequip path (#3488); `loot_appearance::reconcile_worn_gear`,
shared with the live gear-handoff path (#5034/#5255); and
`inventory::reset_player_factions_to_record` (#5058). There is still no
generic delete persistence
mechanism. Reference visibility is no longer part of that gap: scripted
`Disable()` records the stable FormID in the saved `ReferenceEnableState`
resource, and reload/spawn/render consumers reapply it — which requires the
resource-restore ordering in §6 step 2, without which the reload consumer
read the wrong ledger (#3789). Future destructive
domain operations still need an explicit saved fact plus reconciler rather
than teaching the generic `apply_deltas` driver domain semantics.

## Engine-native extension state

Sandboxed extension component rows and principal-private key/value storage use
the normal `BYRSAVE` container rather than a script-extender cosave.
`SaveCommand` asks the executable-owned `ExtensionHost` for a bounded,
format-version-2 `ByroExtensionState` resource before encoding the snapshot.
Live `EntityRef` handles are never serialized: each materialized row must
resolve to a stable SDK `FormRef`, derived from the same load-order-neutral
`FormIdPair` used by the core save remapper. A row on a transient entity without
stable form identity aborts the save rather than being dropped silently.

Live load decodes and validates this payload before cell/streaming teardown.
The current format requires exact active component and principal-storage schema
versions; migrations remain a later Phase 4 deliverable. After the destination
world is rebuilt, rows bind to fresh opaque handles and principal storage is
restored before the next callback. State owned by an unavailable extension or
attached to a form outside the loaded world remains opaque in the host and is
emitted again on the next save. If that form later participates in an event,
its retained row is rebound before guest delivery, so the callback observes and
advances the saved value. Component and storage commands from one callback are
staged and commit together. The outer save CRC covers the extension payload
alongside the ordinary ECS snapshot.

Full original-engine cosave compatibility is explicitly out of scope
("speculative and not a priority," ROADMAP design-decisions table).
There's no versioned migrator chain for save-schema changes — a
`FORMAT_MAJOR` version bump is the only sanctioned path when a saved
struct's shape changes, enforced by a source-scanning tripwire test that
fails CI if `#[serde(default)]` is added to a saved struct without one.
The current major version is 11: provider continuations now persist tagged
literal/local argument expressions, an incompatible nested-shape change that
is rejected rather than guessed in the absence of a migrator.
