# NIFAL-D1-2026-09-21-02: LOD translate_material callers resolve base textures clamp-unaware — the canonical texture_clamp_mode they just produced is not consumed

**Labels**: low, nifal, renderer, bug

**Severity**: LOW · **Dimension**: Material (canonical carried field dropped by a consumer) · **Tier Violated**: no-leak · **Game Affected**: placement-LOD (Oblivion/FO3/FNV `_far.nif`); object-LOD (Skyrim/FO4 `.bto`)
**Location**: `byroredux/src/cell_loader/placement_lod.rs:564`; `byroredux/src/cell_loader/object_lod.rs:492` (contrast `nif_loader.rs:1241`, `mesh_instance.rs:992`)
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
Both full-detail spawn paths resolve the base texture with `resolve_texture_with_clamp(.., material.texture_clamp_mode)` (#2571/#610 contract). The two LOD callers run `translate_material` (clamp mode in scope and riding to the entity on the `Material`), then resolve with plain `resolve_texture`, which hardcodes clamp 3 (WRAP_S_WRAP_T). These paths attach no `MaterialTextureHandles` (the documented #4246 exemption), so the base texture is the *only* sampler the authored clamp could reach on these draws — and it doesn't. Same shape as #4426 (flipbook clamp bypass), on the LOD lane.

### Evidence
`placement_lod.rs:546` builds the material; `:564` resolves via `resolve_texture`. `object_lod.rs:492` same, inside the loop feeding `insert_object_lod_submesh_material` (`:417` atlas resolve is worldspace-authored, default WRAP correct there).

### Impact
Authored CLAMP on a `_far.nif`/`.bto` material is sampled WRAP — #610's edge-texel-bleed class on distant geometry. Structural divergence: four callers of one boundary, two honoring a canonical field and two not. Population unmeasured (needs a `_far.nif`/`.bto` clamp census).

### Related
#610, #2571, #4246, #4264, #4426

### Suggested Fix
Resolve with `resolve_texture_with_clamp` threading `material.texture_clamp_mode` at both sites; census the authored-clamp population first, the way the 09-16 flipbook census did.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **CANONICAL-BOUNDARY**: The fix keeps per-game logic at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
