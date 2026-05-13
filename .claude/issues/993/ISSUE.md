# Issue #993

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/993
**Title**: REN-AMBIENT-DALC: Consume the SkyrimAmbientCube (DALC 6-axis ambient) — replaces the AO floor constant with per-axis sky-fill / cavity-fill
**Labels**: enhancement, renderer, medium, vulkan
**Parent**: bf40401 (AO floor interim) / fe73357 (Skyrim WTHR parser)

---

**Severity**: MEDIUM
**Domain**: renderer + WTHR-data consumer
**Source**: deferred from \`bf40401\` (\"Floor AO modulation on the ambient term\"). That commit applied a single-constant 0.3 floor on the ambient × AO product to unblock Skyrim canyon-dimness. The architecturally correct fix is to consume the per-TOD 6-axis directional ambient cube the Skyrim WTHR parser (\`fe73357\`, #539 closeout) now exposes on \`WeatherRecord::skyrim_ambient_cube\`.

## Background

Skyrim WTHR records ship a \`DALC\` sub-record set (4× 32-byte entries — sunrise / day / sunset / night). Each entry encodes a 6-axis directional ambient probe + a specular tint + a fresnel power. Layout per UESP (Skyrim mod docs):

```
bytes  0..4   = +X ambient (R G B 0)    (engine east / right)
bytes  4..8   = -X ambient (R G B 0)    (engine west / left)
bytes  8..12  = +Y ambient (R G B 0)    (engine north / forward)
bytes 12..16  = -Y ambient (R G B 0)    (engine south / back)
bytes 16..20  = +Z ambient (R G B 0)    (engine up — sky-fill)
bytes 20..24  = -Z ambient (R G B 0)    (engine down — ground/bounce)
bytes 24..28  = specular colour (R G B 0)
bytes 28..32  = fresnel power (f32)     (typically 1.0)
```

Captured into \`SkyrimAmbientCube\` (\`crates/plugin/src/esm/records/weather.rs\`) — already populated by \`parse_wthr_skyrim\` and ready to consume.

## Why this replaces the AO floor

The current temporary floor (\`bf40401\`):
```glsl
const float AMBIENT_AO_FLOOR = 0.3;
float ambientAO = max(combinedAO, AMBIENT_AO_FLOOR);
vec3 indirectLight = ambient * ambientAO + indirect * combinedAO;
```

is a hand-tuned constant that says "real-world bounce always contributes at least 30% in cavities." That's empirically reasonable but treats the surrounding hemisphere as uniform.

A 6-axis cube sample along the surface normal is the physically motivated answer:
```glsl
// Sample the cube along the world-space surface normal.
vec3 ambient_dir = dalc_sample(skyrimAmbient, N);
// `N.y > 0` weights toward +Z (sky-fill); `N.y < 0` toward -Z (ground-fill);
// lateral N weights toward the cardinal axes.
vec3 indirectLight = (ambient_dir + indirect) * combinedAO;
```

With this:
- Up-facing surfaces (e.g. canyon-floor) sample +Z (bright sky-fill) → AO-modulated against the canyon walls
- Down-facing surfaces (e.g. overhangs) sample -Z (dim ground-bounce) → AO-modulated
- Wall surfaces sample lateral axes (the cool grey side of the cube)

The AO floor is no longer needed because the down/lateral axes naturally carry the cavity-fill colour authored by Bethesda. Canyon walls receiving heavy AO from each other still get the bounce from the floor (-Z axis) which the cube authors brightly.

## Scope

**CPU side** (~50 LOC):

1. Add `dalc_per_tod: [Option<SkyrimAmbientCube>; 4]` (or `Option<[SkyrimAmbientCube; 4]>` mirroring the existing field) to `SkyParamsRes` in `byroredux/src/components.rs`.
2. `weather_system` lerps the cube between TOD slot pairs the same way it lerps the per-group colours today. The lerp acts on each of the 8 fields (6 axes + specular + fresnel) per component.
3. Push the interpolated cube into the per-frame `SkyParams` UBO. New UBO fields: 6× `vec4` axes + `vec4 specular_and_fresnel` (RGB+power packed). 28 bytes new → round to 32 with padding → fits in the existing UBO with a slot extension.

**Shader side** (~30 LOC across 3 shaders):

1. `triangle.frag` — replace the AO floor with a normal-driven cube sample. Helper function:
   ```glsl
   vec3 sampleDalcCube(SkyrimAmbientCube cube, vec3 N) {
       // 6 weighted axes — clamp(N.x, 0, 1) etc. with corresponding neg axis
       vec3 pos_weights = max(N, vec3(0.0));
       vec3 neg_weights = max(-N, vec3(0.0));
       return cube.pos_x * pos_weights.x + cube.neg_x * neg_weights.x
            + cube.pos_y * pos_weights.y + cube.neg_y * neg_weights.y
            + cube.pos_z * pos_weights.z + cube.neg_z * neg_weights.z;
   }
   ```
   Then `vec3 ambient_dir = sampleDalcCube(skyrimAmbient, N);` and the indirect-pipe consumes `ambient_dir` instead of the cell-flat `sceneFlags.yzw`.

2. `composite.frag` — the sky pass already reads zenith / horizon / lower; the down-axis (cube.neg_z) and the directional axes share a wire so the composite ground-fade can use them. Optional refinement: feed the lateral axes into the aerial-perspective fog tint so the fog colour smoothly transitions as the camera turns. Defer for v2.

3. Fall-back path for non-Skyrim cells: when `dalc_per_tod = None`, the cube degenerates to `vec3 ambient_dir = sceneFlags.yzw` (the existing flat ambient), and the AO-floor logic restores. So the FNV / FO3 / Oblivion render path is unchanged.

**Tests**:

- Round-trip test: synthetic SkyrimAmbientCube with distinctive +X/+Y/+Z values; sample at `N = (1, 0, 0)`, `(0, 1, 0)`, `(0, 0, 1)` etc.; assert the result matches the authored axis.
- Lerp test: cube_a, cube_b, t=0.5 → midpoint colours per axis.
- UBO size test pinning the new field layout.

## Acceptance criteria

- Markarth canyon renders with **directionally-correct ambient** — rock walls darker where they face other walls, brighter where they face the sky. Currently with the AO floor the canyon is uniformly mid-grey.
- Open Tamriel terrain (grid 0,0) is identical-or-better vs the AO-floor baseline. The +Z axis sample on flat ground should match the current flat `ambient` value within 5%.
- No regression on FNV / FO3 / Oblivion exterior cells (the cube field is `None` → falls back to flat ambient + AO floor).
- 30-frame bench on Whiterun exterior + Markarth (when accessible): no measurable per-frame cost (sample is 6 multiplies + 6 adds — well under 0.1 ms at 1080p).

## Related

- \`fe73357\` — Skyrim WTHR parser closeout (#539 / M33-04..07). Exposes the data this issue consumes.
- \`bf40401\` — AO floor (this issue's interim fix). To be removed when this lands.
- The renderer audit dim 15 (Sky/Weather/Exterior Lighting) — the DALC consumer is one of the explicitly-deferred items in that audit's NEW checklist.
- Cross-cuts the per-TOD shadow softness work the user is unblocking (\"fix the sky to keep fixing shadows\") — directional ambient is the floor the shadow term sits on, so correct per-direction ambient bounds the shadow contribution correctly.

