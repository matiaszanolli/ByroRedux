# SAVE-D2-03: SAVE_TYPE_SOURCES (the #1714 guard's file scan list) omits actor_values.rs

**Labels**: high, ecs, bug

**Severity**: HIGH
**Dimension**: Registry & (De)serialization Fidelity
**Source**: `docs/audits/AUDIT_SAVE_2026-07-16.md`

## Location
`byroredux/src/save_io.rs:1196-1211` (`SAVE_TYPE_SOURCES`) vs. `save_io.rs:191` (`register_component::<ActorValues>`) and `crates/core/src/ecs/components/actor_values.rs`

## Description
The `#1714` guard test (`serde_default_on_saved_struct_requires_format_major_bump`) exists to catch a save-participating struct gaining a `#[serde(default)]` field without a `FORMAT_MAJOR` bump — a change that `schema_fingerprint` (type-key-only) can't detect. It works by statically scanning `SAVE_TYPE_SOURCES`, whose own doc comment says "KEEP IN LOCKSTEP with `build_save_registry`." `db121f96` registered `ActorValues` (fixing `#1834`/`#1835`) and correctly updated `MUTABLE_DELTA_COLUMNS` and added a round-trip test — but never added `actor_values.rs` to `SAVE_TYPE_SOURCES`. Today `ActorValue` has zero `#[serde(default)]` fields, so there's no live corruption — but the guard's own "scans every save-participating type" claim is now false while the test still reports green.

Verified current: `SAVE_TYPE_SOURCES` (byroredux/src/save_io.rs:1196-1211) lists 14 files; `crates/core/src/ecs/components/actor_values.rs` is not among them, despite `ActorValues` being registered at line 191.

## Evidence
`git show db121f96 -- byroredux/src/save_io.rs | grep -n "SAVE_TYPE_SOURCES\|actor_values"` matches only the new test function name, never the array.

## Impact
No current data loss. The next field added to `ActorValue`/`ActorValues` with a `#[serde(default)]` escape hatch will not be caught by the regression guard built specifically to catch it — every existing save would silently default-fill the new field on load, on the actor-value system `#1834` already proved is read every frame (`GetActorValue`).

## Related
`#1714` (guard mechanism, closed); `#1834`/`#1835` (closed — registered `ActorValues` but missed this one line). This is a coverage gap introduced by `db121f96` (2026-07-05), two days after `#1714` shipped — not a re-opening of `#1714` itself, which remains correctly fixed as a mechanism.

## Suggested Fix
Add `crates/core/src/ecs/components/actor_values.rs` to `SAVE_TYPE_SOURCES`. Since the list is manual/comment-driven and has now missed an entry once, consider deriving it from `SaveRegistry`'s type list plus a name→file map asserted at test time, so a future omission fails loudly instead of silently passing.

## Completeness Checks
- [ ] **SAVE-REGISTRY**: Component added to `build_save_registry` AND to the `#1714` regression-guard's file-scan list (`SAVE_TYPE_SOURCES`)
- [ ] **TESTS**: A regression test pins this specific fix
