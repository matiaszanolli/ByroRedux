# #958 — REN-D8-NEW-14: skinned BLAS BUILD/UPDATE flags lack shared constant

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM8_v2.md`
**Dimension**: Acceleration Structures
**Severity**: LOW
**Confidence**: HIGH
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/958

## Locations

- `crates/renderer/src/vulkan/acceleration.rs:948-949` (build_skinned_blas)
- `crates/renderer/src/vulkan/acceleration.rs:1213-1214` (refit_skinned_blas)

## Summary

Both sites inline `ALLOW_UPDATE | PREFER_FAST_BUILD`. The static path uses the centralised `STATIC_BLAS_FLAGS` (`acceleration.rs:716-720`); skinned path is duplicated. Vulkan spec requires UPDATE flags to match the source BUILD flags (`VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667`).

## Fix (preferred)

Lift `SKINNED_BLAS_FLAGS = ALLOW_UPDATE | PREFER_FAST_BUILD` to a module constant. Mirror `STATIC_BLAS_FLAGS`.

## Tests

Trivially satisfied once both sites reference the same constant.
