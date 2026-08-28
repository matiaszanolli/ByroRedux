# Issue #3452: REN-2026-08-27-D6-01: FO4's Rimlight Power FLT_MAX sentinel is carried verbatim through NIFAL into GpuMaterial and clamps to the maximum rim exponent

**Filed**: 2026-08-27 via /audit-publish from `docs/audits/AUDIT_RENDERER_2026-08-27.md`

**Severity**: MEDIUM
**Dimension**: NIFAL Material
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-27.md` (REN-2026-08-27-D6-01)
**Status**: NEW

## Location
- `crates/nif/src/blocks/shader.rs:1070-1089` (the sentinel read in `parse_fo4`), `:1119` (the struct literal storing it)
- `crates/nif/src/import/material/dedicated_shader.rs:336` (`info.rimlight_power = shader.rimlight_power`)
- `byroredux/src/material_translate.rs:520` (`rimlight_power: source.rimlight_power`)
- `byroredux/src/render/static_meshes.rs:691`
- `crates/renderer/shaders/include/lighting.glsl:110-116` (`bethesdaRimFactor`)

## Description
`parse_fo4` correctly implements `nif.xml`'s conditional: `Backlight Power` is present iff `Rimlight Power >= FLT_MAX`. That makes `FLT_MAX` a *discriminator*, not an authored exponent — and `nif.xml` declares it the field's **default** (`default="#FLT_MAX#"`, `/mnt/data/src/reference/nifxml/nif.xml:6608`), so it is the common value on FO4 content that authors backlighting.

Nothing between the parser and the GPU normalises it: it flows unchanged into `ImportedMaterial.rimlight_power`, through `translate_material` into canonical `Material.rimlight_power` (`Material::sanitize`'s `fix_scalar!` only repairs non-finite values, and `FLT_MAX` is finite), into `GpuMaterial.rimlight_power`, and finally into `bethesdaRimFactor`, where `rimlightPower > 0.0` is true and `clamp(FLT_MAX, 0.25, 16.0)` yields exponent **16.0** — the tightest rim the shader can express — for a material that authored no rim power at all.

## Evidence
The parser's own comment names the value as a marker — *"nif.xml gates Backlight Power on `Rimlight Power >= FLT_MAX` … the `#FLT_MAX#` sentinel"* (`shader.rs:1076-1080`) — and then the struct literal at `shader.rs:1119` stores that same `rim` verbatim:
```rust
let back = if rim >= f32::MAX && rim.is_finite() {
    stream.read_f32_le()?
} else {
    0.0
};
(sub, rim, back)
```
The only site that overwrites it is `byroredux/src/asset_provider/material.rs:85` (`material.rimlight_power = bgsm.rim_power;`), which fires only when a BGSM/BGEM sidecar resolves.

## Impact
Visual only, and gated on `MAT_FLAG_RIM_LIGHTING` also being set, so it needs an FO4 lit material that both sets `SLSF2_Rim_Lighting` and leaves `Rimlight Power` at the backlight-marker default with no BGSM override — authoring that is inconsistent but expressible. The reason to fix it anyway is that this is a per-game wire encoding surviving past the NIFAL boundary into a canonical field, which is exactly what `docs/engine/nifal.md`'s no-fabrication rule forbids; the severity table's NIFAL floor applies.

## Related
#1901 (the `FLT_MAX` bound that made the parse correct), `docs/engine/nifal.md`, `feedback_format_translation.md`. Adjacent to the rim clamp-floor finding filed alongside this one.

## Suggested Fix
Normalise at the parser, where the sentinel's meaning is known: when the `FLT_MAX` branch is taken, store `rimlight_power` as the format's real no-value default (BGSM's own `rim_power: 2.0`, `crates/bgsm/src/bgsm.rs:159`) or `0.0`, and keep `backlight_power` as the only thing the branch communicates. A regression test on the `rim >= f32::MAX` arm asserting that no `f32::MAX` reaches `ImportedMaterial` would pin it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other sentinel-gated fields in `parse_fo4` / `parse_fo76_plus`, other per-game wire markers crossing into `ImportedMaterial`)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
