# #589: FO4-DIM4-03: PKIN routed through MODL-only parser (PKIN has no MODL) — 872 vanilla records silently empty

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/589
**Labels**: bug, medium, legacy-compat, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 4)
**Severity**: MEDIUM
**Location**: `crates/plugin/src/esm/cell.rs:521` (`b"PKIN"` in the MODL catch-all arm)

## Description

PKIN (Packin) records group LVLI / CONT / STAT content via CNAM into a reusable bundle (scene building block — e.g. "generic workbench loot"). Routed through `parse_modl_group` which reads only MODL, but PKIN records **don't carry MODL**. The parser silently produces zero meaningful output for 872 vanilla records.

No `records/pkin.rs` file exists. `EsmIndex` has no `packins` or equivalent field. `parse_modl_group` at `cell.rs:1384-1475` only pulls EDID / MODL / VMAD / LIGH DATA / ADDN DATA / ADDN DNAM — none of PKIN's defining sub-records.

## Impact

PKIN references in CELL `XPCN` (pack-in child) sub-records never resolve to a bundle content list, so spawning a PKIN-backed container produces an empty world. FO4 / Starfield workshop content relies on PKIN for modular room furniture. Vanilla Fallout4.esm has 872 of these; all silently empty.

## Suggested Fix

Add `records/pkin.rs` with:
```rust
pub struct PkinRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub contents: Vec<PkinEntry>,  // CNAM → u32 FormID refs to LVLI / CONT / STAT children
}
```

Add `packins: HashMap<u32, PkinRecord>` to `EsmIndex`. Resolution at cell-load time: when a REFR's base is a PKIN, enumerate `contents` and place each child at the REFR's transform (analogous to SCOL expansion in FO4-DIM4-01).

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: Check other CNAM-heavy records (LVLI, LVLN, LVLC) for similar silent-drop patterns
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Corpus test — assert vanilla Fallout4.esm yields ≥870 PkinRecord entries with non-empty `contents`.

## Related

- Prior audit H2 (AUDIT_FO4_2026-04-17) noted this pattern but no issue was filed.
- Cross-ref: FO4-DIM4-01 (SCOL expansion — same architectural pattern).
