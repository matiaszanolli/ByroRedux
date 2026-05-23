# M47.0 — Event Hooks Runtime Design

**Status**: in flight (started 2026-05-23). Tier 3 milestone unblocked by R5 closure (2026-05-16, "go ECS-native" verdict).

**Goal**: ECS-native event-hook dispatch for the canonical Bethesda event set (`OnActivate`, `OnHit`, `OnTriggerEnter`, `OnCellLoad`, `OnEquip`), driven by SCPT cross-refs already parsed in `EsmIndex`. Opaque SCDA bytecode is ignored; M47.0 ships hand-translated scripts via the R5 prototype shape, M47.2 auto-transpiles from `.psc` source.

**Non-goals**:
- No Papyrus VM. No stack-walking continuations.
- No bytecode interpretation. SCDA is logged but never decoded.
- No VMAD decode (Skyrim+ per-instance script overrides) — defaults from the .psc only. VMAD property overrides land with M47.2.

## Data flow

```
ESM parse (already done):
  EsmIndex.scripts        : HashMap<u32 SCPT_FormID, ScriptRecord>
                            { editor_id, source: Option<String>,
                              compiled: Vec<u8> opaque,
                              locals, ref_form_ids, … }
  EsmIndex.npcs / activators / containers / terminals / items
                          : each has `script_form_id: u32` (0 = no script)

REFR spawn (cell_loader/references.rs:169 — the existing per-REFR loop):
  placed_ref.base_form_id → base record (NPC_ / ACTI / CONT / TERM / ITEM)
  base_record.script_form_id → SCPT FormID (or 0)

M47.0 NEW chain:
  if script_form_id != 0:
    script: &ScriptRecord = index.scripts.get(&script_form_id)?
    spawner: ScriptSpawnFn = script_registry.get(&script.editor_id)?
    spawner(world, refr_entity, script)
      → inserts script-state component on refr_entity
        (with .psc defaults; VMAD overrides come later)
```

After spawn, the script's behaviour is per-frame: its dispatcher system
watches the relevant event marker (e.g., `ActivateEvent`) and runs the
state machine.

## ScriptRegistry shape

```rust
// In crates/scripting/src/lib.rs (or new src/registry.rs):
pub type ScriptSpawnFn = fn(
    world: &mut World,
    entity: EntityId,
    script: &byroredux_plugin::esm::records::ScriptRecord,
);

#[derive(Default)]
pub struct ScriptRegistry {
    /// editor_id → spawn function. Editor IDs are stable across plugin
    /// loads (they're authored strings, not FormIDs), so this is the
    /// natural key — multiple SCPT records across plugins that share an
    /// editor_id are intentionally the same script.
    spawners: HashMap<String, ScriptSpawnFn>,
}

impl ScriptRegistry {
    pub fn register(&mut self, editor_id: &str, spawn: ScriptSpawnFn) { … }
    pub fn lookup(&self, editor_id: &str) -> Option<ScriptSpawnFn> { … }
}

impl byroredux_core::ecs::Resource for ScriptRegistry {}
```

Each `papyrus_demo` module exposes a `register_spawner(&mut ScriptRegistry)` that knows its editor_id and how to build the script-state component:

```rust
// crates/scripting/src/papyrus_demo/mod.rs (mirroring the existing register fn):
pub fn register_spawners(registry: &mut ScriptRegistry) {
    registry.register("defaultRumbleOnActivate",
                      spawn_rumble_on_activate);
    quest_advance::register_spawner(registry);
    actor_stats::register_spawner(registry);
    dlc2_ttr4a::register_spawner(registry);
    mg07_door::register_spawner(registry);
}

fn spawn_rumble_on_activate(world: &mut World, entity: EntityId, _script: &ScriptRecord) {
    let mut q = world.query_mut::<RumbleOnActivate>().unwrap();
    q.insert(entity, RumbleOnActivate::default());
}
```

Skyrim VMAD property decoding would, when it lands, override `RumbleOnActivate::default()` with the authored values from VMAD before the insert. For now, defaults from the `.psc` ship through.

## Canonical event markers

Today in `crates/scripting/src/events.rs`:

| Marker | Status | Emit sites |
|--------|--------|------------|
| `ActivateEvent { activator }` | ✅ defined | ❌ no emit site — Phase 4 |
| `HitEvent { aggressor, source, … }` | ✅ defined | ❌ no emit site — future combat work |
| `TimerExpired { timer_id }` | ✅ defined | ✅ `timer_tick_system` |
| `AnimationTextKeyEvents` | ✅ defined | ✅ `animation_system` |
| `OnTriggerEnter { activator }` | ❌ not defined — Phase 5 | ❌ Phase 5 (Rapier sensor) |
| `OnCellLoad` | ❌ not defined — Phase 5 | ❌ Phase 5 (REFR spawn time) |
| `OnEquip { item }` | ❌ not defined — Phase 5 | ❌ Phase 5 (M41 equip pipeline) |

