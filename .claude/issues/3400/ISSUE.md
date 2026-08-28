# #3400 — ESM-2026-08-27-D3-01: SCOL and PKIN child FormIDs bypass the load-order remap — the second and later FO4 DLC's static collections and package-ins lose every child

**Labels**: high, esm-plugin, bug, game:fo4
**Source**: `docs/audits/AUDIT_ESM_2026-08-27.md`

---

**Audit**: `docs/audits/AUDIT_ESM_2026-08-27.md` (`/audit-esm`, deep, tree `main` @ `969d81c8`)
**Severity**: HIGH · **Dimension**: FormID Remap, Load Order & ESL Space
**Record / Sub-record**: `SCOL` / `ONAM`, `FLTR`; `PKIN` / `CNAM`, `VNAM`
**Location**: `crates/plugin/src/esm/records/scol.rs` (`parse_scol` — `ONAM`, `FLTR` arms), `crates/plugin/src/esm/records/pkin.rs` (`parse_pkin` — `CNAM`, `VNAM` arms). Consumers: `byroredux/src/cell_loader/refr.rs` (`expand_scol_placements_with_depth`, `expand_packin_placements_with_depth`)
**Status as filed**: NEW (same defect class as `#3314`, CLOSED — which fixed `cell/`; `records/` was never swept)

## Description

`#3314` established the crate's rule and made it structural for the cell tier: `read_form_id` in `crates/plugin/src/esm/cell/helpers.rs` now takes `&EsmReader` as a *required* parameter precisely so a sub-record FormID cannot be read without being remapped. The record tier was not swept. `parse_scol` and `parse_pkin` take no remap at all and store their child references raw, and those raw values are then looked up in `index.scols` / `index.packins` / `index.statics` — every one of which is keyed by the **remapped** id, because `EsmReader::read_record_header` remaps every record's own FormID.

## Evidence

`crates/plugin/src/esm/records/pkin.rs`:

```rust
b"CNAM" => {
    if let Some(form) = read_u32(&sub.data) {
        contents.push(form);
    }
}
```

`crates/plugin/src/esm/records/scol.rs` is the same shape for `ONAM`. Consumed unremapped in `byroredux/src/cell_loader/refr.rs`:

```rust
for part in &scol.parts {
    for p in &part.placements {
        ...
        if depth + 1 < MAX_PKIN_DEPTH && index.scols.contains_key(&part.base_form_id) {
```

and `index.packins.get(&base_form_id)` / `index.packins.contains_key(&child_form_id)` in `expand_packin_placements_with_depth`.

The remap is non-identity on ordinary multi-DLC load orders. Master lists read directly off disk (outside this parser):

```
Fallout4.esm       []
DLCRobot.esm       ['Fallout4.esm']
DLCCoast.esm       ['Fallout4.esm']
DLCNukaWorld.esm   ['Fallout4.esm']
```

Every FO4 DLC has exactly one master, so its self mod-index is `0x01`. `allocate_global_slot` (`byroredux/src/cell_loader/load_order.rs`) assigns global slots by position, so in `[Fallout4.esm, DLCRobot.esm, DLCCoast.esm]` `DLCCoast` gets slot `0x02` while its own forms are authored `0x01…` — `FormIdRemap::remap` rewrites its record keys to `0x02…`, and every raw `ONAM`/`CNAM` still says `0x01…`. The same arithmetic makes `HearthFires.esm` (slot 3, self index 2) and `Dragonborn.esm` (slot 4, self index 2) non-identity in the vanilla five-plugin Skyrim order, and **any** ESL is non-identity by construction (`GlobalSlot::Light` composes into `0xFE…`).

## Impact

On the second and later DLC/mod plugin of an FO4 load order, every `SCOL` child lookup and every `PKIN` `contents` lookup misses. `expand_scol_placements_with_depth` emits placements whose base form resolves to nothing, and `expand_packin_placements_with_depth` returns `None` for the whole package-in — so mod- and DLC-added static collections and package-ins (`FalloutNV.esm` alone ships 98 SCOLs behind 1,084 REFRs; vanilla FO4 ships 2,617) render as empty space rather than as geometry. Silent: the miss is indistinguishable from an unshipped `CM*.NIF`.

## Related

`#3314` (CLOSED — the CELL/WRLD/LAND half of exactly this); the sibling MEDIUM finding ESM-2026-08-27-D3-02 (the latent remainder of the same sweep).

## Suggested Fix

`parse_scol_group` / `parse_pkin_group` (`crates/plugin/src/esm/cell/support.rs`) already hold a `&mut EsmReader`, and their sibling `parse_modl_group` already calls `reader.get_form_id_remap()`. Thread `&Option<FormIdRemap>` into `parse_scol` / `parse_pkin` and apply it to `ONAM` / `FLTR` / `CNAM` / `VNAM`, preserving the null-is-not-a-form rule `FormIdRemap::remap` already implements. Pin it with a two-plugin `merge_from` fixture, the shape `#2991`'s regression test uses.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the remaining unremapped `records/` parsers — see the D3-02 sibling finding)
- [ ] **TESTS**: A regression test pins this specific fix
