# SCR-D7-2026-10-17-06: Lowering accepts fragments with more provider barriers than the runtime can cross; the tail past barrier 64 is dropped after the prefix has applied

Issue: #6022
Labels: low, scripting, bug

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

> Residual of closed #3946 (its fix chose the runtime-side option). Filed as a new issue because #3946 is closed.

### SCR-D7-2026-10-17-06: Lowering accepts fragments with more provider barriers than the runtime can cross; the tail past barrier 64 is dropped after the prefix has applied

- **Severity**: LOW
- **Dimension**: Provider/Extender Layer
- **Location**: `crates/scripting/src/translate/effects/prims/mod.rs:61-70` (no barrier count), `fragment/effects/deferred.rs:464-488` (`at_barrier_limit` skip), `fragment/state.rs:637`
- **Status**: Existing: #3946 (closed; its fix chose "skip the tail whole at runtime", leaving the lowering-side bound as the issue's own suggested alternative)
- **Untrusted-Input**: Yes
- **Description**: `classify_effect_with_providers` turns every claimed provider call into `Effect::ProviderCall` with no count. A straight-line quest-stage fragment with >= 64 StorageUtil/JContainers calls (a plausible mod "init" fragment) lowers fine; at runtime barrier 64 executes and its tail - including any later `SetStage` - is silently dropped with one warn. The fragment-level contract is "check ahead of time, run whole or not at all".
- **Evidence**: `deferred.rs:464-488`; test `provider_barrier_cap_declines_the_deepest_tail_whole` pins the runtime behaviour only.
- **Impact**: Quest stuck mid-fragment on a mod with a long provider-heavy fragment. Low because #3946's author judged the number "plausible for StorageUtil-heavy init fragments" and accepted a warn.
- **Related**: #3946.
- **Suggested Fix**: Count `ProviderCall` effects along the longest path in `lower_fragment` and decline above `MAX_PROVIDER_FRAGMENT_BARRIERS`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
