# Issue #707: FX-2: NiPSysColorModifier data discarded — particle color comes from name-heuristic preset instead of NIF

**Severity**: MEDIUM
**Files**:
- `crates/nif/src/blocks/particle.rs:174-180` (`parse_color_modifier` is a stub — reads `color_data_ref` and discards it; never resolves the linked `NiColorData` keyframe stream)
- `byroredux/src/scene.rs:1018-1038` (host-node name heuristic: `torch_flame()` / `smoke()` / `magic_sparkles()` / fallback)
- `crates/core/src/ecs/components/particle.rs` (`ParticleEmitter.start_color` / `end_color` populated from preset, not from NIF)

**Dimension**: Rendering Path / Asset Pipeline

`NiPSysColorModifier` advertises a `color_data_ref` to a `NiColorData` block carrying a 4-channel keyframe stream (RGBA-over-lifetime). Parser captures the ref, then drops it.

In `scene.rs`, `ImportedParticleEmitter` instances pick a `ParticleEmitter` preset by host-node name substring match (`torch_flame()` / `smoke()` / `magic_sparkles()` / fallback). Real Skyrim/FNV emitter NIFs author per-particle colour curves with much richer data.

**Symptom**: Dragonsreach hearth shows generic dark-grey smoke columns at the base where authored embers + colored smoke should be.

**Fix sketch**:
1. Resolve `NiPSysColorModifier.color_data_ref` → `NiColorData` during NIF import.
2. Capture the keyframe stream on a new `ImportedParticleEmitter.color_curve: Option<Vec<ColorKey>>` field.
3. In `scene.rs`, when import provides a curve, populate `ParticleEmitter` directly from it.
4. Keep the name heuristic as a fall-back.

A minimal first-pass collapses the curve to (start, end).
