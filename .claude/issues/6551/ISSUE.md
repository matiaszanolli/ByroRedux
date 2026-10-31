# EXT-D1-2026-10-31-01: `exal.md` still says "Oblivion and FO3/FNV only … not measured on the Skyrim+ children" after #6464 added FO4

Labels: low,terrain-exterior,documentation,doc-rot,game:fo4

Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-31.md` (finding `EXT-D1-2026-10-31-01`).

- **Severity**: LOW (doc-rot)
- **Dimension**: 1 EXAL boundary
- **Tier Violated**: n/a
- **Game Affected**: FO4 (documentation)
- **Location**: `docs/engine/exal.md:148-154` (the "Landscape (#5505)" paragraph)
- **Status**: NEW. The sentence was current at `fa6be4a6b`; `977936719` (#6464, 2026-10-29) made it wrong.
- **Description**: The paragraph says the landscape inheritance runs "once after the load order is merged and for Oblivion and FO3/FNV only (`needs_exterior_processing`; …) — not measured on the Skyrim+ children". Since `977936719`, `needs_exterior_processing` is `Oblivion | Fallout3NV | Fallout4`. NukaWorldMarket and NukaWorldAmphitheater were measured decisive, and Goodneighbor is consistent. The rustdoc on `needs_exterior_processing`, `SharedParseCache::get_or_parse` and `EsmCellIndex::inherit_parent_landscapes` was updated in the same commit. The spec was not. The spec is also the doc of record for the measurement that justifies the gate. Skyrim and Starfield are the games "not measured".
- **Evidence**:
  ```
  order is merged and for Oblivion and FO3/FNV only (`needs_exterior_processing`;
  #6089 measured the FO3 "Use Land Data" children co-located on the Wasteland's
  grid, …) — not measured on the Skyrim+ children.
  ```
- **Impact**: A reader trusting the spec concludes FO4 children do not inherit LAND and may re-open #6464, or may not know that the FO4 evidence (Nuka-World pair; Goodneighbor not visually confirmed) exists.
- **Suggested Fix**: Say "Oblivion, FO3/FNV and FO4", cite #6464 with the measured medians (|dz| 69 / 168 against 448+ / 713+ for shifted controls) and the Goodneighbor caveat, and narrow "not measured" to Skyrim and Starfield.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games / siblings)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
