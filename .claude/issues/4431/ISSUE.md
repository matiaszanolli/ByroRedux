# #4431: NIFAL-D8-2026-09-16-04: `slot_to_colocated_role` still groups Starfield with Skyrim after #3900 moved Starfield to FO76's slot vocabulary

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4431
- **Labels**: low,nifal,nif-parser,game:starfield,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW (the arm is unreachable today)
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: single-boundary (one game, two slot vocabularies inside one module)
- **Game Affected**: Starfield (latent)
- **Location**: `crates/nif/src/import/material/slot_role.rs:276-286` (`(TextureSlotLayout::Skyrim | TextureSlotLayout::Starfield, 2)`); compare with `:288-328` (#3900 doc) and `:368-376`
- **Status**: NEW. The arm dates from `d5a8c36c0` (#3458, 2026-08-28). #3900 (`c5f35544c`, 2026-09-07) moved `slot_to_role` and did not update this sibling.
- **Description**: #3900's stated rule is that one canonical boundary must not hold "two rival vocabularies for one game", and it moved Starfield onto FO76's arms. `slot_to_colocated_role` was not moved: for Starfield it still returns `LightingMask` on tint-family slot 2 when soft/rim lighting is set, while FO76 returns `None`. The arm cannot fire today:
  - `apply_bs_lighting_shader` computes `soft_lighting`/`rim_lighting` only for the Skyrim layout (`crates/nif/src/import/material/dedicated_shader.rs:169-181`).
  - #3900's census found 0 Starfield `BSShaderTextureSet` blocks.

  The REFR overlay also calls this function (`byroredux/src/cell_loader/spawn/mesh_instance.rs:326-329`).
- **Evidence**: see the match arm at `:278`. The module's own test (`:645-651`) asserts only the FO4 `None`.
- **Impact**: None on current content. A future Starfield soft-lighting capture would silently take the Skyrim co-location, the exact drift #3900 was meant to close.
- **Related**: #3900, #3458, #3732, #2695
- **Suggested Fix**: Drop `Starfield` from the colocated arm (or group it with `Fallout76`), and extend the `:645` test to cover every non-Skyrim layout.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **TESTS**: A regression test pins this specific fix
