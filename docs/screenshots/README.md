# Screenshots

Hero shots for the top-level [README](../../README.md) and engine docs.
All captures are from unmodified Bethesda game data loaded directly
through the Redux asset pipeline — no re-exports, no intermediate
tools, no mod manager.

## Current captures

| File | Game | Cell | Notes |
|---|---|---|---|
| [`whiterun-bannered-mare.png`](whiterun-bannered-mare.png) | The Elder Scrolls V: Skyrim SE | `WhiterunBanneredMare` | 6,228 entities, 460 meshes, 352 textures; RT multi-light, captured 2026-11-03 after the lighting overhaul |
| [`prospector-saloon.png`](prospector-saloon.png) | Fallout: New Vegas | `GSProspectorSaloonInterior` | 2,278 entities, 382 meshes, 345 textures; cell XCLL lighting, RT multi-light, captured 2026-11-03 after the lighting overhaul |
| [`prospector-saloon-exterior.png`](prospector-saloon-exterior.png) | Fallout: New Vegas | Goodsprings — outside Prospector Saloon | Exterior scene with procedural clouds |
| [`oblivion-cyrodiil-ruins.png`](oblivion-cyrodiil-ruins.png) | The Elder Scrolls IV: Oblivion | Cyrodiil exterior (location not recorded) | SpeedTree canopy, ground-cover grass, ruins in the distance |
| [`anvil-oaken-halls.png`](anvil-oaken-halls.png) | The Elder Scrolls IV: Oblivion | `AnvilHeinrichOakenHallsHouse` | 1,193 entities, 281 meshes, 251 textures; cell XCLL + per-mesh NiLight torches, RT multi-light, captured 2026-11-03 after the lighting overhaul |

`anvil-oaken-halls.png` is the hero shot for the Oblivion tier going
green.

## How to reproduce

```bash
# Skyrim SE — the Bannered Mare in Whiterun (the `skyrim_se` profile supplies
# Skyrim.esm and the vanilla mesh / texture / script / sound / voice BSAs)
cargo run --release -- --game skyrim_se --cell WhiterunBanneredMare

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
