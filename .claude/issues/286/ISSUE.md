# #286: P6-02: Per-frame to_ascii_lowercase() in draw command loop — 300 String allocs/frame

## Finding
**Severity**: MEDIUM | **Dimension**: CPU Allocations | **Type**: performance
**Location**: `byroredux/src/render.rs:252`
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-04-13.md`

## Description
`tp.to_ascii_lowercase()` allocates a new String for every textured entity each frame to check for effect mesh patterns (fxsoftglow, fxpartglow, etc.).

## Impact
~300 String allocations per frame on a 500-entity cell.

## Fix
Add `is_effect_mesh: bool` to Material, populated once at import time in cell_loader.rs and scene.rs. Eliminates per-frame string allocs entirely.

## Completeness Checks
- [ ] **SIBLING**: Check cell_loader.rs FX mesh filtering (line ~291) for same pattern
- [ ] **TESTS**: Existing tests stay green — pure optimization