## System registration (M27-aware)

Every demo system that consumes the registry gets declared access at registration in `byroredux/src/main.rs`. The pattern matches Phase 2 of M27 — each system already has clear reads/writes documented in the demo modules.

| System | Stage | Reads | Writes |
|--------|-------|-------|--------|
| `rumble_on_activate_system` | Update | `ActivateEvent`, `RumbleOnActivate`, `PlayerEntity` resource | `RumbleOnActivate` (state transition), `CameraShakeCommand`, `ControllerRumbleCommand` |
| `rumble_tick_system` | Update | (none) | `RumbleOnActivate` |
| `quest_advance_on_activate_system` | Update | `ActivateEvent`, `QuestAdvanceOnActivate`, `QuestStageState` resource | `QuestStageState` resource |
| `actor_stats_*_system` | Update | (see actor_stats.rs body) | (see actor_stats.rs body) |
| `mg07_door_*_system` | Update | (see mg07_door.rs body) | (see mg07_door.rs body) |
| `dlc2_ttr4a_*_system` | Update | (see dlc2_ttr4a.rs body) | (see dlc2_ttr4a.rs body) |

Demo systems land in `Stage::Update` (after `Stage::Early` events like input/weather, before `Stage::PostUpdate` transform propagation). They consume `ActivateEvent` (added by Phase 4) and emit transient marker components that get cleaned up by `event_cleanup_system` at end-of-frame.

## Phase ordering (revisited)

1. **Wire R5 demos into engine init** — papyrus_demo::register(world) called from scripting::register; demo systems added to scheduler with declared access.
2. **ScriptRegistry resource + per-demo register_spawner** — registry is a resource, populated at engine init.
3. **Cell-loader integration** — in references.rs's REFR loop, after entity spawn, look up base_form_id → base record → script_form_id → SCPT → editor_id → ScriptRegistry → spawner.
4. **ActivateEvent emit site** — input action ("E" or controller button) → raycast → if hit a REFR entity, insert ActivateEvent.
5. **Add missing canonical events** — OnTriggerEnter (Rapier sensor body type already supports this; need ECS marker + emit on collision), OnCellLoad (REFR spawn time), OnEquip (M41 equip pipeline).
6. **E2E integration test** — synthetic ESM cell + scripted REFR + synthetic ActivateEvent → assert state transition + cross-subsystem command.

## Lookups needed

A few lookup functions need to exist on `EsmIndex` (or be added) to support Phase 3:

```rust
impl EsmIndex {
    /// Get the script form id attached to a base record. Walks every
    /// record map that carries `script_form_id` in its CommonFields.
    /// Returns `None` when the base record isn't found OR has no script.
    pub fn base_record_script(&self, base_form_id: u32) -> Option<u32> {
        // ACTI / CONT / TERM / NPC_ / item / etc.
        if let Some(r) = self.activators.get(&base_form_id)  { return non_zero(r.script_form_id); }
        if let Some(r) = self.containers.get(&base_form_id)  { return non_zero(r.script_form_id); }
        if let Some(r) = self.terminals.get(&base_form_id)   { return non_zero(r.script_form_id); }
        if let Some(r) = self.npcs.get(&base_form_id)        { return non_zero(r.common.script_form_id); }
        if let Some(r) = self.items.get(&base_form_id)       { return non_zero(r.common.script_form_id); }
        None
    }
}
```

This lives in the plugin crate; the cell loader consumes it.

## Verification checklist for "M47.0 done"

- [ ] `cargo test -p byroredux_scripting` passes the existing R5 demo unit tests
- [ ] `cargo test --workspace` passes 
- [ ] `papyrus_demo::register` is called from `scripting::register`
- [ ] Demo systems are in the engine scheduler with declared access
- [ ] `ScriptRegistry` resource is inserted at engine init, populated with every demo spawner
- [ ] Cell loader looks up base_record.script_form_id → SCPT → ScriptRegistry on every REFR spawn
- [ ] At least one synthetic E2E test: spawn an ACTI REFR whose base has SCRI → defaultRumbleOnActivate; assert RumbleOnActivate component lands; fire ActivateEvent; assert state machine fires + CameraShakeCommand appears on player entity
- [ ] ActivateEvent has an actual emit site in the binary (use-key + raycast)
- [ ] OnTriggerEnter / OnCellLoad / OnEquip exist + have emit sites
- [ ] sys.accesses still reports 0 unknown / 0 conflicts after new system additions
