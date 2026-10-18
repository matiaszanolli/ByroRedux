# SCR-D4-2026-10-17-01: Attach emits `OnCellLoadEvent` for 3D-less identity-only entities, so the VM delivers OnLoad / OnCellAttach / OnCellLoad to them

Issue: #5997
Labels: medium, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D4-2026-10-17-01: Attach emits `OnCellLoadEvent` for 3D-less identity-only entities, so the VM delivers OnLoad / OnCellAttach / OnCellLoad to them

- **Severity**: MEDIUM
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/cell_loader/references/attach.rs:158-185` (unconditional marker), `crates/scripting/src/fragment/reference.rs:615-649` (marker -> `OnInit/OnCellAttach/OnLoad/OnCellLoad`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `attach_script_for_refr` inserts `OnCellLoadEvent` on every entity where anything attached, including VM reference scripts. It does not distinguish entities that have no 3D or no loaded cell. Such entities are: (a) the `placement_disabled` actor / trigger / light identity branches (#4326/#4327 deliberately keep scripts but no body), (b) persistent-worldspace logical-actor stubs and `RemoteSceneActorStub` scene stubs (home cell not loaded), (c) marker-model and missing-mesh fallbacks that spawn `spawn_logical_quest_reference`. `collect_deliveries` turns the marker into `OnInit, OnCellAttach, OnLoad, OnCellLoad` for any attached script that handles them, with no 3D or cell-loaded check.
- **Evidence**: CK `OnLoad - ObjectReference`: "Event called when the object's 3d is loaded... This event is only sent when an object's 3d is loaded." `OnCellLoad`: "every object's 3d in this object's parent cell loaded". The scene-stub materializer exists precisely because the actor's home cell is not loaded (`script.rs:1337-1347`), yet `spawn_logical_quest_reference_with_scripts` -> `attach_script_for_refr` fires the load events at stub creation. Pre-VM this was harmless because the recognized archetypes ignore `OnCellLoadEvent`. The VM now runs arbitrary `OnLoad` bodies.
- **Impact**: A quest actor's `Event OnLoad()` / `OnCellAttach()` body (state setup, `Enable`/`Disable`, `MoveTo`, `EvaluatePackage`, `SetStage`) runs while the actor is a stub or disabled, and runs again when the real cell streams in. This is the wrong-state-without-fallback class the audit treats as worse than inert. Not demonstrated on vanilla data; it needs a `refr_script_coverage` run or an `m47-triggers`-style smoke on a stub actor with an `OnLoad` handler.
- **Related**: #4112, #4331, #4326, #4327 (identity-only paths kept scripts by design), the documented stub/real double-attach residual.
- **Suggested Fix**: Emit `OnCellLoadEvent` only from paths that spawn real 3D into a loaded cell (the static-mesh and actor-complete arms). Pass a `has_3d` flag into `attach_script_for_refr`, or insert the marker at the call sites. Keep identity-only entities on `OnInit` only.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
