# SCR-D3-2026-10-17-06: `NativeActivation` is honoured by only some OnActivate consumers

Issue: #5994
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D3-2026-10-17-06: `NativeActivation` is honoured by only some OnActivate consumers

- **Severity**: LOW
- **Dimension**: Event Runtime
- **Location**: `crates/scripting/src/events.rs:30-39` (contract) vs consumers `papyrus_demo/quest_advance.rs:363`, `papyrus_demo/mod.rs:298`,
  `papyrus_demo/mg07_door.rs:325`, `papyrus_provider/execute.rs:109`, `byroredux/src/extensions/systems.rs:570`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: CK `Activate(ref, true)`: "no OnActivate event will be sent to any scripts". Only the VM delivery (`fragment/reference.rs:602-614,830`)
  filters `NativeActivation`. The lowered quest-advance, rumble, MG07, Papyrus-provider and WASM extension `OnActivate` paths read the raw
  `ActivateEvent`.
- **Impact**: a script's native-only activation of an object that carries one of those handlers runs the handler anyway. Low frequency; no loop
  today (provider IR has no Activate op).
- **Suggested Fix**: share one `papyrus_activations(world)` accessor (ActivateEvent minus NativeActivation) used by all OnActivate-modelling consumers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
