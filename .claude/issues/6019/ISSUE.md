# SCR-D7-2026-10-17-03: ModEvent delivery fan-out is 256 events x 4 096 registrations = ~1M handler runs per frame, all deep-cloned up front, with no frame budget

Issue: #6019
Labels: medium, scripting, bug, performance

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-03: ModEvent delivery fan-out is 256 events x 4 096 registrations = ~1M handler runs per frame, all deep-cloned up front, with no frame budget

- **Severity**: MEDIUM
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:245-273` (push), `277-384` (run), constants `mod.rs:77-79`
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: For every pending event (<=256) and every matching registration of every program entity (<=4 096 total) the loop pushes `(handler.statements.clone(), locals, ..)` into one `handlers` Vec before running anything. `PapyrusProviderInvocation` carries a full `PapyrusProviderRoute`/`ScriptFunctionDeclaration` (parameter id Strings), so a statement clone is not small. Each handler then runs `validate_provider_statements` and one host callback (a Wasm guest entry for extension-published routes) per call statement. There is no analogue of the VM's per-frame statement budget (#5763) here.
- **Evidence**: bound = `MAX_PENDING_PAPYRUS_MOD_EVENTS` (256) x `MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS` (4 096) = 1 048 576 handler instances in one `Late` system. A guest can publish 256 events in one callback, and 4 096 registrations is one script on 4 096 refs registering the same event (see -02, they accumulate anyway). Memory estimate (unmeasured): ~1 KB per call statement -> GB-scale transient allocation and a multi-second stall. Every handler also may `SendModEvent`, so a self-sustaining storm is rate-limited only by the 256/frame publish cap.
- **Impact**: Mod-triggerable frame-time/memory spike (bounded, but the bound is far above any legitimate load). Not an OOB/panic.
- **Related**: #5763 (VM frame budget), #5640.
- **Suggested Fix**: Add a per-frame handler-run budget to `papyrus_provider_system` (carry the excess pending events to the next frame instead of cloning them all), and iterate registrations lazily instead of materializing every handler first.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
