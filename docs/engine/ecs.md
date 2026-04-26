# Entity-Component-System

The ECS is the backbone of the engine. All game state lives here — entities
are integers, components are data, systems are logic. There is no base class
hierarchy.

Source: `crates/core/src/ecs/`

## Entities

```rust
pub type EntityId = u32;
```

Entities are plain integers allocated by `World::spawn()`. There is no
`Entity` struct — just a `u32`. This keeps entity handles trivially
copyable and avoids the overhead of generational indices (which can be
added later if needed for entity recycling).

## Components

```rust
pub trait Component: 'static + Send + Sync + Sized {
    type Storage: ComponentStorage<Self> + Default + Send + Sync + 'static;
}
```

Every component declares its preferred storage backend via an associated
type. This is a compile-time decision — no runtime dispatch on storage layout.

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
    fn is_empty(&self) -> bool;
    fn iter(&self) -> Box<dyn Iterator<Item = (EntityId, &T)> + '_>;
    fn iter_mut(&mut self) -> Box<dyn Iterator<Item = (EntityId, &mut T)> + '_>;
}
```

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
shifting.

### PackedStorage

Source: `crates/core/src/ecs/packed.rs`

```
entities: Vec<EntityId>    sorted by EntityId
data:     Vec<T>           parallel to entities
```

- **Insert:** O(log n) — binary search for position, shift right
- **Remove:** O(log n) — binary search, shift left
- **Lookup:** O(log n) — binary search
- **Iteration:** Linear, sorted by EntityId — cache-friendly, SIMD-ready

Best for: components read every frame by many systems (Transform, Velocity,
mesh references). The sorted layout means iterating over two PackedStorage
types simultaneously is a merge-join — very efficient for intersection queries.

## World

Source: `crates/core/src/ecs/world.rs`

```rust
pub struct World {
    storages:  HashMap<TypeId, RwLock<Box<dyn Any + Send + Sync>>>,
    resources: HashMap<TypeId, RwLock<Box<dyn Any + Send + Sync>>>,
    next_entity: EntityId,
}
```

World is a type-map: one storage per component type, one slot per resource
type. Both are wrapped in `RwLock` for interior mutability.

### The key insight

Structural mutation methods take `&mut self`:
```rust
world.spawn()                    // &mut self
world.insert(entity, component)  // &mut self
world.remove::<T>(entity)        // &mut self
world.insert_resource(resource)  // &mut self
```

Query and resource access methods take `&self`:
```rust
world.query::<T>()               // &self → QueryRead
world.query_mut::<T>()           // &self → QueryWrite
world.resource::<R>()            // &self → ResourceRead
world.resource_mut::<R>()        // &self → ResourceWrite
```

The `RwLock` provides the interior mutability. Multiple `QueryRead`s can
coexist. A `QueryWrite` is exclusive per component type. This is the same
pattern as `RefCell` but thread-safe.

### Multi-component queries

```rust
// Read A, write B — locks acquired in TypeId order
let (q_a, mut q_b) = world.query_2_mut::<A, B>().unwrap();

// Write both — locks still in TypeId order
let (mut q_a, mut q_b) = world.query_2_mut_mut::<A, B>().unwrap();
```

**Deadlock prevention:** Locks are always acquired in `TypeId` sort order,
regardless of declaration order. Same-type double-lock panics immediately
with a clear message.

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

### Entity lookup by name

```rust
world.find_by_name("player")  // → Option<EntityId>
```

Resolves through the `StringPool` resource, then scans `Name` components.
No string comparisons in the scan — pure integer equality on `FixedString`.

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

### Built-in resources

| Resource | Type | Purpose |
|---|---|---|
| `DeltaTime(f32)` | Time | Seconds since last frame |
| `TotalTime(f32)` | Time | Accumulated wall-clock seconds |
| `EngineConfig` | Config | vsync, target_fps, debug_logging |
| `StringPool` | Interning | Global string table |

### Usage

```rust
// Structural — &mut self
world.insert_resource(DeltaTime(0.0));

