# Investigation — Issue #490 (FO4-BGSM-1)

## Authoritative reference

`/mnt/data/src/reference/Material-Editor/MaterialLib/` (ousnius/Material-Editor C# source, cloned this session). Files:
- `BaseMaterialFile.cs` — common prefix (Deserialize at line 179) + `ReadString` helper (line 326) + `Color` struct (line 425) + `AlphaBlendMode` enum.
- `BGSM.cs` — lit material, signature `0x4D534742` = "BGSM". Deserialize at line 321.
- `BGEM.cs` — effect material, signature `0x4D454742` = "BGEM". Deserialize at line 178.

Versions observed: 1–22 (FO4 vanilla peaks at v2; Skyrim SE ships v20; FO76/Starfield ship higher).

## Format summary

**ReadString**: `u32 length` + `length` chars (null-terminator counted; reader trims trailing '\0').
**Color**: 3 × f32 (R, G, B). Serialized as packed RGB24.

**Common prefix** (Base):
- u32 magic + u32 version
- u32 tile_flags (bit 1 = TileV, bit 2 = TileU)
- u32×4 uv_offset_u/v + uv_scale_u/v
- f32 alpha
- u8 + u32 + u32 alpha_blend_mode triplet (→ enum)
- u8 alpha_test_ref, bool alpha_test
- 8 bools (z_write, z_test, ssr, wetness_ssr, decal, two_sided, decal_no_fade, non_occluder)
- bool refraction, bool refraction_falloff, f32 refraction_power
- version < 10: bool env_mapping, f32 env_map_mask_scale; else: bool depth_bias
- bool grayscale_to_palette_color
- version >= 6: u8 mask_writes

**BGSM extension** (on top of common): 4 texture strings always (diffuse, normal, smooth_spec, greyscale), version > 2 adds 5 more (glow, wrinkles, specular, lighting, flow) + version >= 17 adds distance_field_alpha. Version <= 2 keeps older layout (envmap, glow, inner_layer, wrinkles, displacement). Plus `root_material_path` (template pointer!), specular color/mult/smoothness, wetness controls, emittance, hair tint, terrain, translucency (v >= 8), PBR (v > 2), etc.

**BGEM extension**: 5 texture strings always (base, grayscale, envmap, normal, envmap_mask), v >= 11 adds specular/lighting/glow, v >= 21 adds glass overlays. Then env_mapping (v >= 10), bools (blood, effect_lighting, falloff, etc.), base color + scale, falloff angles/opacity, lighting influence, envmap min LOD, soft depth, emittance color (v >= 11), adaptive emissive (v >= 15), glowmap (v >= 16), effect PBR (v >= 20).

## Scope

5 new files in `crates/bgsm/`:

1. `Cargo.toml` — workspace-consistent `anyhow` + `byteorder` + `log`
2. `src/lib.rs` — public API: `MaterialFile` enum, `parse`, `parse_bgsm`, `parse_bgem` + module declarations
3. `src/base.rs` — common-prefix struct `BaseMaterial` + `Color` + `AlphaBlendMode`
4. `src/bgsm.rs` — `BgsmFile` struct + field parse
5. `src/bgem.rs` — `BgemFile` struct + field parse
6. `src/template.rs` — `TemplateResolver` trait + `TemplateCache` LRU + `resolve()` that follows `root_material_path` chain

+ workspace `Cargo.toml` update to register the new member. Total: 7 files touched.

## Template resolver design

`TemplateResolver` trait with one method: `fn read(&mut self, path: &str) -> Option<Vec<u8>>`. Caller implements — passes the resolver that knows how to open BGSM files (Materials.ba2 extract, or filesystem walk, or test harness HashMap). Keeps the crate dependency-free.

`TemplateCache` is an LRU `HashMap<String, Arc<MaterialFile>>` — cap 256 entries default. Key: lowercase `root_material_path`. Value: the parsed + ALREADY-RESOLVED chain (child's fields already merged onto parent), so repeated references to the same template hit cache.

Merge semantics: child overrides parent. For each field on the merged struct, the child value wins if the child authored it. Boolean/float defaults don't distinguish "not authored" from "authored to the default value" — cleanest implementation is "child wins entirely once it's parsed." The template provides fallback DEFAULTS only when the child references a template (root_material_path non-empty).

## Test plan

Unit tests inside `src/base.rs`, `src/bgsm.rs`, `src/bgem.rs`:
- Round-trip a known magic (BGSM / BGEM) and assert `parse` routes correctly.
- Common prefix with version=2 on each (FO4 default).
- BGSM v2 with 4 diffuse textures + `root_material_path` populated.
- BGEM v2 with 5 textures + `base_color` populated.
- Template resolver: 3-level chain resolves (A → B → C), cache hit on repeat.

Out of scope (tracked in #491): corpus-scale test against `Fallout4 - Materials.ba2` (6,616 BGSM + 283 BGEM).

## Scope
7 files. In scope for the pre-agreed new-crate split.
