# SCR-D7-2026-10-17-05: A script with provider handlers runs only those handlers - its non-provider handlers are neither lowered nor given to the Papyrus VM

Issue: #6021
Labels: medium, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-05: A script with provider handlers runs only those handlers - its non-provider handlers are neither lowered nor given to the Papyrus VM

- **Severity**: MEDIUM
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/lower_program.rs:20, 94-103` (non-provider handlers skipped: `return Ok(())` at the "mentions provider" check), `byroredux/src/cell_loader/references/attach.rs:812-831` (`None if !provider_attached && attach_vm_reference_script`)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: `lower_event_into` skips any handler that does not mention a provider ("remain available to existing recognizers"), but the recognizers are whole-script, and the VM attach is explicitly suppressed once a provider program attached (`!provider_attached`, added with the VM reference-script wave `5f1671e59`). For a script such as `OnInit: StorageUtil.IntListAdd(...)` plus `OnActivate: self.Disable()` the first runs and the second never does, so the script executes half-way. The decline-on-unmodeled contract is per script ("a partial or approximate lowering is worse than none").
- **Evidence**: no test covers a mixed script; `attach_vmad_scripts` has no path that runs the remaining handlers. The VM cannot run the provider handlers (it has no provider natives), so the suppression avoids double-fire but at the price of leaving the vanilla handlers inert.
- **Impact**: State drift between handlers that share script variables (e.g. provider OnActivate increments a counter, a non-provider OnUpdate reads it): silent half-behaviour for every mixed SKSE-using script.
- **Related**: pre-VM behaviour was the same (inert), so this is a gap rather than a regression; flagging for an explicit decision.
- **Suggested Fix**: Either decline the provider program when the script also has handlers the engine cannot run (whole-script decline), or let the VM run the non-provider handlers alongside the provider program with a handler partition so each event has exactly one owner.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
