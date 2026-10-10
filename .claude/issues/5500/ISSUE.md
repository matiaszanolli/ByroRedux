# #5500: GAME-D5-2026-10-09-02: #5391 still anchors In-Cell and Near-Linked-Reference Eat/Sleep packages on wherever the actor stands, and caches an unresolved Near-Reference target as the actor's position for the life of the package

**Labels**: ai, bug, gameplay, medium

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-09.md` — finding `GAME-D5-2026-10-09-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM
- **Dimension**: Dim 5
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:149-161` (`eat_sleep_location`); `byroredux/src/systems/eat_sleep.rs:126-139` (one-shot cache), `:199-218` (`resolve_anchor`), `:289-294` (seat radius)
- **Status**: NEW (an incomplete part of the #5391 fix)
- **Trigger**: FNV/FO3. Any NPC whose winning Eat or Sleep package authors PLDT type 1 (In Cell) or type 6 (Near Linked Reference). Separately, a Near-Reference package that wins at spawn during a budgeted cell load or streaming load.
- **Description**: #5391 fixed Near Editor Location by adding `EditorPlacement`. Three of the remaining cases still land on the actor's current position:
  1. **In Cell.** The anchor is `current` and seats are searched within `radius.unwrap_or(SEAT_SEARCH_RADIUS)` (512 BU) of the arrival point. The GECK's Standard Location says "If 'In Cell' is selected, Radius is greyed out", meaning the whole cell is the location. All 70 FNV In-Cell Eat/Sleep packages author radius 0. A sleeper in a large interior whose bed is more than 512 BU from where it stands takes the nearest chair or nothing. This covers FNV 32 Eat and 142 Sleep NPC-default references (23% of all FNV Sleep references) and FO3 16 Eat and 22 Sleep references. All of the target cells are interiors (`incell_census.py`).
  2. **Near Linked Reference (type 6).** This falls to `_ => NearCurrentLocation`. The commit describes that fallback as "every type with no resolvable anchor", but type 6 is resolvable: `PackageTargetRegistry` already holds every placed reference's XLKR edges and the positions of both ends (`asset_provider/script.rs:716-791`, `linked_reference()` at `crates/scripting/src/package.rs:100`). This covers FNV 11 Eat and 47 Sleep references (e.g. `GamblerEatAtLinkedPackage`, `VRRCJackDianeSleep`) and FO3 2 (`DefaultSleepAlwaysLinkedRef`).
  3. **Unresolved Near Reference.** The target is resolved once and cached: `resolve_near_reference_target(...).unwrap_or(current)` is stored in `EatSleepState` and never retried. The reference loader walks references in authored order and stalls on each NPC job (`references/mod.rs:760`). The scheduler keeps running during budgeted loads (dt is pinned to 0, `app_events.rs:977`). So a spawn-time Eat/Sleep winner can resolve before its marker exists. In FNV, 163 actor × Near-Reference-package pairs place the target *after* the actor in the same cell, and 167 place it in another cell (`ref_order.py`). This leg depends on the frame budget. XMarker targets do resolve once spawned: `spawn_logical_quest_reference` stamps them, and they are the targets for 269 of 288 Eat and 230 of 293 Sleep references.
- **Evidence**:
  ```rust
  PackLocationTarget::Other(_) if location.location_type == 3 => EatSleepLocation::NearEditorLocation,
  _ => EatSleepLocation::NearCurrentLocation,          // types 2, 5, 6, 7
  …
  EatSleepLocation::InCell(cell_form_id) => cell_is_resident(world, cell_form_id).then_some(current),
  ```
- **Impact**: About 31% of FNV Sleep references, and the type-6 Eat references, sleep or dine wherever an earlier package left the actor rather than at the authored bed or table. A load-order race can freeze a Near-Reference diner at its spawn point. This is the same misroute class #5391 was filed for (GAME-D5-2026-10-08-03).
- **Related**: #5391 (closed), #5390, GAME-D5-2026-10-08-02, #5452 (PHYS: `step_toward` discards `blocked`), #5449 (PERF: furniture gather retry).
- **Suggested Fix**:
  - **Type 6**: resolve through `PackageTargetRegistry::linked_reference(<actor ACHR FormID>)` and `position()`.
  - **In Cell**: search seats across the resident cell (no radius cap) rather than 512 BU of the actor.
  - **Unresolved Near Reference**: leave `EatSleepState` unset, so the next tick retries, rather than caching `current`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
