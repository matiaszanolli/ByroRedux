# SAVE-D4-01: validation gate does not check ItemStack.instance resolvability against ItemInstancePool

Labels: bug import-pipeline medium 

- **Severity**: MEDIUM
- **Dimension**: Validation Gates
- **Data-Loss Class**: corruption-on-load
- **Location**: `crates/save/src/validate.rs:49-58` (`validate_world` covers only Hierarchy / Equipment / Animation); ref class at `crates/core/src/ecs/components/inventory.rs` (`ItemStack.instance: Option<ItemInstanceId>`) → `crates/core/src/ecs/resources.rs` (`ItemInstancePool`)

## Description
`validate_world` enumerates exactly three reference classes. `Inventory` rows can carry `ItemStack.instance` = an `ItemInstanceId` indexing the per-world `ItemInstancePool` (saved as a resource). The gate never checks that those instance ids resolve in the pool. A dangling `ItemInstanceId` (pool entry dropped while the stack referencing it survived) passes validation, is written, and on load indexes a non-existent / wrong instance — the exact "persist an inconsistent reference" the format's thesis claims to prevent.

## Evidence
`validate.rs` has no `ItemInstancePool`/`ItemInstanceId` reference. The crate's docstring explicitly defers *cross-plugin FormId* checks to the binary; but the binary side (`SaveCommand::execute`) calls only the core `validate_world` and layers **no** additional check — so the deferred FormId-resolvability check is also **MISSING, not deferred** (a second instance of the same gap).

## Impact
A corrupted instance-pool reference, or an unresolvable `FormIdComponent`, seeds a corruption tail on load — defeating the format's whole defense-in-depth premise for those reference classes.

## Suggested Fix
Add an `ItemInstancePool`-resolvability sub-check to `validate_world` (it needs only core types) and a `ValidationKind::ItemInstance`; add the deferred binary-side FormId-resolve check (it needs the `DataStore`/`FormIdPool`) into `SaveCommand::execute` before the write, as the docstring promises.

## Completeness Checks
- [ ] **SIBLING**: The binary-side deferred FormId-resolvability check is added alongside the core ItemInstance check
- [ ] **TESTS**: A regression test covers a dangling `ItemInstanceId` being rejected by the gate
