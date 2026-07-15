# Save/Load Round-Trip: Snapshot to Live Reload

Third in the cross-cutting series alongside [Pipeline Overview](pipeline-overview.md)
(interior cell load) and [Exterior Grid Streaming](exterior-grid-streaming.md).
This one traces the M45/M45.1 save system: what a save actually captures,
how it's written safely to disk, and — the part that makes this engine's
save/load different from a typical "restart and reload a level" design —
how a save gets applied to a *running* engine without a process restart.

> **Currency note.** Verified against the tree as of 2026-07-15, all
> citations checked against current source. One real contradiction found
> in ROADMAP.md while writing this: its M45 milestone row (line 314)
> says "M45.1 player-pose restore closed 2026-06-21" with full
> implementation detail, but its Known Issues list (line 671) still
> says "Open refinement: precise player/camera-pose restore (load
> currently lands at cell default)" — directly contradicting the row
> above it. The code confirms pose restore is implemented and tested
> (`PlayerPose`, `capture_player_pose`, `apply_player_pose`, three
> dedicated tests). Fixed alongside this doc.

## 1. Save trigger

Console command only — no keybind, no CLI flag. `SaveCommand`
(`byroredux/src/save_io.rs:378`, name `"save"`) resolves the target slot
(empty args → `SaveRing::advance()`; explicit `u32` → that slot), runs
the validation gates (§3), then calls `save_world` → `encode` →
`disk::write_slot`. It only needs `&World`, so it's a plain
`ConsoleCommand`, not a deferred/queued action. `SaveInfoCommand`
(`save_io.rs:449`, `"save.info"`) is the read-only companion — decode +
verify a slot without touching the live world.

## 2. ECS snapshot capture

Despite "full-ECS snapshot" shorthand elsewhere, this is a **curated
subset by design**: "only types that carry player-visible game state —
derived data, GPU handles, transient event markers are reconstructed on
load, never serialised" (`crates/save/src/registry.rs:146-151`).
`SaveRegistry` (`registry.rs:70`) is a type-erased registry — the same
shape as debug-server's `ComponentDescriptor` (per CLAUDE.md) — storing
a boxed save/load closure pair per component or resource, keyed by a
stable string name. `build_save_registry()` (`byroredux/src/save_io.rs:162`)
is the binary-side population point: today 10+ components (`Transform`,
`Name`, `Parent`, `Children`, `Inventory`, `EquipmentSlots`,
`LightSource`, `LightFlicker`, `AnimationPlayer`, `AnimationStack`,
`ScriptTimer`, `ActorValues`, `FormIdComponent`) and 4 resources
(`ItemInstancePool`, `CurrentCellContext`, `PlayerPose`, quest stage
state). `save_world` (`crates/save/src/driver.rs:28`) walks the
registry's component/resource entries and a `StringPool` dump (symbol
order) into a `Snapshot` (`crates/save/src/snapshot.rs:64`); rows are
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

