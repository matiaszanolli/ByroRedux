# #3934 — SCR-D5-2026-09-06-01: the provider catalog is consulted before the canonical effect-primitive table, so one manifest alias under `Self`/`Game`/`Utility` declines every fragment that uses `Self.SetStage`/`Game.*`/`Utility.Wait`, and an exact-name a...

- **Finding ID**: SCR-D5-2026-09-06-01
- **Labels**: high,scripting,quests,bug
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3934

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: HIGH (silent, all-content blast radius under a realistic enabling condition — one installed extension; the substitution leg is a wrong lowering, the domain table's HIGH row)
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No (enabling condition is an installed extension manifest; affected content is every vanilla QF/SF fragment)
- **Location**: `crates/scripting/src/translate/effects.rs:555-574` (`classify_effect_with_providers`: `lower_provider_call` first, `Err(_) => return None`, then `classify_effect`); `crates/scripting/src/papyrus_provider/lower_call.rs:107-115` (`resolve` miss + `is_known_provider_call` → `Err(UnknownFunction)`), `:302-308` (`contains_provider(provider) || classify_static_call(..)`); `crates/scripting/src/papyrus_provider/catalog.rs:83-111` (`insert_route(.., strict_provider = true)` records the provider name), `:119-121`; `byroredux/src/extensions.rs:444-451` (every manifest alias inserted strictly)
- **Status**: NEW
- **Description**: When `catalog.resolve(ident, method)` misses, `is_known_provider_call` returns true if `catalog.contains_provider(ident)` — true for *any* strictly-inserted manifest alias whose provider is `ident` — and `lower_provider_call` returns `Err`, which `effects.rs:570` turns into a whole-fragment decline. (1) **Wholesale decline**: the SDK prescribes `Self` as the reserved provider for receiver-method aliases (`papyrus_provider/mod.rs:7, 67`; `docs/engine/sdk-v0.1-development-plan.md:1528`); one installed extension with one instance method makes `contains_provider("self")` true, and `Self.SetStage(10)` — the most common fragment statement — declines every fragment containing it. Same for `Game` (kills the MQ101 cart primitives) or `Utility` (kills `Utility.Wait`). `PapyrusFunctionAlias::is_valid` checks identifier syntax only; `insert_route` validates the declaration and rejects duplicates only. (2) **Silent substitution**: an alias exactly spelling `Utility.Wait` / `Game.SetPlayerAIDriven` wins over the canonical primitive and lowers to a deferred host barrier; for `Utility.Wait` this also defeats `has_latent` (`effects.rs:448-458` matches only `Effect::Wait`).
- **Evidence**: orchestrator re-read `effects.rs:555-574`, `lower_call.rs:94-128, 302-308`, `catalog.rs:83-125`, `extensions.rs:444-451` — the ordering, the `contains_provider` predicate, the strict insert, and the absence of a reserved-provider rejection are all as described.
- **Impact**: With one ordinary extension active, the whole quest-stage fragment population (742 lowered fragments on vanilla Skyrim) goes inert at cell load with only `debug!` output; the substitution leg changes vanilla semantics without declining.
- **Disproof attempted**: the no-extension path is unaffected — `engine_compatibility()` inserts non-strictly so `contains_provider` is empty, `classify_static_call` lists neither `Utility` nor the `Game.*` control functions, and `provider_aware_fragment_population_resumes_after_native_call` passes with `Utility.Wait` + `Game.GetModCount` + `Self.SetStage`. The defect needs a strict manifest alias — reachable and, for `Self`, prescribed. No test combines a strict `Self.*`/`Game.*`/`Utility.*` alias with a canonical primitive.
- **Related**: #3159; the SDK coverage-gap note
- **Suggested Fix**: consult `EFFECT_PRIMITIVES` first and hand only unclaimed statements to `lower_provider_call`; reject manifest aliases whose provider is a Papyrus-native receiver/static (`Self`, `Game`, `Utility`, `Quest`, `ObjectReference`, `Actor`, `Debug`, …) at `PapyrusProviderCatalog::insert`. Guard: strict `Self.Touch` alias + `Self.SetStage(10)` must still lower to `Effect::SetStage`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
