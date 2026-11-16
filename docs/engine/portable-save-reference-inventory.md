# Portable save reference inventory

**Status:** IN PROGRESS (2026-11-15). This is a partial Phase 0 inventory for the [Portable Save References PRD](../prd/portable-save-references.md), checked against commit `278886579`. It is not a complete registry census and does not satisfy `PSR-ID-001` or `PSR-ID-002`. In particular, the full scripting registry, nested parked state, extension state and runtime-created identities remain to be classified.

The authoritative registration entry point is `byroredux/src/save_io/registry.rs::build_save_registry`, including its delegated `register_saved` calls. For each registration, the completed catalog must traverse its actual serialized type, nested types and custom codecs. A field's Rust integer type alone does not establish its identity semantics. Classification must include no-reference types and a registry/shape guard so additions cannot bypass the catalog.

## Pre-reload resources

`byroredux/src/save_io/live_load.rs::PRE_RELOAD_RESOURCES` is the authoritative list of state installed before cell construction. These rows must resolve against the destination identity before their first spawn-time reader; a later entity overlay cannot repair them. The candidate disposition is to preserve a resolved identity, diagnose a missing one, and refuse before teardown unless the complete row and all its dependents have a proven safe drop. No candidate below has yet been approved as safe to drop.

| Registered type and field | Meaning at HEAD | Work needed |
|---|---|---|
| `ReferenceEnableState.disabled` / `enabled` | Reference FormID sets in `crates/scripting/src/fragment/state.rs` | Resolve keys before spawn; classify runtime-created references and missing-row policy. |
| `ReferenceLockState.overrides` key | Reference FormID map key in the same file | Resolve keys before spawn. |
| `LockOverride::Locked.key_form_id` | Optional key-record FormID nested in a lock override | Resolve separately from the reference key; a missing key cannot silently become another key. |
| `ReferenceScriptState.script_parked` | Reference FormID set in the same file | Resolve keys before spawn; classify runtime-created references and missing-row policy. |
| `ReferenceMotionTypeState.overrides` key | Reference FormID map key in the same file | Resolve keys before collider creation; values are motion types, not FormIDs. |
| `ScriptedFactionChanges.by_reference` | Reference FormID map keys and faction FormIDs in each `(u32, Option<i8>)` row (`crates/scripting/src/faction_changes.rs`) | Resolve both identities before faction stamping; rank is an ordinary optional integer. |
| `ActorRelocations.placements` / `journeys` keys | Actor reference FormIDs (`crates/scripting/src/created_references.rs`) | Resolve before home-cell spawning; account for created actors. |
| `Journey.destination` | Destination reference FormID in the same file | Resolve independently of the actor key. `origin`, `arrival` and `last_door` use `CreatedPlacement`; its `CreatedCell` uses editor-id strings and grid coordinates, not FormIDs. |

The loader captures live versions of these six resources in `LiveReloadLedgers` and reinstates them on an aborted reload. Any new preflight or conversion must retain that rollback guarantee, and must not install the saved `ItemInstancePool` before teardown: the outgoing world's teardown uses that live pool.

## Other confirmed identity boundaries

| Type and field | Meaning at HEAD | Work needed |
|---|---|---|
| `Inventory.items[].base_form_id` | Plugin-record key in `crates/core/src/ecs/components/inventory.rs` | Convert by durable identity. A missing stack cannot simply be removed or reordered without addressing its indexed dependents. |
| `ItemStack.instance` | `ItemInstanceId`, an index into `ItemInstancePool` | Preserve pool-index semantics; do not resolve as a plugin record. |
| `EquipmentSlots.occupants[]` / `weapon` | `InventoryIndex` into the inventory vector | Preserve slot-to-stack relationships when a stack is unavailable; prove a safe drop or refuse. |
| `PersistentReferenceStates.rows` / `scripts` keys | Already durable `FormIdPair` map keys (`byroredux/src/cell_loader/reference_state.rs`) | Audit every nested `ReferenceState` and `ScriptState` field as well as resident and nonresident restore paths. |
| `ActorValues`, `ActorVitals`, `TimedRestorations`, `ActiveMagicEffects`, `SkillProgress` actor-value fields | Existing actor-value codecs use durable pairs (`crates/scripting/src/actor_value_save.rs`) | Preserve these codecs and inspect the other fields, including source, effect, caster and script references. |
| `build_form_id_remap` output | Saved entity id to live entity id (`crates/save/src/driver.rs`) | Keep separate from record-reference conversion; it does not remap arbitrary nested FormIDs. |

## Remaining work and acceptance

1. Enumerate every registry row, each serialized field and recursively reachable type, including non-reference types and custom serialization. Record source paths, identity domains, consumers and failure disposition; verify special values and runtime-created ids from their producers and consumers.
2. Design a guard tied to actual registered rows and saved type shapes. A list of selected field names or a search for `form_id` is insufficient: maps and tuples can encode identities without those names.
3. Establish the destination plugin identity before pre-reload state is decoded, while preserving preflight and rollback. Validate current versus saved location and plugin-set handling for both interior and exterior loads.
4. Define tests for regular and light slot movement, removal and slot reuse, all six pre-reload resources, inventory/equipment and item-pool relationships, resident and parked state, created ids, diagnostics and abort preservation.

The owner-run live scenario and the PRD's other exit criteria remain required. This document records observed seams and unresolved work; it makes no claim that changed-order live loads are safe today.
