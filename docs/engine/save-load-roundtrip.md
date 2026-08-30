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
(`save_io.rs:783`, `"save.info"`) is the read-only companion — decode +
verify a slot without touching the live world.

## 2. ECS snapshot capture

Despite "full-ECS snapshot" shorthand elsewhere, this is a **curated
subset by design** — only types that carry player-visible game state;
derived data, GPU handles, and transient event markers are reconstructed
on load, never serialised (`crates/save/src/registry.rs`'s module doc).
`SaveRegistry` (`registry.rs:74`) is a type-erased registry — the same
shape as debug-server's `ComponentDescriptor` (per CLAUDE.md) — storing
a boxed save/load closure pair per component or resource, keyed by a
stable string name. `build_save_registry()` (`byroredux/src/save_io.rs:223`)
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
order) into a `Snapshot` (`crates/save/src/snapshot.rs:78`); rows are
sorted by entity id first for a reproducible CRC.

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
(`ItemStack.instance` resolves in `ItemInstancePool`), UnsavedProgression
(`CharacterLevel.xp != 0` while that type is still save-exempt as
re-derivable — #2947, aborts loudly rather than silently discarding
progress), and NonFiniteMaterial (a `Material` scalar is NaN/Inf — #2687,
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
`boot.rs`), filename scheme `save_<n>.ess`. `BYROREDUX_SAVE_DIR` overrides the
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

Orchestrator: `execute_pending_save_loads` (`byroredux/src/save_io.rs:1203`),
drained once per frame by `App::step_save_loads` (`byroredux/src/app_step.rs:642`),
called from `app_events.rs`'s per-frame driver in tick order
`step_streaming → step_debug_loads → step_save_loads →
step_cell_transition`. Sequence:

1. **Pre-flight**: `validate_snapshot_types` decodes every typed column before
   any mutation. The interior branch then calls
   `cell_loader::validate_cell_loadable`; the exterior branch builds and
   validates its world context. A corrupt/stale save therefore cannot strand
   the player mid-teardown.
2. **Tear down**: drain the streaming state, unload the current interior
   (`streaming_helpers::drain_streaming_state`,
   `cell_loader::unload_current_interior`). The latter clears both
   `CurrentCellRoot` and `CurrentCellContext` together (#3021 — before
   this fix an exterior save taken after visiting *any* interior still
   carried the departed interior's `CurrentCellContext`, so loading it
   reloaded the wrong worldspace entirely; the two resources are one
   invariant now, cleared at every site that clears either).
