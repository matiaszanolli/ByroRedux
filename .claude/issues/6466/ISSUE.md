# GAME-D5-2026-10-27-01: The Find package's "activate" arrival inserts `ActivateEvent` directly from PostUpdate, so every Update OnActivate consumer misses it

Labels: medium, gameplay, ai, game:fnv, game:fo3, bug
Source: docs/audits/AUDIT_GAMEPLAY_2026-10-27.md

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-27.md` finding `GAME-D5-2026-10-27-01` (MEDIUM)
**Location (re-mapped to current tree)**: `byroredux/src/systems/find_package.rs` (`Arrival::Activate` arm, ~:375-380; registered in `Stage::PostUpdate`, `byroredux/src/boot/schedule/post_update.rs:127`)

**Severity**: MEDIUM. Dimension: NPC Spawn -> AI Package (Dim 5); stage order (Dim 7). Games: FO3/FNV. From `fcd1cce4a` (2026-10-24).

**Trigger**: a Find package whose target classifies as `Arrival::Activate` (a non-actor, non-pickup reference: activator, door, container; `classify_arrival`). The actor reaches it.

## Description
`PendingFragmentActivations::push` (`crates/scripting/src/fragment/state.rs` ~:891-901) documents itself as "the supported way for any producer that runs *after* an `ActivateEvent` consumer to reach every consumer exactly once", warning that a direct insert "is delivered to whichever consumers happen to be scheduled later and drained by `event_cleanup_system` the same frame". This hazard is closed issue #2654 (quest fragments), #3936 (scene-package Activate leaves) and #4712 (loot). `find_package_system` runs in PostUpdate and calls `events.insert(target, ActivateEvent { activator: actor })` directly. By then every Update consumer has already run: `fragment_activation_flush_system`, `door_activation_system`, `container_loot_system`, `two_state_activator_system`, the ObScript object-script and activate-parent passes, and the Papyrus OnActivate dispatch. Only the Late readers (`late.rs` ~:437-551) see the event before cleanup removes it.

## Evidence
```rust
Arrival::Activate => {
    if let Some(mut events) = world.query_mut::<byroredux_scripting::ActivateEvent>() {
        events.insert(target, byroredux_scripting::ActivateEvent { activator: actor });
    }
    progress.done = true;
}
```
(verified at HEAD; `find_package_system` registered via `add_exclusive(Stage::PostUpdate, make_find_package_system())`).

## Impact
An authored "find X and activate it" never runs the target's `OnActivate` script, never flips a two-state activator, and never reaches the Update script blocks. Late consumers see it, so delivery is inconsistent. `progress.done = true` means the package never retries: the miss is silent and permanent until the package is re-installed. How many of the 81 FNV / 78 FO3 Find packages target an activator was not measured.

## Related
#2654, #3936, #4712 (same class, closed); `activation_flush_is_scheduled_before_every_activate_event_consumer` pins consumers, not producers.

## Suggested Fix
Push through `PendingFragmentActivations` (`push`, or `push_native` if default processing only is meant) so the event is delivered at the head of the next frame. Add a PostUpdate-producer case to the activation-order guard.

## Completeness Checks
- [ ] **SIBLING**: Other PostUpdate/Late producers of `ActivateEvent` audited
- [ ] **TESTS**: A regression test pins this specific fix

