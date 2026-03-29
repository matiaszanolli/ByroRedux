# ByroRedux Engine Documentation

## Engine Documentation

- [Architecture Overview](architecture.md) — design principles, workspace structure, crate graph
- [Entity-Component-System](ecs.md) — components, storage backends, world, queries, resources, systems, scheduler
- [Vulkan Renderer](renderer.md) — initialization chain, modules, frame rendering
- [Game Loop](game-loop.md) — winit integration, per-frame tick, event handling
- [String Interning](string-interning.md) — FixedString, StringPool, Name component
- [C++ Interop](cxx-interop.md) — cxx bridge, FFI boundary
- [Platform Abstraction](platform.md) — windowing, raw handles
- [Dependencies](dependencies.md) — external crates, internal crate graph
- [Testing](testing.md) — 57 tests, coverage by module, how to run

## Legacy Engine Reference

- [Gamebryo 2.3 Architecture](../legacy/gamebryo-2.3-architecture.md) — directory structure, class hierarchy, compatibility mapping
- [Key Source Files](../legacy/key-files.md) — paths to critical headers by subsystem
- [API Deep Dive](../legacy/api-deep-dive.md) — detailed class analysis for NiObject, NiAVObject, NiStream, NiProperty, NiTransform

## Quick Reference

| What | Where |
|------|-------|
| ECS types | `byroredux_core::ecs::*` |
| Math (Vec3, Quat, etc.) | `byroredux_core::math::*` |
| String interning | `byroredux_core::string::{StringPool, FixedString}` |
| Color type | `byroredux_core::types::Color` |
| Vulkan context | `byroredux_renderer::VulkanContext` |
| Window creation | `byroredux_platform::window::create_window` |
| C++ bridge | `byroredux_cxx_bridge::ffi::*` |

## Stats

| Metric | Value |
|--------|-------|
| Rust source files | 30 |
| Lines of Rust | ~3,500 |
| Unit tests | 57 |
| Workspace crates | 6 |
| External dependencies | 13 |
