# FO3-6-01: Uncompressed DDS with 16/24-bpp fails parse_dds (FO3 fonts + HUD render as placeholder)

- **Issue**: #1542
- **Severity**: LOW
- **Labels**: low, import-pipeline, bug
- **Dimension**: BSA / DDS decode (import-pipeline)
- **Source audit**: docs/audits/AUDIT_FO3_2026-06-14.md (FO3-6-01)
- **Location**: `crates/renderer/src/vulkan/dds.rs:134-141`

## Description
The uncompressed-RGB branch of `parse_dds` hard-rejects any pixel format not
exactly 32-bpp (`ensure!(bpp == 32, ...)`). A sweep of all 12,261 FO3 textures
found 9 uncompressed RGB textures the parser rejects: 8 at 16-bpp + 1 at 24-bpp,
all carrying `DDPF_RGB`.

## Evidence
```rust
} else if pf_flags & DDPF_RGB != 0 {
    let bpp = pf_rgb_bit_count;
    ensure!(bpp == 32, "Unsupported uncompressed DDS: {} bpp (only 32-bit RGBA supported)", bpp);
    ...
}
```
Real FO3 files: 8 × `textures\fonts\*_lod_a.dds` glyph atlases (16-bpp, flags
0x41) + `textures\interface\hud\hud_comp_direction_vertical.dds` (24-bpp, flags 0x40).

## Impact
Graceful degradation, not a crash — `texture_registry.rs:738-748` catches the
`Err`, drops the upload, bindless slot stays on the checker placeholder. Affected
fonts/HUD compass render as magenta placeholder. Blast radius 9/12,261 = 0.07%,
UI/font only. Cross-game (`dds.rs` shared; FNV has same era textures).

## Suggested Fix
Extend the uncompressed branch to handle 24-bpp (R8G8B8 → expand to RGBA) and
16-bpp (A1R5G5B5 / A4R4G4B4) before the `ensure!`. If deferring, downgrade the
message and add a tracking comment naming the FO3/FNV font atlases.

## Completeness Checks
- [ ] SIBLING: 16-bpp and 24-bpp both handled; A1R5G5B5 vs A4R4G4B4 by channel masks
- [ ] DROP: new `vk::Format` block size handled in upload + staging path
- [ ] TESTS: regression test pins a 24-bpp and a 16-bpp DDS header
