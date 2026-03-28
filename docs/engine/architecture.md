# Gamebyro Redux — Architecture Overview

Gamebyro Redux is a clean rebuild of the Gamebryo and Creation engine lineage,
built from scratch in Rust and C++ using Vulkan. The goal is a modern engine
with Rust's safety guarantees that can eventually load and run content from
Gamebryo/Creation-era games (Oblivion, Fallout 3, New Vegas, Skyrim).

## Workspace Structure

```
gamebyro-redux/
├── Cargo.toml                 Workspace root
├── gamebyro-redux/            Binary crate — game loop entry point
├── crates/
│   ├── core/                  ECS, math, types, string interning
│   ├── renderer/              Vulkan graphics via ash
│   ├── platform/              Windowing via winit (Linux-first)
│   ├── scripting/             Placeholder for embedded scripting
│   └── cxx-bridge/            C++ interop via cxx
└── docs/
    ├── engine/                This documentation
    └── legacy/                Gamebryo 2.3 analysis
```

## Design Principles

1. **ECS over scene graph.** The legacy Gamebryo engine uses a hierarchical
   scene graph where `NiAVObject` is a God Object bundling transforms,
   properties, bounds, collision, and flags. Redux decomposes these into
   independent components. Systems that only need transforms never touch
   collision data.

2. **Interior mutability for concurrent access.** Every component storage
   and resource is wrapped in `RwLock`. Query methods take `&self`, not
   `&mut self`. This enables multiple systems to read different component
   types simultaneously, and is the foundation for future parallel dispatch.

3. **Components declare their own storage.** Each component type specifies
   whether it uses `SparseSetStorage` (O(1) mutation, gameplay data) or
   `PackedStorage` (sorted, cache-friendly iteration, hot-path data) via an
   associated type. No runtime branching on storage layout.

4. **Rust owns the architecture, C++ provides interop.** The engine core,
   ECS, and renderer are Rust. C++ is available through the `cxx` crate
   for performance-critical code or legacy library integration. The FFI
   boundary is explicit and type-safe.

5. **Vulkan done properly.** No shortcuts in the initialization chain.
   Validation layers in debug builds, proper swapchain recreation on
   resize, sorted lock acquisition to prevent deadlocks, clean teardown
   in reverse initialization order.

6. **Linux-first, multiplatform later.** The primary development platform
   is Linux. Platform abstractions exist in the `platform` crate to enable
   future Windows/macOS support without touching engine internals.

## Crate Dependency Graph

```
gamebyro-redux (binary)
├── gamebyro-core
├── gamebyro-renderer
│   ├── gamebyro-core
│   └── gamebyro-platform
│       └── gamebyro-core
├── gamebyro-platform
├── gamebyro-scripting
│   └── gamebyro-core
└── gamebyro-cxx-bridge
```

`gamebyro-core` is the leaf dependency — it has no engine crate dependencies.
All other crates depend on it for ECS types, math, and string interning.

## Current State

- **~3,500 lines** of Rust across 30 source files
- **57 unit tests** all passing
- **4 commits** on master
- Vulkan window opens, clears to cornflower blue, handles resize, shuts down cleanly
- Full ECS with pluggable storage, RwLock-based queries, system scheduler, resources
- String interning with entity naming and lookup
- C++ interop bridge operational
- Game loop wired: time resources updated per frame, systems execute before render
