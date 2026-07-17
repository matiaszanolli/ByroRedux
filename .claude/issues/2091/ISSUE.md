# FO4-D5-01(residual): FO4 shader-flag alpha-test still inert when a blend-only/opaque NiAlphaProperty was consumed

- **Severity**: LOW
- **Labels**: low, nif-parser, bug
- **Location**: `crates/nif/src/import/material/walker.rs:346-351`
- **Status note**: a narrow residual edge of the closed #1985 fix (which resolved the prior MEDIUM FO4-D5-01 for the common no-`NiAlphaProperty` case), not a regression of it.

## Description
The FO4 `F4SF2::Alpha_Test` consumption block sets `info.alpha_test = true` unconditionally, but seeds the usable cutout threshold (`128.0/255.0`) only when `!info.alpha_property_consumed`. #1985 closed the common case (no `NiAlphaProperty` present at all). It does not cover the case where a `NiAlphaProperty` *was* consumed but authored no test threshold — i.e. blend-only (`flags & 0x200 == 0`) or explicit-opaque (`flags == 0`). `apply_alpha_flags` (`crates/nif/src/import/material/mod.rs:1091-1093,1107`) only writes a threshold when the property's own test bit is set, and always sets `alpha_property_consumed = true` regardless.

## Evidence
The `F4SF2::Alpha_Test` arm (`walker.rs:346-351`):
```rust
if shader.shader_flags_2 & crate::shader_flags::fo4_slsf2::ALPHA_TEST != 0 {
    info.alpha_test = true;
    if !info.alpha_property_consumed {
        info.alpha_threshold = 128.0 / 255.0;
    }
}
```
`apply_alpha_flags` (`mod.rs:1084-1108`) only writes `alpha_threshold` inside its own test-bit branch, but unconditionally sets `info.alpha_property_consumed = true` at line 1107 regardless of which branch fired. `extract_material_info_from_refs` processes `alpha_property_ref` before `shader_property_ref` (deliberate ordering, also used by an unrelated BSEffectShader gate), so any FO4 shape with a blend-only/opaque `NiAlphaProperty` plus the `ALPHA_TEST` shader flag reaches the walker.rs arm with `alpha_property_consumed` already `true`, skips the threshold seed, and ends up with `alpha_test=true, alpha_threshold=0.0`. `triangle.frag:216-217` gates the discard on `aThresh > 0.0`, so a `0.0` threshold renders the surface solid with no cutout — the same symptom class as the original #1985 finding, in a sub-case the fix didn't reach.

## Impact
A FO4 mesh that signals cutout via the shader flag while also binding a non-test `NiAlphaProperty` renders without the discard. This is contradictory authoring (cutout intent split across a shader flag and an opaque/blend alpha property) with no known vanilla-FO4 trigger — vanilla cutout content either ships a proper `NiAlphaProperty` test bit or a BGSM that supplies the threshold via `merge_bgsm_into_mesh`. Blast radius is effectively hypothetical/modded content only; hence LOW.

## Related
Prior FO4-D5-01 (#1985, closed — this is a residual, not a reopen); #1592 (flag consumption); #1201/#1202 (explicit-alpha-property-wins precedence).

## Suggested Fix
In the `F4SF2::Alpha_Test` arm, seed `128.0/255.0` whenever the resolved `alpha_threshold == 0.0` regardless of `alpha_property_consumed` (not just when no property was consumed at all). Alternatively, apply the #1201/#1202 principle and have an explicit-opaque `NiAlphaProperty { flags: 0 }` suppress the shader-flag alpha-test entirely (defer to the property's own intent). Either way, add a regression case to `fo4_shader_flag_tests` covering a blend-only/opaque `NiAlphaProperty` combined with the `Alpha_Test` shader flag.

## Completeness Checks
- [ ] **SIBLING**: Check the equivalent FO76/Starfield shader-flag arms (if any share this dispatch code) for the same guard gap
- [ ] **TESTS**: A regression test pins this specific fix (blend-only/opaque `NiAlphaProperty` + `Alpha_Test` shader flag combination)
