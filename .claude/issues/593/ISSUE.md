# #593: FO4-DIM2-02: arraySize = 1 in synthesized DX10 header for cubemaps (spec says 6)

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/593
**Labels**: bug, import-pipeline, low, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 2)
**Severity**: LOW
**Location**: `crates/bsa/src/ba2.rs:572`
**History**: Carry-forward of AUDIT_FO4_2026-04-17 L3, never previously filed.

## Description

The DX10 extended header's `arraySize` field is hard-coded to `1u32` regardless of whether the texture is a cubemap. Per Microsoft's DDS_HEADER_DXT10 spec, `arraySize` for a cubemap is `6 × number_of_cubes`. DXGI loaders that honor this field (Direct3D `CreateTexture2D` with `D3D10_RESOURCE_MISC_TEXTURECUBE`) will reject the synthesized DDS as "arraySize must be a multiple of 6."

## Evidence

```rust
// ba2.rs:566-572
let misc_flag = if is_cubemap { D3D10_MISC_TEXTURECUBE } else { 0 };
hdr.extend_from_slice(&misc_flag.to_le_bytes());
hdr.extend_from_slice(&1u32.to_le_bytes()); // arraySize
```

## Impact

28 cubemaps in vanilla `Fallout4 - Textures1.ba2` (skybox, env maps, reflection probes) emit a technically-invalid DDS when extracted. In-engine rendering is unaffected (`crates/renderer/src/vulkan/dds.rs` is lenient — reads `miscFlag` to detect cubemaps, ignores `arraySize`). External tools (DirectXTex, texconv, third-party DDS viewers) reject the output.

## Suggested Fix

One-line:
```rust
let array_size: u32 = if is_cubemap { 6 } else { 1 };
hdr.extend_from_slice(&array_size.to_le_bytes());
```
Add a unit test locking the cubemap `arraySize = 6` invariant. Update `build_dds_header_is_148_bytes` to cover the cubemap branch.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: n/a (single DDS synthesis site)
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Lock cubemap arraySize=6 invariant in new unit test.
