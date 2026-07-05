**Severity**: LOW · **Dimension**: Shader-Type Dispatch · **Source**: `docs/audits/AUDIT_SKYRIM_2026-07-04.md` (SKY-D2-001)
**Status**: NEW
**Location**: `crates/nif/src/shader_flags.rs::skyrim_slsf2::CLOUD_LOD`

## Description
`CLOUD_LOD` is defined `0x0020_0000` (bit 21) and documented "Bit 21 — `Cloud_LOD` on Skyrim". nif.xml `SkyrimShaderPropertyFlags2` places **Cloud_LOD at bit 20** (`0x0010_0000`) and **Anisotropic_Lighting at bit 21** (`0x0020_0000`). The constant's value is therefore Anisotropic_Lighting, and the doc-comment is wrong about which flag lives at bit 21. The sibling `fo4_slsf2::ANISOTROPIC_LIGHTING` already documents `0x0020_0000` correctly — the two modules disagree on the same numeric value.

## Evidence
```rust
/// Bit 21 — `Cloud_LOD` on Skyrim (NOT `Alpha_Decal` …).
pub const CLOUD_LOD: u32 = 0x0020_0000;      // shader_flags.rs
```
```xml
<option bit="20" name="Cloud_LOD"></option>            <!-- nif.xml SkyrimShaderPropertyFlags2 -->
<option bit="21" name="Anisotropic_Lighting">Hair only?</option>
```

## Impact
None functionally today — the constant participates in no live decode (the live decal/two-sided helpers do not test it). Risk is latent: future code reaching for "Skyrim Cloud_LOD" via this constant would read Anisotropic_Lighting. The `walker.rs` comment ("flags2 bit 21 is `Cloud_LOD` on Skyrim") inherits the same off-by-one. **Note:** the value is currently pinned by `shader_flags.rs::tests` (`assert_eq!(skyrim_slsf2::CLOUD_LOD, 0x0020_0000)` and `assert_eq!(fo3nv_f2::ALPHA_DECAL, skyrim_slsf2::CLOUD_LOD)`) — any value change must update those asserts.

## Suggested Fix
Either (a) rename the constant to `ANISOTROPIC_LIGHTING` and add a separate `CLOUD_LOD = 0x0010_0000` (bit 20) if a Skyrim Cloud_LOD constant is wanted, or (b) if Cloud_LOD is the intended semantic, set the value to `0x0010_0000`. Update the doc-comment, the `walker.rs` comment, and the two pinning asserts in `shader_flags.rs::tests`. No behavioral change to shipping code.

## Related
nif.xml `SkyrimShaderPropertyFlags2` · correct sibling `fo4_slsf2::ANISOTROPIC_LIGHTING` · #414 (modern-vs-legacy decal split)

## Completeness Checks
- [ ] **SIBLING**: Cross-check `fo4_slsf2` / `fo3nv_f2` bit-21 vocabulary stays consistent after the rename/revalue
- [ ] **TESTS**: Update the two pinning asserts in `shader_flags.rs::tests` to match the corrected value/name