`validate_world` (`crates/save/src/validate.rs:60`) checks four
invariants: Hierarchy (`Parent`⇄`Children` agreement, dangling refs),
Equipment (`EquipmentSlots` occupant indexes resolve into `Inventory`),
AnimationClip (`AnimationPlayer.clip_handle` resolves in the registry),
ItemInstance (`ItemStack.instance` resolves in `ItemInstancePool`) —
plus a binary-side `validate_form_ids` (`byroredux/src/save_io.rs:352`,
needs `FormIdPool`, which the save crate doesn't own).

The two run **before** writing (`save_io.rs:407-408`): a non-empty
result aborts the save outright — `save_world`/`write_slot` are never
called, and the command reports the first 20 issues instead. On the
**load** side the same checks run again, but only as a diagnostic
(`log_validation_warnings`) — a load can't cleanly roll back after the
world's already been torn down, so validation there can only warn, not
prevent (documented explicitly in the source as an intentional
asymmetry).

## 4. Atomic write + ring buffer

`write_slot` (`crates/save/src/disk.rs:34`): write to
`save_<slot>.ess.tmp` → flush + `sync_all` → re-read and byte-compare
(catches a lying/short-write filesystem) → `fs::rename` over the live
`save_<slot>.ess` → `fsync` the parent directory itself (a bare rename
isn't durable until the directory entry is synced too; Unix-only, this
last step is skipped on Windows).

`SaveRing` (`disk.rs:117`) is a fixed-size round-robin cursor — size
`10`, directory `saves/` (both set at `boot.rs:894-897`), filename
scheme `save_<n>.ess`. `SaveRing::resume` (`disk.rs:140`) scans on-disk
mtimes at boot and starts one slot *past* the newest, so a post-restart
quicksave can't clobber the most recent good save.

## 5. Load trigger

There's no `--load` CLI flag and no separate "cold boot load" code
path — the only load entry point is the `load <slot>` console command,
`LoadCommand` (`byroredux/src/save_io.rs:533`). Being read-only against
`&World`, it can only decode + verify the slot and check it carries a
`CurrentCellContext` (a loose-NIF or exterior-only save has no cell to
reload into — that's an error here); it then pushes the decoded
`Snapshot` into a `PendingSaveLoadSlot` resource for the next frame to
drain, because actually applying a load needs `&mut World` **and**
`&mut VulkanContext`, which a console command can't hold. Every load in
this engine — whether "at boot" conceptually or mid-session — goes
through the same live load-apply path in §6; there's nothing else to
distinguish, since a fresh process simply has no world state to overlay
onto yet.

## 6. Live load-apply (M45.1)

Orchestrator: `execute_pending_save_loads` (`byroredux/src/save_io.rs:589`),
drained once per frame by `App::step_save_loads` (`byroredux/src/app_step.rs:230`),
called from `main.rs` in tick order `step_streaming → step_debug_loads →
step_save_loads → step_cell_transition`. Sequence:

1. **Pre-flight**: `cell_loader::validate_cell_loadable` (`save_io.rs:625`)
   — non-destructively parses the ESM and confirms the target cell
   exists, so a corrupt/stale save can't strand the player mid-teardown.
2. **Tear down**: drain the streaming state, unload the current interior
   (`streaming_helpers::drain_streaming_state`,
   `cell_loader::unload_current_interior`).
3. **Reload**: the **same** `cell_loader::load_cell_with_masters`
   [Pipeline Overview](pipeline-overview.md) traces (`save_io.rs:646`)
   — not a load-specific variant. The cell comes back exactly as a fresh
   visit would: full GPU upload, physics bodies, everything.
4. **Restore whole resources**: `restore_resources` (`save_io.rs:681`)
   replaces resources like `ItemInstancePool` wholesale, first, so
   instance ids referenced by the delta overlay below resolve correctly.
5. **Reconcile entity identity**: `build_form_id_remap`
   (`crates/save/src/driver.rs:143`) matches each saved `FormIdPair`
   against the freshly-reloaded cell's live `FormIdComponent`s, building
   a saved-entity → live-entity map. (This is the piece §2's "entity ids
   round-trip exactly" doesn't cover — those ids are stable *within* a
   snapshot, but the reload just spawned brand-new session-local ids for
   the same logical objects.)
6. **Overlay deltas**: `apply_deltas` (`driver.rs:205`) — additive-only,
   over a curated *mutable* column set (`Transform`, `Inventory`,
   `EquipmentSlots`, `LightSource`, `LightFlicker`, `ScriptTimer`,
   `ActorValues`). Structural columns and `AnimationPlayer`/
   `AnimationStack` are deliberately excluded — their values embed
   session-local entity/registry-handle fields a key-based remap can't
   fix.
7. **Player pose**: `apply_player_pose` (`save_io.rs:288`), last. Backed
   by a `PlayerPose` resource refreshed every frame post-scheduler by
   `capture_player_pose` (`save_io.rs:242`) — position plus yaw/pitch
   restored onto `InputState` (both camera systems rebuild rotation from
   that each frame, so writing `Transform.rotation` directly wouldn't
   survive the next tick), with a Character-vs-FlyCam branch (Character
   re-pins the camera next frame via `camera_follow_system` and re-syncs
   the kinematic Rapier body; FlyCam just repositions the camera
   directly).

## What's not covered

Per ROADMAP's M45 row (closed 2026-06-21) and `crates/save/src/driver.rs:194-204`:
delta application is **additive-only** — there's no enable/disable/
delete persistence mechanism yet (a latent gap, not an active bug: an
entity despawned mid-session and then loaded-over just comes back).
Full original-engine cosave compatibility is explicitly out of scope
("speculative and not a priority," ROADMAP design-decisions table).
There's no versioned migrator chain for save-schema changes — a
`FORMAT_MAJOR` version bump is the only sanctioned path when a saved
struct's shape changes, enforced by a source-scanning tripwire test that
fails CI if `#[serde(default)]` is added to a saved struct without one.
