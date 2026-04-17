# OBL-D6-2: Legacy particle stack parses but has no renderer path — every torch/fire invisible

**Issue**: #401 — https://github.com/matiaszanolli/ByroRedux/issues/401
**Labels**: bug, nif-parser, renderer, high

---

## Finding

13+ legacy particle block types parse successfully in `crates/nif/src/blocks/particle.rs` and `blocks/legacy_particle.rs`:
- `NiParticleSystem`, `NiParticleSystemController`, `NiPSysEmitter` variants
- `NiPSysMeshEmitter`, `NiPSysSphereEmitter`, `NiPSysBoxEmitter`, `NiPSysCylinderEmitter`
- `NiPSysGravity`, `NiPSysRotationModifier`, `NiPSysColorModifier`
- `NiPSysUpdateCtlr`, `NiPSysEmitterCtlr`, `NiPSysModifierActiveCtlr`
- etc.

**Zero** references to `Particle` or `particle` exist anywhere under `crates/renderer/src/` or `byroredux/src/` (verified via Grep). The NIF importer in `crates/nif/src/import/walk.rs` also has no particle-emitting branch.

## Impact

Every Oblivion torch flame, fire brazier, dust motes, smoke effect, ghost trail, spell projectile, enchantment sparkle, and magic pulse renders as an invisible node. Anvil houses happen to look fine because they're mostly clutter + LIGH-driven illumination; dungeons (Fort Carmala, ayleid ruins, undead crypts), shrines, and any combat-heavy cell look "dead" — right geometry, wrong atmosphere.

This affects every target game equally — FNV, Oblivion, FO3 all use similar particle stacks; Skyrim+ partly migrated to `BSEffectShaderProperty`.

## Fix (M36-shaped, ~1-2 weeks)

1. **ECS component** (`byroredux-core`): `ParticleEmitter { kind, rate, lifetime, max_count, texture, velocity_range, gravity, color_over_life, size_over_life, … }`.
2. **ECS system**: spawn (emit at rate), integrate (velocity + gravity), expire (by age), write into a shared particle VBO per frame.
3. **Renderer path**: instanced billboard rendering — extend the existing geometry pipeline or add a dedicated particle pipeline. Optional: compute shader for simulation on GPU.
4. **NIF importer**: walk `blocks/particle.rs` output, map emitter shape + modifier stack → `ParticleEmitter` component, spawn it as a child entity of the host NiNode.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Design for Skyrim/FO4 `BSEffectShaderProperty` path reuse (partial overlap).
- [ ] **DROP**: Particle buffer lifecycle tied to ECS entity lifecycle — verify Drop on entity despawn frees GPU resources.
- [ ] **LOCK_ORDER**: Particle update system reads Transform, writes ParticleEmitter — establish lock order alongside other physics systems.
- [ ] **FFI**: N/A
- [ ] **TESTS**: Load `meshes\fire\firetorchsmall01.nif` (common torch), verify ParticleEmitter component attaches, verify billboards render with a screenshot diff test.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 6 #2.
