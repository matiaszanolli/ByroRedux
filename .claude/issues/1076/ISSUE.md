# #1076 — FO4-D6-002: Forward BGSM specular_texture / lighting_texture / flow_texture / wrinkles_texture to ImportedMesh

Labels: enhancement, import-pipeline, medium
State: OPEN

## Description

`merge_bgsm_into_mesh` in `byroredux/src/asset_provider.rs:640–723` forwards 6 texture slots from a BGSM file to `ImportedMesh`, but omits 4 slots that FO4 content uses:

| BGSM field | FO4 use case | Forwarded? |
|---|---|---|
| `specular_texture` | Standalone PBR specular (separate from smooth_spec) | ✗ |
| `lighting_texture` | Pre-integrated lighting LUT | ✗ |
| `flow_texture` | Animated water/fluid surface direction | ✗ |
| `wrinkles_texture` | NPC age wrinkle blending | ✗ |

These are version-gated fields (appear in BGSM v>2) and affect visible rendering: FO4 animated water surfaces use `flow_texture`, NPC skin uses `wrinkles_texture`, and `specular_texture` provides per-texel specular distinct from the `smooth_spec_texture`.

## Location

- `byroredux/src/asset_provider.rs:640–723` — `merge_bgsm_into_mesh`
- `crates/nif/src/import/mod.rs` — `ImportedMesh` struct (needs 4 new `Option<FixedString>` fields)

## Suggested Fix

1. Add fields to `ImportedMesh`: `specular_map`, `lighting_map`, `flow_map`, `wrinkle_map`
2. Extend `merge_bgsm_into_mesh` to copy these from `BgsmFile` when non-empty
3. Pass through to `Material` / `GpuMaterial` if/when renderer-side consumers exist

## Source

Audit: `docs/audits/AUDIT_FO4_2026-05-15.md` § FO4-D6-002 (MEDIUM)  
See also: FO4-D6-003 (PBR flags), FO4-D6-004 (smooth_spec roughness path)

## Completeness Checks

- [ ] **SIBLING**: Check BGEM (`bgem.rs`) — does it have analogous texture slots not forwarded?
- [ ] **TESTS**: Add unit test that loads a BGSM fixture with non-empty specular_texture and asserts it appears on ImportedMesh
