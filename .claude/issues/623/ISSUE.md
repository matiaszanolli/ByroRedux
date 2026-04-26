# SK-D3-LOW: BSLightingShaderProperty hardening — FO76 type 12 EyeEnvmap unverified payload + vec4-share enum invariant

## Finding: SK-D3-LOW (bundle of SK-D3-06 + SK-D3-07)

- **Severity**: LOW (both items)
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`

## SK-D3-06: FO76 type 12 EyeEnvmap claimed no trailing payload — unverified vs nif.xml

**Location**: [crates/nif/src/blocks/shader.rs:1041](crates/nif/src/blocks/shader.rs#L1041)

`parse_shader_type_data_fo76` catch-all returns `ShaderTypeData::None` for FO76 type 12 with the comment `12 Eye Envmap … no trailing`. The legacy/FO4 parser at shader.rs:903-921 reads 28 bytes (cubemap scale + two reflection centers) for type **16** Eye Envmap. Per the No-Guessing policy, this needs verification against `/mnt/data/src/reference/nifxml/nif.xml` `BSShaderType155`.

If FO76 carries the same trailing payload, we under-read 28 bytes per FO76 eye mesh and stream drifts into the next block's prefix.

**Fix**: read nif.xml `BSShaderType155` definition. If FO76 eye envmap has a trailing payload, add the read at shader.rs:1041; otherwise, leave a comment with the nif.xml reference confirming the omission is correct.

## SK-D3-07: multi_layer_envmap_strength shares vec4 with hairTint{R,G,B} — no enforcement

**Location**: [crates/renderer/src/vulkan/scene_buffer.rs:252-260](crates/renderer/src/vulkan/scene_buffer.rs#L252-L260)

The packing layout claims "the two variants never overlap" — holds because `ShaderTypeData` is a single-tag enum. No `debug_assert!` enforces it. A future bitflag refactor (or a malformed input) that lets a mesh carry both Type 6 and Type 11 fields would silently render one as the other.

**Fix**: in `apply_shader_type_data` at [crates/nif/src/import/material.rs:1062-1106](crates/nif/src/import/material.rs#L1062):

```rust
debug_assert!(
    f.hair_tint_color.is_none() || f.multi_layer_envmap_strength.is_none(),
    "vec4 share between HairTint and MultiLayer must remain mutually exclusive"
);
```

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit other vec4-share layouts in scene_buffer.rs for the same enum-tag dependency.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: For SK-D3-06, after verifying nif.xml, add a roundtrip test on a synthetic FO76 type-12 block. For SK-D3-07, no test needed — debug_assert is the test.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
