# NIF-LOW-BUNDLE-03: Import pipeline polish (vertex_map fallback, parallax slot, doc comments)

URL: https://github.com/matiaszanolli/ByroRedux/issues/725
Labels: enhancement, nif-parser, import-pipeline, low

---

## Severity: LOW (bundled)

## Bundled findings
3 low-severity import-pipeline polish items from AUDIT_NIF_2026-04-26.md.

### NIF-D4-04: try_reconstruct_sse_geometry vertex_map fallback aliases out-of-range indices
- **Location**: `crates/nif/src/import/mesh.rs:494-498`
- **Description**: Triangle index remap does `part.vertex_map.get(local).copied().unwrap_or(local as u16)` — when partition-local index is out of range it falls back to raw local cast to global. Confined to malformed content; vanilla Bethesda BSAs always supply complete `vertex_map`. Silent alias means truncated NIFs reach GPU with collapsed faces instead of clean abort+log.
- **Fix**: Drop the triangle (skip inner loop) when `vertex_map.get(local)` returns `None`; track per-partition drop count and `log::debug!` if non-zero. Matches `remap_bs_tri_shape_bone_indices` pattern.

### NIF-D4-06: NiTexturingProperty parallax slot 7 forwarded but scalar parameters not derived
- **Location**: `crates/nif/src/import/material/walker.rs:372-377`
- **Description**: NiTexturingProperty parallax slot populates `info.parallax_map` but `parallax_max_passes` / `parallax_height_scale` are taken only from a co-bound BSShaderPPLightingProperty. If a mesh has the legacy NiTexturingProperty parallax slot WITHOUT the PPLighting block (rare on FO3/FNV with Oblivion-style property chain), parallax texture binds with default scalars → renderer falls back to flat shading.
- **Fix**: When `NiTexturingProperty.parallax_texture` is populated and `info.parallax_max_passes` is `None`, set the pair to engine defaults (e.g., 4 passes / 0.04 scale). Defaults from Gamebryo 2.3 `NiTexturingProperty.cpp` if available.
- **Game**: FO3, FNV (edge case)

### NIF-D3-03: Lighting30ShaderProperty doc comment
- **Location**: `crates/nif/src/blocks/mod.rs:294`
- **Description**: `Lighting30ShaderProperty` is the only block in the PPLighting alias arm whose nif.xml entry actually inherits `BSShaderPPLightingProperty` (line 6367 `inherit="BSShaderPPLightingProperty"`, no extra fields), so the alias is semantically correct. NIF-D3-02 (#717) splits 4 other shader types out of the alias; without a comment, a future cleanup pass may delete this too.
- **Fix**: Add inline comment, e.g. `// Lighting30 inherits PPLighting per nif.xml line 6367`.

## Impact
None on shipped content. Cleanup / documentation hygiene to prevent regression during future refactors.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D4-04, NIF-D4-06, NIF-D3-03)
- Coordinate with: #717 (NIF-D3-02 — when splitting 4 shader types out of alias arm, leave Lighting30 with its new comment)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: N/A
- [ ] **TESTS**: D4-04 — regression with malformed vertex_map, assert triangle drop instead of alias. D4-06 — regression with NiTexturingProperty-only parallax NIF