3. **Reload**: interiors use the same `cell_loader::load_cell_with_masters`
   (`cell_loader/load.rs:298`) that [Pipeline Overview](pipeline-overview.md)
   traces. Exteriors rebuild `WorldStreamingState` through
   `assemble_exterior_streaming` in `FullRadius` bootstrap mode. That mode is
   load-bearing: all cells within the saved radius settle before delta remap,
   preventing saved state outside the arrival cell from being dropped (#3280).
4. **Restore whole resources**: `restore_resources`
   (`crates/save/src/driver.rs`) replaces resources like `ItemInstancePool`
   and `GameTimeRes` wholesale, so instance ids resolve correctly and the
   next weather tick re-derives sky, fog, sun, and exterior directional
   lighting from the saved clock.

   It runs **twice**, and the first call is the load-bearing one (#3789).
   `ReferenceEnableState` has a *spawn-time* consumer —
   `cell_loader::spawn::placement_is_disabled` reads it per placed REFR,
   ahead of any mesh, collider or light — so the cell reload in §3 takes its
   spawn decisions against whatever ledger is live at that moment. Restoring
   only afterwards meant a fresh `--load N` (live ledger empty, everything
   enabled) respawned every `Disable()`d reference solid and interactive,
   with nothing to correct it: `apply_deltas` is additive-only and can
   neither spawn nor despawn. The restore therefore happens before the
   reload; the second call re-asserts saved values over what the reload
   itself rebuilds (`CurrentCellContext`, `PlayerPose`). It is a plain
   per-resource overwrite, so running it twice is idempotent. Doing it first
   also means a resource-restore failure aborts before the irreversible
   cell/streaming teardown rather than after it.
5. **Reconcile entity identity**: `build_form_id_remap`
   (`crates/save/src/driver.rs:226`) matches each saved `FormIdPair`
   against the freshly-reloaded cell's live `FormIdComponent`s, building
   a saved-entity → live-entity map. (This is the piece §2's "entity ids
   round-trip exactly" doesn't cover — those ids are stable *within* a
   snapshot, but the reload just spawned brand-new session-local ids for
   the same logical objects.)
6. **Overlay deltas**: `apply_deltas` (`driver.rs:318`) — additive-only,
   over `MUTABLE_DELTA_COLUMNS` (`save_io.rs`), a curated mutable-column
   set that has grown well past its original seven entries (now ~20:
   `Transform`, `Inventory`, `EquipmentSlots`, `ActorValues`, `Dead`, the
   M42 AI-procedure state, `CharacterController`, `RigidBodyData`, …) —
   see that constant's own doc for the session-stability checklist a new
   entry must pass, and step 7 below for why *this* list can't just be
   "everything registered." Structural columns and `AnimationPlayer`/
   `AnimationStack` are deliberately excluded — their values embed
   session-local entity/registry-handle fields a key-based remap can't
   fix.
7. **Reconcile derived removals**: `combat::reconcile_dead_actor_runtime_state`
   (`byroredux/src/save_io.rs`, called immediately after step 6, both on
   success and on an apply failure). `apply_deltas` can only insert/update
   a row, never remove one, so a runtime removal that's a *consequence* of
   overlaid state has to be rebuilt explicitly afterward. Death is the one
   case wired today: `Dead` is overlaid as a normal delta, then this call
   removes the respawned AI/animation state and reactivates ragdoll —
   `reconcile_dead_actor` is the same reconciler the live combat-kill path
   uses, so save/load and in-session death can't drift apart (#3022). See
   "What's not covered" below for why this is a pattern, not blanket
   removal support.
8. **Player pose**: `apply_player_pose` (`save_io.rs:493`), last. Backed
   by a `PlayerPose` resource refreshed every frame post-scheduler by
   `capture_player_pose` (`save_io.rs:447`) — position plus yaw/pitch
   restored onto `InputState` (both camera systems rebuild rotation from
   that each frame, so writing `Transform.rotation` directly wouldn't
   survive the next tick), with a Character-vs-FlyCam branch (Character
   re-pins the camera next frame via `camera_follow_system` and re-syncs
   the kinematic Rapier body; FlyCam just repositions the camera
   directly).

## What's not covered

Delta application is still structurally **additive-only**
(`crates/save/src/driver.rs`, `apply_deltas`'s doc comment) — it can
insert or update a row, never remove one. That was an unqualified latent
gap as of the 2026-08-18 pass ("an entity despawned mid-session and then
loaded-over just comes back"); it no longer is, for the one case that
started depending on removal semantics. §6 step 7 above is the general
shape the fix established: persist a marker fact via the normal additive
overlay (`Dead`), then run the *same* reconciler the live-session path
already uses to rebuild whatever runtime state that fact implies removing
(`combat::reconcile_dead_actor`, shared between the in-session kill branch
and this load path — see #3022). There is still no generic delete persistence
mechanism. Reference visibility is no longer part of that gap: scripted
`Disable()` records the stable FormID in the saved `ReferenceEnableState`
resource, and reload/spawn/render consumers reapply it — which requires the
resource-restore ordering in §4, without which the reload consumer read the
wrong ledger (#3789). Future destructive
domain operations still need an explicit saved fact plus reconciler rather
than teaching the generic `apply_deltas` driver domain semantics.

Full original-engine cosave compatibility is explicitly out of scope
("speculative and not a priority," ROADMAP design-decisions table).
There's no versioned migrator chain for save-schema changes — a
`FORMAT_MAJOR` version bump is the only sanctioned path when a saved
struct's shape changes, enforced by a source-scanning tripwire test that
fails CI if `#[serde(default)]` is added to a saved struct without one.
