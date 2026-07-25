# Reference-guided texture upscaling

`byro-texture-upscale` is an offline workbench for Bethesda texture sets. It
reads loose directories, BSA archives, and BA2 archives in load order. It sends
only each set's color reference through a learned upscaler such as
Real-ESRGAN, then uses that high-resolution result as a joint-bilateral guide
for the companion maps.

This distinction is load-bearing:

- color/albedo is allowed to gain learned detail;
- normal RGB is edge-guided and renormalized as a vector;
- normal alpha (legacy gloss/specular), glow, specular, masks, height, and
  reference alpha remain derived from their authored low-resolution values;
- learned color is never copied into a data map.

The output is lossless PNG intentionally. DDS compression and mip generation
are a separate finalization step because the correct format is game- and
map-specific (for example, classic Bethesda normal maps often need BC3 to
retain gloss in alpha). Feeding every map through one BC preset would silently
destroy data.

## Workflow

Discover conservative filename-based sets:

```sh
cargo run --release -p byro-texture-upscale -- discover \
  --source "/games/Fallout New Vegas/Data/Fallout - Textures.bsa" \
  --source "/games/Fallout New Vegas/Data/Fallout - Textures2.bsa" \
  --manifest texture-sets.toml \
  --scale 4
```

Review the manifest. Discovery recognizes only unambiguous companions:
`_n` normal, `_g` glow, `_s` specular, `_m` mask, and `_p` height/parallax.
It deliberately does not guess `_d`, because that suffix is used for both
diffuse and dark maps by different content pipelines. Explicitly add unusual
sets and roles before running.

The generated external command defaults to:

```toml
[upscaler]
program = "realesrgan-ncnn-vulkan"
args = ["-i", "{input}", "-o", "{output}", "-s", "{scale}"]
```

Any ESRGAN-family wrapper works if it accepts input/output file arguments.
Edit `program` and `args`; `{input}`, `{output}`, and `{scale}` are expanded as
individual process arguments without invoking a shell.

Inspect the plan:

```sh
cargo run --release -p byro-texture-upscale -- run \
  --source "/games/Fallout New Vegas/Data/Fallout - Textures.bsa" \
  --source "/games/Fallout New Vegas/Data/Fallout - Textures2.bsa" \
  --manifest texture-sets.toml \
  --output upscale-work \
  --dry-run
```

Then run it without `--dry-run`. Existing output is protected unless
`--overwrite` is supplied. `texture-upscale-report.json` records every source,
role, original size, and generated size.

## Current format boundary

The built-in image path decodes PNG/TGA/BMP/JPEG and the BC1/BC2/BC3 DDS
formats common to Oblivion, Fallout 3/New Vegas, and classic Skyrim. Convert
BC5/BC7 sources to lossless PNG before processing for now. A DDS finalizer
should be a separate adapter with per-game/per-role compression profiles,
rather than part of the learned-upscale stage.
