**Severity**: LOW · **Dimension**: NPC Equip + FaceGen
**Location**: `byroredux/src/npc_spawn.rs:290-366` (`build_npc_equip_state`, called from `spawn_prebaked_npc_entity`) vs the kf-era `resolve_inherited_inventory` call at `npc_spawn.rs:498`
**Status**: NEW (confirmed still present 2026-06-18; never filed)

## Description
The kf-era spawn path resolves effective inventory through `byroredux_plugin::equip::resolve_inherited_inventory`, which walks the `TPLT` chain when `template_flags & TEMPLATE_FLAG_USE_INVENTORY (0x0100)` is set. The Skyrim/prebaked path (`build_npc_equip_state`) iterates `npc.default_outfit`/`npc.inventory` directly with no TPLT walk (`npc_spawn.rs:306-331`). `template_flags`/`template_form_id` are parsed cross-game (`crates/plugin/src/esm/records/actor.rs`), so leveled/templated Skyrim NPCs with an empty own CNTO inherit gear via TPLT and will spawn naked. The 6 named Bannered Mare NPCs author their own DOFT/CNTO and are unaffected.

## Evidence
- `build_npc_equip_state` (`npc_spawn.rs:290-366`) seeds inventory only from `npc.default_outfit` (`:306`) and `npc.inventory` (`:322`) via `expand_leveled_form_id`; it never calls `resolve_inherited_inventory`.
- The kf-era path calls `byroredux_plugin::equip::resolve_inherited_inventory(npc, npc.level, index)` at `npc_spawn.rs:498` — that helper is already game-agnostic.

## Impact
Render-only naked actors for templated Skyrim NPCs that rely on inherited CNTO (narrower than the kf-era case the same helper already covers — most Skyrim NPCs use DOFT, which is handled). LOW for the named-NPC target.

## Suggested Fix
Seed `build_npc_equip_state`'s inventory from `resolve_inherited_inventory(npc, npc.level, index)`, identical to the kf-era path (already game-agnostic).

## Completeness Checks
- [ ] **SIBLING**: Confirm both spawn paths (prebaked + kf-era) now route inventory through the same `resolve_inherited_inventory` helper, so TPLT handling cannot diverge again
- [ ] **TESTS**: A regression test pins a templated Skyrim NPC (empty own CNTO, `TEMPLATE_FLAG_USE_INVENTORY` set) resolving inherited gear via `build_npc_equip_state`
