# #3786 — SCR-D3-2026-08-30-01: decompile_script's auto-state match is the one case-SENSITIVE Papyrus identifier comparison in lower.rs

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, scripting, bug

---

**Audit**: `/audit-scripting` — `docs/audits/AUDIT_SCRIPTING_2026-08-30.md` (Dimension 3 — Decompiler Control-Flow / Boolean / Lower), HEAD `64f64480`
**Finding ID**: `SCR-D3-2026-08-30-01`

- **Severity**: LOW
- **Status**: NEW
- **Untrusted-Input**: Yes (both names come from the `.pex` string table)

## Location

`crates/pex/src/decompile/lower.rs:415`

## Description

```rust
for state in &object.states {
    if state.name == object.auto_state_name {
```

Papyrus identifiers are **case-insensitive**, and every other identifier comparison in this same file is case-insensitive:

- `parent_class_name.eq_ignore_ascii_case("none")` (`:437`)
- `return_type_name.eq_ignore_ascii_case("none")` (`:251`)
- `is_event_name` lowercases before its binary search
- `lower_expr` lowercases the `true`/`false`/`none` identifier literals

This one site uses `==`.

If a `.pex` ever carried `auto_state_name = "Waiting"` alongside a state named `"waiting"`, the auto state would be emitted as a named `ScriptItem::State { is_auto: false }` instead of having its callables hoisted to script scope. `translate_script`'s recognizers walk script-scope handlers, so **every event handler in that object would become invisible and the script would silently decline** — the same failure mode as a missing `EVENT_NAMES` entry.

## Evidence

Re-verified at HEAD: `lower.rs:415` is `if state.name == object.auto_state_name {`, immediately followed by the comment "Auto/default state: its callables live at script scope."

## Honest severity qualification

**Defensive, not a live bug.** Champollion compares string-table *indices* here, which is stricter still, and a compiler emitting the auto-state name twice with different casing has not been observed. This pass did not run the 26k-file corpus to look for one, so there is no corpus evidence either way.

Filed LOW because the inconsistency is real, the fix is one word, and the failure is silent if it ever occurs.

## Suggested Fix

`if state.name.eq_ignore_ascii_case(&object.auto_state_name)`, plus a two-line unit test with a mismatched-casing auto state asserting the handler lands at script scope.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — any remaining `==` over a Papyrus identifier pair across `crates/pex/src/decompile/` and `crates/papyrus/src/`
- [ ] **TESTS**: A regression test pins this specific fix — a mismatched-casing auto state must still hoist its callables to script scope
