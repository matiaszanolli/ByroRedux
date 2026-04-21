# Investigation — Issues #446 + #447

## Domain
ESM — `crates/plugin/src/esm/records/`

## Pattern match

Follows the #458 supplementary-record pattern (WATR/NAVI/NAVM/REGN/ECZN/LGTM/HDPT/EYES/HAIR in `misc.rs`): minimal stub struct capturing EDID + FULL + any key form-ID refs / scalars. No deep decoding — unblocks the catch-all skip and surfaces the records to dangling-reference resolution; full per-record decoding lands with the consuming subsystem.

## Coverage

**#446** PACK (AI packages)

**#447** QUST + DIAL + MESG + PERK + SPEL + MGEF. INFO is deferred (nested under DIAL's GRUP tree and needs a multi-type walker extension to `extract_records`).

## Field-floor per type (UESP-backed minimum)

| Type | Struct fields |
|------|---|
| PACK | form_id, editor_id, package_flags (u32 from PKDT), procedure_type (u32 from PKDT) |
| QUST | form_id, editor_id, full_name, script_ref (from SCRI), quest_flags (u8 from DATA), priority (u8 from DATA) |
| DIAL | form_id, editor_id, full_name, quest_refs (Vec<u32> from QSTI) |
| MESG | form_id, editor_id, full_name, description, owner_quest (from QNAM) |
| PERK | form_id, editor_id, full_name, description, perk_flags (u8 from DATA) |
| SPEL | form_id, editor_id, full_name, spell_flags (u32 from SPIT), cost (u32 from SPIT) |
| MGEF | form_id, editor_id, full_name, description, effect_flags (u32 from DATA) |

## Tests

- Extend `records/mod.rs` record-count assertions to include the 7 new categories (just `>= 1` sanity floors — mod data drives exact counts).
- Extend `crates/plugin/tests/parse_real_esm.rs` with assertions against FNV + FO3 baselines.

## Scope
3 files modified. Within the /fix-issue 5-file budget.
