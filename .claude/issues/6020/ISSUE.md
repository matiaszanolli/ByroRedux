# SCR-D7-2026-10-17-04: `papyrus_provider_system` builds a map of every FormID-bearing entity every frame, even with nothing pending

Issue: #6020
Labels: medium, scripting, bug, performance

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-04: `papyrus_provider_system` builds a map of every FormID-bearing entity every frame, even with nothing pending

- **Severity**: MEDIUM
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/execute.rs:163-179`
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: `owner_form_ids` iterates the whole `FormIdComponent` storage and does a `FormIdPool::resolve` + `BTreeMap::insert` per entity, unconditionally, to serve only `SendModEvent` senders (`execute.rs:297-315`). The system runs every frame in `Stage::Late` whenever the extension host exists (the default: `ExtensionHostSlot::initialize_default`), because the early return at `execute.rs:26-29` only triggers when no callback is installed. #5640 removed the equivalent per-frame registration walk; this one remains.
- **Evidence**: O(entities with FormId) per frame (every placed ref in loaded cells), allocation of a fresh BTreeMap, regardless of `handlers` being empty. Cost not measured; estimated on the order of 1-3 ms at tens of thousands of refs.
- **Impact**: Constant per-frame CPU tax for a feature (SendModEvent senders) that almost never has work; "CPU bottleneck = bug" on the dev profile.
- **Related**: #5640.
- **Suggested Fix**: Build the map lazily - only when some collected handler `statements_need_owner_sender`, and only look up the specific owner entities (`world.get::<FormIdComponent>(owner)`) instead of materializing the full table.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
