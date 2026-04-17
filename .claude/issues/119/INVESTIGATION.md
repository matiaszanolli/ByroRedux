# #119 / NIF-302 — Shader map loop misses `has_texture_transform`

## Root cause

`NiTexturingProperty::parse` (properties.rs:311-332) iterates the shader-map trailer and, for each entry with `has_map = 1`, reads:

```
source_ref (i32)
flags (u16)               // v >= 20.1.0.3
— or —
clamp/filter/uv (3× u32)  // v < 20.1.0.3
map_id (u32)
```

Per nif.xml `<struct name="ShaderTexDesc">` each entry is a full `TexDesc` body plus a trailing `Map ID`, and `TexDesc` includes `Has Texture Transform` (bool) + optional 32-byte transform body `since="10.1.0.0"`:

```xml
<field name="Has Texture Transform" type="bool" since="10.1.0.0" />
<field name="Translation" type="TexCoord"     cond="Has Texture Transform" since="10.1.0.0"/>
<field name="Scale"       type="TexCoord"     cond="Has Texture Transform" since="10.1.0.0"/>
<field name="Rotation"    type="float"        cond="Has Texture Transform" since="10.1.0.0"/>
<field name="Transform Method" type="uint"    cond="Has Texture Transform" since="10.1.0.0"/>
<field name="Center"      type="TexCoord"     cond="Has Texture Transform" since="10.1.0.0"/>
```

The existing `read_tex_desc` helper (line 348+) already handles this gate correctly for the regular texture slots (base, dark, detail, gloss, glow, bump, normal, parallax). The shader-map trailer was a custom inline path that inherited only the prefix — dropping 1 byte per entry (bool only) or 33 bytes per entry (bool + full body) on any file with shader maps and version >= 10.1.0.0.

## Games affected

- **Oblivion** (v20.0.0.5): uses the `clamp/filter/uv` branch, `has_texture_transform` is expected. Every NIF with shader maps and a set `has_texture_transform=1` would desync by 33 B/entry.
- **FO3 / FNV** (v20.2.0.7, bsver 21/34): use the `flags` u16 branch; same `has_texture_transform` gate applies.
- **Skyrim LE/SE** (v20.2.0.7, bsver 83/100): same.
- **Post-Skyrim** (FO4+): BSLightingShaderProperty replaced NiTexturingProperty, so this code path is rare but not unused.

## Fix

`properties.rs:324-332` — insert the `has_texture_transform` read between the flags/clamp block and the trailing `map_id`, using the same version gate (`>= 10.1.0.0`) and the existing `Self::read_tex_transform(stream)` helper that the regular-slot path already uses.

Kept as an inline insertion rather than a full refactor to `read_tex_desc_body`: the shader-map entry has slightly different trailing semantics (extra `map_id`) and leading state (the `has_map` bool is already consumed by the outer loop before we enter the body), so factoring out a shared helper would save three lines at the cost of a more awkward API. Same logic as `read_tex_desc` for the gate itself.

## Sibling check

`read_tex_desc` (properties.rs:348+) independently implements the same `>= 10.1.0.0` gate, correctly, for the standard texture slots. No other code path reads partial TexDesc bodies.

## Regression tests

Two new tests in `blocks::properties::tests`:

- `parse_ni_texturing_property_shader_map_consumes_has_transform_bool` — `has_transform = 0` case. Verifies the single gating bool is consumed between `flags` and `map_id`.
- `parse_ni_texturing_property_shader_map_consumes_full_transform` — `has_transform = 1` case with the 32-byte TexTransform body. Verifies the full body is consumed.

Each fixture is a minimal NiTexturingProperty at v20.2.0.7 with `texture_count = 0` (so only the mandatory base_texture `has=0` byte is read) and one shader map entry carrying the transform bool / body. Stream-consumption assertion catches any future regression that drops or doubles the fields.
