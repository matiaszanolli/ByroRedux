# #3483 — CHAR-2026-08-27b-D4-01: flat Fatigue regen is gated behind the `CharacterRuleset` lookup that only Magicka needs

**Labels**: bug, low, game:oblivion, character
**Filed from**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` via `/audit-publish`

---

**Severity**: LOW
**Dimension**: Pools, Afflictions & Reputation
**Game**: Oblivion (the only game whose regen config exists)
**Location**: `crates/core/src/character/regen.rs:174-200` (`pool_regen_tick_system`)
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` (CHAR-2026-08-27b-D4-01), HEAD `969d81c8`

## Description

Latent — the system is inert today (`PoolRegenConfig` has no production insertion site).

`pool_regen_tick_system` acquires `CharacterRuleset` with a `let … else { return; }` **before** the actor loop, but the ruleset is used only inside the Magicka branch (to look up the max-Magicka row and check its `DerivedScope`). Fatigue's regen is a flat constant that needs no ruleset at all — `regen.rs:70-73` documents `FATIGUE_REGEN_PER_SEC = 10.0` as *"vanilla Oblivion's Endurance coefficient (`fFatigueReturnMult`) is `0.0`, so this is the whole formula"* (`charal-oblivion-ruleset.md:386-388`), and it reads no ruleset row.

Yet a load with a `PoolRegenConfig` and no `CharacterRuleset` silently regenerates **neither** pool.

## Evidence

```rust
let Some(ruleset) = world.try_resource::<CharacterRuleset>() else {
    return;                       // ← Fatigue never reached
};
let Some(mut avs_q) = world.query_mut::<ActorValues>() else { return; };
for (_entity, avs) in avs_q.iter_mut() {
    if avs.get(config.fatigue_avif).is_some() {
        avs.restore(config.fatigue_avif, FATIGUE_REGEN_PER_SEC * elapsed);
    }
```

The three prior "silent gate" fixes in this file (#2950's two-resource gate, #2932's scope check, #2153's guard scope) all addressed the *documented* preconditions; this fourth gate is undocumented — the system's own docstring enumerates the gates as `PoolRegenConfig` and `PoolRegenAccumulator` and does not mention `CharacterRuleset`.

## Impact

None today. When Oblivion wiring lands, a load order that resolves the regen AVIFs but not a ruleset loses Fatigue regen with no log line — the same "indistinguishable from *no game loaded*" failure mode #2950 was filed for.

## Related

- #2950, #2932 (the earlier silent-gate fixes in this same system)
- #3444 — the concurrently-filed #2153 guard-drop finding at `regen.rs:153-180`. **This is a different defect at an adjacent line** (an over-broad early-return, not a lock hold) and does not overlap; the two fixes touch the same function and should be sequenced.
- #3441 — the `ActorValues ↔ CharacterRuleset` lock-order cycle, which involves the same `CharacterRuleset` acquire; narrowing this acquire's scope interacts with that fix.

## Suggested Fix

Move the `CharacterRuleset` acquire inside the Magicka branch (or make it a `try_resource` whose `None` only disables the scoped-max lookup, falling back to `base_max` as the branch already does for player-only rows), and add `CharacterRuleset` to the docstring's gate list either way.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other `try_resource`-gated character systems — `affliction_tick_system`, and the CHARAL consumers in `crates/scripting/src/condition.rs`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved — narrowing the `CharacterRuleset` hold interacts with #3441 and #3444
- [ ] **TESTS**: A regression test pins this specific fix (a world with `PoolRegenConfig` + `PoolRegenAccumulator` but no `CharacterRuleset` still regenerates Fatigue)
