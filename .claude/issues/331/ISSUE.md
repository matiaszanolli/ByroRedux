# NIF-D3-05: Havok constraint parsers under-read by ~141 bytes

## Finding: NIF-D3-05 (LOW)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md` / Dimension 3
**Games Affected**: Fallout NV, FO3, Oblivion, Skyrim
**Location**: `crates/nif/src/blocks/collision.rs` (Havok constraint parsers)

## Description

Constraint blocks consistently under-read in the FNV corpus:
- `bhkLimitedHingeConstraint` — 480 blocks, `expected 157 bytes, consumed 16` (−141 B each)
- `bhkRagdollConstraint` — 573 blocks
- `bhkMalleableConstraint` — 59 blocks

Consumed-16 across the entire population indicates the parsers bail after only the constraint header (`num_entities: u32` + two `entity_ref`s + `priority: u32` ≈ 16 bytes) and return without reading the per-constraint descriptor body (pivots, axes, limits).

## Impact

Ragdoll, hinge, prismatic, and malleable constraints lack correct limits/pivots/axes → once physics is wired these joints will flail. No renderer impact today.

## Suggested Fix

Defer until M-collision lands. Audit against `CoreLibs/NiCollision/` Gamebryo 2.3 source and the Havok 2007/2013 reference repos under `/mnt/data/src/reference/`. Track as `legacy-compat` + `low` priority.


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
