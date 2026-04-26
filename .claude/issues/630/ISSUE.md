# FNV-D2-02: FLST FormID lists dropped — PERK IsInList conditions and Caravan deck unreachable

## Finding: FNV-D2-02

- **Severity**: MEDIUM
- **Source**: `docs/audits/AUDIT_FNV_2026-04-24.md`
- **Game Affected**: FNV (PERK conditions, COBJ recipes, CCRD/CDCK Caravan), FO3, Skyrim, FO4
- **Location**: no `b"FLST"` arm in [crates/plugin/src/esm/records/mod.rs](crates/plugin/src/esm/records/mod.rs)

## Description

FLST is a flat list of FormIDs referenced from:
- PERK entry-point conditions (`IsInList <flst>`) — ~50 vanilla FNV PERKs check at least one FLST
- COBJ recipe ingredient lists
- Quest objective filters

With FLST undispatched, every `IsInList <flst>` returns "not in list" because the list is empty at lookup. Affects perk conditional logic and the Caravan deck (CCRD / CDCK) gameplay.

Skyrim and FO4 use FLST extensively as well (faction overrides, quest enablers).

## Suggested Fix

Add a parser and dispatch:

```rust
// records/list_record.rs (new)
pub struct FlstRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub entries: Vec<u32>,  // LNAM array
}

// records/mod.rs dispatch
b"FLST" => extract_records(reader, group, &mut |r| {
    let flst = parse_flst(r)?;
    index.form_lists.insert(flst.form_id, flst);
    Ok(())
})?,
```

UESP FLST layout: EDID + LNAM (each is a u32 FormID, can repeat).

## Related

- FNV-D2-01 (companion) — ENCH same dispatch shape.
- #520 (open) — PerkRecord stub; FLST is needed to evaluate PERK conditions.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Once FLST and ENCH land, audit other top-level GRUPs in FNV.esm dropped at the catch-all skip (Dim 2 listed 30 such labels).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Parse FNV.esm; assert `index.form_lists.len() > 0`; assert a known FLST (e.g. `WeapTypeAssaultCarbineList`) has > 1 entry.

_Filed from audit `docs/audits/AUDIT_FNV_2026-04-24.md`._
