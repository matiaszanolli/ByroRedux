# SAFE-D11-01: queue_increment_own_i64 skips the entity-visibility check every sibling write function enforces

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/4119
**Labels**: bug, medium, safety

**Severity**: MEDIUM
**Dimension**: 11 - Sandboxed Mod Runtime Trust Boundary
**Location**: `crates/mod-runtime/src/runtime/host/state.rs:6-51` (the WIT `state::Host::queue_increment_own_i64` impl); contrast `crates/mod-runtime/src/runtime/host/animation.rs:19-51`, `reputation.rs:19-60`, `actor_values.rs:31-80`, `packages.rs:19-50` (all check `self.entity_projections.get(&entity)`/`contains_key` before queuing); commit path `crates/sdk/src/component.rs` (`ExtensionComponentStore::apply_batch`, `IncrementI64` arm), sole call site `byroredux/src/extensions/commands.rs:335-336`
**Status**: NEW (from `docs/audits/AUDIT_SAFETY_2026-09-11.md`)

## Description
Every other guest-reachable command that targets a specific entity (`animation::queue_play_idle`, `actor_values::queue`, `reputation::queue`, `packages::queue_evaluate`) validates that the target entity is present in `self.entity_projections` — the set the host populated for *this specific callback* from the actual event's subject/activator/aggressor, built in `extensions/capture.rs` + `extensions/dispatch.rs::bind_entity` — before it will queue a mutation, bailing with an explicit "target is not visible in this callback" error otherwise. `state::Host::queue_increment_own_i64` skips this check entirely: it validates `accepting_commands`, `COMPONENTS_WRITE_OWN_CAPABILITY`, the per-entry command budget, and the schema/field/type of the write, then goes straight from the raw guest-supplied `EntityRef` to a queued `ExtensionCommand::IncrementI64` command. The commit-side `ExtensionComponentStore::apply_batch` (`crates/sdk/src/component.rs`) does not backfill the gap — it validates schema/field/type/size/row-budget but never checks the entity against any live/visible set, and stores the row keyed by `(principal, schema, entity)` regardless of whether that entity was ever shown to the guest.

## Evidence
Confirmed by direct read of `crates/mod-runtime/src/runtime/host/state.rs` — no `entity_projections` check anywhere in `queue_increment_own_i64`:
```rust
fn queue_increment_own_i64(&mut self, entity: state::EntityRef, schema_index: u32,
    field_index: u32, delta: i64) -> wasmtime::Result<()> {
    if !self.accepting_commands { wasmtime::bail!(...); }
    if !self.grants.contains(COMPONENTS_WRITE_OWN_CAPABILITY) { wasmtime::bail!(...); }
    if self.pending_commands.len() >= self.max_commands_per_entry { wasmtime::bail!(...); }
    // ... schema/field lookup + type check ...
    let entity = byroredux_sdk::identity::EntityRef::new(entity.world_generation, entity.object)
        .ok_or_else(|| wasmtime::Error::msg("entity reference contains a reserved zero value"))?;
    self.pending_commands.push(HostCommand::Component(ExtensionCommand::IncrementI64 { entity, .. }));
    Ok(())
}
```
vs. the sibling pattern confirmed present in `animation.rs::queue_play_idle`:
```rust
let entity = sdk_entity_ref(entity)?;
if !self.entity_projections.contains_key(&entity) {
    wasmtime::bail!("animation target is not visible to this callback");
}
```
No guest-reachable *read* WIT function exists for extension component rows today — `state::Host` exposes only `queue_increment_own_i64` (no `get`), and `ExtensionComponentStore::row` is called only from tests and internal save/dispatch accessors, not from any WIT host function.

## Impact
A guest can guess or brute-force `EntityRef{world_generation, object}` pairs (small, likely-sequential integers) and stamp component data onto entities it was never introduced to via any callback, undermining the "operate only on what you've been shown" model the rest of the interface enforces. Currently narrow and mostly inert: with no guest-reachable read-back path for these rows today, the observable effect is limited to extra, budget-capped, save-persisted rows with no in-game effect, and no principal-isolation boundary is crossed. The risk is forward-looking — the moment any consumer of `ExtensionComponentStore` reads these rows back (the store's entire purpose), this becomes a live way to tag/track arbitrary world entities without ever being shown them, exactly the leak class `entity_projections` exists elsewhere in this file to prevent.

## Related
None found in open issues or prior audit reports.

## Suggested Fix
Add the same `self.entity_projections.contains_key(&entity)` check used by `animation.rs`/`reputation.rs`/`actor_values.rs`/`packages.rs` to `queue_increment_own_i64`, bailing with an explicit "component target is not visible in this callback" error before queuing the command, matching the established sibling pattern exactly.

## Completeness Checks
- [ ] **SIBLING**: Same `entity_projections.contains_key` check applied identically to `queue_increment_own_i64` as in `animation.rs`/`reputation.rs`/`actor_values.rs`/`packages.rs`
- [ ] **TESTS**: A regression test confirms a guest cannot queue an `IncrementI64` command against an entity outside its callback's `entity_projections`
