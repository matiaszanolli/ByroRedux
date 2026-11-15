# #6918 — INC-2026-11-14-04: SDK inventory capture folds 64 biped slots into a `u32` (incomplete fix of #6778)

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-4). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: MEDIUM
- **Location**: `byroredux/src/extensions/capture.rs` (`biped_slots` fold); `crates/sdk/src/inventory.rs` (`InventoryEntry::biped_slots: u32`)
- **Changed in**: `crates/core/src/ecs/components/inventory.rs` (commit `9405138e9` widened `MAX_BIPED_SLOTS` to 64 without touching this caller)

## Description
`slots.occupants.iter().enumerate().fold(0_u32, |mask, (bit, occupant)| ... mask | (1_u32 << bit))` now iterates 64 entries. A debug build panics ("attempt to shift left with overflow") for any occupied slot at bit ≥ 32. `[profile.release]` has no `overflow-checks` override, so in release the shift is masked and bit 35 (Starfield spacesuit body) aliases onto bit 3. The SDK type is `u32`, so it cannot express Starfield's 64-bit layer.

## Evidence
`capture.rs`: `.fold(0_u32, |mask, (bit, occupant)| { if *occupant == Some(index) { mask | (1_u32 << bit) } ... })`; `MAX_BIPED_SLOTS: usize = 64`. `git grep occupants` finds no other production fold left un-narrowed (`.take(32)`) or un-widened.

## Impact
A Studio/SDK snapshot of an actor wearing a Starfield spacesuit piece panics the engine thread in a debug build and reports the wrong slot in release.

## Related
#6778; `crates/mod-runtime/src/runtime/convert.rs` (passes `biped_slots` through)

## Suggested Fix
Either widen the fold and `InventoryEntry::biped_slots` / `new()` to `u64` (SDK contract change; update `mod-runtime`), or `.take(32)` with a documented clothing-layer-only contract. Add a capture test with bit 35 occupied.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other `occupants` fold or `u32` biped mask)
- [ ] **TESTS**: A regression test pins this specific fix
