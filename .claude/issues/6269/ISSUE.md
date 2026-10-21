# #6269: SCR-2026-10-21-12: `ResetInventory` replaces a container's `Inventory` without releasing its stacks' `ItemInstancePool` entries

**Labels**: low,scripting,gameplay,inventory,bug

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: LOW · **Dimension**: Legacy ObScript (request consumers) · **Location**: `byroredux/src/systems/script_requests.rs:186-192`

**Audit status**: NEW

## Description

`inventories.insert(entity, fresh)` drops the old stacks. Elsewhere, consumption releases `stack.instance` back to `ItemInstancePool` (`inventory.rs:1576-1585`). The reset does not, so each scripted reset orphans the pool entries of the instanced items the container held.

## Suggested Fix

Release every `stack.instance` of the old inventory before the insert. Route to `/audit-gameplay` for the loot-persistence ledger interaction.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other paths that replace or drop a whole `Inventory` (cell unload, actor reset, loot ledger))
- [ ] **TESTS**: A regression test pins this specific fix
