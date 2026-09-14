# #4410 — NIFAL-D3-2026-09-14-03: nifal.md "Skinning" prose is stale — #3930 still described as an open proposal, and the cell loader said to read `mesh.skin` "exactly once"

**Labels**: low,nifal,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (doc)
- **Dimension**: Skinning/Lights
- **Tier Violated**: parked-not-leak
- **Game Affected**: Starfield (point 1); all (point 2)
- **Location**: `docs/engine/nifal.md:190-194`, `docs/engine/nifal.md:153-157`
- **Status**: NEW (drift since #3958)
- **Description**:
  1. #3930 is closed and implemented (`SkinAttach` primary), and #4270 now skips the #3549 geometric solve when `SkinAttach` covers every bone. The spec still calls #3930 an open proposal.
  2. The cell loader reads `mesh.skin` four times, two of them positive consumers (proxy bounding sphere at `byroredux/src/cell_loader/spawn.rs:194`; MorphSlot creation at `byroredux/src/cell_loader/spawn/mesh_instance.rs:1010`). The spec says "exactly once, as a negative filter".
- **Evidence**: See Location; `gh issue view 3930` shows CLOSED.
- **Impact**: A future audit would re-propose #3930, and would miss that the cell path already acts on `mesh.skin` positively — which is how NIFAL-D3-2026-09-14-02 went unnoticed.
- **Related**: #3930, #4270, #3958, #2440, NIFAL-D3-2026-09-14-02.
- **Suggested Fix**: Rewrite both passages to the live state.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
