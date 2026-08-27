# FNV-2026-08-26-D4-05

**Issue**: #3340
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 4 — ESM Record Parser
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/plugin/src/equip.rs:186-192` (constant + comment), `:374-388`
(cap check ordering).

**Premise verified**: the comment reads *"Vanilla outfit nesting tops out around
3-4 levels … 8 leaves comfortable headroom"*. In `expand_leveled_inner` the
`if depth >= LVLI_MAX_DEPTH { return; }` guard runs **before**
`if index.items.contains_key(&form_id) { out.push(form_id); return; }`, so a base
item invoked at `depth == 8` is dropped, not pushed.

**Evidence** — full LVLI graph over FalloutNV.esm (2,738 lists, 13,319 LVLO
entries, 6,430 of them nested LVLI refs):

```
chain-depth histogram: {0:1221, 1:780, 2:521, 3:128, 4:76, 5:8, 6:2, 7:2}
max depth: 7      chains at depth >= 8: 0
depth-7 roots: 000CAE03 VendorChestLydiaMontenegroWeaponsArmor
               000BE41B VendorChestProntoFreeformListGoodStuff
depth-6 roots: 000B41A2 VendorChestWeaponsAndArmor
               00068E96 VendorCaravanJunk2List
```

Simulating the exact cap-8 semantics over **every** NPC-referenced LVLI root:
`kept = 9523, dropped = 0`. The deep chains are all vendor-**chest** roots with 0
direct NPC `CNTO` references, and `expand_leveled_form_id`'s only runtime callers
are the NPC/player inventory paths (`byroredux/src/inventory.rs:203,225`,
`byroredux/src/npc_spawn.rs:850,879`) — CONT inventories are not expanded through
it. **No live defect.**

**Impact**: none today. The margin is one level, not the "comfortable headroom"
claimed, and the boundary is one deeper than an implementer reading the comment
would assume; a mod inserting a single wrapper list above a vendor chest, or a
future container-loot consumer, silently loses the deepest tier.

**Fix sketch**: correct the comment to the measured FNV maximum (7) and move the
`index.items.contains_key` push above the depth guard so a terminal base item at
the boundary is still collected.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
