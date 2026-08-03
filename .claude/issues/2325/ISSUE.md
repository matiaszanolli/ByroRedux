# SK-D4-NEW-01: No overflow guard on regular/light-master slot counters in the load-order assignment loop

**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-08-03.md` (Dimension 4)
**GitHub issue**: #2325

**Severity**: LOW
**Location**: `byroredux/src/cell_loader/load_order.rs:158-191`

## Description

`next_regular: u8` / `next_light: u16` are incremented unconditionally with
no ceiling check, unlike every other failure mode in this function
(duplicate plugin, missing master, misordered master), all of which error
loudly. Past 254 regular or 4096 ESL `--master` plugins, this silently
wraps/aliases two plugins' FormID spaces together.

## Evidence

Confirmed at HEAD (1ae86f62): no `checked_add` / ceiling check on either
counter in the load-order assignment loop.

## Impact

Only reachable past 254 regular or 4096 ESL plugins — not realistic for any
current Skyrim SE load order (~7-10 plugins including all official DLC), and
matches the real engine's own hard ceiling. LOW/hardening, not a compat bug.

## Suggested Fix

Replace the increments with `checked_add`, erroring in the same style as the
function's other guards.

## Completeness Checks
- [ ] **SIBLING**: Check other load-order/slot-assignment counters in the same module for the same missing-ceiling pattern
- [ ] **TESTS**: A regression test pins the new overflow-guard error path (e.g. synthetic 255th/4097th plugin)
