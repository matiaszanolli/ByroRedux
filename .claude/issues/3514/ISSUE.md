# #3514: FO3-2026-08-27-D1-01: refraction_strength is the one write in apply_pp_lighting_property #2328 left ungated, so an inherited property can still overwrite the shape's own value

**Labels**: low, nif-parser, nif, bug, game:fo3, legacy-compat
**Audit**: `docs/audits/AUDIT_FO3_2026-08-27.md`

---

Source: `docs/audits/AUDIT_FO3_2026-08-27.md` — finding `FO3-2026-08-27-D1-01` (LOW, Dimension 1 — inline-shader material path / hardening).

## Location
`crates/nif/src/import/material/legacy_properties.rs` — the bare `info.refraction_strength = shader.refraction_strength;` write inside `apply_pp_lighting_property` (~L452)

## Description
`apply_legacy_property_chain` walks the shape's own properties first, then the inherited parent-NiNode ones —

```rust
for prop_ref in direct_properties.iter().chain(inherited_props.iter()) {
```

— and the documented precedence (#208) is that the shape wins. #2328 implemented that for this function by converting each scalar write to a `_consumed` gate — `texture_clamp_mode` and `env_map_scale` in both the PPLighting and NoLighting arms, and again in `apply_misc_shader_properties` / `apply_base_only_shader_property`. One write in the same function was not converted:

```rust
        info.refraction_strength = shader.refraction_strength;
```

A bare assignment inside a loop over `direct_properties.chain(inherited_props)` is last-writer-wins, i.e. inherited-parent-wins — the exact inversion #2328 fixed for its siblings, two statements above.

## Evidence
Contrast with the immediately preceding writes in the same function:

```rust
        if !info.texture_clamp_mode_consumed {
            info.texture_clamp_mode = shader.texture_clamp_mode as u8;
            info.texture_clamp_mode_consumed = true;
        }
        ...
        if !info.env_map_scale_consumed {
```

and with the sibling first-wins policies for `texture_path` (`is_none()`) and `no_lighting_falloff` (`get_or_insert`).

## Impact
**Not reachable on vanilla FO3, measured.** A probe over all 17 172 FO3 NIFs across all six archives found **0** `NiNode` carrying a `BSShaderPPLightingProperty` or `BSShaderNoLightingProperty` in `av.properties` — FO3 binds its BS shader properties per-`NiTriShape` without exception, so no chain ever contains two of them and the last-writer rule never fires. (For scale: 49 619 PPLighting blocks, of which only 70 carry a non-zero `refraction_strength` at all.) This is therefore hardening + consistency, not a live defect: the exposure is modded FO3/FNV content that authors an inherited BS shader property, and the cost of divergence from its own siblings is that the next reader has to re-derive which of the two policies this function uses.

## Related
#2328 (FO3-D1-06, closed — this is its remainder); #2321 (which added the write). The `material_kind = 103` fire-refraction promotion below it is a monotone latch and is not affected the same way.

## Suggested Fix
Add a `refraction_strength_consumed` gate matching its two neighbours, or document in-place why this field is deliberately last-writer-wins.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the NoLighting arm, `apply_misc_shader_properties`, `apply_base_only_shader_property`, and the Skyrim+ `dedicated_shader.rs` writer of the same field)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix (a chain with an inherited PPLighting property must not overwrite the shape's own `refraction_strength`)
