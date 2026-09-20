# EXT-D5-2026-09-19-05: WATR MATT impact material decoded but unconsumed and absent from the watal open-items register

- **ID**: EXT-D5-2026-09-19-05
- **Labels**: low,water,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4501

**Severity**: LOW · **Dimension**: WATAL (promotion completeness / watal §2 register) · **Game Affected**: Skyrim / FO4 / FO76 (9 vanilla records)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D5-2026-09-19-05)

**Location**: `crates/plugin/src/esm/records/misc/water.rs:123-128` (field); zero consumers in `byroredux/src` / `crates/physics/src`; absent from `docs/engine/watal.md` §2/§4

**Description**
The decode census captured `material_type_form` (WATR `TNAM` → MATT impact material; Skyrim 5, FO4 2, FO76 2 records) so it would not be misread as Oblivion's text TNAM, but nothing consumes it and watal.md's open-item inventory does not list it — unlike siblings `surface_sound` ("consumer pending", §4) and `effect_form`/XNAM (§4, needs SPEL runtime). A reader of watal.md concludes the WATR decode is fully accounted for; it is not.

**Impact**
Footstep/impact surface material on water-adjacent surfaces is not reproducible from canonical state; small (9 records), gameplay-audio only.

**Suggested Fix**
Either add a watal §2/§4 row ("surface impact material — consumer pending") or consume it alongside the surface-sound consumer when water audio lands.

## Completeness Checks
- [ ] **TESTS**: If consumed, a fixture pins the MATT link resolving one hop
