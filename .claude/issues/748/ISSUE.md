# #748: SF-D1-06: BSShaderCRC32 flag-name table covers ~32 of nif.xml's ~120 entries

URL: https://github.com/matiaszanolli/ByroRedux/issues/748
Labels: enhancement, nif-parser, medium, legacy-compat

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 1, SF-D1-06)
**Severity**: MEDIUM
**Status**: NEW (extension of closed **#712** — which only pinned the 11 literal nif.xml CRC32 values present at the time)

## Description

`crates/nif/src/shader_flags.rs::bs_shader_crc32` defines 32 `pub const` entries covering the high-impact render-routing subset. nif.xml `<enum name="BSShaderCRC32">` defines roughly 120 entries spanning all SLSF1 + SLSF2 bit positions. **~88 entries unmapped.**

Raw u32 values still preserve on disk (parsed into `sf1_crcs` / `sf2_crcs` Vec<u32>), but importer-side `bs_shader_crc32::contains_any` checks for unmapped flags silently miss-route.

## Evidence

```bash
$ grep -c '^    pub const' crates/nif/src/shader_flags.rs
32

$ grep -c '<option' /mnt/data/src/reference/nifxml/nif.xml | grep BSShaderCRC32
~120 entries
```

**Covered (high-impact subset)**: DECAL, DYNAMIC_DECAL, TWO_SIDED, CAST_SHADOWS, ZBUFFER_TEST/WRITE, VERTEX_COLORS/ALPHA, SKIN_TINT, ENVMAP, FACE, EMIT_ENABLED, GLOWMAP, REFRACTION, REFRACTION_FALLOFF, NOFADE, INVERTED_FADE_PATTERN, RGB_FALLOFF, EXTERNAL_EMITTANCE, MODELSPACENORMALS, TRANSFORM_CHANGED, GREYSCALE_TO_PALETTE_COLOR, HAIRTINT, PBR (and ~10 others).

**Unmapped (silently miss-route)**: LANDSCAPE, MULTIPLE_TEXTURES, FIRE_REFRACTION, EYE_ENVIRONMENT_MAPPING, CHARACTER_LIGHTING, SOFT_EFFECT, TESSELLATE, SCREENDOOR_ALPHA_FADE, LOCALMAP_HIDE_SECRET, LOD_LANDSCAPE, LOD_OBJECTS, all SLSF1 / SLSF2 bits 16-31 not in the table.

## Impact

Renderer routing decisions that rely on `contains_any` for the unmapped flags return false even when the flag is set — e.g. landscape-tinted shader paths, character-lighting passes, or fire-refraction effects on FO76 / Starfield blocks all silently fall back to the default routing.

## Suggested Fix

Generate the missing entries mechanically from nif.xml `<enum name="BSShaderCRC32">`. Each entry is `pub const NAME: u32 = 0xHHHHHHHH;` where the value is CRC32 of the canonical name string. Lock the new entries with literal-pin tests at `shader_flags.rs:355` (already mirrors the pattern for the existing 11).

## Completeness Checks

- [ ] **TESTS**: Extend the literal-pin test to cover all ~120 entries.
- [ ] **SIBLING**: Verify any importer that walks `sf1_crcs` / `sf2_crcs` doesn't have a hardcoded subset list elsewhere.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.

## Related

- Closed #712 (literal-pin CRC32 values present at time of closure)
