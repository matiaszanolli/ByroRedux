# #6311: EXT-D5-2026-10-21-06: `watal.md`'s surface-sound row names a deleted field and the wrong game coverage after #6045

**Labels**: low,water,documentation,doc-rot

**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-21.md · **Severity**: LOW · **Dimension**: Water translation (WATAL) · **Location**: `docs/engine/watal.md (surface sound row)`

- **Tier Violated**: n/a (doc rot)
- **Game Affected**: documentation only
- **Location**: `docs/engine/watal.md:746`
- **Status**: NEW
- **Description**: `1dd217412` (#6045) replaced `WatrRecord::surface_sound: String` with `sound_form: u32`. The new field is remapped like XNAM and decoded on every schema (SOUN on Oblivion/FO3/FNV, SNDR on Skyrim). The row still reads "Oblivion `SNAM` → `surface_sound`", with FO3/FNV and Skyrim+ marked `n/a`. `surface_sound` no longer exists in the tree.
- **Suggested Fix**: Re-point the row to `sound_form`, mark FO3/FNV and Skyrim+ AUTHORED, and keep "(consumer pending)".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
