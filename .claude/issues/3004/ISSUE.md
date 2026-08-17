# RT-2026-08-16-05: fixing the AVIF prefix bug will not make actors damageable — no Health term in auto-calc

**Issue**: #3004
**Severity**: MEDIUM
**Dimension**: Actor value derivation
**Labels**: `medium,import-pipeline,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RUNTIME_2026-08-16.md`.

**Location**: `crates/plugin/src/esm/records/actor_value_derive.rs`:180-208 · `byroredux/src/npc_spawn.rs`:102-114

## Description

#2986 (ESM-2026-08-16-D7-01) shows the FO3/FNV `AVIF` lookup resolves nothing because vanilla EditorIDs are `AV`-prefixed. **Fixing that prefix bug will not make FO3/FNV actors damageable**, because `derive_autocalc_actor_values` has **no Health term at all**.

## Evidence

Re-verified 2026-08-17: `sed -n '180,208p' crates/plugin/src/esm/records/actor_value_derive.rs | grep -i "health\|vitals"` returns **nothing**. The derivation produces SPECIAL and skill pairs; Health is not among them.

`byroredux/src/npc_spawn.rs`:102-114 returns early on `pairs.is_empty()`, so today the absence is masked by the prefix bug — both failures produce "no `ActorVitals`".

## Impact

Anyone fixing #2986 will reasonably expect FO3/FNV actors to become damageable and find they are not, because a second independent gap sits behind the first. Without `ActorVitals`, `byroredux/src/combat.rs`:200 still returns before touching the target.

Worth filing separately precisely so the two are not conflated: #2986 is a lookup bug, this is a missing derivation term.

## Suggested Fix

Add a Health term to the FO3/FNV auto-calc derivation (sourced from the CHARAL FNV/FO3 ruleset's derived-stat formula, not invented), and verify end-to-end that an FO3/FNV actor gains `ActorVitals` after #2986 lands.

**Do not guess the formula** — `docs/engine/charal-fnv-fo3-ruleset.md` is the authority.

## Related

- #2986 (ESM-2026-08-16-D7-01 — the prefix bug that currently masks this)
- `AUDIT_CHARACTER_2026-08-16` § CHAR-2026-08-16-D1-01

## Completeness Checks
- [ ] **ORDER**: Verified together with #2986 — fixing either alone leaves FO3/FNV actors undamageable
- [ ] **NO-GUESSING**: The Health formula comes from `charal-fnv-fo3-ruleset.md`, not from inference
- [ ] **END-TO-END**: An FO3/FNV actor demonstrably gains `ActorVitals` and takes damage
- [ ] **TESTS**: A regression test pins a non-empty derived Health pair on real FNV data

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3004 --json state` when live state is needed.*
