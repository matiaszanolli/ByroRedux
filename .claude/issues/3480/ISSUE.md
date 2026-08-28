# #3480 — CHAR-2026-08-27b-D5-01: the Skyrim pool bases now come from the `Use Stats` template chain, but race is a `Use Traits` field — 1,180 NPCs contradict `Background` on the same entity

**Labels**: bug, medium, game:skyrim, esm-plugin, character
**Filed from**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` via `/audit-publish`

---

**Severity**: MEDIUM
**Dimension**: Population Boundary
**Game**: Skyrim SE
**Location**: `crates/plugin/src/esm/records/actor_value_derive.rs:188` (the hoisted resolve) and `:201-220` (`derive_skyrim_actor_values`)
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` (CHAR-2026-08-27b-D5-01), HEAD `969d81c8`

## Description

`derive_npc_actor_values` resolves the `TPLT` chain **once**, through `resolve_inherited_stats` (gate bit `0x0002`, "Use Stats"), and hands the single resolved record to all three arms. The Skyrim arm then reads `npc.race_form_id` off it to look up `RACE.DATA`'s starting Health / Magicka / Stamina.

Race is not a stat: it is inherited through the independently-set **`Use Traits` (`0x0001`)** bit, which the same codebase resolves with a *different* function (`equip::resolve_inherited_traits`) and which `stamp_character_components` correctly uses for `Background` on the very same entity. The two bits are separate, so a shell can carry one without the other — and the shipped data confirms it does, at scale.

The codebase's own documentation says so: `NpcRecord::template_flags`'s doc comment (`crates/plugin/src/esm/records/actor/mod.rs:370-371`) reads *"`0x0001` — **Use Traits** (race). Consumed by `equip::resolve_inherited_traits` (#2956)"*, and `crates/plugin/src/equip.rs:314-318` documents `resolve_inherited_traits` as *"the NPC record that should supply **race** (and other 'traits' fields)"*.

This is **not** a re-file of #3381: that issue was "the arm reads the shell"; this is "the arm now reads the wrong chain". Introduced by `7445506c`, which landed after the earlier same-day report and was unaudited.

## Evidence

```rust
// actor_value_derive.rs:188
let npc = crate::equip::resolve_inherited_stats(npc, effective_actor_level(npc), index);
// …:201-209
fn derive_skyrim_actor_values(npc: &NpcRecord, index: &EsmIndex) -> Vec<(u32, f32)> {
    let Some(race) = index.races.get(&npc.race_form_id) else {
        return Vec::new();
    };
    …
    ("Health", race.starting_health, npc.health_offset),
```

against the sibling that gets it right:

```rust
// byroredux/src/npc_spawn.rs:151-152,174
let stats_npc  = resolve_inherited_stats(npc, shell_level, index);
let traits_npc = resolve_inherited_traits(npc, shell_level, index);
…
race_form_id: traits_npc.race_form_id,
```

Measured on vanilla `Skyrim.esm` (temporary probe over `index.npcs`, using the crate's own `resolve_inherited_stats` / `resolve_inherited_traits`):

| Measure | Count | Share of 5,118 `NPC_` |
|---|---:|---:|
| `NPC_` with a `TPLT` | 3,651 | 71.3 % |
| `Use Stats` bit set | 3,182 | 62.2 % |
| **stats-chain race ≠ traits-chain race** | **1,180** | **23.1 %** |
| …and the two races give a different `RACE.DATA` (H, M, S) triple | **118** | 2.3 % |
| stats-chain race ≠ the shell's own `RNAM` | 1,680 | 32.8 % |

Concrete cases, all from vanilla `Skyrim.esm`:

```
MS07LvlBlackbloodMissileNordM (000DC8DC)
  stats-race 00109C7C → (H 12,  M —,    S 200)      ← what the code uses
  traits-race 00013746 (NordRace) → (H 50, M 50, S 50)  ← what Background says
DA02CultistF2 (0004D8D3)
  stats-race 00109C7C → (H 12,  M —,    S 200)
  traits-race 00013742 → (H 50,  M 50,  S 50)
```

Note `00109C7C` authors **no** starting Magicka, so those actors also lose their Magicka actor value entirely — not merely a wrong number.

## Impact

118 Skyrim NPCs receive pool bases from the wrong `RACE` record (some losing a pool outright), and 1,180 carry a `Background.race_form_id` that disagrees with the race their own vitals were derived from — the exact same-entity contradiction #3171 and #3381 were each filed to eliminate.

Against the pre-fix baseline this is still a large net win (512 wrong pool triples before, 118 after), so **the fix must be refined, not reverted**.

## Related

- #3381 (the fix this refines), #3171 (same defect class)
- #3444 (`regen.rs` guard-drop) and #3441 (ActorValues↔CharacterRuleset lock-order cycle) — concurrently filed, different sites
- CHAR-2026-08-27-D5-01

## Suggested Fix

Resolve **both** chains in `derive_npc_actor_values` and hand each arm the record its fields actually belong to — pass `resolve_inherited_traits(...)` to `derive_skyrim_actor_values` for the `race_form_id` lookup while keeping the `Use Stats` record for `health_offset` / `magicka_offset` / `stamina_offset` (which *are* stats, from `ACBS`). A regression test should assert that a shell with `Use Stats` set and `Use Traits` clear keeps its own `RNAM` race, and that `Background.race_form_id` and the race behind the derived pools are always the same FormID.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other two `derive_*_actor_values` arms, and every other `resolve_inherited_stats` consumer)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
