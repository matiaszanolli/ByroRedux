# PAR-D5-2026-09-21-02: BGSM specular_enabled = false is never read, so specular is forwarded unconditionally

Labels: high,bug,nifal,game:fo4,game:fo76

## Description
`byroredux/src/asset_provider/material/merge.rs:792-797` (`merge_bgsm_arm`) copies `specular_color` and `specular_mult` into `ImportedMaterial` for the first BGSM in the chain without checking `bgsm.specular_enabled`. The adjacent glossiness block (`:562-576`) also derives roughness from `smoothness` regardless of the same flag.

The NIF path honours the authoring intent this drops: `NiSpecularProperty` disabled zeroes both `specular_strength` and `specular_color` (`crates/nif/src/import/material/walker.rs:188-191`, #696), specifically so the glass-IOR branch cannot re-promote specular. `translate_material` then passes both fields straight through (`material_translate.rs:608-614`) with no BGSM-side gate to match the NIF-side one.

Verified unchanged at HEAD `ee6d3fb39`: `merge.rs:792-797` still reads
```rust
if !set_specular {
    material.specular_color = bgsm.specular_color;
    material.specular_strength = bgsm.specular_mult;
    set_specular = true;
    *touched = true;
}
```
with no `bgsm.specular_enabled` check anywhere in the function.

## Evidence
Probe `bgsm-fields` / `bgsm-specoff` over the vanilla material archives:

```
Fallout4 - Materials.ba2   : 467 BGSM specular_enabled=false, 466 of them with specular_mult > 0 (of 6,616)
SeventySix - Materials.ba2 : 653 / 612 (of 25,888)
FO4 by folder: paintingsgeneric 82, comicsandmagazines(+highres) 53, signage 31, buildings 27, grognak 25,
               interiors\building 20, vault 18, diamondcity 17, ...
typical values: specular_color [1,1,1], specular_mult 1.0, smoothness 1.0 (defaults left in place)
```

`bgsm_merge.rs` has no test with `specular_enabled: false`.

## Impact
Matte printed or painted surfaces (paintings, magazines, signage, architecture) render with full-strength white specular, and a smoothness-derived roughness they were authored to ignore. This is a divergent `Material` out of NIFAL translation (HIGH floor per `/audit-nifal`'s own severity ceiling for translation defects) on about 7% of vanilla FO4 BGSMs (and a comparable share of FO76's).

## Related
#696 (the NIF-side gate this should mirror), #220, #1454, #3639; overlaps `/audit-nifal` territory (`byroredux/src/asset_provider/material/merge.rs`) — checked against NIFAL's own two published reports (#4632-4637): none of those findings cover this specular_enabled gap, so this is filed fresh rather than as a duplicate. #4636 (NIFAL-D8, merge.rs `fill()` precedence doc) touches the same file at a different site.

## Suggested Fix
- Gate the specular forward (and the `set_specular` sentinel) on `bgsm.specular_enabled` across the chain.
- When disabled, zero `specular_strength` and `specular_color` exactly as `walker.rs` does, and leave roughness at the matte default rather than `1 - smoothness`.
- Add a `bgsm_merge.rs` test with `specular_enabled: false`.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D5-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.