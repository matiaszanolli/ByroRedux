# FNV-2026-08-26-D4-06

**Issue**: #3341
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: LOW
**Dimension**: 4 — ESM Record Parser
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/plugin/src/equip.rs:390-396` (the
`let Some(lvli) = index.leveled_items.get(&form_id) else { return; }` branch).

**Premise verified**: the expansion recognises a form only if it is in
`index.items` (ARMO/WEAP/MISC/ALCH/KEYM/AMMO/NOTE/BOOK/INGR) or
`index.leveled_items`. CCRD, CMNY and IMOD are dispatched into their own maps
(`dispatch_misc_stub.rs:152,172`; `dispatch_misc_gameplay_b.rs:101`) and never
reach `items`.

**Evidence** — census of all 13,319 LVLO leaf targets by resolved record type:

```
{LVLI: 6430, ALCH: 2472, MISC: 1465, WEAP: 1146, ARMO: 941, AMMO: 342,
 CCRD: 270, CMNY: 97, IMOD: 69, BOOK: 46, KEYM: 38, NOTE: 3}
```

Also 3 NPC `CNTO` entries point at `LIGH` and 3 at `CMNY`, likewise unresolvable.

**Impact**: correct for the equip use case (caravan cards, caravan money and
weapon mods are not wearable), but the drop is silent and the same helper is the
obvious candidate for a future container/loot generator, where 3.3% of FNV leaves
vanishing would be wrong.

**Fix sketch**: leave behaviour as-is; log at `debug!` when a leaf resolves to a
known-but-non-item map so a loot consumer sees the boundary, or split a
`expand_leveled_any` that also consults the stub maps.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
