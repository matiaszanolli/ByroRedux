# REN-D21-03: Cornell harness cannot exercise fire-refraction: mat.set has no ior field and Cornell probes carry no normal map

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2249

**Dimension**: 21 (Cornell harness coverage)
**Location**: `byroredux/src/commands/scene.rs:598-668` (`MatSetCommand` — the `field` match arm list has no `ior`/`distortion_strength` case, falls to `other => Err(...)`); `byroredux/src/cornell.rs` (no normal-map-carrying probe)
**Status**: NEW

**Description**: The new `MATERIAL_KIND_FIRE_REFRACTION` material kind (Cross-Cutting Themes: "four independent consistency gaps") has no way to be exercised in the Cornell harness at all: `mat.set` cannot reach `ior` (the field fire-refraction overloads as distortion strength — see REN-D6-01), and Cornell's probes carry no normal map, so the distortion math (`tangentWarp = N - macroN * dot(N, macroN)`) is structurally a no-op even at maximum authored strength.

**Evidence**: `scene.rs`'s `MatSetCommand` field list (metalness, roughness, alpha, glossiness, emissive_mult, specular_strength, env_map_scale, colors, material_kind) has no `ior` entry; unknown fields fall through to `other => Err(format!("unknown field \`{other}\`"))`.

**Impact**: Every consistency gap found this session in `MATERIAL_KIND_FIRE_REFRACTION` (REN-D2-01, REN-D11-02, REN-D12-02) had to be found by static code reading rather than the Cornell harness, and any future fire-refraction regression will similarly go uncaught.

**Suggested Fix**: add an `ior`/`distortion_strength` case to `MatSetCommand`, and add a normal-map-carrying probe surface to the Cornell scene so the fire-refraction distortion path has something to actually warp.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
