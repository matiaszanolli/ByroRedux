# EXAL — Terrain height maps

Per-terrain-layer height maps feed the terrain shader's height-based layer
blending (#6195, #6196). The engine does not ship them and the games mostly do
not author them, so they are generated from each layer's textures on a
background pool and cached on disk. The generator is
[`crates/terrain-height`](../../crates/terrain-height/src/lib.rs) (pure CPU, no
archive or GPU knowledge); the job runner and cache are
[`byroredux/src/asset_provider/terrain_height.rs`](../../byroredux/src/asset_provider/terrain_height.rs).

## Source tiers

For one layer the first tier whose texture is present in the provider (last
listed archive wins, loose files on top; see [archives.md](archives.md)) and
decodable is used:

1. **Authored** — a parallax / height / displacement map, rescaled only.
2. **Normals** — the layer's normal map integrated into a height field and
   high-passed (`NORMALS_HIGHPASS_SIGMA_FRACTION`). The green-channel
   convention (Y-up / Y-down) is part of the input.
3. **Brightness** — the high-passed luminance of the diffuse.

A tier whose texture is missing or undecodable (BC6H / BC7 and other exotic
formats) falls through to the next; if none works the job returns an error that
lists why each was skipped. Output is a normalised `[0, 1]` BC4 `.dds` with a
full mip chain, at most `MAX_EDGE` (1024) texels on a side.

## Disk cache

| | |
|---|---|
| Location | `terrain-height/` next to the executable (not the working directory, so it survives a launcher changing it) |
| Override | `BYRO_CACHE_DIR` replaces the *parent*, so the cache lands in `$BYRO_CACHE_DIR/terrain-height/` (tests, read-only installs) |
| Files | `<key>.dds` + `<key>.json` per source |
| Key | 64-bit xxHash of `GENERATOR_VERSION`, the tier, the green convention (Normals only) and the **raw source bytes** |
| Invalidation | automatic: a changed texture, tier or convention hashes to a new key, and `GENERATOR_VERSION` is bumped whenever generation output changes for the same input |

The key hashes the source bytes before decoding, so a warm cache is answered
without the (up to 2048² BC → RGBA8) decode (#6314). To clear it, delete the
directory; nothing else reads it. The resident size is in
[memory-budget.md](memory-budget.md).

The blend band is a live debug-UI setting
(`TERRAIN_HEIGHT_BLEND_BAND_SETTING_ID`, applied through
`set_terrain_height_blend_band`); at `0` the height blend is off without
touching the cache.
