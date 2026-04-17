# FO4-D4-C2: SCOL body not parsed — 15,878 ONAM/DATA placement entries silently discarded

**Issue**: #405 — https://github.com/matiaszanolli/ByroRedux/issues/405
**Labels**: bug, critical, legacy-compat

---

## Finding

`crates/plugin/src/esm/cell.rs:233` routes `SCOL` to `parse_modl_group` (lines 818–924), which only reads `EDID` and `MODL`. SCOL's defining subrecord set (`ONAM` per-prefab + `DATA` placement arrays) is never touched.

## Evidence — vanilla Fallout4.esm byte-level scan

2617 SCOL records; subrecord frequency:

```
ONAM 15878   DATA 15878   OBND 2617   EDID 2617   MODT 2616   MODL 2616
FLTR 2244    FULL 124     PTRN 5      MODS 3
```

Each `ONAM` (4 bytes) is a child base form ID. Each `DATA` carries an array of 28-byte placements (`pos[3f32] + rot[3f32] + scale[f32]`) for that base. Average 6 child placements per SCOL → **15,878 per-instance placements dropped**.

Sample SCOL dump:
```
SCOL 0x00249DF2   — CambridgeDecoInt01
  EDID, OBND, MODL="SCOL\Fallout4.esm\CM00249DF2.NIF", MODT, ONAM
  DATA(280), ONAM DATA(112), ONAM DATA(56), ONAM DATA(112)
```

## Impact

- **Vanilla rendering**: LOW. 2616/2617 SCOLs ship a cached combined mesh at `SCOL\Fallout4.esm\CM*.NIF` (BSPackedCombined path, closed by #365), so the geometry renders from the cached NIF.
- **Mods**: HIGH. Mod-added SCOLs rarely ship a `CM*.NIF` (previsibine step is author-gated); those silently disappear.
- **Edge cases lost**: the one SCOL without MODL contributes nothing; 2244 SCOLs have `FLTR` entries, 3 have `MODS` (material swap) — all unparseable.

## Fix

Multi-step:

1. Create `crates/plugin/src/esm/records/scol.rs` with:
   ```rust
   pub struct ScolPlacement { pub pos: [f32; 3], pub rot: [f32; 3], pub scale: f32 }
   pub struct ScolPart { pub base_form_id: FormId, pub placements: Vec<ScolPlacement> }
   pub struct Scol { pub edid: String, pub model_path: String, pub parts: Vec<ScolPart>, pub filter: Option<Vec<FormId>> }
   ```
2. Split SCOL out of the MODL arm in `cell.rs`; plumb SCOL parts into `StaticObject` (add `scol_parts: Option<Vec<ScolPart>>` per Dim 4 M1) or a new `statics_composite` map.
3. Cell loader at `byroredux/src/cell_loader.rs` needs a SCOL-expansion pass: when `REFR.NAME` resolves to a SCOL whose cached mesh is missing OR whose mesh path is empty, instantiate each child placement as a synthetic `PlacedRef`.
4. Add `SCOL` constant to `crates/plugin/src/record.rs` `RecordType` enum (Dim 4 M2).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: `MOVS` (Dim 4 H1 — 0 vanilla records, DLC-only), `PKIN` (Dim 4 H2 — CNAM bundle) need parallel treatment.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic SCOL record with 2 ONAM/DATA pairs round-trips; live test confirms 2617/2617 vanilla SCOLs expand to 15,878 placements in aggregate.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 4 C2 (and Dim 6 B2/B3).
