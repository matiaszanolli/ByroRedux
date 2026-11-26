#!/usr/bin/env python3
"""Write a Starfield material fixture with known texel values (SMM, #7420).

The acceptance scenarios A4 to A6 of docs/prd/starfield-material-maps.md
read pixels from a surface whose authored values are known. This writes a
loose root holding a loose `.mat` at a vanilla material's own path, and the
textures it names. Nothing here comes from game data.

    python3 scripts/starfield-material-fixture.py <kind> <material> <out-root>

    <kind>      opacity | pbr | ao | ao-open
    <material>  the material path to override, as `mat.dump` prints it
    <out-root>  an empty or earlier fixture directory

Launch the Studio viewer on the mesh that names <material> with
`--loose-root <out-root>`. That flag also replaces the game's `Data`
directory as the loose tier, so installed loose files cannot leak in.

Every texture is 256x256 uncompressed RGBA8 with a full mip chain, in a 4x4
grid of 64-texel blocks. A block stays uniform down to mip 6, so a sample
near a block's centre reads the authored value at any level the test mesh
reaches. Data maps are authored `R8G8B8A8_UNORM` (the same value in red,
green and blue); the colour map is mid grey, `R8G8B8A8_UNORM_SRGB`.
"""

import json
import pathlib
import struct
import sys

SIZE = 256
GRID = 4
DXGI_RGBA8_UNORM = 28
DXGI_RGBA8_UNORM_SRGB = 29

# Texture-set slot numbers (docs/prd/starfield-material-maps.md §3.2).
SLOTS = {"color": 0, "opacity": 2, "rough": 3, "metal": 4, "ao": 5}

# Block values, as 8-bit red, by (column, row).
PATTERNS = {
    "color": lambda x, y: 186,
    # A checkerboard: half the blocks closed, half open.
    "opacity": lambda x, y: 255 if (x + y) % 2 == 0 else 0,
    # Roughness varies by column and metalness by row, so each of the 16
    # blocks has its own pair.
    "rough": lambda x, y: (64, 128, 192, 255)[x],
    "metal": lambda x, y: (0, 85, 170, 255)[y],
    # Above both occlusion floors in triangle.frag (0.2 and 0.3), so a floor
    # cannot hide the value.
    "ao": lambda x, y: 128,
    "ao-open": lambda x, y: 255,
}

KINDS = {
    "opacity": ["color", "opacity"],
    "pbr": ["color", "rough", "metal"],
    "ao": ["color", "ao"],
    "ao-open": ["color", "ao-open"],
}


def mip_chain(value_at):
    """RGBA8 levels from SIZE down to 1, box-filtered."""
    block = SIZE // GRID
    level = [[value_at(x // block, y // block) for x in range(SIZE)] for y in range(SIZE)]
    while True:
        yield bytes(c for row in level for v in row for c in (v, v, v, 255))
        edge = len(level)
        if edge == 1:
            return
        level = [
            [
                (level[2 * y][2 * x] + level[2 * y][2 * x + 1]
                 + level[2 * y + 1][2 * x] + level[2 * y + 1][2 * x + 1] + 2) // 4
                for x in range(edge // 2)
            ]
            for y in range(edge // 2)
        ]


def dds(value_at, dxgi_format):
    levels = list(mip_chain(value_at))
    # DDSD: caps, height, width, pitch, pixel format, mip count.
    flags = 0x1 | 0x2 | 0x4 | 0x8 | 0x1000 | 0x20000
    header = struct.pack("<4sIIIIIII44x", b"DDS ", 124, flags, SIZE, SIZE, SIZE * 4, 0, len(levels))
    pixel_format = struct.pack("<II4s5I", 32, 0x4, b"DX10", 0, 0, 0, 0, 0)
    caps = struct.pack("<5I", 0x1000 | 0x8 | 0x400000, 0, 0, 0, 0)
    # DX10 extension: format, 2D resource, no flags, one element.
    extension = struct.pack("<5I", dxgi_format, 3, 0, 1, 0)
    head = header + pixel_format + caps + extension
    assert len(head) == 148
    return head + b"".join(levels)


def relative(path):
    """A material path as a lowercase relative file path under the root."""
    parts = [p for p in path.replace("\\", "/").lower().split("/") if p]
    if parts[:1] == ["data"]:
        parts = parts[1:]
    if not parts or parts[0] != "materials" or not parts[-1].endswith(".mat") or ".." in parts:
        raise SystemExit(f"not a material path: {path!r}")
    return pathlib.PurePosixPath(*parts)


def write(kind, material, root):
    material_file = root / relative(material)
    components = []
    for name in KINDS[kind]:
        slot_name = "ao" if name == "ao-open" else name
        file_name = f"{kind}_{slot_name}.dds"
        texture = root / "textures" / "smm_fixture" / file_name
        texture.parent.mkdir(parents=True, exist_ok=True)
        dxgi = DXGI_RGBA8_UNORM_SRGB if name == "color" else DXGI_RGBA8_UNORM
        texture.write_bytes(dds(PATTERNS[name], dxgi))
        components.append({
            "Type": "BSMaterial::MRTextureFile",
            "Index": SLOTS[slot_name],
            "Data": {"FileName": f"Data\\Textures\\smm_fixture\\{file_name}"},
        })
    if "opacity" in KINDS[kind]:
        components.append({
            "Type": "BSMaterial::AlphaSettingsComponent",
            "Index": 0,
            "Data": {"HasOpacity": "true", "AlphaTestThreshold": "0.5",
                     "OpacitySourceLayer": "MATERIAL_LAYER_0"},
        })
    material_file.parent.mkdir(parents=True, exist_ok=True)
    material_file.write_text(json.dumps({"Version": 1, "Objects": [{"Components": components}]},
                                        indent=2) + "\n")
    return material_file


def main(argv):
    if len(argv) != 3 or argv[0] not in KINDS:
        print(__doc__, file=sys.stderr)
        return 2
    kind, material, root = argv
    print(write(kind, material, pathlib.Path(root)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
