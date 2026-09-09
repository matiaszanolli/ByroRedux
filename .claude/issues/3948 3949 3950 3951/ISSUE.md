===== 3948 =====
OPEN | bug low safety scripting 
# SCR-D5-2026-09-06-07: the #1816 panic net covers `decompile_script` only — `analyze_pex_compatibility` and `lower_provider_program` run on the same untrusted-derived data outside `catch_unwind` on every entry variant

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness (seam into the unaudited SDK layer) · **Untrusted-Input**: Yes · **Location**: `crates/scripting/src/translate/mod.rs:152-153, 158`; `crates/scripting/src/fragment.rs:1926-1927, 2178-2179` · **Status**: NEW (hardening; no panic demonstrated)
- **Description**: ~7k LOC of unaudited code (incl. the `unreachable!` arms in `papyrus_provider/execute.rs:848-858`) now sits on the cell loader's untrusted path with no net; a panic there aborts cell load exactly as #1816 did.
- **Suggested Fix**: widen `decompile_catching_panics` to the whole decompile → preflight → provider-lower → recognize sequence; route "can it panic?" to the dedicated SDK pass.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix

===== 3949 =====
OPEN | documentation low scripting doc-rot 
# SCR-D6-2026-09-06-02: `apply_effect`'s doc comment — the inventory the Dim-6 checklist delegates to — describes a lock-nesting shape that no longer exists

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems · **Untrusted-Input**: No · **Location**: `crates/scripting/src/fragment.rs:766-796` · **Status**: NEW (#3493 CLOSED re-attached this doc; drift is post-fix)
- **Description**: says the nested acquisitions run "while the caller still holds the `QuestStageFragments`/`QuestStageState`/`QuestObjectiveState` resource locks for the whole cascade loop" and counts "12 component-storage acquisitions". `QuestStageFragments` is a clone, never a guard; since the guard-free rework the two quest guards are scoped per fragment and re-acquired per provider tail; the real count is ~15 storage types across ~25 sites plus `EquipItemCatalog`, `SceneRegistry`, `PapyrusPlayerEntity` ×2, `FormIdPool`, and the `FragmentExecutionQueue` write.
- **Suggested Fix**: rewrite around `apply_fragment_guard_free`'s per-fragment scope; list acquisitions by helper rather than a hand count.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix

===== 3950 =====
OPEN | documentation low scripting doc-rot 
# SCR-D6-2026-09-06-03: `OnEquipEvent` was deleted but four ground-truth doc lines still describe it as defined/shipped

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems · **Untrusted-Input**: No · **Location**: `docs/engine/scripting.md:145`; `docs/engine/m47-0-design.md:103, 162`; `docs/engine/m47-2-design.md:312, 371` · **Status**: NEW
- **Description**: `events.rs:189-210` replaced it with `EquipmentChange` + `EquipmentEventBatch` (wearer-keyed batch, `get_mut`-then-`extend`); the docs the skill names as ground truth still list `OnEquipEvent { wearer }` as shipped.

- **Suggested Fix**: replace the four `OnEquipEvent` lines with `EquipmentEventBatch` / `EquipmentChange` (wearer-keyed batch, `get_mut`-then-`extend`) and name the two emit sites (`crates/scripting/src/equipment.rs:51`, `byroredux/src/inventory.rs:519`).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix

===== 3951 =====
OPEN | bug low scripting concurrency 
# SCR-D6-2026-09-06-04: `papyrus_provider_system` and `legacy_obscript_load_order_system` declare a fraction of what they acquire — documentation-only today, but that is the declared purpose the under-declaration defeats

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems · **Untrusted-Input**: No · **Location**: `byroredux/src/boot.rs:1789-1799, 1802-1811` vs `crates/scripting/src/papyrus_provider/execute.rs:31, 35, 71-157, 370, 387`; `obscript_runtime.rs:698-700` · **Status**: NEW
- **Description**: `add_exclusive_with_access` declarations do not affect scheduling (`scheduler.rs:340-353`: the analyzer "only walks parallel-stage pairs today"; exclusives run serially at `:511-512`) — so no deadlock vector. But their stated purpose (#3473: the declaration "to be compared against if either system is ever promoted to a parallel lane") is defeated: `papyrus_provider_system` actually `resource_mut`s `PapyrusProviderContinuationQueue` and `PapyrusModEventRuntime` and reads `OnInitEvent`, `HitEvent`, `EquipmentEventBatch`, `OnTriggerEnterEvent`, `OnUpdateEvent`, `FormIdComponent`, `FormIdPool` — none declared.
- **Suggested Fix**: add the missing entries; optionally a `BYRO_LOCK_ORDER_CHECK` assertion that a declared exclusive's recorded edges ⊆ its declaration.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix

