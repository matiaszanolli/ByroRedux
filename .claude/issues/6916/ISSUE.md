# #6916 — INC-2026-11-14-01: A Papyrus `MoveTo` on a `PlayerRef` property toward an unloaded target evicts the player body

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-1). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: HIGH
- **Location**: `crates/scripting/src/fragment/effects/placement.rs` (`Effect::MoveTo` fallthrough), `byroredux/src/cell_loader/relocated.rs` (`resolve_actor_moves` `is_actor` gate; `live_body`'s `PLAYER_REF` case), `byroredux/src/app_step.rs` (`step_actor_relocations`: `live_body` → `evict_actor`)
- **Changed in**: `placement.rs`, `relocated.rs`, `app_step.rs` (commit `4b71155ca`; `fa2a17854` only added a debug log and does not change this)

## Description
Before 4b71155ca, `Effect::MoveTo` returned early when either end had no body. It now falls through to `object_form_id(moved)` and queues `ActorMoveRequest::ToReference`.

- The lowering recognises the player only via `player_expr_ref` (`translate/effects/receivers.rs`), which matches `Game.GetPlayer()` and locals assigned from it. A `PlayerRef` script property lowers to a plain `Effect::MoveTo`, and `resolve_property_form_id` yields `0x14`.
- `resolve_actor_moves` admits `0x14`: its gate `locate_refr(actor).is_none_or(is_actor)` passes both an unknown reference and the player.
- `step_actor_relocations` then runs `ledger.relocate(0x14, ...)`; `live_body(0x14)` returns the player entity (`if form == PLAYER_REF`), and `evict_actor` releases it.
- The ObScript path is safe: `move_to` diverts the player to `PlayerMoveRequest` first.

## Evidence
- `placement.rs`: `let actor = object_form_id(vmad, world, context, moved, deferred)?; ... queue_actor_move(ToReference { actor, target, .. })`
- `relocated.rs`: `let is_actor = index.cells.locate_refr(actor).is_none_or(|(_, placed)| index.is_actor(placed.base_form_id));` and `if form == PLAYER_REF { return resolve_entity_by_global_form_id(world, form) }`
- `app_step.rs`: `if let (Some(entity), Some(ctx)) = (relocated::live_body(&self.world, actor), ...) { cell_loader::unload::evict_actor(...) }`

## Impact
The player body is despawned on a common Skyrim quest idiom (a scripted teleport to a marker in another cell), and a bogus `0x14` row is written into the saved `ActorRelocations`. The same `is_none_or` gate also admits quest/script FormIDs and runtime-created actors; a created actor is evicted and never respawned (`locate_refr` is `None` for it). Not measured: how often vanilla content calls `PlayerRef.MoveTo(<marker in an unloaded cell>)`.

## Related
XCT-LOC-004, #6852

## Suggested Fix
Reject `actor == 0x14` / the player entity in `resolve_actor_moves` and route it to `PlayerMoveRequest`; require `locate_refr(actor).is_some()` (or an explicit created-actor path); add a regression test with a `PlayerRef`-valued property receiver.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`MoveToPackageLocation`, the ObScript `MoveTo` fallthrough)
- [ ] **SAVE**: If a saved type's shape changes, `serde_default_guard_tests.rs` baseline is refreshed / `FORMAT_MAJOR` bumped
- [ ] **TESTS**: A regression test pins this specific fix
