# SCR-D7-2026-10-17-07: A failing provider call discards the handler's collected ModEvent registrations although earlier callbacks already had effect

Issue: #6023
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-07: A failing provider call discards the handler's collected ModEvent registrations although earlier callbacks already had effect

- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:362-383`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: `execute_statements` pushes `Register/Unregister` actions into a local `registrations` Vec that is only applied on `Ok(..)`. On `Err(error)` (a host callback error, a type mismatch at execution, a limit hit) the arm only logs, so a handler that does `RegisterForModEvent(...)` and then a failing `StorageUtil` call leaves the earlier provider-call side effects applied but the registration lost. Papyrus semantics keep the statements that ran before a failed one.
- **Impact**: Handler never receives its mod events after any transient error in a later statement.
- **Suggested Fix**: Apply `registrations` on the `Err` path too (they correspond to statements that already executed).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
