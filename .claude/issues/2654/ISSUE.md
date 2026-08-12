# #2654: SCR-D6-NEW11-01: A fragment's Effect::Activate marker is emitted after 3 of its 4 consumers and drained the same frame -- every lowered <Ref>.Activate() in a quest fragment is inert

**Severity**: HIGH
**Dimension**: Scripting Runtime Systems (Dimension 6)
**Untrusted-Input**: No
**Location**: producer `crates/scripting/src/fragment.rs:569-585` (`Effect::Activate` arm of `apply_effect`); scheduling `byroredux/src/boot.rs:748` (`rumble_on_activate_dispatch`), `:757` (`quest_advance_dispatch`), `:788` (`two_state_activator_system`), `:797` (`quest_fragment_dispatch`), `:843` (`mg07_on_activate_dispatch`), `:1196` (`event_cleanup_system`, `Stage::Late`)
**Status**: NEW

## Description

`quest_fragment_dispatch_system` is the *last* producer of `ActivateEvent` in the Update stage, but three of its four consumers are scheduled **earlier** in the same stage, and `event_cleanup_system` drains `ActivateEvent` at `Stage::Late` in the *same* frame.

A marker emitted at scheduler slot 797 therefore never reaches a slot-748 / 757 / 788 consumer on this frame, and no longer exists on the next. Only `mg07_on_activate_dispatch` (843) and `mg07_tick_system` (846) sit downstream of the producer.

This is the inverse of the usual marker defect: not an undrained marker that re-fires every frame, but a marker drained before its consumers ever run.

## Evidence

Scheduler order confirmed directly in `byroredux/src/boot.rs`:

```
748  rumble_on_activate_dispatch      <- consumer
757  quest_advance_dispatch           <- consumer
788  two_state_activator_system       <- consumer
797  quest_fragment_dispatch          <- PRODUCER
843  mg07_on_activate_dispatch        <- consumer (only one downstream)
1196 event_cleanup_system (Stage::Late) -- drains ActivateEvent
```

Proven with a schedule-order probe (temporary test, run, reverted): marker emitted = `true`, `TwoStateActivator.is_open` after the next frame = **`false`**.

The cited guard `dispatch_activate_then_set_open_updates_mq101_style_gate` only asserts the component exists -- it never runs the consumer, which is why the gap survived prior passes.

## Impact

Every lowered `<Ref>.Activate()` in a quest fragment silently no-ops against a two-state activator or a quest-advance REFR. Silent failure: no crash, no log contradiction, just a door / lever / quest gate that should have fired and didn't.

This is the same "silently corrupts game logic" failure class the decline invariant exists to prevent, arriving through scheduling rather than through lowering -- and it is invisible to the recognizer-side tests because the lowering is correct; only the delivery is broken.

## Related

#2269 (closed), #2539 (closed) -- both prior fixes to the same fast-growing dispatch function

## Suggested Fix

Either move `quest_fragment_dispatch` ahead of the `ActivateEvent` consumers in `boot.rs`, or route fragment-emitted activations through the existing `DeferredFragmentEffects` queue so they land at the head of the next frame before any consumer runs (the deferral machinery added by `dc9ba0e5` already exists for exactly this shape).

Add a regression test that actually *runs* the consumer after the producer and asserts `is_open` flips -- the existing guard asserts only that the component was inserted.

## Completeness Checks
- [ ] **SCHEDULE-ORDER**: Producer runs before every consumer of the marker, and the drain still runs last
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
