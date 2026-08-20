# SAVE-D2-2026-08-20-02: the rewritten serde guard's file discovery has three residual holes — a dead match prefix, an unscanned nested type, and a line-bound matcher

**Issue**: #3167 — https://github.com/matiaszanolli/ByroRedux/issues/3167
**Finding ID**: `SAVE-D2-2026-08-20-02`
**Severity**: LOW
**Dimension**: 2 — Registry & (De)serialization Fidelity
**Audit**: `/audit-save` — 2026-08-20 comprehensive suite, HEAD `bb0b92f2`
**Labels**: low, tech-debt, bug

---

**Audit**: `/audit-save` — `docs/audits/AUDIT_SAVE_2026-08-20.md` (HEAD `bb0b92f2`)
**Finding ID**: `SAVE-D2-2026-08-20-02`
**Severity**: LOW
**Dimension**: 2 — Registry & (De)serialization Fidelity
**Data-Loss Class**: none today (latent — no unsafe attribute exists in any unreached file at HEAD)

## Location

- `byroredux/src/save_io/serde_default_guard_tests.rs:24-40` — `registered_type_names`, the dead prefix at `:28`
- `:49-72` — `save_type_sources`, the retain predicate at `:63-70`
- `:77-108` — `serde_attribute_body`, the `trimmed.starts_with("#[")` gate at `:78-81`

## Status

**NEW — residual of the correctly-CLOSED #3025**, filed as a new recurrence rather than a
regression: the hand-maintained `SAVE_TYPE_SOURCES` really is gone, and the derived replacement
really does cover the six files #3025 named.

## Description

#3025's fix replaced the hand list with derivation. Re-implementing `save_type_sources()` in
Python against the live tree selects **41 of 393** candidate files. Three mechanisms leave gaps:

**1. Dead match prefix.** `registered_type_names` matches `".register_form_id_component::<"`
(`:28`), but that method's real signature is
`register_form_id_component(&mut self, name: &'static str)` (`crates/save/src/registry.rs:197`)
and the sole call site is `.register_form_id_component("FormIdComponent")` (`save_io.rs:315`) —
**no turbofish**. The prefix matches zero occurrences. Consequently `crates/core/src/form_id.rs`,
which defines `FormIdPair` / `LocalFormId` / `PluginId` — *the exact payload the form-id column
serialises* — is **not scanned**, and neither is `crates/core/src/ecs/components/form_id.rs`.

**2. Nested types in files that define no registered type.** The retain predicate keeps a file
only if it contains `cfg_attr(feature = "save"` *or* defines a type whose name appears in a
turbofish registration. `crates/plugin/src/esm/records/script_instance.rs` satisfies neither, yet
`ScriptInstanceData` is nested inside `PendingFragmentExecution`
(`crates/scripting/src/fragment.rs:117`), the payload of the registered `FragmentExecutionQueue`.
This is **one of the two files #3025's own suggested fix named**, and the derived replacement does
not reach it. (The other, `crates/scripting/src/translate/effects.rs`, **is** reached — it carries
a `feature = "save"` `cfg_attr`.)

**3. Line-bound matcher.** `serde_attribute_body` requires the trimmed line to start with `#[`
*and* contain `serde(` on that same line, so a rustfmt-wrapped multi-line attribute is invisible.
No such attribute exists in the tree today, so this is pure hardening — but the guard's whole
purpose is to survive future edits it cannot anticipate.

## Evidence

Python re-implementation of `registered_type_names` + `save_type_sources` over HEAD: **42
registered names extracted, 0 of them via the `register_form_id_component::<` prefix; 41 files
selected**. `crates/core/src/form_id.rs`, `crates/core/src/ecs/components/form_id.rs`,
`crates/core/src/string/mod.rs` and `crates/plugin/src/esm/records/script_instance.rs` are all
**OUT**. `grep -n "serde("` on all four returns nothing but one doc-comment mention, confirming
**zero live exposure**.

`grep -n "register_form_id_component" crates/save/src/registry.rs byroredux/src/save_io.rs`
confirms the `&'static str` signature and the single non-turbofish call site.
`grep -rn --include='*.rs' 'cfg_attr($'` and `'^[[:space:]]*serde('` both return nothing.

## Impact

**None at HEAD.** The cost is that the guard *reads as exhaustive* — its module docstring says the
scan set "is derived from `build_save_registry`… moving or registering a type changes the scan
automatically" — while three defining files of save-participating data sit outside it, one of them
the form-id payload that every cross-session reference depends on.

## Related

- **#3025** — CLOSED, correctly. This is the residual, not a regression.
- **#2015** / **#2181** / **#2537** — the hand-list era of the same drift.
- `SAVE-D2-2026-08-20-01` — the same guard's other, larger blind spot (required-field additions).
- `SAVE-D1-2026-08-20-02` — the sibling guard's reach gap.

## Suggested Fix

1. Delete the dead `".register_form_id_component::<"` prefix and add the form-id column's payload
   files explicitly (or match the string-argument form as well).
2. Widen the retain predicate to accept any `cfg_attr(feature = "` + serde-derive line rather than
   the literal `"save"` — `crates/core` gates on `"inspect"`.
3. Make `serde_attribute_body` operate on a whitespace-joined attribute span rather than a single
   line, and add a wrapped-attribute case to the three existing matcher unit tests.

## Completeness Checks
- [ ] **SIBLING**: `registry.rs`'s *other* non-turbofish registration forms (if any are added later) are matched too
- [ ] **TESTS**: `source_discovery_follows_registry_and_nested_save_modules` asserts the four currently-OUT files are IN after the fix
- [ ] **TESTS**: a wrapped multi-line `#[cfg_attr(...)]` fixture is added to the matcher unit tests
