# #3401 — ESM-2026-08-27-D3-02: ~12 more embedded-FormID sites in records/ still bypass the remap, including two inside a function that already holds it

**Labels**: medium, esm-plugin, bug
**Source**: `docs/audits/AUDIT_ESM_2026-08-27.md`

---

**Audit**: `docs/audits/AUDIT_ESM_2026-08-27.md` (`/audit-esm`, deep, tree `main` @ `969d81c8`)
**Severity**: MEDIUM · **Dimension**: FormID Remap, Load Order & ESL Space
**Record / Sub-record**: `ACTI`/`SNAM`,`RNAM`,`RADR`; `MOVS`/`LNAM`,`ZNAM`; `NAVM`/`DATA`,`NVEX`,`NVDP`,`NVNM`; `FLST`/`LNAM`; `TREE`; `REGN`/`RDWT`,`RDSD`
**Location**: `crates/plugin/src/esm/records/misc/world.rs` (`parse_acti`, `NavmRecord::cell_form`, `NavmExternalConnection::mesh_form`, `NavmDoorTriangle::door_form_id` ×2 — the last added this delta by `#3300`); `crates/plugin/src/esm/records/movs.rs` (`parse_movs`); `crates/plugin/src/esm/records/list_record.rs` (`parse_flst`)
**Status as filed**: NEW (latent half of the `#3314` class; kept separate from the HIGH SCOL/PKIN sibling because these have no live consumer yet)

## Description

The sharpest instance is `parse_acti`, which *receives* `remap: &Option<FormIdRemap>`, applies it to the shared named-field helper, and then reads two more FormIDs raw twelve lines later:

```rust
pub fn parse_acti(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> ActiRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);   // remapped
    ...
    b"SNAM" => out.sound_form_id = SubReader::new(&sub.data).u32_or_default(),   // raw
    b"RNAM" | b"RADR" => {
        out.radio_form_id = SubReader::new(&sub.data).u32_or_default();          // raw
    }
```

`parse_navm` takes no remap at all and stores three cross-record references raw — the parent `CELL` (`DATA` word 0), the `NAVM` on the far side of each external connection (`NVEX` / packed `mesh_form`), and, new this delta, the door `REFR` per threshold triangle (`NVDP` stride-8 row and the packed stride-10 row). `parse_movs`, `parse_flst` and `parse_tree` are the same shape. `grep -c remap` returns **0** for `list_record.rs`, `scol.rs`, `tree.rs`, `pkin.rs`, `movs.rs`, `mswp.rs`, `soun.rs`, `misc/character.rs`, `misc/imagespace.rs` (verified at HEAD).

## Impact

Latent today — consumers were traced for each and none found: `sound_form_id` / `radio_form_id` / `loop_sound_form_id` / `activate_sound_form_id` have zero references outside `crates/plugin`, and `EsmIndex::form_lists` is only populated, never read. The `NAVM` references are the ones nearest to a consumer (`#2372` / EX-16 wants exactly the cross-tile join and door association these fields carry). Filed so the first consumer does not inherit a silently-wrong map key, which is precisely how `#3314` came about. Severity is MEDIUM rather than HIGH only because nothing reads them yet.

## Related

`#3314`; `#2372` (EX-16, the NAVM consumer that will make this live); `#3300` (added the `NVDP` fields); the HIGH sibling ESM-2026-08-27-D3-01 (SCOL/PKIN, which *does* have a live consumer).

## Suggested Fix

Finish the `#3314` sweep on the record tier the same way it was finished on the cell tier — make the remap a required parameter rather than an optional courtesy. The dispatch routers already resolve `reader.get_form_id_remap()` for `NPC_`/`CREA`/`FACT`/`WATR`/`PACK`/`SCEN`/`PERK`, so the plumbing exists; the change is per-parser signature work plus one grep-based guard test.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (every `records/` parser with `grep -c remap == 0`)
- [ ] **TESTS**: A regression test pins this specific fix
