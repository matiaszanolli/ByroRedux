# SAVE-03: No live-load replay path for player-body-owned mutable state (player body carries no form-id key)

**Labels**: medium, ecs, bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1846
**Source**: docs/audits/AUDIT_SAVE_2026-07-02.md

**Severity**: MEDIUM
**Dimension**: Snapshot Completeness & Determinism / M45.1 Live Load-Apply
**Data-Loss Class**: silent-drop (latent)
**Location**: `byroredux/src/scene.rs:677-711` (player-body spawn — no `FormIdComponent`, no `Inventory`); `crates/save/src/driver.rs:132-167` (`build_form_id_remap`)

## Description
The live-load overlay is **exclusively form-id-keyed**: `build_form_id_remap` matches saved→live entities by `FormIdPair`, and `apply_deltas` `filter_map`s out any saved row whose id isn't in that map. The player character body (`scene.rs:677`) is spawned with `Transform` / `GlobalTransform` / `CollisionShape` / `RigidBodyData` but **no `FormIdComponent`** — so it is absent from the remap by construction. Player *pose* is rescued out-of-band by `PlayerPose` + `apply_player_pose`, but any other mutable component that lands on the player body has **no live-load replay path**: it is captured to disk (if registered) yet silently dropped on load because its saved id can't be remapped. NPCs are unaffected — they DO get a `FormIdComponent` at cell spawn (`cell_loader/spawn.rs:263`) and an `Inventory` (`npc_spawn.rs:424`), so their deltas remap correctly.

## Evidence
`grep` confirms `Inventory::new()` is attached only in `npc_spawn.rs` (lines 424, 1120), never to `PlayerEntity`. The player body has no form id. `apply_deltas` → `ApplyFn` drops non-remapped rows (`registry.rs:120-124`).

## Impact
Latent today (the player owns no persistable mutable component besides `Transform`, which is pose-restored separately). The day a player inventory / equipment / actor-value system lands and attaches those components to the player body, **the player's inventory and equipment changes are silently lost on every live `load`** — the single worst data-loss class for a save system, arriving invisibly. (A full `restore_world` loose-mode load would preserve them via saved ids, but the LIVE overlay path — the one players use — cannot.)

## Related
SAVE-02 (form-id keying is the single mechanism).

## Suggested Fix
Before the player system grows persistable state, give the player body a stable identity the remap can key on (a reserved sentinel `FormIdPair`, or a dedicated `PlayerTag` remap entry that `build_form_id_remap` seeds `saved-player-id → live-player-id` from `PlayerEntity`). Add a regression test that a player-body `Inventory` survives a live load.

## Completeness Checks
- [ ] **SIBLING**: Any other player-only component (equipment, actor values) added in future work is checked against this same remap gap before shipping
- [ ] **TESTS**: A regression test attaches `Inventory` to the player body, does a save → live load round trip, and asserts the inventory survives (would currently fail, demonstrating the gap)
