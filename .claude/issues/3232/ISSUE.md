# 3232: NIFAL-D3: spawn_nif_lights rotates a placed NIF-light's position but never its direction

**Severity**: HIGH · **Dimension**: NIFAL Skinning/Lights · **Report**: `docs/audits/AUDIT_NIFAL_2026-08-23.md` (NIFAL-D3-2026-08-23-01)

## Description

`spawn_nif_lights` (`byroredux/src/cell_loader/spawn.rs:878-940`) is the single shared boundary for direct-embedded NIF lights (both the cell-loader and loose-NIF paths call it). It correctly folds the REFR's placement rotation (`ref_rot`) into the spawned light's **position**:

```rust
// spawn.rs:909
let final_pos = GlobalTransform::compose_translation(ref_pos, ref_rot, ref_scale, nif_pos);
```

but never applies the same `ref_rot` to `light.direction` before handing it to `LightSource::from_legacy_world_units` (line 936), and the spawned entity's own rotation is hardcoded `Quat::IDENTITY` (line 925) — nothing downstream gets a second chance to apply it.

The sibling ESM-LIGH boundary (`byroredux/src/systems/light_anim.rs::translate_light`, from the same #2439 fix cycle) already gets this right for its own producer:

```rust
let direction = (ref_rot * Vec3::new(1.0, 0.0, 0.0)).to_array();
```

The established pattern simply was never applied to this older, structurally distinct boundary when kind/direction/outer_angle were wired through by #2205.

## Evidence

```rust
// byroredux/src/cell_loader/spawn.rs:904-940 (spawn_nif_lights)
let nif_pos = Vec3::new(light.translation[0], light.translation[1], light.translation[2]);
let final_pos = GlobalTransform::compose_translation(ref_pos, ref_rot, ref_scale, nif_pos); // ref_rot applied to POSITION
...
world.insert(entity, GlobalTransform::new(final_pos, Quat::IDENTITY, 1.0)); // entity rotation is IDENTITY
world.insert(entity, LightSource::from_legacy_world_units(
    radius, light.color,
    byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL, 0.0,
    light.kind,
    light.direction,      // <-- raw NIF-local direction, ref_rot NEVER applied
    light.outer_angle,
    byroredux_core::ecs::LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL,
));
```

Consumer that trusts the value verbatim (`byroredux/src/render/lights.rs:102-106`):
```rust
direction_angle: [emitter.direction[0], emitter.direction[1], emitter.direction[2], outer_angle_cos],
```

## Impact

Any placed REFR with non-identity rotation whose own mesh authors a `NiSpotLight`/`NiDirectionalLight` renders that light aimed in NIF-local space instead of the correct world direction — pointed into a wall, away from its intended surface, or only coincidentally correct. Real content carrier: the #2205 investigation found 95 `NiDirectionalLight` blocks in Oblivion's `Meshes.bsa` alone (vines, statues, hair/ear kits — routinely placed at arbitrary rotations). Ambient/point lights unaffected (no direction). NPCs and their equipped lights unaffected (they route through the correct `translate_light` boundary). No render-time fallback masks this — the canonical `LightSource.emitter.direction` has no error signal, so a wrong value is indistinguishable from a correct one downstream.

## Suggested Fix

```rust
let world_direction = (ref_rot * Vec3::from_array(light.direction)).to_array();
```
in `spawn_nif_lights`, mirroring `translate_light`'s pattern, and pass `world_direction` in place of `light.direction`. `kind`/`outer_angle` need no correction (kind is rotation-invariant; the half-angle is a scalar).

## Completeness Checks
- [ ] **SIBLING**: Verify the fix's rotation composition matches `translate_light`'s exactly (both should compose `ref_rot` the same way)
- [ ] **TESTS**: A regression test spawning a placed NIF with an embedded `NiSpotLight` at a non-identity REFR rotation and asserting the resolved `LightSource.emitter.direction` accounts for it
