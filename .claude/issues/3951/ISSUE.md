# #3951 — SCR-D6-2026-09-06-04: `papyrus_provider_system` and `legacy_obscript_load_order_system` declare a fraction of what they acquire — documentation-only today, but that is the declared purpose the under-declaration defeats

- **Finding ID**: SCR-D6-2026-09-06-04
- **Labels**: low,scripting,concurrency,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3951

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
