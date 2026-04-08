# Screenshots

Hero shots for the top-level [README](../../README.md) and engine docs.
All captures are from unmodified Bethesda game data loaded directly
through the Redux asset pipeline — no re-exports, no intermediate
tools, no mod manager.

## Current captures

| File | Game | Cell | Notes |
|---|---|---|---|
| [`prospector-saloon.png`](prospector-saloon.png) | Fallout: New Vegas | `GSProspectorSaloonInterior` | 789 entities, 85 FPS, cell XCLL lighting |
| `anvil-oaken-halls.png` | The Elder Scrolls IV: Oblivion | `AnvilHeinrichOakenHallsHouse` | 379 entities, 376 meshes, 104 textures, 12 lights (cell XCLL + per-mesh NiLight torches), ~1600 FPS |

`anvil-oaken-halls.png` is the hero shot for the Oblivion tier going
green. Save the screenshot you captured to this path.

## How to reproduce

```bash
# Oblivion — Heinrich Oaken Halls House in Anvil
cargo run --release -- \
    --esm "$OBLIVION_DATA/Oblivion.esm" \
    --cell AnvilHeinrichOakenHallsHouse \
    --bsa "$OBLIVION_DATA/Oblivion - Meshes.bsa" \
    --textures-bsa "$OBLIVION_DATA/Oblivion - Textures - Compressed.bsa"

# FNV — Prospector Saloon in Goodsprings
cargo run --release -- \
    --esm "$FNV_DATA/FalloutNV.esm" \
    --cell GSProspectorSaloonInterior \
    --bsa "$FNV_DATA/Fallout - Meshes.bsa" \
    --textures-bsa "$FNV_DATA/Fallout - Textures.bsa" \
    --textures-bsa "$FNV_DATA/Fallout - Textures2.bsa"
```

Press **Escape** to capture the mouse, **WASD** + mouse look to fly
around, **Space/Shift** for up/down, **Ctrl** for speed boost. Take
the screenshot with your WM's tool (GNOME Screenshot, `grim` on
Wayland, etc.) and save it to this directory.

## Conventions

- **Format**: PNG, uncompressed or lightly compressed (git can handle
  a few MB per hero shot — the FNV capture is 2.7 MB).
- **Resolution**: native engine window, usually 1920×1080 or the
  default 1280×720. Don't upscale.
- **Framing**: interior shot, camera positioned so at least one light
  source is visible for lighting context, rugs / clutter / doorways in
  frame to show mesh + texture variety.
- **File name**: `<cell-or-location>.png`, lowercased and hyphenated.
