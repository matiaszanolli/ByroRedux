# NIF-D1-01: NiTransformData drops legacy Order float for XYZ rotations on pre-10.1 NIFs

URL: https://github.com/matiaszanolli/ByroRedux/issues/714
Labels: bug, nif-parser, medium

---

## Severity: MEDIUM

## Game Affected
Pre-Oblivion (Morrowind hybrid content / NetImmerse v < 10.1.0.0). Not directly in the Bethesda target band but related to #697.

## Location
- `crates/nif/src/blocks/interpolator.rs:280-336` (`NiTransformData::parse`)

## Description
nif.xml line 4333 specifies `<field name="Order" type="float" cond="Rotation Type == 4" until="10.1.0.0" />` — a 4-byte phantom float that sits **between** `Rotation Type` and the three `XYZ Rotations` `KeyGroup<float>` entries when (a) the rotation type is `XyzRotation` (=4) and (b) the file version is ≤ 10.1.0.0. The current parser jumps straight from reading `Rotation Type` to parsing the three XYZ KeyGroups (lines 291-296). On any pre-10.1 NIF that uses XYZ rotation the parser under-reads by 4 bytes and the entire post-NiTransformData stream walks 4 bytes early.

## Evidence
```rust
// interpolator.rs:288-296
let rt = KeyType::from_u32(stream.read_u32_le()?)?;
rotation_type = Some(rt);

if rt == KeyType::XyzRotation {
    // XYZ rotation: no quaternion keys, three float key groups instead
    let x_keys = KeyGroup::<FloatKey>::parse(stream)?;
    let y_keys = KeyGroup::<FloatKey>::parse(stream)?;
    let z_keys = KeyGroup::<FloatKey>::parse(stream)?;
```
vs nif.xml:
```xml
<field name="Rotation Type" type="KeyType" cond="Num Rotation Keys != 0">…</field>
<field name="Quaternion Keys" type="QuatKey" … cond="Rotation Type != 4">…</field>
<field name="Order" type="float" cond="Rotation Type == 4" until="10.1.0.0" />
<field name="XYZ Rotations" type="KeyGroup" template="float" length="3" cond="Rotation Type == 4">…</field>
```

## Impact
Pre-10.1 KF clips containing XYZ-rotation channels misalign the stream. Bethesda's vanilla content all sits at 20.x so this doesn't bite the supported corpora directly, but mod-imported KF or non-Bethesda Gamebryo sources can. The `block_sizes` table in 20.x recovers, but on Morrowind-style sourceless-streamed content there is no recovery and the cascade kills downstream blocks.

## Suggested Fix
Insert before the three `KeyGroup` reads:
```rust
if rt == KeyType::XyzRotation && stream.version() <= NifVersion(0x0A010000) {
    let _order = stream.read_f32_le()?;
}
```

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D1-01)
- Adjacent: #697 (NiTransformData partial-unknown — Oblivion-era runtime drift, different scope)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check NiKeyframeData (predecessor of NiTransformData) for same legacy field
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Byte-exact regression with a captured pre-10.1 XYZ-rotation NiTransformData fixture
