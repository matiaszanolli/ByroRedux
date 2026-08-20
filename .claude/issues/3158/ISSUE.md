# Issue #3158: SCR-D6-2026-08-20-01: #2940's HasPerk fix reads a component the player never gets and only FO4+ NPCs ever get — still structurally 0.0 on Skyrim, FO3 and FNV

- **Finding ID**: `SCR-D6-2026-08-20-01`
- **Severity**: MEDIUM
- **Labels**: `medium,scripting,bug`
- **Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-20.md`
- **Filed**: 2026-08-20 (comprehensive 25-audit sweep, `/audit-publish`)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3158

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3158 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: 6 — Scripting Runtime Systems
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/condition.rs`:692-703 (the read) · `byroredux/src/npc_spawn.rs`:204-215 (the only production writer) · `crates/plugin/src/esm/records/actor/mod.rs`:1082-1086 (the `PRKR` parse arm) · `crates/plugin/src/esm/reader.rs`:236-238 (the gate) · `byroredux/src/scene.rs`:1170-1220 (the player-entity component set)
- **Status**: NEW

## Description

`a605ee93` (Fix #2940) correctly repointed `ConditionFunction::HasPerk` from the
dead `PerkList` projection to the canonical `byroredux_core::character::Perks`,
and the FormID spaces line up — `Perks::perk_form_id` is written through
`remap_fid` and `param_1` is load-order remapped by `remap_condition_form_ids`
for indices 448/449, so the comparison is apples-to-apples.

What the fix did **not** change is *who writes `Perks`*.

There is exactly one production writer, `spawn_npc_entity`, fed by
`NpcRecord::perks`, which is populated only inside the
`captures_av_props = game.uses_actor_value_properties()` gate — i.e.
**`Fallout4 | Fallout76 | Starfield` only**.

Separately, the **player** entity (`scene.rs`, the `PlayerEntity` body) is given
`Transform`, `GlobalTransform`, a character controller, `CollisionShape`,
`RigidBodyData` and a `FormIdComponent`, and nothing else from the CHARAL family
— no `Perks`, no `ActorValues`.

`HasPerk`'s own doc-comment (`condition.rs`:142) claims indices **449 (FO3/FNV)**
and **448 (Skyrim)**. For neither of those families, and for the player in *any*
game including FO4, can the `world.get::<Perks>()` at `condition.rs`:697 ever
return `Some`.

## Evidence

```rust
// crates/scripting/src/condition.rs:696-698 — the read
use byroredux_core::character::Perks;
let Some(perks) = world.get::<Perks>(entity) else {
    return 0.0;
};
```

```rust
// byroredux/src/npc_spawn.rs:204-208 — the only writer
// Perks (FO4+ `PRKR`) — skip the component entirely when the NPC has none.
if !npc.perks.is_empty() {
    world.insert(placement_root, Perks { .. });
}
```

```rust
// crates/plugin/src/esm/reader.rs:236-238 — the gate on the only producer of npc.perks
pub fn uses_actor_value_properties(self) -> bool {
    matches!(self, Self::Fallout4 | Self::Fallout76 | Self::Starfield)
}
```

`crates/plugin/src/esm/records/actor/mod.rs`:360 says it outright: *"Populates a
`Perks` component at spawn. **Empty for pre-FO4 NPCs.** Gated on …"*.

`grep -rn "Perks" byroredux/src crates` outside `crates/core/src/character`
returns those two sites plus `condition.rs` and two save-registry notes — **no
player-side insert anywhere**.

## Impact

Perk-gated dialogue, quest and package CTDAs silently evaluate **false** for the
player in every game, and for every NPC outside FO4 / FO76 / Starfield.

This is the *same observable behaviour* CHAR-D3-01 (#2940) described and was
closed for, so the closed issue reads as resolved while the user-visible symptom
is unchanged for the reference title (Skyrim) and for the reference-of-record
(FNV).

A condition returning `0.0` is the Bethesda-correct safe default in isolation,
which is exactly why it is silent: there is no log, no telemetry and no test that
distinguishes *"this actor genuinely lacks the perk"* from *"no actor in this
game can ever have one"*.

## Related

- **#2940 (CLOSED)** — the fix is correct as far as it goes; this is the
  untouched half upstream of it
- #2947, #2944 — sibling CHARAL perk findings
- The ESM-side question *"does Skyrim `NPC_` carry `PRKZ`/`PRKR`, and if so
  should `uses_actor_value_properties` gate it?"* belongs to `/audit-esm` Dim 4.
  **This finding deliberately does not assert the Skyrim wire format.**

## Suggested Fix

Two independent halves:

**(a)** Give the player entity a `Perks` component (empty is fine) at spawn, so
the distinction between "checked and absent" and "unrepresentable" exists at all,
and so a future `AddPerk` effect has somewhere to write.

**(b)** Either widen the `PRKR` parse gate past `uses_actor_value_properties` for
the games whose `NPC_` actually carries it, **or** add a one-line `log::debug!`
at the `else` arm of `condition.rs`:697 naming the game, so the structural zero is
at least diagnosable.

A regression test asserting `HasPerk` is non-zero for a Skyrim-parsed NPC would
pin whichever choice is made.

---
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-20.md` (finding `SCR-D6-2026-08-20-01`)

## Completeness Checks
- [ ] **SIBLING**: The other CHARAL components with the same single-writer/`uses_actor_value_properties` shape (`ActorValues`, `CharacterLevel`, `Background`) checked for the same reachability hole, on the player especially
- [ ] **TESTS**: A regression test pins this specific fix — one that would go RED if `Perks` stopped being written for the game family the fix targets
