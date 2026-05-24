# NIF-DIM4-NEW-03: BSShaderPropertyBaseOnly subclasses (Hair/VolumetricFog/DistantLOD/DistantTree) — base-data never consumed

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1244

**Source**: `docs/audits/AUDIT_NIF_2026-05-23.md` (Dim 4)
**Severity**: LOW (orphan-parse)
**Dimension**: Import Pipeline

## Description

Four legacy shader-property types share the `BSShaderPropertyBaseOnly` struct (#717 parser shape): `HairShaderProperty`, `VolumetricFogShaderProperty`, `DistantLODShaderProperty`, `BSDistantTreeShaderProperty`. Each carries `NiObjectNETData` + `BSShaderPropertyData` base (which has `env_map_scale` and `texture_clamp_mode`). None have a consumer arm in `crates/nif/src/import/material/walker.rs`. Their `block_type_name()` is even surfaced through the `type_name: &'static str` field, but no downcast site reads them.

## Evidence

```
$ grep -rn "get_as::<BSShaderPropertyBaseOnly>" crates/nif/src/import/
(no matches)
```

Dispatch arms exist at `crates/nif/src/blocks/mod.rs:426-429`:
```rust
"HairShaderProperty" => Ok(Box::new(BSShaderPropertyBaseOnly::parse(stream, "HairShaderProperty")?)),
"VolumetricFogShaderProperty" => Ok(Box::new(BSShaderPropertyBaseOnly::parse(stream, "VolumetricFogShaderProperty")?)),
// + DistantLOD, DistantTree
```

## Impact

Same shape as #1243 (WaterShaderProperty orphan). `Hair*` surfaces on Oblivion hair NIFs may be the most visible: `env_map_scale` not feeding through means stray reflective hair never gets its authored modulator. `BSDistantTreeShaderProperty` surfaces are usually hidden by SpeedTree-billboard paths (Phase 1 placeholder), so the consumer is academic. `DistantLOD` / `VolumetricFog` are rare enough to stay LOW.

## Suggested Fix

Single shared consumer in `walker.rs`:

```rust
if let Some(shader) = scene.get_as::<BSShaderPropertyBaseOnly>(idx) {
    info.env_map_scale = shader.shader.env_map_scale;
    if info.texture_clamp_mode == 3 {
        info.texture_clamp_mode = shader.shader.texture_clamp_mode as u8;
    }
}
```

Could also be deferred — the chrome / placeholder failure modes from `feedback_chrome_means_missing_textures.md` would surface a higher-priority symptom before any of these become noticeable.

## Related

- #717 (CLOSED): parser shape — at that point the consumer side was deliberately deferred until a corpus need surfaced
- #1243 (this audit): sibling WaterShaderProperty orphan with similar shape

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: paired with #1243 — both gaps fixed in the same hygiene PR makes sense (single `walker.rs` import addition + 2 consumer arms)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic Oblivion HairShaderProperty fixture; assert `MaterialInfo::env_map_scale` non-zero after import