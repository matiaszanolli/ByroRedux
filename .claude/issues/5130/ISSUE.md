# PHYS-D2-2026-09-29-03: the #4997 guard was inserted inside the #3266 test's doc comment

**Labels**: low,documentation,doc-rot,physics

**Source**: `docs/audits/AUDIT_PHYSICS_2026-09-29.md`
**Severity**: LOW
**Dimension**: Step & Sync (test hygiene)
**Location**: `crates/physics/src/sync.rs` tests — `register_newcomers_parallel_section_holds_no_world_guard` and `physics_diagnostics_resolve_forms_after_storage_guards_drop`

## Description
`6d05c2bc0` inserted `register_newcomers_parallel_section_holds_no_world_guard` (#4997) between the `/// #3266 regression guard: …` doc lines and `physics_diagnostics_resolve_forms_after_storage_guards_drop`. The #3266 rationale now documents the wrong test (it is prepended to the #4997 doc block), and the #3266 test has none.

## Impact
Doc-only; a reader deleting "the #4997 test" takes the #3266 rationale with it.

## Related
#4997 (closed), #3266 (closed).

## Suggested Fix
Move the three `/// #3266 …` lines down to `physics_diagnostics_resolve_forms_after_storage_guards_drop`.

Validated at HEAD 9fcfdc3fc: the `/// #3266 regression guard: both diagnostic paths stage runtime FormIds …` lines directly precede the `/// #4997 — register_newcomers …` doc and `fn register_newcomers_parallel_section_holds_no_world_guard`.

## Completeness Checks
- [ ] **SIBLING**: other recent test insertions that split an existing doc comment
