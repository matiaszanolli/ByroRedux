# SCR-D6-NEW4-01: Hardcoded ScriptRegistry demo registration still not retired from boot.rs

**Issue**: #2191
**Labels**: low, tech-debt, bug
**Dimension**: Scripting Runtime Systems / Engine Attach Path
**Untrusted-Input**: No
**Location**: `byroredux/src/boot.rs:475-489` (the call site); `docs/engine/m47-2-design.md:212-237` ("Engine integration" section, stating "The hardcoded demo registration is retired in favor of this path") and `:333` (`- [ ] hardcoded \`papyrus_demo::register_spawners\` retired from the attach path`, still unchecked)
**Status**: NEW

## Description

`boot.rs` still builds a `ScriptRegistry`, calls `byroredux_scripting::papyrus_demo::register_spawners(&mut script_registry)` (which registers exactly one entry, `"defaultRumbleOnActivate" → spawn_default_rumble`), and inserts it as a live world resource that `attach_scpt_script` (the pre-Skyrim `SCRI`→`SCPT`→`ScriptRegistry` Obscript path) consults on every cell load for every REFR. `m47-2-design.md`'s prose says this was supposed to be retired once the dynamic VMAD/`.pex` recognizer path (M47.2, Phase 0) landed — its own "Verification checklist for 'M47.2 done'" leaves that exact line unchecked, consistent with `boot.rs` never having been updated to drop the call.

## Evidence

`boot.rs:483-484`: `let mut script_registry = ...; byroredux_scripting::papyrus_demo::register_spawners(&mut script_registry);`. `crates/scripting/src/papyrus_demo/mod.rs:230-238`: `register_spawners` registers only `"defaultRumbleOnActivate"`. `crates/plugin/src/esm/records/index.rs:552-585` (`base_record_script`, the SCRI/SCPT path this registry feeds) only ever returns a form ID sourced from an `SCRI` sub-record — a field Skyrim+ records don't carry, since Skyrim+ scripting is exclusively `VMAD`-based.

## Impact

Traced this through rather than assumed: because `attach_scpt_script`'s `ScriptRegistry` lookup is only ever reached via an `SCRI`-sourced `script_form_id` (pre-Skyrim: Oblivion/FO3/FNV), and `"defaultRumbleOnActivate"` is a Skyrim-era script name that would never appear as an `SCPT` record's `editor_id` cross-referenced by an `SCRI` sub-record in real pre-Skyrim content, this registration is provably inert against every real game's data today — not a double-attach or correctness risk, purely dead wiring that the design doc's own exit criteria say should have been removed. No live bug; a stale TODO that outlived the milestone it was scoped to.

## Suggested Fix

Drop the `register_spawners` call (and, if nothing else populates it, the `ScriptRegistry` resource itself) from `boot.rs` once confirmed no other in-tree code path still depends on it, per the design doc's own Phase-0 checklist; or, if the demo registration is being deliberately kept as a smoke-test convenience, check the box in `m47-2-design.md` and add a comment at the `boot.rs` call site explaining why it's being kept past its original retirement plan.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other in-tree caller depends on `ScriptRegistry`/`register_spawners` before removing
- [ ] **DOCS**: Check the box in `m47-2-design.md`'s Phase-0 checklist once resolved (either by removal or by an explicit "kept intentionally" note)