// Access — &self (interior mutability)
let dt = world.resource::<DeltaTime>();          // read
let mut dt = world.resource_mut::<DeltaTime>();  // write
dt.0 = new_value;

// Non-panicking variants
if let Some(dt) = world.try_resource::<DeltaTime>() { ... }
```

Missing resource panics include the type name:
`"Resource 'DeltaTime' not found — call world.insert_resource() first"`

## Systems

Source: `crates/core/src/ecs/system.rs`

```rust
pub trait System: Send + Sync {
    fn run(&mut self, world: &World, dt: f32);
    fn name(&self) -> &str { std::any::type_name::<Self>() }
}
```

Systems take `&World` (not `&mut World`) because all mutation goes through
queries and resources, which use interior mutability via `RwLock`.

### Blanket impl for closures

```rust
impl<F: Fn(&World, f32) + Send + Sync> System for F {
    fn run(&mut self, world: &World, dt: f32) { self(world, dt) }
}
```

Any `Fn(&World, f32)` is a system with no boilerplate:

```rust
scheduler.add(|world: &World, dt: f32| {
    let mut q = world.query_mut::<Health>().unwrap();
    for (_, health) in q.iter_mut() {
        health.0 -= 10.0 * dt;
    }
});
```

### Stateful systems

Implement the trait directly:

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
}
```

## Scheduler

Source: `crates/core/src/ecs/scheduler.rs`

```rust
pub struct Scheduler {
    systems: Vec<Box<dyn System>>,
}
```

Systems run in registration order. Sequential for now — the `RwLock`-per-storage
design already supports concurrent reads, so parallel dispatch (via rayon) is a
matter of replacing the loop. A `TODO` marks the exact insertion point.

```rust
let mut scheduler = Scheduler::new();
scheduler.add(physics_system);
scheduler.add(animation_system);
scheduler.add(render_prep_system);

// In the game loop:
scheduler.run(&world, dt);
```

Mutations from system N are visible to system N+1 in the same `run()` call.
This is tested.

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
[`crates/core/src/ecs/components/`](../../crates/core/src/ecs/components/).
Grouped by domain:

| Domain | Components |
|---|---|
| Hierarchy | `Parent`, `Children`, `CellRoot`, `Transform` (Packed), `GlobalTransform`, `LocalBound`, `WorldBound` |
| Identity | `Name` (Sparse, FixedString-keyed), `FormIdComponent` |
| Render | `MeshHandle`, `Material`, `LightSource`, `Camera` + `ActiveCamera`, `Billboard`, `BSXFlags`, `SceneFlags` |
| Skinning | `SkinnedMesh` (M29 — bone palette, weights, partition data) |
| Animation outputs | `AnimatedVisibility`, `AnimatedAlpha`, `AnimatedDiffuseColor`, `AnimatedAmbientColor`, `AnimatedSpecularColor`, `AnimatedEmissiveColor`, `AnimatedShaderColor` |
| Physics | `RigidBody`, `Collider` (in `byroredux-physics`) |

#### The animated-color split (#517)

Pre-#517 every color channel wrote into a single `AnimatedColor` slot,
so an emissive pulse on the same entity clobbered a diffuse tint and
vice-versa. Each material channel now lives in its own
`SparseSetStorage` so an entity carrying both a diffuse and an
emissive controller keeps both animations independent. The matching
animation systems live in
[`byroredux/src/systems.rs::apply_color_channels`](../../byroredux/src/systems.rs)
and route by `ColorChannel.target` to the right component.

## Lock-ordering policy

Multi-component queries acquire RwLocks in `TypeId` sort order
(`query_2_mut`, `query_3_mut`, etc.) regardless of declaration order.
This guarantees deadlock-freedom across systems that take
overlapping component sets — the global lock-order graph in
`byroredux_core::sync` (opt-in) cross-checks at runtime in debug
builds, catching ABBA cycles before they can materialise.

Same-type double-lock panics immediately with a clear message
(`"already write-locked"`) rather than blocking forever, so the
classic mistake of `query_2_mut::<Foo, Foo>()` is loud at the
boundary.
