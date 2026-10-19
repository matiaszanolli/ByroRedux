# ECS-SD-01: `World::init_resource` split `insert_resource`'s doc comment, and has no test

Labels: low,ecs,documentation,doc-rot,test-gap
URL: https://github.com/matiaszanolli/ByroRedux/issues/6136

**Severity**: LOW · **Dimension**: 4 Resources & World-Level State (`world.rs` resource half)

## Location
`crates/core/src/ecs/world.rs` (the doc block above `init_resource`, and `insert_resource`)

## Description
`init_resource` was pasted between the `/// Insert a global resource ... # Panics ... (#1837)` block and `pub fn insert_resource`. Rustdoc now attaches the whole block to `init_resource` (its page reads "Insert a global resource. Returns the previous value if one existed", which is false for it) and `insert_resource` has no doc at all. Same class as the #6064 `memory_barrier` doc split. Introduced by `d0572ed94` (Fix #5806).

## Evidence
```
/// Insert a global resource. Returns the previous value if one existed.
/// # Panics ... (#1837). The freshly-installed replacement value ...
/// #5806 - the "insert the default if absent" idiom ...
pub fn init_resource<R: Resource + Default>(&mut self) { ... }
pub fn insert_resource<R: Resource>(...) -> Option<R> { ... }
```
(verified at publish time). `rg 'init_resource' crates/core/src/ecs/world.rs` returns only the definition: no unit test pins the one property its ~155 call sites (area: `cell_loader/created.rs`, `npc_spawn/seam_blend/tone.rs`) rely on, "an existing value, e.g. a save restore's, is left untouched". The migrated sites (`ff132d272`, `d0572ed94`, `e470f5e6d`) all previously inserted `Default::default()`, so no behaviour changed.

## Impact
Documentation only today; the missing test means a future edit to "always insert" would silently clobber restored resources at ~155 sites.

## Related
#5806, #6064 (same class).

## Suggested Fix
Move the `init_resource` doc and fn below `insert_resource`, restore the original doc block, and add a 6-line test (insert a non-default value, call `init_resource`, assert unchanged; call on an absent type, assert default inserted).

Source: docs/audits/AUDIT_ECS_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
