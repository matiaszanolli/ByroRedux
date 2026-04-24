# #594: FO4-DIM2-03: DDSD_LINEARSIZE set unconditionally for non-block-compressed DDS formats

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/594
**Labels**: bug, import-pipeline, low, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 2)
**Severity**: LOW
**Location**: `crates/bsa/src/ba2.rs:514, 582-604`
**History**: Carry-forward of AUDIT_FO4_2026-04-17 L1, never previously filed.

## Description

`dwFlags` unconditionally includes `DDSD_LINEARSIZE (0x80000)`. Per DDS spec, that flag is only valid for block-compressed textures; for uncompressed formats the correct flag is `DDSD_PITCH (0x8)` with `dwPitchOrLinearSize = row pitch in bytes`.

For uncompressed DXGI formats (87=R8G8B8A8_UNORM, 91=R8G8B8A8_UNORM_SRGB, 28=R8G8B8A8_UNORM_SRGB alt, 61=R8_UNORM, 56=R16_UNORM), `linear_size_for` falls through to the `total_bytes` fallback — the emitted value is not a valid pitch.

## Evidence

```rust
// ba2.rs:514
let mut flags = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT | DDSD_LINEARSIZE;
// ba2.rs:582-604 — linear_size_for only covers BC1-BC7 (71,72,74,75,77,78,80,81,83,84,95,96,98,99)
```

## Impact

Strict DDS validators reject the file for uncompressed formats. Since the DX10 extended header is always present and the DXGI format at offset 128 disambiguates layout, Vulkan/Direct3D loaders ignore the legacy `pitchOrLinearSize` field — harmless for the engine; breaks interop with `texconv.exe`, DirectXTex, Paint.NET DDS plugin.

## Suggested Fix

Extend `linear_size_for` to compute row pitch for the uncompressed formats actually present (87, 91, 28, 61, 56). For those formats, OR `DDSD_PITCH` into flags instead of `DDSD_LINEARSIZE`, and write `width * bytes_per_pixel` into the field.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: Check `crates/renderer/src/vulkan/dds.rs` pitch path for consistency
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Unit test locking uncompressed-format pitch math.
