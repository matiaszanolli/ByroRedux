# Gamebyro SDK — Architecture Reference for Gamebyro Redux

Source: Emergent Game Technologies SDK, VC80 (Visual Studio 2005), ~2007.  
Used in: Oblivion, Fallout 3, Fallout: New Vegas, and the basis for Creation Engine.  
Files analysed: `CompileSDK.sln` (26 projects), `CoreLibs.zip` (1,825 files, 10 modules).

---

## Module Map

| Module | Our Equivalent | Notes |
|---|---|---|
| `NiSystem` | `platform/` crate | Memory, threading, platform abstraction |
| `NiMain` | `core/` + `renderer/` | Scene graph, rendering pipeline, math |
| `NiAnimation` | Future `animation/` crate | Interpolators, actor manager, blend trees |
| `NiFloodgate` | `Scheduler` (rayon TODO) | Parallel job system — see below |
| `NiCollision` | Future `physics/` crate | Triangle/bound intersection |
| `NiPortal` | Future scene management | Portal-based visibility culling |
| `NiParticle` | Future `vfx/` crate | Particle system |
| `NiDX9Renderer` / `NiD3D10Renderer` | `renderer/` (Vulkan) | We replace entirely |
| `NiAudio (Miles)` | Future `audio/` crate | Miles Sound System — replace with kira/cpal |

---

## The Object Hierarchy (What We're Replacing with ECS)

```
NiMemObject          ← custom allocator
  NiRefObject        ← ref counting + NiPointer<T> smart pointers
    NiObject         ← RTTI, cloning, NiStream serialisation (.nif files)
      NiObjectNET    ← Name + ExtraData (proto-ECS!) + TimeControllers
        NiAVObject   ← Transform (local+world), Bounds, Properties, Collision, Culling flags
          NiNode     ← Children array (scene graph node), DynamicEffects
          NiGeometry ← Renderable geometry base
            NiTriShape     ← Triangle mesh
            NiTriStrips    ← Strip-optimised mesh
            NiParticles    ← Particle geometry
```

**The core problem:** Every object carries everything — name, extra data, controllers,
transform, bounds, collision, render properties — whether it needs them or not.
A static rock and a player character share the same base allocation.
This is the weight Bethesda has been carrying since 1996.

Our ECS replaces this entirely: components only exist on entities that need them.

---

## What to Steal

### 1. NiFixedString / NiGlobalStringTable

Gamebyro interns all strings into a global table. Equality is O(1) pointer comparison.
Every object name, extra data key, and shader constant is a `NiFixedString`.

**Why it matters:** An open-world engine names thousands of objects. String comparison
in hot paths (scene graph search, shader lookup) is death. Interning fixes this.

**Our implementation:** Use the `string-interner` crate in `core/`.

```rust
// core/src/string_interner.rs
use string_interner::{StringInterner, DefaultSymbol};

pub type FixedString = DefaultSymbol;
// Equality is integer comparison. Zero allocation after first intern.
```

### 2. NiExtraData — The Proto-ECS Pattern

`NiObjectNET` carries `NiExtraData** m_ppkExtra` — a sorted array of typed data
blobs keyed by `NiFixedString`. This is literally ad-hoc components bolted onto
every object. SKSE exploits this heavily for modding because it's the only
extensible data attachment mechanism in the engine.

**The lesson:** They knew they needed arbitrary per-object data. They just solved it
wrong (per-object array instead of central storage, string keys instead of TypeId).
Our ECS component system is the correct version of this idea.

**What this tells us about modding:** Script extenders (SKSE, F4SE) exist largely
because modders need to attach state to objects that Papyrus can't express.
Our scripting layer must treat extensible component attachment as a first-class
feature, not an afterthought.

### 3. NiFloodgate — Parallel Job System

`NiStreamProcessor` (singleton) manages a thread pool with four priority queues:
`LOW`, `MEDIUM`, `HIGH`, `IMMEDIATE`. Jobs are `NiSPTask` instances grouped
into `NiSPWorkflow` dependency graphs, analysed by `NiSPAnalyzer` before dispatch.

**This is exactly our Scheduler's `// TODO: rayon` comment.**

The mapping:
- `NiSPWorkflow` → a frame's set of systems with declared dependencies
- `NiSPTask` → a single System run
- `NiSPAnalyzer` → dependency analysis: systems with non-overlapping write sets run in parallel
- Priority queue → Pre-update / Update / Post-update / Render phases

**Our path:** When adding parallel execution, model the phase structure on Floodgate.
Systems declare `fn reads() -> &[TypeId]` and `fn writes() -> &[TypeId]`.
Scheduler builds a dependency graph per frame, runs non-conflicting systems via rayon.

### 4. NiTimeController — Animation Attachment

Controllers are a linked list (`m_spControllers`) on every `NiObjectNET`.
Each controller polls time and mutates the object's properties.

**Translation to ECS:**
- `NiTimeController` → `AnimationController` component
- The controller's `Update(float fTime)` → an `animation_system` that queries
  all entities with `AnimationController` + `Transform`
- Blend weights (`NiBlendInterpolator`) → `AnimationBlendState` component

### 5. NiStream — .nif Serialisation

The `.nif` format is Gamebyro's scene serialisation. `NiStream` handles
load/save with object linking (PostLinkObject resolves inter-object pointers
after loading). This is the source of much save game complexity in Bethesda games.

