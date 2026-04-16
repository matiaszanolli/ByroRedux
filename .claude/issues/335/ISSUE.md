# D1-04: NiDynamicEffect affected_nodes list ignored during light import

## Finding: D1-04 (LOW)

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`
**Dimension**: Scene Graph Decomposition
**Games Affected**: Oblivion, FO3, FNV, Skyrim
**Location**: `crates/nif/src/import/walk.rs:362-444` (walk_node_lights)

## Description

NiDynamicEffect (NiLight base) has an `affected_nodes` field (parsed at `blocks/light.rs:48`) that specifies which scene graph subtrees a light should affect. The light walker ignores this list — every NiLight is treated as globally affecting all geometry.

In Gamebryo, a light with a non-empty affected_nodes list only illuminates those specific nodes (e.g., a lantern that should only light the hand holding it).

## Evidence

```
grep -r "affected_nodes" crates/nif/src/import/
# No matches — field never accessed during import
```

`ImportedLight` struct has no field for affected node references.

## Impact

Lights intended to affect specific objects illuminate all nearby geometry instead. Mostly cosmetic; affects character-attached lights most visibly.

## Suggested Fix

Add `affected_node_names: Vec<Arc<str>>` to `ImportedLight`. During import, resolve affected_nodes block indices to node names. In ECS, store as a light-target filter component.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`._
