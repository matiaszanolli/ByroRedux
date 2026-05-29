# Unified Severity Definitions — ByroRedux

This file is referenced by all audit skills. Do NOT use as a slash command.

**Severity is about IMPACT, not likelihood.** A rare but catastrophic bug is CRITICAL, not MEDIUM.

## CRITICAL
Immediate, unrecoverable failure. No workaround.
- GPU crashes (VkDevice lost, unrecoverable pipeline state)
- Memory corruption (writing freed GPU memory, double-free)
- Undefined behavior (data races on Vulkan queue, use-after-free)
- Data loss (corrupted NIF parse state affecting subsequent blocks)
- FFI lifetime violations (dangling pointers across cxx bridge)

## HIGH
Fails under realistic conditions. Workaround exists but is fragile.
- Deadlocks (RwLock ordering violation in common query patterns)
- Resource leaks that compound per frame (GPU memory, descriptors, command buffers)
- Incorrect synchronization (missing pipeline barrier, fence misuse)
- Missing cleanup on swapchain recreate
- Vulkan validation layer errors in normal operation
- NIF parse failures that prevent loading game content

## MEDIUM
Incorrect behavior with workarounds, defense-in-depth gaps.
- Inefficient GPU memory usage (unnecessary staging, suboptimal layouts)
- Missing error handling on recoverable paths
- NIF parser consuming wrong byte count (block_size adjustment covers it)
- Suboptimal pipeline state (unnecessary state transitions)
- Unsafe blocks without safety comments

## LOW
Code quality, maintainability, hardening opportunities.
- Dead code, unused imports
- Missing documentation on public APIs
- Naming convention violations
- Redundant allocations in non-hot paths
- Test coverage gaps (but code works correctly)

## Special Rules

| Condition | Minimum Severity |
|-----------|-----------------|
| Vulkan spec violation | HIGH |
| `unsafe` block without safety comment | MEDIUM |
| Memory/resource leak per frame | HIGH |
| NIF parse failure (hard error) | HIGH |
| NIF parse mismatch (stream position off) | MEDIUM |
| ECS deadlock potential | HIGH |
| FFI lifetime violation | CRITICAL |
| BLAS/TLAS build with wrong geometry or address | CRITICAL |
| Missing AS barrier (build → shader read) | HIGH |
| Ray query self-intersection (wrong tMin/origin bias) | HIGH |
| SVGF reprojection using wrong motion vectors | HIGH |
| Denoiser ghosting (missing disocclusion check) | MEDIUM |
| SSBO index mismatch (instance_custom_index vs draw index) | CRITICAL |
| G-buffer format mismatch (shader output vs attachment) | HIGH |
| Composite reassembly wrong order (tone map before add) | MEDIUM |
| Wrong/divergent `Material` out of NIFAL `translate_material` | HIGH |
| Translatable block silently dropped by NIFAL (collision shape / particle emitter params) | MEDIUM |
| `#[repr(C)]` GPU struct size/layout drifts from shader struct (`GpuInstance` / `GpuCamera` / `GpuMaterial`) | HIGH |

> **NIFAL rows** (the canonical-translation rows above) gate the single
> `ImportedMesh → Material` boundary in `byroredux/src/material_translate.rs`
> (`translate_material`). A wrong translation there is silently wrong across
> *every* game because `Material::metalness` / `roughness` are plain resolved
> `f32` (`Material::resolve_pbr`, `crates/core/src/ecs/components/material.rs`)
> with no per-draw classifier fallback to mask it — hence the HIGH minimum.
> See also `/audit-nifal` for the dimension-level checklist of this layer.

## Decision Tree

```
Is it a Vulkan spec violation?
  → YES: At least HIGH
Does it corrupt acceleration structures or SSBO indexing?
  → YES: CRITICAL (wrong geometry in AS or wrong SSBO lookup = GPU crash or garbage rendering)
Does a `#[repr(C)]` GPU struct (GpuInstance/GpuCamera/GpuMaterial) drift from its shader-side layout?
  → YES: At least HIGH (the size/offset pins in scene_buffer + material.rs are the lockstep guard; silent per-instance/per-material corruption otherwise)
Does it emit a wrong/divergent Material from NIFAL translate_material?
  → YES: At least HIGH (one boundary, all-game blast radius, no per-draw fallback)
Does NIFAL silently drop a translatable block (collision shape / particle emitter params)?
  → YES: At least MEDIUM (escalate to HIGH if it removes visible game content)
Does it affect GPU memory or rendering correctness?
  → YES: At least HIGH
Does it affect ray tracing synchronization (missing AS barriers)?
  → YES: At least HIGH
Does it affect ECS state or query safety?
  → YES: At least HIGH
Does it affect resource cleanup (leaks per frame)?
  → YES: At least HIGH
Is it an unsafe block without a safety comment?
  → YES: At least MEDIUM
Is it a NIF parse failure (blocks future parsing)?
  → YES: At least HIGH
Does it cause visual artifacts only (denoiser ghosting, wrong tone map order)?
  → YES: At least MEDIUM
Is it a code quality issue only?
  → YES: LOW
Otherwise → MEDIUM
```
