# EXT-D2-2026-10-27-01: Distance softening's shape is two bare literals, and the setting's doc states the wrong onset

Labels: low, terrain-exterior, shaders, tech-debt, test-gap, bug
Source: docs/audits/AUDIT_EXTERIOR_2026-10-27.md

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-27.md` finding `EXT-D2-2026-10-27-01` (Dimension 2, terrain) · Severity LOW · Status NEW (`e71451d08`) · Tier violated: no-fabrication (engine-choice constants unnamed; one documented incorrectly).

**Location (current)**: `byroredux/src/app_frame.rs:86-90` (`ring_edge * (1.0 - 0.35 * amount)`); `crates/renderer/shaders/triangle.frag:399-406` (`smoothstep(0.5 * terrainTile.lodSoftenEnd, terrainTile.lodSoftenEnd, …)`); `crates/core/src/settings/builtin.rs:53-55` (`TERRAIN_LOD_SOFTEN_SETTING_ID` doc).

## Description
Distance softening's shape is two bare literals, and the setting's doc states the wrong onset.

- The setting's doc says "1 = soften from about a fifth of the ring out". At amount 1 the full-softening distance is 0.65 × the ring edge, and the onset (half of it) is 0.325 × the ring edge, so about a third, not a fifth.
- `TERRAIN_LOD_SOFTEN_MIPS` is a named generated constant. The 0.5 onset ratio in GLSL and the 0.35 host scale are not named.
- No test covers `push_terrain_lod_soften`'s setting-to-distance mapping: `lod_soften` is pinned only by the offset-220 layout assert.

## Evidence
```rust
// app_frame.rs
let ring_edge = (state.radius_load as f32 + 0.5) * EXTERIOR_CELL_UNITS;
ring_edge * (1.0 - 0.35 * amount)
```
```glsl
// triangle.frag
float lodSoften = smoothstep(0.5 * terrainTile.lodSoftenEnd, terrainTile.lodSoftenEnd,
                             length(cameraPos.xyz - fragWorldPos));
```
```rust
// settings/builtin.rs
/// ... 0 = off, 1 = soften from about a fifth of the ring out.
```

## Impact
A tuning knob whose documentation misleads whoever adjusts it, and two shader/host constants that can drift apart unnoticed (no test, no shared name).

## Related
#6197 (the former pad lane), `EXT-D2-2026-10-27-02` (`exal.md` silent on the feature).

## Suggested Fix
Name both values (the onset fraction in `shader_constants_data.rs` so GLSL and Rust share it, and a host constant for 0.35), correct the setting doc, and add one pure-function test of the setting-to-distance mapping.

## Completeness Checks
- [ ] **SIBLING**: Other bare literals next to the softening path in `triangle.frag` / `app_frame.rs` checked
- [ ] **SHADER-SYNC**: If a constant moves into `shader_constants_data.rs`, the GLSL include and the regenerated `.spv` stay in lockstep (plain `-V`, not `-g0`)
- [ ] **TESTS**: A regression test pins the setting-to-distance mapping

