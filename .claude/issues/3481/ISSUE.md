# #3481 — CHAR-2026-08-27b-D5-02: FO4 `calculated_health == 0` "absent" sentinel is not honoured across template resolution — 54 NPCs lose their authored Health

**Labels**: bug, medium, game:fo4, esm-plugin, character
**Filed from**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` via `/audit-publish`

---

**Severity**: MEDIUM
**Dimension**: Population Boundary
**Game**: Fallout 4
**Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:188` (the hoisted resolve) and `:228-242` (`derive_stored_actor_values`)
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` (CHAR-2026-08-27b-D5-02), HEAD `969d81c8`

## Description

A regression introduced by `7445506c` (the #3382 fix), unaudited until now.

`derive_stored_actor_values` reads `npc.calculated_health` / `npc.calculated_action_points` off the **`Use Stats`-resolved** record and pushes the value only `if baked > 0`. Because `0` means *absent*, not *zero*, a shell that authors its own baked `DNAM` Health but inherits from a template that authors none now yields **nothing** where it previously yielded the shell's authored value.

The sentinel is documented in the parser itself — `crates/plugin/src/esm/records/actor/mod.rs:390-394`: *"FO4+ `DNAM` baked `Calculated Health` (u16 @ 0). … **`0` = absent** (no live NPC has 0 base Health, so the sentinel is unambiguous and avoids an `Option` discriminant)."*

Template precedence is correct for a field the template actually carries; an *absent* field should fall back down the chain, exactly the way `resolve_inherited_record` already falls back to the input NPC when the flag or the template is missing.

## Evidence

```rust
// actor_value_derive.rs:231-240
for (avif_editor_id, baked) in [
    ("Health", npc.calculated_health),           // ← npc is the resolved template
    ("ActionPoints", npc.calculated_action_points),
] {
    if baked > 0 {
        if let Some(fid) = index.actor_value_form_id(avif_editor_id) {
            out.push((fid, f32::from(baked)));
```

Measured on vanilla `Fallout4.esm` (3,015 `NPC_`), comparing each record's own `calculated_health` against the one `resolve_inherited_stats` now returns:

| Measure | Count |
|---|---:|
| own `DNAM` Health > 0 but the resolved template's `== 0` (**Health lost**) | **54** |
| own `DNAM` Health `== 0` but the resolved template's > 0 (Health gained) | 35 |
| own `PRPS` non-empty but the resolved template's empty (PRPS lost) | **0** |
| actors ending with no Health value at all | **349** (was **330** pre-fix, net **+19**) |

The `PRPS` half is clean — no FO4 shell loses its property array — so the defect is specific to the `> 0` sentinel test on the baked `DNAM` pair.

## Impact

`stamp_actor_values` (`byroredux/src/npc_spawn.rs:99-112`) only inserts `ActorVitals` when the derived pairs contain the Health AVIF key, so these actors spawn with **no `ActorVitals`** and cannot be damaged or killed by `combat_damage_system` — the project's active P2 execution focus. 19 more FO4 actors are in that state than before the fix.

## Related

- #3382 (the fix this regresses out of)
- CHAR-2026-08-27-D5-02 (which measured the 330 baseline)
- Sibling finding at the same call site: the Skyrim `Use Traits` chain defect (filed alongside this one)

## Suggested Fix

Make the baked-`DNAM` read fall back down the chain on the absent sentinel — take the resolved template's value when it is `> 0`, otherwise the shell's own — and pin it with a test built from a shell whose `Use Stats` template has no `DNAM`. Apply the same fallback explicitly for `calculated_action_points`, which carries the identical sentinel.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`calculated_action_points`, the `PRPS` path, and the other `derive_*_actor_values` arms)
- [ ] **TESTS**: A regression test pins this specific fix
