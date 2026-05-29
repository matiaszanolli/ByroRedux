# #1305 -- OBL-D6-NEW-02: Tamriel ocean never renders

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: MEDIUM | **Dim 6** — Blockers & Game-Specific Quirks
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D6-NEW-02)

**Location**: `byroredux/src/cell_loader/exterior.rs:259-278` (gate on `cell.water_height`); `crates/plugin/src/esm/cell/wrld.rs:120-123` (NAM2 stored on `record.water_form`, never propagated); `crates/plugin/src/esm/cell/mod.rs:684-715` (DNAM unparsed)

**Issue**: Every coastal/sea cell in Tamriel (Abecean Sea, Niben Bay, Lake Rumare / Imperial City moat, entire ocean shoreline) renders with no water surface — dry seabed exposed. The worldspace default water height + form (WRLD NAM2/DNAM) is not resolved as a per-cell fallback: DNAM is unparsed and NAM2 is stored on `record.water_form` but never propagated to cells whose XCWT/water_height is absent.

**Suggested fix**: parse WRLD DNAM (default land height + default water height, 2× f32) onto `WorldspaceRecord`; in `build_exterior_world_context` resolve the worldspace `water_form` → `WatrRecord` + default water height once; in `load_one_exterior_cell`, fall back to the worldspace water when the per-cell water is absent.

## Completeness Checks
- [ ] **SIBLING**: verify FO3/FNV/Skyrim worldspace-default water path follows the same pattern
- [ ] **TESTS**: integration test on a coastal Tamriel cell asserting water plane is spawned
- [ ] **CANONICAL-BOUNDARY**: cell-loader only; no material/translate impact
- [ ] **UNSAFE**: no unsafe involved
