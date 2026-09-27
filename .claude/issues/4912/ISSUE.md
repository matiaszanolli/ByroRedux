# EXT-D6-2026-09-27-02: `.btr` distant terrain samples its diffuse and normal with WRAP; the NIFs author CLAMP_S_CLAMP_T (#4553 did not reach the texture-only LOD families)

**Issue**: #4912
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: medium,terrain-exterior,bug,game:skyrim,game:fo4

**Severity**: MEDIUM (visual)
**Dimension**: Distant LOD and trees
**Tier Violated**: single-boundary
**Game Affected**: Skyrim, FO4 (and FO3/FNV synthesized quads)
**Status**: NEW (unfixed sibling of closed #4553)
**Location**:
- `byroredux/src/cell_loader/terrain_lod_btr.rs:335,342,360` (`resolve_texture` / `resolve_linear_texture` at WRAP).
- `:456` (the boundary call carries the MSN bit but not the clamp).
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
Every imported land shape authors `texture_clamp_mode = 0`: 5 Skyrim quads and 3 FO4 quads. Only the water plate authors 3. The spawner already reads the imported meshes for the MSN bit but ignores the clamp.

## Impact
Filtering bleeds the opposite edge in at quad borders, 2^mip texels wide. That gives seam lines in both colour and normals.

## Suggested Fix
- Carry the clamp into the `.btr` `Material` the way MSN is carried.
- Resolve with clamp; this needs a linear-with-clamp variant for the normal map.
- Extend #4553's source pins to `terrain_lod_btr.rs` and `terrain_lod.rs`.

## Related
#4553, #2571, #4632, EXT-D6-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
