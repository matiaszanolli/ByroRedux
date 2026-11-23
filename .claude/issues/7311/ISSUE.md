# #7311: SCR-D7-01: `papyrus_provider_system`'s access row omits `NativeActivation`, which it reads through `papyrus_activations`

- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Untrusted-Input**: No
- **Location**: `byroredux/src/boot/schedule/late/script_hosts.rs:42-58` (row) vs `crates/scripting/src/papyrus_provider/execute.rs:173-174` (acquisition); scan root `byroredux/src/boot/schedule/mod.rs:735-736,1142-1146`
- **Status**: NEW (no matching issue; closed #3951/#4574 cover the same class, for other types)
- **Description**: The body calls `crate::papyrus_activations(world)`. That function (`crates/scripting/src/events.rs:63-73`) takes `query::<ActivateEvent>()` and `query::<NativeActivation>()`. The sibling row `legacy_obscript_load_order_system` declares it with a #6218 comment. The provider row declares `ActivateEvent` but not `NativeActivation`. `papyrus_provider_system_declares_everything_it_acquires` passes anyway, because its only scan root is `execute.rs` and `events.rs` is not a listed hop (the scan follows callees only within a listed file).
- **Evidence**:
  ```rust
  // execute.rs:173-174
  let activated: BTreeMap<EntityId, EntityId> =
      crate::papyrus_activations(world).into_iter().collect();
  // script_hosts.rs: papyrus_provider row lists .reads::<ActivateEvent>() ... no .reads::<NativeActivation>()
  // script_hosts.rs:30 (legacy row only): .reads::<byroredux_scripting::NativeActivation>()
  ```
- **Impact**: None today. The system is an exclusive, so the declaration does not affect scheduling. It is the comparison baseline if the system is ever promoted to a parallel lane, and an under-declaration makes that promotion look safe (#3473/#3951 rationale).
- **Related**: #6218 (`NativeActivation`), #3951.
- **Suggested Fix**: Add `.reads::<byroredux_scripting::NativeActivation>()` to the row. Add `(EVENTS_SRC, "papyrus_activations")` to the `papyrus_provider_system` scan list so the next hop is not silently missed.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