**Our serialisation target:** A clean binary format (consider `rkyv` for
zero-copy deserialisation) with explicit versioning. The .nif lessons:
- Every object type must declare its serialisable fields explicitly
- Inter-object references need a two-pass load (load all → resolve links)
- Save bloat comes from serialising object state that should be derived, not stored

---

## What NOT to Steal

### Selective Update Flags

`NiAVObject` carries `SetSelectiveUpdate()`, `SetSelectiveUpdateTransforms()`,
`SetSelectiveUpdateRigid()` — manual flags to skip subtrees during world update.

**Why they exist:** Tree traversal is expensive when the hierarchy is deep.
These are hand-tuned performance patches for a fundamental architecture problem.

**Why we don't need them:** ECS flat component storage means systems iterate
only entities with relevant components. There's no tree to traverse. A static rock
simply has no `Velocity` component — the physics system never touches it.

### NiPropertyState Propagation

Render properties (alpha, material, shader, fog, stencil, wireframe) cascade
down the scene graph via `UpdatePropertiesDownward` / `UpdatePropertiesUpward`.
Parent nodes accumulate property state for their children.

**Why it's painful:** Property inheritance requires a tree walk every frame,
and overriding a parent property deep in the hierarchy is non-obvious.

**Our approach:** Render properties are components on the entity. No inheritance,
no propagation. Explicit always beats implicit in a moddable engine.

### NiSmartPointer / Reference Counting

`NiRefObject` implements intrusive ref counting. `NiPointer<T>` wraps it.
The codebase is littered with `NiXxxPtr spkObj = new NiXxx(...)`.

**Why we don't need it:** Rust's ownership model and `Arc<T>` handle this
correctly at compile time with no runtime overhead compared to intrusive counting.

### The DX9/D3D10 Renderer

`NiDX9Renderer` and `NiD3D10Renderer` are Windows-only, legacy API.
The abstraction layer (`NiRenderer`) is our model — renderer-agnostic scene
submission — but the implementations are irrelevant.

---

## Key Insights for Gamebyro Redux Design

### The ExtraData Lesson → Scripting Layer

Bethesda's scripters needed to attach arbitrary state to game objects.
Papyrus couldn't do it natively. SKSE had to crack the engine open to provide it.

**Our scripting layer must expose the component system directly.**
A script should be able to define new component types and attach them to entities
at runtime. This is the single biggest moddability win over Creation Engine.

### The NiNode Lesson → Scene Hierarchy as Components

Gamebyro's scene graph (`NiNode` parent/child) mixes two concerns:
1. Spatial hierarchy (parent transform applies to children)
2. Logical grouping (lights affect objects in subtree)

In our ECS:
- Spatial hierarchy → `Parent(EntityId)` component + `transform_propagation_system`
- Light influence → query-based: lights affect entities within range, not subtree

### The NiFloodgate Lesson → Phase-Based Scheduler

Floodgate's priority enum maps cleanly to game loop phases:

| Floodgate Priority | Our Phase | Contains |
|---|---|---|
| `HIGH` | Pre-update | Input, physics, AI |
| `MEDIUM` | Update | Game logic, animation |
| `LOW` | Post-update | Camera, culling, bounds |
| `IMMEDIATE` | Render | Submission to Vulkan |

### The NiFixedString Lesson → Name as Component

In Gamebyro, every object has a name baked into `NiObjectNET`.
In our engine, name is a component:

```rust
struct Name(FixedString);
impl Component for Name {
    type Storage = SparseSetStorage<Self>; // most entities won't have names
}
```

Static geometry typically has no name. Only actors, triggers, markers, and
quest-relevant objects need names. Sparse storage makes this free.

---

## Recommended Next Steps for Gamebyro Redux

In priority order:

1. **FixedString resource** — `string-interner` crate in `core/`. Register as a `Resource<StringInterner>`. All entity names, asset paths, and shader identifiers go through it.

2. **Name component** — `Name(FixedString)` with `SparseSetStorage`. Gives us `GetObjectByName` equivalent via ECS query.

3. **Transform hierarchy** — `Parent(EntityId)` + `Children(Vec<EntityId>)` components + `transform_propagation_system`. Replaces `NiNode`'s scene graph. This is the first system with real game-world meaning.

4. **Scene serialisation design** — before the first real game objects are created, design the save format. The .nif lessons are expensive to learn after the fact.

5. **Scheduler phases** — split `Scheduler` into Pre-update / Update / Post-update / Render phases matching Floodgate's priority model. Systems declare their phase at registration.

---

## File Reference

Key files for future deep-dives:

| File | What it contains |
|---|---|
| `NiMain/NiObject.h` | Root of hierarchy — RTTI, cloning, streaming |
| `NiMain/NiObjectNET.h` | ExtraData pattern, TimeController attachment |
| `NiMain/NiAVObject.h` | Transform, bounds, properties, selective update flags |
| `NiMain/NiNode.h` | Scene graph children, effect propagation |
| `NiMain/NiFixedString.h` | String interning — steal this |
| `NiMain/NiStream.h` | Serialisation — learn from mistakes |
| `NiFloodgate/NiStreamProcessor.h` | Parallel job system — our Scheduler target |
| `NiAnimation/NiActorManager.h` | High-level animation state machine (79KB — substantial) |
| `NiAnimation/NiBlendInterpolator.h` | Animation blending — blend trees |
| `NiCollision/*.h` | Collision kernels — future physics crate reference |
