# Issues 2204 + 2205

## #2204: NIFAL-D6-02 — bhkBoxShape half-extents run through the position-space Z-up->Y-up map, negating one lane
- Severity: HIGH
- Labels: bug, nif-parser, high, nif
- State: OPEN
- Dimension: 6 (Collision), tier violated: single-boundary
- Games affected: ALL (Oblivion, FO3, FNV, Skyrim LE/SE)
- Location: crates/nif/src/import/collision/shape.rs:139-144 (BhkBoxShape); consumed at crates/physics/src/convert.rs:117-125
- Root cause: havok_to_engine(x,y,z) == (x,z,-y) is correct for POSITIONS but wrong for a half-extent (size), which should stay positive. Applied to dimensions negates the y-lane (mapped to z).
- Impact: every box collider in every supported game becomes a paper-thin sheet along a horizontal axis (e.g. 500x27x502 -> 500x27x0.002); the .max(1e-3) clamp downstream silently absorbs the sign error instead of catching it.
- Related: possible contributing cause for #2193 (is_grounded stays false at Oblivion interior spawn)
- Suggested fix: .abs() the mapped vector for extents; unit test dimensions [1,2,3] -> half_extents (1,3,2)*scale all positive; add debug_assert!(non-negative) in physics/convert.rs. Sibling extent/size uses of havok_to_engine (capsule radius, sphere radius, BhkMultiSphereShape) need auditing; position uses stay signed.

## #2205: NIFAL-D3-01 — LightKind / direction / cone resolved at import, then discarded — canonical LightSource has no field to receive them
- Severity: HIGH
- Labels: bug, nif-parser, renderer, high
- State: OPEN
- Dimension: 3 (Skinning/Lights), tier violated: parked-not-leak
- Games affected: Oblivion (severe, measured: 95 NiDirectionalLight blocks all become full-white omni lights over 8192-unit range); FO76 (2 spot blocks); all games structurally
- Location: canonical LightSource at crates/core/src/ecs/components/light.rs (fields: radius, color, flags, dimmer, intensity, falloff_exponent -- no kind/direction/cone); consumed by spawn_nif_lights; resolved at crates/nif/src/import/walk/mod.rs
- Renderer already supports it: GpuLight.color_type.w documents 0=point/1=spot/2=directional, crates/renderer/shaders/include/lighting.glsl:80-101,300-315 implements both -- NOT a renderer gap.
- Related: NIFAL-D3-02 (uncited 2048.0 no-attenuation fallback) -- sibling finding, not in scope here.
- Suggested fix: add kind/direction/outer_angle to canonical LightSource, populate at the spawn boundary, wire GpuLight.color_type.w from kind. Update docs/engine/nifal.md Section 2 Lights row.
