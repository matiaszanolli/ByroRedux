# FNV-D4-2026-09-11-01: effective_actor_level returns 0 verbatim, silently emptying every leveled list an authored-level-0 NPC_ owns

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4232
**Labels**: bug, medium, legacy-compat, gameplay, game:fnv, esm-plugin
**Source**: `/audit-fnv` — `docs/audits/AUDIT_FNV_2026-09-11.md`, finding FNV-D4-2026-09-11-01

**Severity**: MEDIUM
**Dimension**: ESM Record Parser (FNV Data) — `/audit-fnv` Dimension 4
**Location**: `crates/plugin/src/esm/records/actor/mod.rs:96-102` (`effective_actor_level`); consumed at `crates/plugin/src/equip.rs:637-716`; called from `byroredux/src/npc_spawn.rs:82,157,208,858` and `byroredux/src/cell_loader/references/mod.rs:644`

**Description**: `effective_actor_level` returns the raw authored `level` field verbatim for non-PC-level-mult `NPC_` records (`npc.level.max(0)`), deliberately never floored to 1. But every `LVLI` on `FalloutNV.esm` has a minimum entry level of 1, so an actor at `effective_actor_level == 0` finds **zero eligible entries in every leveled list it owns**, silently and simultaneously emptying armor, weapon, ammo, grenade, chem and money loot.

**Evidence**: Confirmed in current source — `effective_actor_level`'s non-mult branch is `npc.level.max(0)`, not `.max(1)`. Measured live against `FalloutNV.esm`: 30/3,816 `NPC_` records (0.8%) author `level == 0`; at least 3 of a 5-NPC sample are confirmed spawn-reachable via an `LVLN` pick at entry-level 1 (e.g. `VHDNCRTrooper1aHeavy`, picked by `LVLN VarVHDNCRTrooper1Heavy`), and each resolves every owned leveled list to an empty `Vec` at its own authored level=0 but non-empty at any level ≥ 1 — isolating the defect to the level value itself, not list content or recursion logic.

**Impact**: Silently empties every leveled-list-derived item an affected actor owns (armor, weapon, ammo, grenade, chem, money) at spawn time. Distinct from, and additive to, the already-tracked CMNY leaf-kind silent-skip (`FNV-2026-08-26-D4-06`) and from the already-fixed #2955/#3081/#3171 level-derivation fixes, whose non-mult branch this sits entirely inside.

**Related**: #2955, #3081, #3171 (prior level-derivation fixes to the same function, this defect sits inside their surviving branch), `FNV-2026-08-26-D4-06` (adjacent but distinct CMNY silent-skip).

**Suggested Fix**: Thread the `LVLN` pick's entry-level into gear resolution at the call sites that already know it, rather than re-deriving from the (possibly template-only, level-0) base `NPC_` record. Do not simply `.max(1)` `effective_actor_level` itself — #3081 previously and deliberately rejected that exact fix in a sibling copy of this function for reasons that likely still apply; the real fix threads the *contextual* spawn-level (already known at the `LVLN` pick site) into gear resolution instead of coercing the record-level value.

## Completeness Checks
- [ ] **SIBLING**: Check `actor_value_derive.rs`'s `effective_npc_level` (the historical duplicate #3081 deleted) hasn't reappeared with the same divergence.
- [ ] **TESTS**: A regression test pins gear resolution for a level-0-authored `NPC_` spawned via an `LVLN` pick at a non-zero entry level, asserting non-empty leveled-list resolution.
