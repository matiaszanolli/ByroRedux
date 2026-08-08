# SF-D9-2026-08-07-04: BGEM palette bits mutually exclusive losing color variant; envmap fill ignores env_mapping_enabled

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2643
**Finding ID**: SF-D9-2026-08-07-04

**Severity**: LOW
**Dimension**: 9 (BGSM/BGEM External Material Flow)
**Location**: `byroredux/src/asset_provider/material.rs:1173-1200`, `byroredux/src/cell_loader.rs:272-278`
**Status**: NEW

## Description
Two small BGEM asymmetries: (1) authoring both palette-enable bits loses
the color variant — `bgsm_greyscale_lut_is_alpha`/`_enabled` derivation
makes `PALETTE_ALPHA`/`PALETTE_COLOR` mutually exclusive; a BGEM authoring
both independent bits (the format permits it) yields `PALETTE_ALPHA` only,
dropping color — the inline NIF effect-shader path
(`pack_effect_shader_flags`) derives the two bits independently and can set
both, so the "documented mirror" paths diverge on this input. (2) envmap
texture fill ignores `env_mapping_enabled()` —
`textures.environment`/`environment_mask` fill unconditionally from
`bgem.envmap_texture`/`envmap_mask_texture` without consulting
`BgemFile::env_mapping_enabled()` (the #2358 accessor for the authoritative
v10+ subclass copy) — `bgem_uses_glass_behavior` *does* consult it, so the
same authored bit is honoured for classification and ignored for texture
binding within one file.

## Evidence
`byroredux/src/asset_provider/material.rs:1173-1200` (palette bits),
`byroredux/src/cell_loader.rs:272-278` (envmap fill without
`env_mapping_enabled()` check).

## Impact
Both narrow, neither known to fire on vanilla content.

## Suggested Fix
For (1), preserve both bits or pick a documented precedence; for (2), gate
the environment fill on `env_mapping_enabled()`.

## Related
#2358 (the `env_mapping_enabled()` accessor already used by
`bgem_uses_glass_behavior` but not here).

## Completeness Checks
- [ ] **TESTS**: A fixture with both palette bits set asserts the fix's chosen precedence; a fixture with `env_mapping_enabled()==false` asserts the envmap fill is skipped
