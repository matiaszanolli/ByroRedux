# #3217 — SKY-2026-08-20-D3-01: TES5 LVLF bit 0x02 is a per-count re-roll, not "take all" — 1,491 of 5,118 Skyrim NPCs over-expand their outfit (mean 38.7 items vs 2.5, worst case 1,612)

**Issue**: #3217 — https://github.com/matiaszanolli/ByroRedux/issues/3217
**Finding ID**: `SKY-2026-08-20-D3-01`
**Severity**: HIGH
**Dimension**: 3 — NPC equip (M41)
**Audit**: `/audit-skyrim` — `docs/audits/AUDIT_SKYRIM_2026-08-20.md` (HEAD `bb0b92f2`, 2026-08-20 comprehensive suite)
**Labels**: high, legacy-compat, import-pipeline, gameplay, bug
**Filed**: 2026-08-20 · `/audit-publish`

---

**Audit**: `/audit-skyrim` — `docs/audits/AUDIT_SKYRIM_2026-08-20.md` (Dim 3 — NPC equip / M41), HEAD `bb0b92f2`
**Finding ID**: `SKY-2026-08-20-D3-01`

- **Severity**: HIGH
- **Status**: **NEW — the unfixed half of CLOSED #3069.** #3069 fixed the `0x04` "Use All" half exactly as it recommended (verified in place at `equip.rs:411`, pinned by the test at `:791`). #3069's own Impact section also called out *"the inverse error is also present … over-equipping"* — **that half shipped unchanged.** Filed as a new issue rather than reopening because it is a separate rule with its own measured blast radius; cross-referenced both ways.

## Location

`crates/plugin/src/equip.rs:411`

```rust
let multi_pick = lvli.flags & (0x02 | 0x04) != 0;
if multi_pick { … }
```

Doc premise at `:346-350`.

## Description

The resolver treats `0x02` and `0x04` as interchangeable multi-pick triggers. They are not:

- TES5 `LVLF` bit 2 (`0x04`, xEdit **"Use All"**) means *add every entry*.
- TES5 `LVLF` bit 1 (`0x02`, **"Calculate for each item in count"**) means *repeat the single roll `count` times*.

Expanding every eligible entry on a `0x02` list turns a **level-tier ladder** into a bundle.

## Evidence

Independent Python walk of `Skyrim.esm`'s OTFT->LVLI closure (481 `OTFT`, 3 075 `LVLI`, 5 118 `NPC_`):

```
OTFT-reachable LVLI:                    279
  LVLF histogram: 0x03 x162, 0x04 x49, 0x02 x30, 0x00 x37, 0x01 x1
  bit 0x02 set AND bit 0x04 clear:      192   (69 %)
OTFTs containing >= 1 such list:      70 / 481
NPC_ whose default outfit does:     1491 / 5118  (29 %)
```

A representative list — unambiguously a tier ladder, not a set:

```
LVLI 000FDA10 LItemEnchArmorHeavyGauntletsNoDragon  flags=0x03
    lvl= 1 SublistEnchArmorIronGauntlets01
    lvl= 4 SublistEnchArmorIronGauntlets02
    lvl= 7 SublistEnchArmorSteelGauntlets01
    lvl=13 SublistEnchArmorDwarvenGauntlets02
    lvl=19 SublistEnchArmorSteelPlateGauntlets02
    lvl=26 SublistEnchArmorOrcishGauntlets03
    lvl=33 SublistEnchArmorEbonyGauntlets03      (18 entries total)
```

Each sublist is itself `flags=0x03` with five enchantment variants, so the expansion multiplies. Simulating both rules over every NPC that has an outfit (3 633 of them) at `actor_level = 38`:

```
mean flattened outfit size, current rule (0x02|0x04):   38.74 items
mean flattened outfit size, single-pick on 0x02:         2.50 items
NPCs whose outfit flattens to > 20 items:             1238 / 3633
worst case: dunIronbindBeemJa                          1612 items  (single-pick: 5)
            DA13Orchendor                               219 items  (single-pick: 6)
            DA13EncAfflicted05* family (x several)      196 items  (single-pick: 2)
```

## Impact

**1 491 of 5 118 `Skyrim.esm` NPCs over-expand their outfit** — 34 % of *outfitted* NPCs receive 20+ spurious inventory rows, mean **38.7 items against a correct 2.5**, worst case **1 612 `ItemStack`s on a single actor**.

The *worn* result is mostly salvaged by downstream luck: `equipment_slots.equip()` is last-write-wins over the expansion order (`byroredux/src/npc_spawn.rs:866`) and the weapon picker takes max damage (`:846-853`), so the highest tier usually ends up on the body. But that is an accident of ordering, not a rule — and every inventory-facing surface is wrong by an order of magnitude: loot, barter, pickpocket, save-snapshot size, per-actor allocation.

FO3/FNV/Oblivion `LVLF` has no `0x04`, so the shared `0x02` arm makes this Skyrim/FO4-shaped.

## Related

- **#3069** (CLOSED) — fixed the complementary `0x04` half; this is the half it named but did not ship. Please cross-link on close.
- #896 — LVLI dispatch in outfits
- #2094 — occupancy filter (operates on meshes; does not prune the inventory)
- #2956 — `Use Stats` template inheritance, same delta

## Suggested Fix

Make `0x04` the sole multi-pick trigger and route `0x02` back to the single-pick arm. `0x02`'s faithful meaning — repeat the roll `count` times — is the same base FormID `count` times, which for `count == 1` (the vanilla case in every list sampled above) is exactly single-pick.

Pin with a fixture `LVLI` carrying `flags = 0x03` and level-1/4/7 entries asserting **one** expansion, alongside the existing `flags = 0x04` all-expand guard at `:791`.

## Completeness Checks
- [ ] **SIBLING**: the same `0x02` arm is shared by FO3/FNV/Oblivion/FO4 leveled-list expansion — confirm the change is correct (or inert) for each
- [ ] **TESTS**: a fixture `LVLI` with `flags = 0x03` and a level ladder asserts a single expansion; the existing `0x04` all-expand guard still passes
- [ ] **TESTS**: an outfit-level test asserts the flattened size for a known multi-tier `OTFT` (e.g. `dunIronbindBeemJa`) is single-digit, not four-digit
