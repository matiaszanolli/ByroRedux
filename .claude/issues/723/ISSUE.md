# NIF-LOW-BUNDLE-01: Pre-Gamebryo / pre-Bethesda NIF version-gate compat hardening

URL: https://github.com/matiaszanolli/ByroRedux/issues/723
Labels: enhancement, nif-parser, low, legacy-compat

---

## Severity: LOW (bundled)

## Game Affected
Pre-Bethesda NetImmerse / Morrowind-era NIFs. **Not in current target band.** Filed for format-abstraction policy compliance: every version branch should match nif.xml even when supported games don't bite.

## Bundled findings
This issue bundles 5 low-severity pre-Bethesda compat gaps from AUDIT_NIF_2026-04-26.md, all sharing the same character: a missing `until=` / `since=` version branch for v ≤ 10.x content that no Bethesda title ships.

### NIF-D1-03: HavokMaterial missing `Unknown Int` prefix for pre-10.0.1.2 collision shapes
- **Location**: `crates/nif/src/blocks/collision.rs:389-611` (BhkSphereShape, BhkBoxShape, BhkCapsuleShape, BhkConvexVerticesShape, all bhk*Shape)
- **Fix**: Centralize `read_havok_material()` helper on `NifStream`; gate the legacy `Unknown Int (uint, until=10.0.1.2)` prefix once per nif.xml line 2293-2299.

### NIF-D1-04: NiSkinInstance reads `Skin Partition` ref unconditionally
- **Location**: `crates/nif/src/blocks/skin.rs:36-53`
- **Fix**: nif.xml line 5079 gates `Skin Partition: Ref` on `since="10.1.0.101"`. Wrap in `if stream.version() >= NifVersion(0x0A010065) { ... } else { BlockRef::NULL }`.

### NIF-D1-05: NiTextKeyExtraData skips legacy NiExtraData prefix on pre-Gamebryo content
- **Location**: `crates/nif/src/blocks/interpolator.rs:711-734`
- **Fix**: Delegate to `NiExtraData::parse()` for the base fields (handles `Next Extra Data: Ref until=4.2.2.0` + `Num Bytes: uint since=4.0.0.0 until=4.2.2.0`), then read the text-key tail.

### NIF-D2-04: NiTextureEffect missing pre-4.1 `Unknown Short` field
- **Location**: `crates/nif/src/blocks/texture.rs:506-515`
- **Fix**: Add `if stream.version() <= NifVersion(0x0401000C) { let _ = stream.read_u16_le()?; }` per nif.xml line 5201.

### NIF-D2-05: NiStencilProperty pre-10.0.1.3 leading u16 flags branch missing
- **Location**: `crates/nif/src/blocks/properties.rs:1419-1486`
- **Fix**: nif.xml lines 5149-5158 split into 3 version regions; current parser has only 2 branches. Add leading `if stream.version() <= NifVersion(0x0A000102) { let _flags = stream.read_u16_le()?; }` before the expanded-field reads.

## Impact
None on shipped Bethesda content. Format-abstraction defense-in-depth — if we ever extend to Civ IV, Freedom Force, or other Gamebryo titles in the v ≤ 10.x band, these gaps would bite.

## Suggested Approach
File this as a single tracking issue; close-by-attrition as each finding gets addressed (or close as `wontfix` if pre-Bethesda compat is permanently out of scope).

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D1-03, NIF-D1-04, NIF-D1-05, NIF-D2-04, NIF-D2-05)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Sweep for other pre-10.x version gates that may be missing
- [ ] **TESTS**: Each fix needs a byte-exact regression with captured pre-Gamebryo fixture (or skip-test if no fixture available)
