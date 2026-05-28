# Entity-Component-System

The ECS is the backbone of the engine. All game state lives here — entities
are integers, components are data, systems are logic. There is no base class
hierarchy.

Source: `crates/core/src/ecs/`

> Reconciled 2026-05-28 (Session 42 close) against the current tree. The
> scheduler is no longer "sequential for now": stage-based parallel
> dispatch landed with M27 (closed 2026-05-23) and the declared-access
> diagnostics that unblocked it landed with R7. Both are reflected below.

## Entities

```rust
pub type EntityId = u32;
```

Source: `crates/core/src/ecs/storage.rs`

Entities are plain integers allocated by `World::spawn()`. There is no
`Entity` struct — just a `u32`. This keeps entity handles trivially
copyable and avoids the overhead of generational indices (which can be
added later if needed for entity recycling).

`spawn()` allocates IDs monotonically via `checked_add(1)` and **panics**
if the `EntityId` counter would overflow `u32::MAX` (issue #36) — silent
wraparound used to alias a live ID and corrupt that entity's component
data. `next_entity_id()` exposes the next ID that will be handed out (a
high-water mark, not a live-entity count).

## Components

```rust
pub trait Component: 'static + Send + Sync + Sized {
    type Storage: ComponentStorage<Self> + DynStorage + Default + Send + Sync + 'static;
}
```

Source: `crates/core/src/ecs/storage.rs`

Every component declares its preferred storage backend via an associated
type. This is a compile-time decision — no runtime dispatch on storage layout.
The storage bound additionally requires `DynStorage` (see below) so `World`
can despawn an entity by walking every storage type-erased.

### Declaring a component

```rust
// Gameplay data: O(1) mutation, use SparseSetStorage
struct Health(f32);
impl Component for Health {
    type Storage = SparseSetStorage<Self>;
}

// Hot-path data: cache-friendly iteration, use PackedStorage
struct Transform { position: Vec3, rotation: Quat, scale: f32 }
impl Component for Transform {
    type Storage = PackedStorage<Self>;
}
```

### Storage trait

```rust
pub trait ComponentStorage<T: Component> {
    fn insert(&mut self, entity: EntityId, component: T);
    fn remove(&mut self, entity: EntityId) -> Option<T>;
    fn get(&self, entity: EntityId) -> Option<&T>;
    fn get_mut(&mut self, entity: EntityId) -> Option<&mut T>;
    fn contains(&self, entity: EntityId) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool { self.len() == 0 }

    // Bulk insert — append + single-sort fast path on PackedStorage (#467).
    // Default loops `insert`; called by `World::insert_batch`.
    fn insert_bulk<I: IntoIterator<Item = (EntityId, T)>>(&mut self, iter: I) { ... }

    fn iter(&self) -> Box<dyn Iterator<Item = (EntityId, &T)> + '_>;
    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = (EntityId, &mut T)> + '_>;
}
```

`insert_bulk` is the bulk-import path: `PackedStorage` overrides it with an
append-then-single-sort that turns a cell load's O(n²) shifting into O(n log n).
`World::insert_batch` routes here automatically.

### DynStorage — the type-erased view

```rust
pub trait DynStorage: Send + Sync + 'static {
    fn remove_entity_erased(&mut self, entity: EntityId);
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
```

`ComponentStorage<T>` is parameterised by `T` and returns `Option<T>`, which
makes it non-object-safe. `DynStorage` exposes the operations `World` needs
when walking its storages without knowing each component type — specifically,
dropping all of an entity's components in `World::despawn`, plus the `Any`
upcasts that let `World` downcast back to the concrete `T::Storage`.

## Storage Backends

### SparseSetStorage

Source: `crates/core/src/ecs/sparse_set.rs`

```
sparse: Vec<Option<u32>>   entity → dense index (None = absent)
dense:  Vec<EntityId>      dense index → entity
data:   Vec<T>             dense index → component (parallel to dense)
```

- **Insert:** O(1) — push to end of dense arrays, record in sparse
- **Remove:** O(1) — swap-remove: move last element into gap, fix sparse pointer
- **Lookup:** O(1) — sparse[entity] → dense index → data[index]
- **Iteration:** Dense, but not sorted by EntityId

Best for: gameplay logic, AI states, status effects, inventory — anything
that mutates frequently. The swap-remove trick ensures no gaps and no
shifting. This is the default backend for nearly every built-in component.

### PackedStorage

Source: `crates/core/src/ecs/packed.rs`

```
entities: Vec<EntityId>    sorted by EntityId
data:     Vec<T>           parallel to entities
```

- **Insert:** O(log n) lookup + O(n) shift right (or amortised via `insert_bulk`)
- **Remove:** O(log n) lookup + O(n) shift left
- **Lookup:** O(log n) — binary search
- **Iteration:** Linear, sorted by EntityId — cache-friendly, SIMD-ready

Best for: components read every frame by many systems. In the current tree
`Transform`, `GlobalTransform`, `WorldBound`, and `SceneFlags` are the packed
components (see `components/transform.rs`, `components/global_transform.rs`,
`components/world_bound.rs`, and `components/scene_flags.rs`). The sorted
layout means iterating two PackedStorage types simultaneously is a merge-join.

## World

Source: `crates/core/src/ecs/world.rs`

```rust
pub struct World {
    storages:   HashMap<TypeId, RwLock<Box<dyn DynStorage>>>,
    type_names: HashMap<TypeId, &'static str>,
    resources:  HashMap<TypeId, RwLock<Box<dyn Any + Send + Sync>>>,
    next_entity: EntityId,
}
```

World is a type-map: one storage per component type, one slot per resource
type. Both are wrapped in `RwLock` for interior mutability. Storages are
boxed as `dyn DynStorage` (not bare `dyn Any`) so `World::despawn` can drop
a component without knowing its concrete type. `type_names` records each
storage's `std::any::type_name::<T>()` at creation so the type-erased panic
paths (e.g. a poisoned lock surfaced during `despawn`) can still name the
offending component (#466).

### The key insight

Structural mutation methods take `&mut self`:
```rust
world.spawn()                    // &mut self
world.register::<T>()            // &mut self — pre-create storage for queries
world.insert(entity, component)  // &mut self
world.insert_batch(items)        // &mut self — bulk import, uses insert_bulk
world.remove::<T>(entity)        // &mut self → Option<T>
world.despawn(entity)            // &mut self
world.get_mut::<T>(entity)       // &mut self → Option<&mut T>
world.insert_resource(resource)  // &mut self → Option<R> (old value, if any)
world.remove_resource::<R>()     // &mut self → Option<R>
```

Query and resource access methods take `&self`:
```rust
world.query::<T>()               // &self → Option<QueryRead>
world.query_mut::<T>()           // &self → Option<QueryWrite>
world.get::<T>(entity)           // &self → Option<ComponentRef<T>>
world.has::<T>(entity)           // &self → bool
world.count::<T>()               // &self → usize
world.resource::<R>()            // &self → ResourceRead
world.resource_mut::<R>()        // &self → ResourceWrite
```

The `RwLock` provides the interior mutability. Multiple `QueryRead`s can
coexist. A `QueryWrite` is exclusive per component type. This is the same
pattern as `RefCell` but thread-safe — and it is what lets systems take
`&World` while still mutating component data, which in turn is what makes
the parallel scheduler (M27) possible.

### Multi-component queries

```rust
// Read A, write B — locks acquired in TypeId order
let (q_a, mut q_b) = world.query_2_mut::<A, B>().unwrap();

// Write both — locks still in TypeId order
let (mut q_a, mut q_b) = world.query_2_mut_mut::<A, B>().unwrap();
```

These are the two multi-component helpers in `world.rs` today
(`query_2_mut`, `query_2_mut_mut`). There is no `query_3_mut`/`query_4_mut`;
systems that need a third storage take it with a separate `query`/`query_mut`
call — the `lock_tracker` (below) catches any ordering hazard that creates.

**Deadlock prevention:** Locks are always acquired in `TypeId` sort order,
regardless of the order the generic parameters are spelled. The tracker
scopes are also set up in TypeId order so the global lock-order graph never
sees a spurious ABBA edge when the caller writes `<B, A>` with
`TypeId(A) < TypeId(B)` (#313). Same-type access panics immediately at the
boundary — `query_2_mut::<Foo, Foo>()` hits an `assert_ne!` with the message
`"query_2_mut: A and B must be different component types"` rather than
deadlocking forever.

### Intersection iteration

The real-world use case — iterating entities that have both A and B:

```rust
let (q_vel, mut q_pos) = world.query_2_mut::<Velocity, Position>().unwrap();

// Iterate the smaller set, look up in the larger
for (entity, vel) in q_vel.iter() {
    if let Some(pos) = q_pos.get_mut(entity) {
        pos.x += vel.dx * dt;
        pos.y += vel.dy * dt;
    }
}
```

### Entity lookup helpers

```rust
world.find_by_name("player")           // → Option<EntityId>
world.find_by_form_id(form_id)         // → Option<EntityId>
```

`find_by_name` resolves the string through the `StringPool` resource to a
symbol, then scans `Name` components — the scan compares interned symbols,
not strings, so it is pure integer equality on `FixedString`.
`find_by_form_id` scans `FormIdComponent` storage for a matching handle.

## Queries

Source: `crates/core/src/ecs/query.rs`

### QueryRead

Holds a `RwLockReadGuard`. Multiple `QueryRead`s can coexist, even for
the same component type. Derefs to the underlying storage.

```rust
let q = world.query::<Health>().unwrap();
let hp = q.get(entity).unwrap();
for (id, health) in q.iter() { ... }
```

Single-component `get` on the world returns a `ComponentRef<'_, T>` (a
guard-backed reference) rather than a bare `&T`, since the read guard must
outlive the borrow.

### QueryWrite

Holds a `RwLockWriteGuard`. Exclusive per component type. Derefs to
the underlying storage, supports mutation.

```rust
let mut q = world.query_mut::<Health>().unwrap();
q.get_mut(entity).unwrap().0 -= 10.0;
q.insert(new_entity, Health(100.0));
q.remove(dead_entity);
for (id, health) in q.iter_mut() { ... }
```

## Resources

Source: `crates/core/src/ecs/resource.rs`, `crates/core/src/ecs/resources.rs`

Resources are global state not tied to any entity. Same `RwLock` guard
pattern as queries — `ResourceRead<R>` derefs to `&R`, `ResourceWrite<R>`
derefs to `&mut R`.

```rust
pub trait Resource: 'static + Send + Sync {}
```

### Selected built-in resources

The full set lives across `resources.rs` (and a few colocated with their
subsystem, e.g. `StringPool` and `SchedulerSystemTimings`). The frequently
touched ones:

| Resource | Where | Purpose |
|---|---|---|
| `DeltaTime(f32)` | `resources.rs` | Seconds since last frame |
| `TotalTime(f32)` | `resources.rs` | Accumulated wall-clock seconds |
| `EngineConfig` | `resources.rs` | `vsync`, `target_fps`, `debug_logging` |
| `StringPool` | `string/mod.rs` | Global string-interning table (`Resource`) |
| `SystemList(Vec<String>)` | `resources.rs` | Registered system names, for the debug CLI |
| `DebugStats` | `resources.rs` | Per-frame counters surfaced over the debug protocol |
| `SelectedRef(Option<EntityId>)` | `resources.rs` | Console-selected reference (`prid`); `cam.tp` falls back to it |
| `SchedulerSystemTimings` | `scheduler.rs` | Per-system wall-time of the last `run`, sorted desc (debug-UI Phase 11) |
| `SchedulerAccessReport` | `resources.rs` | Snapshot of the R7 conflict report, surfaced via `sys.accesses` |
| `CpuFrameTimings` | `resources.rs` | CPU-side frame phase breakdown |
| `SkinCoverageStats` / `SkinSlotPool` | `resources.rs` | Skinned-BLAS observability + per-entity GPU bone-palette slot pool (M29.6) |

### Usage

```rust
// Structural — &mut self
world.insert_resource(DeltaTime(0.0));   // returns Option<R> (previous value)

// Access — &self (interior mutability)
let dt = world.resource::<DeltaTime>();          // read
let mut dt = world.resource_mut::<DeltaTime>();  // write
dt.0 = new_value;

// Two resources at once (TypeId-ordered, deadlock-safe)
let (mut a, mut b) = world.resource_2_mut::<A, B>();

// Non-panicking variants
if let Some(dt) = world.try_resource::<DeltaTime>() { ... }
if let Some(mut dt) = world.try_resource_mut::<DeltaTime>() { ... }
let pair = world.try_resource_2_mut::<A, B>();   // Option<(ResourceWrite, ResourceWrite)>
```

A missing resource panics with the type name:
``"Resource `DeltaTime` not found — call world.insert_resource() first"``

## Systems

Source: `crates/core/src/ecs/system.rs`

```rust
pub trait System: Send + Sync {
    fn run(&mut self, world: &World, dt: f32);
    fn name(&self) -> &str { std::any::type_name::<Self>() }
    fn access(&self) -> Option<Access> { None }   // R7 — opt-in
}
```

Systems take `&World` (not `&mut World`) because all mutation goes through
queries and resources, which use interior mutability via `RwLock`.

`access()` is the R7 declared-access hook (default `None`). When a system
returns `Some(Access)`, the scheduler can classify every parallel-stage
pairing as no-conflict / conflict / unknown before rayon is enabled. See
**Declared access** below.

### Blanket impl for closures

```rust
impl<F: FnMut(&World, f32) + Send + Sync> System for F {
    fn run(&mut self, world: &World, dt: f32) { self(world, dt) }
}
```

Any `FnMut(&World, f32)` is a system with no boilerplate — `FnMut` (not `Fn`),
so a closure can capture mutable state directly:

```rust
// Stateless closure
scheduler.add(|world: &World, dt: f32| {
    let mut q = world.query_mut::<Health>().unwrap();
    for (_, health) in q.iter_mut() { health.0 -= 10.0 * dt; }
});

// Stateful closure — captured counter persists across frames
let mut counter = 0u32;
scheduler.add(move |_world: &World, _dt: f32| { counter += 1; });
```

A closure can't override `System::access`, so declare a closure's access at
the registration site via `Scheduler::add_to_with_access` (below).

### Stateful systems

Implement the trait directly when you also want a stable `name()` or a
declared `access()`:

```rust
struct DamageOverTime { dps: f32 }

impl System for DamageOverTime {
    fn run(&mut self, world: &World, dt: f32) {
        if let Some(mut q) = world.query_mut::<Health>() {
            for (_, health) in q.iter_mut() {
                health.0 -= self.dps * dt;
            }
        }
    }
    fn name(&self) -> &str { "DamageOverTime" }
    fn access(&self) -> Option<Access> {
        Some(Access::new().writes::<Health>())
    }
}
```

## Scheduler

Source: `crates/core/src/ecs/scheduler.rs`

```rust
pub struct Scheduler {
    stages: BTreeMap<Stage, StageData>,   // StageData = { parallel, exclusive }
}
```

Systems are assigned to **stages** that run sequentially in a fixed order.
Within each stage, the non-exclusive systems run **in parallel** via rayon
when the `parallel-scheduler` feature is enabled (flipped on with M27,
closed 2026-05-23). The per-storage `RwLock` design serialises conflicting
accesses automatically — no explicit dependency edges are declared.

### Stages

```rust
pub enum Stage {
    Early = 0,        // input, camera, timers — runs first
    Update = 1,       // core gameplay: animation, AI, scripting
    PostUpdate = 2,   // transform propagation — sees Update's results
    Physics = 3,      // physics sync — sees propagated transforms
    Late = 4,         // stats, cleanup — runs last
}
```

Stages run in discriminant order. `add()` is the backward-compatible default
and targets `Stage::Update`.

### Parallel vs exclusive

Each stage has two buckets:

- **Parallel** — added via `add` / `add_to` / `add_to_with_access`. All
  parallel systems in a stage run concurrently under rayon.
- **Exclusive** — added via `add_exclusive` / `add_exclusive_with_access`.
  These run *alone*, sequentially, *after* the stage's parallel batch
  completes. Use this for cleanup, barriers, or systems whose effects are
  mutually exclusive at runtime (M27 Phase 3 re-staged the audio system,
  `spin_system`, the character-mode dispatcher, and `player_controller_system`
  this way).

```rust
let mut scheduler = Scheduler::new();
scheduler.add(animation_system);                       // → Stage::Update (parallel)
scheduler.add_to(Stage::Physics, physics_sync);        // explicit stage
scheduler.add_exclusive(Stage::Late, cleanup_system);  // runs after the parallel batch

// In the game loop:
scheduler.run(&world, dt);
```

`try_add_to` / `try_add_exclusive` are the non-panicking registration
variants (they skip a duplicate system name instead of asserting).

Mutations from an earlier stage are visible to a later stage in the same
`run()` call. Within a parallel stage, systems must not have conflicting
writes to the same storage — that is exactly what the declared-access
report exists to catch. `run` also times each system (one `Instant::now()`
per system) and writes the sorted `(name, ms)` list into the
`SchedulerSystemTimings` resource for the debug-UI.

### Declared access (R7 / M27)

Source: `crates/core/src/ecs/access.rs`

A system can opt into declaring which component storages and which resources
it reads or writes:

```rust
scheduler.add_to_with_access(
    Stage::Update,
    velocity_integrate_system,
    Access::new().reads::<Velocity>().writes::<Position>(),
);
```

`Access` is a builder (`reads::<T>()`, `writes::<T>()`,
`reads_resource::<R>()`, `writes_resource::<R>()`). It is **runtime data**,
not a compile-time contract — it does not change what `World` lets a system
do; it makes contention diagnosable. Three states per system:

- **Declared (empty)** — `Access::new()`: "I touch no ECS state." Conflict-free.
- **Declared (with claims)** — conflict analysis trusts the shape.
- **Undeclared** — `None` (closures and not-yet-migrated systems): every
  pairing is classified `Unknown`, the pessimistic fallback (*not* "no conflict").

`Scheduler::access_report()` returns an `AccessReport` (per-stage
`StageReport` / `StageConflictRow` / `SystemAccessRow`) with helpers like
`undeclared_count()`, `known_conflict_count()`, `unknown_pair_count()`. The
engine binary snapshots this into the `SchedulerAccessReport` resource and
surfaces it via the `sys.accesses` console command. After the M27 migration
the engine's 12 parallel-stage systems report 0 unknown / 0 conflicts; the
remaining undeclared systems are tracked as incremental migration work.

## Built-in Components

### Name

Source: `crates/core/src/ecs/components/name.rs`

```rust
pub struct Name(pub FixedString);
impl Component for Name {
    type Storage = SparseSetStorage<Self>;
}
```

Sparse storage — most entities (static geometry, particles) have no name.
Only actors, triggers, markers, and quest-relevant objects need one.
Equality is integer comparison via `FixedString`.

### Component inventory

The full set lives in
[`crates/core/src/ecs/components/`](../../crates/core/src/ecs/components/),
one module per domain (see `components/mod.rs`). Grouped by domain:

| Domain | Components (module) |
|---|---|
| Hierarchy / spatial | `Parent`, `Children` (`hierarchy`), `CellRoot` (`cell_root`), `Transform` (Packed), `GlobalTransform` (Packed), `LocalBound`, `WorldBound`, `BSBound` |
| Identity | `Name` (Sparse, FixedString-keyed), `FormIdComponent` |
| Render | `MeshHandle`, `TextureHandle`, `Material`, `LightSource` + `LightFlicker`, `Camera` + `ActiveCamera`, `Billboard` (+ `BillboardMode`), `BSXFlags`, `SceneFlags`, `RenderLayer` |
| Volumetrics / water | `FogVolume` (+ `FogBounds`, `FogSource`) for the M55 froxel driver (#1277); `WaterPlane`, `WaterVolume`, `WaterMaterial`, `WaterFlow`, `WaterKind`, `SubmersionState` (M38) |
| Skinning | `SkinnedMesh` (+ `MAX_BONES_PER_MESH`) — GPU pre-skinning bone palette / weights / partition data (M29) |
| Particles | `ParticleEmitter`, `ParticleSoA`, `ParticleForceField`, `EmitterShape` |
| Attach points | `AttachPoint`, `AttachPoints`, `ChildAttachConnections` (BSConnectPoint, #985) |
| Inventory | `Inventory`, `InventoryIndex`, `ItemStack`, `ItemInstanceId`, `EquipmentSlots`, `MAX_BIPED_SLOTS` |
| Animation outputs | `AnimatedVisibility`, `AnimatedAlpha`, `AnimatedDiffuseColor`, `AnimatedAmbientColor`, `AnimatedSpecularColor`, `AnimatedEmissiveColor`, `AnimatedShaderColor`, `AnimatedShaderFloat`, `AnimatedUvTransform`, `AnimatedMorphWeights` |
| Physics | `CollisionShape`, `MotionType`, `RigidBodyData` (`collision`) — Havok-derived shape + motion class (`MotionType::CharacterKinematic` is one variant). The Rapier-backed runtime lives in the `byroredux-physics` crate (`ContactConfig`, `CharacterController`); the ECS-facing component data stays in `core`. |

Most components use `SparseSetStorage`; only `Transform`, `GlobalTransform`,
`WorldBound`, and `SceneFlags` opt into `PackedStorage`.

#### The animated-color split (#517)

Source: `crates/core/src/ecs/components/animated.rs`

Pre-#517 every color channel wrote into a single `AnimatedColor` slot,
so an emissive pulse on the same entity clobbered a diffuse tint and
vice-versa. Each material channel now lives in its own
`SparseSetStorage` (`AnimatedDiffuseColor`, `AnimatedAmbientColor`,
`AnimatedSpecularColor`, `AnimatedEmissiveColor`, `AnimatedShaderColor`)
so an entity carrying both a diffuse and an emissive controller keeps
both animations independent. The matching animation system lives in
[`byroredux/src/systems/animation.rs::apply_color_channels`](../../byroredux/src/systems/animation.rs)
(moved out of the old monolithic `systems.rs` during the Session 34
submodule split) and routes each `ColorChannel.target` to the right
component.

> Application-side ECS systems (`apply_color_channels`, the camera,
> particle, character, water, light-animation, weather, audio and metrics
> systems) live under
> [`byroredux/src/systems/`](../../byroredux/src/systems/) — one submodule
> per subsystem, re-exported through `byroredux/src/systems.rs`. The
> transform-propagation system is the one ECS system that ships in `core`
> itself, exposed as `make_transform_propagation_system` from
> `crates/core/src/ecs/systems.rs`.

## Lock-ordering policy

Source: `crates/core/src/ecs/lock_tracker.rs`

Multi-component queries (`query_2_mut`, `query_2_mut_mut`) acquire RwLocks in
`TypeId` sort order regardless of declaration order. This guarantees
deadlock-freedom for paired access. The `lock_tracker` extends the guarantee
to arbitrary N-lock hold patterns with two checks:

1. **Thread-local reentrancy check** (always on, debug *and* release) —
   catches a thread that holds a read lock on `T` and then tries to write
   `T` on the same thread, which `std::sync::RwLock` would otherwise
   deadlock on silently. It panics at acquisition instead.

2. **Global lock-order graph** (debug builds only, #313) — records observed
   "acquired-while-held" edges per type across all threads. If one thread
   observed `A → B` and another `B → A`, the graph has a cycle and the
   second observation panics. This catches cross-thread ABBA risks the
   thread-local tracker cannot see — e.g. two systems on separate rayon
   workers acquiring the same pair of single-type queries in opposite
   orders. Once the graph stabilises, every acquisition takes the read-only
   fast path.

Same-type paired access is rejected up front: `query_2_mut::<Foo, Foo>()`
hits an `assert_ne!` (`"query_2_mut: A and B must be different component
types"`) rather than blocking forever. A poisoned storage or resource lock
panics with a message naming the offending type, so a cascading failure
points back at the system that originally panicked while holding the lock.
