# SAVE-02: form_id_column() heuristic mis-keys the entire remap if a future component registers with apply: None

**Labels**: medium, ecs, bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1845
**Source**: docs/audits/AUDIT_SAVE_2026-07-02.md

**Severity**: MEDIUM
**Dimension**: Registry & (De)serialization Fidelity
**Data-Loss Class**: reference-break (latent)
**Location**: `crates/save/src/registry.rs:289-295` (`form_id_column`)

## Description
`form_id_column()` identifies the remap key column as *"the one component with `apply.is_none()`"*. Today only `register_form_id_component` sets `apply: None` on a component, so the heuristic is correct. But it is **structural coincidence, not an assertion**: if any future `register_component` variant (or a second special column) ships with `apply: None`, `.find(|e| e.apply.is_none())` returns whichever comes first in registration order — potentially the WRONG column — and the entire live-load remap is built from non-form-id data → every delta drops or lands on the wrong entity.

## Evidence
```rust
self.components.iter().find(|e| e.apply.is_none()).map(|e| e.name)
```
There is no `is_form_id` flag on `Entry`; the discriminator is the absence of a closure.

## Impact
Latent. Silent mass reference-break on the live-load path the day a second `apply: None` component is registered — with no compile-time or test guard (the `round_trip` tests register exactly one form-id column).

## Related
SAVE-03 (both concern the form-id remap being the single keying mechanism).

## Suggested Fix
Add an explicit `is_form_id: bool` (or a dedicated `form_id: Option<Entry>` slot) to `SaveRegistry` and key `form_id_column()` off it; assert at most one is set. Removes the fragile "no apply ⇒ it's the key" inference.

## Completeness Checks
- [ ] **SIBLING**: `register_form_id_component` and any other `register_*` variant in `crates/save/src/registry.rs` reviewed for `apply: None` usage after the fix
- [ ] **TESTS**: A regression test registers a second `apply: None` component and asserts either a panic/`Result::Err` at registration time, or that `form_id_column()` still resolves correctly
