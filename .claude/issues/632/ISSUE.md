# FNV-D3-03: LIGH refs whose NIF carries zero-color placeholder spawn no LightSource — ESM light_data dropped

## Finding: FNV-D3-03

- **Severity**: MEDIUM
- **Source**: `docs/audits/AUDIT_FNV_2026-04-24.md`
- **Game Affected**: FNV (Prospector Saloon and any cell with light-bulb meshes carrying disabled NiPointLight placeholders), FO3, Skyrim
- **Locations**:
  - Inner filter: [byroredux/src/cell_loader.rs:1874-1879](byroredux/src/cell_loader.rs#L1874-L1879) — skips every `nif_light` whose `color[0]+color[1]+color[2] < 1e-4`
  - ESM-fallback gate: [byroredux/src/cell_loader.rs:2223-2234](byroredux/src/cell_loader.rs#L2223-L2234) — only attaches the ESM `light_data` LightSource when `nif_lights.is_empty()`

## Description

A NIF that authored a placeholder zero-color `NiPointLight` (very common in FNV light-bulb meshes — the artist parks a disabled light to mark intent without baking color) still has `nif_lights.len() == 1` even after the inner filter loop drops it as zero-color.

The fallback gate at line 2223 then sees `!nif_lights.is_empty()` and refuses to attach the ESM `light_data` LightSource. Net: the cell renders dark in that spot despite both NIF intent (the placeholder) and ESM authority (LIGH base record) agreeing it should be lit.

The Prospector Saloon "25 point lights" baseline is brittle on this edge.

## Suggested Fix

Replace the `nif_lights.is_empty()` check with a counter:

```rust
// byroredux/src/cell_loader.rs around 1874-1879 — replace continue with counter
let mut spawned_nif_lights = 0u32;
for nif_light in nif_lights.iter() {
    if nif_light.color.iter().sum::<f32>() < 1e-4 {
        continue;
    }
    // ... spawn ...
    spawned_nif_lights += 1;
}

// at line 2223 — gate on the counter instead of nif_lights.len()
if spawned_nif_lights == 0 {
    if let Some(ld) = light_data {
        attach_esm_light(world, refr_entity, ld);
    }
}
```

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check whether REFR.XESP-disabled refs propagate through the same path (#471 was about default_disabled — probably orthogonal).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic NIF with a single zero-color NiPointLight + ESM LIGH base with non-zero `color`; assert exactly one ECS LightSource spawns.

_Filed from audit `docs/audits/AUDIT_FNV_2026-04-24.md`._
