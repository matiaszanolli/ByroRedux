# ECS-2026-10-31-01: `improve_plan` reads `ItemInstancePool` under the `Inventory` guard

Labels: high,ecs,concurrency,bug
Source: docs/audits/ (2026-10-31 report)

**Severity**: HIGH (ECS deadlock potential; reddens the ABBA lock-order detector lane, so CI fails on every `main` run)
**Dimension**: ECS / Lock Ordering
**Source**: `docs/audits/AUDIT_ECS_2026-10-31.md` ECS-2026-10-31-01. Duplicated by CONC-01 (`AUDIT_CONCURRENCY_2026-10-31.md`) and REG-2026-10-31-01 (`AUDIT_REGRESSION_2026-10-31.md`); this issue is the single tracker for all three. Also a regression of #4982 (that issue removed this exact `Inventory -> ItemInstancePool` shape from `reference_state.rs`).

## Location
`byroredux/src/crafting.rs:222-236` (`improve_plan`: `let inventory = world.get::<Inventory>(player)?` stays live through the `.map(|(stack, row)| (stack, stack_quality(world, row)))`), `stack_quality` (:205-210, takes `try_resource::<ItemInstancePool>()`). The test helper `quality_of` (`crafting.rs:768-771`) has the same shape.

Introduced by `2c55546f4` ("improving items at the grindstone and armor workbench", 2026-10-30); reached from `recipes_at`, `improve_item` (calls `improve_plan` twice per craft) and the bench page. Verified present at HEAD `670f81917`.

## Description
This records the edge `Inventory -> ItemInstancePool`. The rest of the engine takes the pool first: `validate_inventory_instances` (`crates/save/src/validate.rs:488-499`, source pin at :901 "resource before storage") and the `capture` probes in `cell_loader/reference_state.rs` (:231, #4982); the scratch test at `reference_state.rs:1677` also pins `ItemInstancePool -> Inventory`. The two orders close an `ItemInstancePool -> Inventory -> ItemInstancePool` cycle. The same commit added three more `Inventory`-then-pool readers that are safe because they copy the stack out first (`inventory/actions.rs:601`, `armor.rs:150` after the clone at :134, `inventory/actions.rs:16` after the clone at :13). `improve_plan` is the one that keeps the guard.

## Evidence
`BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux` panics in `crafting::tests::improving_raises_a_held_item_in_place_and_never_hands_out_a_copy`: "attempted acquisition of `ItemInstancePool` while holding `Inventory` ... `ItemInstancePool` -> `Inventory` -> `ItemInstancePool`" (`crates/core/src/ecs/lock_tracker.rs:489`). Running that test alone passes, because the opposite edge comes from other tests; `cargo test ... -- crafting:: reference_state` reproduces it, backtrace through `stack_quality` from `quality_of`. CI run 41215450288 (`8985a2e99`) shows the same single cycle.

## Impact
The bench action runs on the main thread today, so no live deadlock is reachable. But the process-wide graph holds a real production inversion against the save path, and while the lane stays red it hides the next real cycle (the 10-27 lesson). Becomes a live deadlock if either side ever runs off the main thread or outside an exclusive system.

## Suggested Fix
In `improve_plan`, clone the matching rows (`base_form_id`, `count`, `instance`, row index) into a `Vec`, drop the `Inventory` guard, then resolve quality per row (same pattern as `armor::totals`). Do the same in `quality_of`. Consider one shared "stack's instance -> pool quality" accessor: the lookup is spelled three times (`crafting.rs:205-209`, `armor.rs:147-151`, `inventory/actions.rs:599-602`) and only the crafting copy holds a guard around it.

## Related
#4982, #4616, #3649, #6443, #6444, #6445, #313

## Completeness Checks
- [ ] **SIBLING**: Other `Inventory`-guard-held callers of pool lookups checked (the three listed above)
- [ ] **LOCK_ORDER**: `ItemInstancePool` before `Inventory` order preserved; `BYRO_LOCK_ORDER_CHECK=1` bin run is green
- [ ] **TESTS**: The `crafting::` + `reference_state` combined lock-order run pins the fix
