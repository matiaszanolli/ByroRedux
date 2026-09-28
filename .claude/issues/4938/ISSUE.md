# #4938 — LC-D2-01: spawn_nif_lights ignores the co-located LIGH falloff sentinel — one LIGH record yields k=1.0 or k=2.0 depending on the lamp NIF

- **ID**: LC-D2-01
- **Labels**: medium,bug,legacy-compat,renderer,game:fnv,game:fo3,game:oblivion
- **Filed from**: docs/audits/AUDIT_LEGACY_COMPAT_2026-09-27.md

**Severity**: MEDIUM · **Dimension**: 2 — Legacy subsystem coverage (light translation boundary)
**Status**: NEW (sibling of closed #4514; not caught by its guard)

**Location**: `byroredux/src/cell_loader/spawn.rs` — `spawn_nif_lights` (literal `0.0` falloff passed to `LightSource::from_legacy_world_units`); guard premise in `byroredux/src/cell_loader/spawn/mesh_instance.rs` — `every_ligh_spawn_site_consumes_the_canonical_falloff_lane` doc comment ("The fourth `from_legacy_world_units` site … is a non-ESM producer")

## Description
`canonical_light_falloff_exponent` (`byroredux/src/systems/light_anim.rs`) resolves the LIGH `falloff_exponent` "field absent" sentinel (`0.0`) per layout generation: **2.0** for Oblivion/FO3/FNV (32-byte LIGH, "FO3/FNV's common quadratic k ≈ 2") and 1.0 for Skyrim+. #4514 routed the three ESM-light spawn sites through it.

`spawn_nif_lights` receives the same REFR's `light_data: Option<&esm::cell::LightData>` and uses it as the authoritative **radius** (`esm_radius`). Its falloff, however, is a hard `0.0`, which `Emitter::from_legacy_world_units`'s last-resort net turns into **1.0** (`crates/core/src/lighting.rs`). The ESM fallback in `mesh_instance.rs` only fires when `spawned_nif_lights == 0`, so the two routes are mutually exclusive by NIF content:

- FO3/FNV `LIGH` REFR whose lamp NIF has **no** spawnable `NiLight` → ESM fallback → k = **2.0**
- The same `LIGH` REFR whose NIF has a spawnable `NiLight` → `spawn_nif_lights` → radius from the same `LIGH`, k = **1.0**

The #4514 source-level guard documents this site as deliberately out of scope because it is "a non-ESM producer with no sentinel to resolve". That premise holds only on the loose-NIF path (`scene/nif_loader.rs`, `light_data = None`). On the cell path the site is ESM-backed: it already reads that record's radius.

## Evidence
```rust
// byroredux/src/cell_loader/spawn.rs — spawn_nif_lights
let esm_radius = light_data.as_ref().map(|ld| ld.radius);
...
let raw_radius = match esm_radius {
    Some(r) if r > 0.0 => r * ref_scale,   // ESM LIGH is authoritative here
    ...
LightSource::from_legacy_world_units(
    radius, light.color,
    byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
    0.0,                                   // falloff: never canonicalized
    light.kind, world_direction, light.outer_angle,
    byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
),
```
```rust
// mesh_instance.rs (#4514 guard doc)
/// The fourth `from_legacy_world_units` site (NIF-authored lights in
/// `spawn_nif_lights`) is a non-ESM producer with no sentinel to
/// resolve; `Emitter`'s own `1.0` net is its documented contract, ...
```

## Impact
This is the same visual class #4514 fixed (MEDIUM there): a too-flat falloff curve on classic-game lamps. It hits exactly the lamps whose NIF carries a live `NiPointLight`. On FO3/FNV that is the common case for meshed lights: `attenuation_radius` documents that 82/82 measured FNV NIF lights ship a zero-only attenuation triple, and radius control is deferred to the `LIGH` record. Two copies of the same `LIGH` base in one cell can render with different falloff shapes if their models differ. Skyrim+ is unaffected (its canonical default is also 1.0). Oblivion is affected on the same terms as FO3/FNV.

## Related
#4514 (closed; same lane on the ESM-fallback site); `6b4e6252c` (first two sites); `render/lights.rs` `FALLOFF_EXPONENT_DEFAULT` doc, which also lists "NIF-direct lights" under the 1.0 default.

## Suggested Fix
Pass a resolved falloff into `spawn_nif_lights`. The caller already has `game` and `light_data`. Use `canonical_light_falloff_exponent(game, ld.falloff_exponent)` when `light_data` is `Some`, and keep the `1.0` net for `None` (the loose-NIF path, which is genuinely non-ESM). Extend `every_ligh_spawn_site_consumes_the_canonical_falloff_lane` to count this site, and correct its "non-ESM producer" prose.

## Completeness Checks
- [ ] **SIBLING**: Every `LightSource::from_legacy_world_units` call site re-classified (ESM-backed vs genuinely non-ESM); the `render/lights.rs` `FALLOFF_EXPONENT_DEFAULT` doc updated to match
- [ ] **TESTS**: A regression test pins this specific fix: an FO3/FNV `LightData` with the `0.0` sentinel through `spawn_nif_lights` yields `falloff_exponent == 2.0`, and `None` still yields `1.0`
