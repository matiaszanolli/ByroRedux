# SAVE-D2-02: FormIdComponent load closure panics (not SaveError) if FormIdPool is absent

Labels: bug low tech-debt 

- **Severity**: LOW
- **Dimension**: Registry & (De)serialization Fidelity
- **Data-Loss Class**: none (panic, not silent loss; guarded in practice)
- **Location**: `crates/save/src/registry.rs:202-217` (load closure: `world.resource_mut::<FormIdPool>()`); panic semantics `crates/core/src/ecs/world.rs`

## Description
The save side resolves the pool defensively via `try_resource` and skips with a WARN on an unresolvable handle (confirmed: save never panics on an unresolvable handle, and the form-id remap on the live path uses `try_resource` too). But the **load** closure uses `resource_mut::<FormIdPool>()`, which **panics** ("Resource not found") if no pool is installed. A save containing a `FormIdComponent` column restored into a world without a `FormIdPool` aborts the whole load via panic rather than a `SaveError::Serde`. The live path always has a pool (boot + reloaded cell install one) and `restore_world` callers install one, so this is latent.

## Evidence
`resource_mut` `unwrap_or_else(panic!)`; load closure calls it directly.

## Impact
Asymmetry with the defensive save side; an unexpected panic instead of a typed error in a degenerate restore. LOW.

## Suggested Fix
Use `try_resource_mut` and return `SaveError` (or insert a default `FormIdPool`) when absent, mirroring the save side's defensiveness.

## Completeness Checks
- [ ] **TESTS**: A regression test restores a FormIdComponent-bearing save into a world without a FormIdPool and asserts a typed error
