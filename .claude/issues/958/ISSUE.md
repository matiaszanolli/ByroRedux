# REN-D8-NEW-14: skinned BLAS BUILD/UPDATE flag set lacks shared constant

**State**: OPEN
**Labels**: enhancement, renderer, low, vulkan

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM8_v2.md`
**Dimension**: Acceleration Structures
**Severity**: LOW
**Confidence**: HIGH

## Observation

`crates/renderer/src/vulkan/acceleration.rs:948-949` (build_skinned_blas):
```rust
let build_flags = vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE
    | vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD;
```

`crates/renderer/src/vulkan/acceleration.rs:1213-1214` (refit_skinned_blas):
```rust
let build_flags = vk::BuildAccelerationStructureFlagsKHR::ALLOW_UPDATE
    | vk::BuildAccelerationStructureFlagsKHR::PREFER_FAST_BUILD;
```

Static path already uses `STATIC_BLAS_FLAGS = PREFER_FAST_TRACE | ALLOW_COMPACTION` (centralised at `acceleration.rs:716-720`, reused at `1512-1513`). The skinned path is inlined at both sites with no shared constant.

## Why it's a bug

Vulkan spec requires `mode = UPDATE` to use the same flag set as the source BUILD (`VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667`). The two literals drift in lockstep on every fix; a future change to one without the other would silently violate the spec invariant.

Not a live bug — the duplicates match today. Future-regression bait.

## Suggested fix

Lift `SKINNED_BLAS_FLAGS = ALLOW_UPDATE | PREFER_FAST_BUILD` to a module constant and reuse at both sites. Mirrors the `STATIC_BLAS_FLAGS` pattern at `acceleration.rs:716-720`.

## Completeness Checks
- [ ] **UNSAFE**: No new unsafe.
- [ ] **SIBLING**: Verify no third `build_skinned_blas`-style site exists. `grep -n "ALLOW_UPDATE" crates/renderer/src/vulkan/acceleration.rs` should return exactly the two known sites.
- [ ] **DROP**: N/A.
- [ ] **LOCK_ORDER**: N/A.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: Optional — `static_assertions::assert_eq!` style pin that both call sites reference the same `SKINNED_BLAS_FLAGS` constant; trivially satisfied once the constant is in place.

## Dedup

- No existing OPEN issue matches.
- Sibling pattern of the existing `STATIC_BLAS_FLAGS` centralization.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
