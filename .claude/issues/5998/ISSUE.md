# SCR-D4-2026-10-17-02: VM / recognized scripts attached to non-primary SCOL/PKIN children have no FormID, so the VM can never deliver to them

Issue: #5998
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D4-2026-10-17-02: VM / recognized scripts attached to non-primary SCOL/PKIN children have no FormID, so the VM can never deliver to them

- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:437-475` (LIGH-only / fxlight non-primary), `:933-945` (static non-primary: `placement_form_id_pair` is `is_primary_synth.then_some(..)`), `crates/scripting/src/fragment/reference.rs:119-153,607-612` (index and delivery both need `entity_global_form_id`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: Per #3016 each synth child attaches its own base-record SCRI/VMAD, but `stamp_quest_reference` runs only for the primary child, so a non-primary child's root carries no `FormIdComponent`. `ReferenceScriptIndex::refresh` skips such an entity, and `collect_deliveries` logs "skipped: the reference has no FormID". `attach_vm_reference_script` still counts it as attached and `attach_script_for_refr` emits `OnCellLoadEvent`, so `scripts_recognized` is inflated while the script is inert.
- **Impact**: A scripted base object that is a secondary content of a Pack-in or SCOL (for example a scripted activator inside a FO4 PKIN) gets a VM instance that never runs. This is silent and only affects load-time telemetry accuracy. Whether the engine gives PKIN children their own reference identity is unverified.
- **Related**: #3016, #2026.
- **Suggested Fix**: Either document that secondary children's VM scripts are intentionally inert and skip the VM attach for them, or give them a synthetic per-child identity as the created-reference path does.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
