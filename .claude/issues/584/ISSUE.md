# #584: FO4-DIM6-02: TXST.MNAM parsed but never resolved at REFR time — 37% of FO4 TXSTs silently drop

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/584
**Labels**: bug, high, legacy-compat, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 6)
**Severity**: HIGH
**Location**:
- Parser: `crates/plugin/src/esm/cell.rs:1618-1624` (TXST.MNAM → `TextureSet.material_path`)
- Storage: `crates/plugin/src/esm/cell.rs:414` (`EsmCellIndex.texture_sets`)
- **Missing consumer**: `rg -n 'texture_sets\b' byroredux/` returns zero hits

## Description

Session-10 closed #406 (`FO4-D4-C3: TXST MNAM silently dropped`) by capturing MNAM into `TextureSet.material_path` and populating `EsmCellIndex.texture_sets`. The consumer side never landed — `byroredux/src/cell_loader.rs` never queries `index.texture_sets`.

Compounds with: REFR override sub-records (XTNM / XTXR / XATO / XMSP / XEMI / XLIG / XSRF) are **not parsed at all**. `cell_loader.rs` reads only `PlacedRef.base_form_id` + six geometric fields. Even once `texture_sets` is consulted, there's no REFR sub-record plumbing to route a per-placement TXST override.

## Evidence

- 37% of vanilla Fallout4.esm TXSTs are MNAM-only (140/382, per closed #406 body).
- `rg 'scols\b|texture_sets\b' byroredux/src/` → 0 hits.
- `cell.rs:174-205` enumerates parsed REFR sub-records: XESP, XTEL, XPRM, XLKR, XRMR, XPOD, XRDS. XTNM / XTXR / XATO / XMSP / XEMI / XLIG are absent.

## Impact

- **140 MNAM-only TXSTs** land in `texture_sets` with nowhere to go.
- **REFR-overridden materials** (weapons, armor, signage placed via REFR with TXST override) render with base-mesh textures. XLIG alone would fix FO4 directional spotlights (vault-signage floods) that currently take base LIGH defaults.

## Suggested Fix (two-stage)

**Stage 1 — extend REFR parser**: add 6 rendering-relevant override sub-records to `PlacedRef`:

| Sub-record | Payload | Purpose |
|---|---|---|
| `XTNM` | u32 TXST FormID | Texture set override for LAND references |
| `XTXR` | u32 TXST + u32 index | Per-slot texture swap |
| `XATO` | u32 TXST FormID | Alternate texture override (FO4 weapons, armor) |
| `XMSP` | u32 MSWP FormID | Material swap per-slot pair list *(MSWP itself unparsed — see FO4-DIM6-05)* |
| `XEMI` | u32 LIGH FormID | Per-REFR emissive attachment |
| `XLIG` | variable | Per-REFR light override (FOV/fade/radius) |

**Stage 2 — thread consumers**: wire `EsmCellIndex.texture_sets` + `MaterialProvider` into the spawn path in `cell_loader.rs`. When a REFR references a TXST FormID in an override sub-record, resolve the `TextureSet`, route `material_path` through `MaterialProvider`, overlay the 8 slot paths onto the spawned `ImportedMesh`.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: Same REFR override pattern applied at WRLD exterior REFR parse and interior cells
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a — `EsmCellIndex` lives on the resource side
- [ ] **FFI**: n/a
- [ ] **TESTS**: Fixture REFR with XTNM + MNAM-only TXST, assert resolved `material_path` populates ECS `Material.material_path`.

## Related

- Parser-side landed as closed #406 (FO4-D4-C3).
- Depends on: FO4-DIM6-05 (MSWP record parser) for full XMSP resolution.
