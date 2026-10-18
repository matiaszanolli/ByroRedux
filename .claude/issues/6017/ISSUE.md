# SCR-D7-2026-10-17-01: Provider `OnInit` handlers re-run on every reference spawn, replaying persisted-state side effects

Issue: #6017
Labels: high, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D7-2026-10-17-01: Provider `OnInit` handlers re-run on every reference spawn, replaying persisted-state side effects

- **Severity**: HIGH
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/papyrus_provider/lower_program.rs:1177-1188` (`attach_papyrus_provider_program` inserts `OnInitEvent` unconditionally), reached from `byroredux/src/cell_loader/references/attach.rs:796-806`; consumer `execute.rs:197-199`
- **Status**: NEW
- **Untrusted-Input**: Yes (mod-supplied script semantics driving saved StorageUtil/JContainers state)
- **Description**: The provider path emits `OnInit` as a one-frame marker every time the program component is attached. `attach_vmad_scripts` runs for every REFR spawn, i.e. every first stream-in, every cell re-entry after an unload, and every cell reload after a save load. There is no per-reference "already initialized" flag. The Papyrus VM reference-script path has exactly that (`ReferenceScriptState` / saved `initialized_key`, `fragment/reference.rs:200-213`, docs/engine/scripting.md:984 "OnInit the first time the reference loads (a saved per-script flag)"). The CK page for OnInit (`/mnt/data/src/reference/ck-uesp-wiki/(main)/OnInit.wiki`): "This event will run once, when the script is initialized".
- **Evidence**: `attach_papyrus_provider_program` -> `world.insert(entity, OnInitEvent)` with no guard; `papyrus_provider_system` -> `initialized.contains(&entity)` -> `enqueue(OnInit)`; the program component is not saved (`registry_completeness_tests.rs:687` "write-once ... attached from static script content on load") so nothing remembers a prior init. A handler `Event OnInit() / StorageUtil.FormListAdd(None, "registry", Self) / EndEvent` (FormListAdd allows duplicates by default) appends on every re-entry; the StorageUtil list lives in saved principal storage, so the duplicates persist across saves.
- **Impact**: Silent, persisted state corruption for any SKSE-era script whose OnInit is non-idempotent (counters, list registration, `SendModEvent` announce, quest-gating flags). Only idempotent RegisterForModEvent-style OnInit bodies are safe, which is the case the tests exercise.
- **Related**: #4334 (once-only persistence for VM reference scripts); docs/engine/scripting.md:147 table row for `OnInitEvent`.
- **Suggested Fix**: Gate the provider OnInit on the same saved per-reference/per-script initialized flag the VM path uses (keyed by FormID + script name), or decline provider `OnInit` bodies that are not provably idempotent until that flag exists.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
