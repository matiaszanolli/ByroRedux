# #3939 — SCR-D5-2026-09-06-03: lowering accepts provider barriers against a catalog the dispatcher may be unable to serve — `PapyrusProviderRuntime::default()` pairs a non-empty `engine_compatibility()` catalog with `callback: None`, a tolerated startup er...

- **Finding ID**: SCR-D5-2026-09-06-03
- **Labels**: medium,scripting,quests,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3939

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (lowering/dispatch consistency)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/papyrus_provider/runtime.rs:18-28`; `crates/scripting/src/fragment.rs:641-658`; `byroredux/src/extensions.rs:4892-5000` (13 `?` exits before `sync_extension_script_function_invoker` at `:5000`); `byroredux/src/main.rs:704-709`; `byroredux/src/asset_provider/script.rs:176-178, 258-260`
- **Status**: NEW
- **Description**: `Default` publishes a non-empty catalog with no callback; `populate_*_fragments` lower against `runtime.catalog()` at cell load, so `Game.GetModCount()`/`StorageUtil.*`/`Input.*`/`UI.*` calls become barriers instead of declining. At dispatch `apply_at_depth` finds no callback and drops every barrier *and its tail* after the prefix already mutated quest state. Reachable: `load_requested_extensions` exits through 13 `?` sites before ever syncing the runtime, and `App::new` logs and continues. Also by design (`failed_provider_barrier_aborts_its_native_fragment_tail`) a host `Err` aborts the tail including `SetStage` — divergent from Papyrus, where a native call never halts the script.
- **Impact**: `Game.GetModByName("X.esp"); Self.SetStage(20)` advances nothing past the barrier and never declines — quest stuck mid-fragment with a `warn!`. Pre-seam the same fragment declined wholesale (inert, consistent).
- **Disproof attempted**: the healthy path is consistent (`ExtensionHost::new` seeds `engine_compatibility()`; `sync_` publishes catalog+callback together); host-init failure publishes an *empty* catalog — consistent. Only the early-return path and the `Default` are inconsistent; no test covers callback-`None` with a non-empty catalog.
- **Related**: SCR-D5-01, SCR-D6-05
- **Suggested Fix**: make `Default` publish an empty catalog (or lower with `None` providers when no callback is live) so barrier-needing fragments decline at the boundary; publish the empty state on every `load_requested_extensions` error exit; consider skip-one-call semantics on host `Err`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
