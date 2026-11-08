# ESM-D4-03: FO76 `VOLI` (213 records) is dropped while the 123 weathers that link to it decode their `HNAM`

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6791
Labels: low, esm-plugin, bug, game:fo76
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (decoded-for-census data, nothing consumes it, PRD Q8)
- **Dimension**: 4 · **Record**: `VOLI`, `WTHR.HNAM`
- **Location**: `dispatch_misc_gameplay_a.rs:157` (`if game == GameKind::Skyrim`), `weather.rs:358` (doc), `weather.rs` `b"HNAM" if game != GameKind::Starfield && len >= 16`
- **Status**: NEW
- **Description**: the `VOLI` arm stores Skyrim only. FO76 authors `VOLI` with the same per-field layout (`FO76.pas:18855`, identical to `tes5.pas:14018`). The FO76 `WTHR` `HNAM` is 8 `VOLI` slots (`FO76.pas:17917`) and is decoded into `WeatherRecord::volumetric_lighting`, so the references dangle in the merged index.
- **Evidence**:
  - `fog_godray_census SeventySix.esm`: `parsed index: 0 VOLI, 0 GDRY; weathers with HNAM 123`.
  - Raw: 213 `VOLI`, every field sub-record 4 B. They carry 11 of the 13 tags: `LNAM` (phase contribution) is absent from all 213, so `parse_voli` would zero that field.
  - The `volumetric_lighting` doc says the 8-slot list belongs to "FO4's four Early / Late slots"; FO4 has no `HNAM` (it authors `WGDR`), and the 8-slot `HNAM` is FO76's.
- **Impact**: none at runtime today; a consumer resolving FO76 weather `HNAM` -> `VOLI` gets nothing.
- **Suggested fix**: store `VOLI` for `Skyrim | Fallout76` (Starfield stays walked), fix the doc to "Skyrim SE 4 slots, FO76 8 slots", and note `LNAM` is optional.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
