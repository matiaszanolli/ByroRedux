# SCR-D6-NEW3-01: RunOn::Reference conditions always evaluate false — resolver exists in the same file but is never called

**Issue**: #2123
**Labels**: medium, bug
**Dimension**: Scripting Runtime Systems
**Untrusted-Input**: Yes — driven by real CTDA data (any condition authored "Run on: Reference" in the Creation Kit).
**Location**: `crates/scripting/src/condition.rs:258-268` (`ConditionContext::resolve`, `RunOn::Reference` arm)
**Status**: NEW (found in `docs/audits/AUDIT_SCRIPTING_2026-07-21.md`, Dimension 6)

## Description

The `RunOn::Reference` arm unconditionally returns `None` with a comment claiming the FormID→EntityId resolver "not yet wired." That resolver already exists in the same file — `resolve_entity_by_global_form_id` (`condition.rs:326-338`) — and is already used a few lines later in the `GetDistance` arm (`condition.rs:395`) to resolve `condition.param_1`, a FormID that goes through the identical parse-time remap as `condition.reference_form_id` (`crates/plugin/src/esm/records/condition.rs:359-360`).

Since `evaluate_condition` returns `false` whenever `ctx.resolve()` returns `None`, every CTDA condition authored with `RunOn::Reference` silently and permanently evaluates false. It does not violate the decline-over-default contract (it never falls back to Subject), but it never succeeds either.

## Evidence

```rust
// condition.rs:258-268
RunOn::Reference => {
    // FormID→EntityId resolver not yet wired: condition.reference_form_id
    // is a raw u32 ESM form ID; find_by_form_id requires an interned FormId.
    // Returns None until a u32→FormId pool lookup is plumbed here.
    log::trace!(...);
    None
}
```
vs. the resolver sitting unused nearby:
```rust
// condition.rs:326-338
pub fn resolve_entity_by_global_form_id(world: &World, form_id: u32) -> Option<EntityId> { ... }
```
and its sibling call site at `condition.rs:395` (`GetDistance`) proving the remap contract lines up.

## Impact

Any perk entry, dialogue INFO, quest stage, AI package, or magic-effect CTDA that targets `RunOn::Reference` (a specific placed REFR chosen in the CK, rather than Subject/Target/CombatTarget) never passes, silently gating off that branch of content on every game. No crash, no log above `trace`, so this is invisible without specifically instrumenting condition evaluation.

## Suggested Fix

`RunOn::Reference => resolve_entity_by_global_form_id(_world, condition.reference_form_id)` (drop the now-inaccurate comment and unused `_world` parameter name). One-line fix; the resolver and remap plumbing are already correct and tested elsewhere.
