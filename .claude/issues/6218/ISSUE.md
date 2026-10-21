# #6218: SCR-2026-10-21-04: Two script-side `OnActivate` consumers still read raw `ActivateEvent`, so default-processing-only activations reach them (#5994 residual)

**Labels**: medium,scripting,bug

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Event Runtime · **Location**: `crates/scripting/src/vm_state.rs:131-137` (`two_state_activator_system`); `crates/scripting/src/obscript_runtime.rs:700-703` (`legacy_obscript_load_order_system`)

**Audit status**: NEW (residual of closed #5994; `6f7dd2213` changed extensions, MG07, rumble, quest_advance and the provider only)

**Untrusted-Input**: No

## Description

CK *Activate - ObjectReference*: with `abDefaultProcessingOnly`, "no OnActivate event will be sent to any scripts". The ObScript runtime queues the same `NativeActivation` when an `OnActivate` block calls `Activate`. Two consumers ignore the marker:
- `two_state_activator_system` is the lowered `OnActivate` of the `two_state_activator` recognizer. On such an activation it toggles `is_open` and `activated_once` and writes `ScriptVariables`, all of which are saved state.
- `legacy_obscript_load_order_system` re-runs the reference's `OnActivate` extension-call handler on the native activation its own ObScript requested.

## Evidence

`grep 'query::<…ActivateEvent>' crates/scripting/src` finds both sites. Neither filters `NativeActivation`. Compare `fragment/reference.rs:684-688` and `obscript_objects.rs:283-293`, which do.

## Impact

A two-state lever or door scripted through the recognizer flips state when another script default-activates it, and stays flipped across save/load. Extender calls in a legacy `OnActivate` run twice.

## Related

#5994.

## Suggested Fix

Read `crate::papyrus_activations(world)` in both systems, and add them to the #5994 test set.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every remaining `query::<ActivateEvent>` consumer in crates/scripting and byroredux/src)
- [ ] **TESTS**: A regression test pins this specific fix
