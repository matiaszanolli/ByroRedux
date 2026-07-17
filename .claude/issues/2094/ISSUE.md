# SKY-D3-NEW-02: Slot-displaced armor pieces still render — no mesh-level exclusion on overlapping biped slots

**Severity**: MEDIUM
**Labels**: medium, ecs, legacy-compat, bug
**Location**: `byroredux/src/npc_spawn.rs:629` (`build_npc_equip_state`, prebaked path), `byroredux/src/npc_spawn.rs:1331` (`spawn_npc_entity`, kf-era path)
**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-07-16.md` (SKY-D3-NEW-02)

## Description
`EquipmentSlots::equip()` returns the inventory indices displaced when a new item claims a biped bit another item already occupied, specifically so callers can drop the displaced mesh from the render set. Neither spawn path does this — the prebaked path discards the return value (`let _ = equipment_slots.equip(...)`), the kf-era path only logs the displaced indices at `debug!`. Every armor whose mesh resolves gets pushed to the render list regardless of later displacement. Reachable via the in-scope multi-pick LVLI mechanic (bit `0x02`), which intentionally expands every eligible entry.

## Evidence
`build_npc_equip_state` loop (`npc_spawn.rs:613-639`) pushes to `armor_to_spawn` unconditionally per resolved form ID; no post-loop filter against `equipment_slots.occupants` exists.

## Impact
Visual z-fight / double-geometry overlap for NPCs whose gear list produces overlapping biped-slot armor (multi-pick LVLI outfits, mod-added CNTO overlapping a default OTFT slot).

## Related
Sibling gap to the already-fixed `body_covered`/`armor_covers_main_body` upperbody-skip mechanism (base-body-vs-armor case); this is the armor-vs-armor case, left unaddressed.

## Suggested Fix
After building the full expanded equip list and running every entry through `equipment_slots.equip()`, do a second pass over `armor_to_spawn` dropping any entry whose inventory index no longer appears in `equipment_slots.occupants`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both the prebaked path at `npc_spawn.rs:629` and the kf-era path at `npc_spawn.rs:1331` need the same post-displacement filter)
- [ ] **TESTS**: A regression test pins this specific fix
