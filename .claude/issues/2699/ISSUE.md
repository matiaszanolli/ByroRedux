# #2699: FO4-D1-02: The new "interactive XPRI retain" gate restores gameplay identity but not visual de-dup, contradicting the documented XPRI contract

- **Severity**: MEDIUM
- **Dimension**: 1 — precombines / cell load
- **Location**: `byroredux/src/cell_loader/references/mod.rs:121-133`, `:420-434`
- **Status**: NEW (introduced `fd3f7080`, 2026-08-10 — after the 2026-08-07 audit)
- **Description**: `fd3f7080` narrowed the absorption skip to `STAT | SCOL` via `precombine_can_replace_record`; any other base record type (`FURN`, `CONT`, `ACTI`, `TERM`, `MSTT`, `DOOR`, …) listed in XPRI now falls through and is spawned **in full, geometry included**. No mechanism suppresses just the 3D. The XPRI contract documented one file over (`crates/plugin/src/esm/cell/walkers.rs:298-305`: *"their geometry is already baked into the `_oc.nif` files"*) and again at `byroredux/src/cell_loader/precombined.rs:6-9` directly contradicts the new comment's premise that dropping the REFR *"drops both their visuals and their gameplay identity"*. Both cannot be true.
- **Evidence**: the retain branch increments `job.absorbed_interactive_retained` and falls through to the ordinary spawn path — no `RenderLayer` change, no mesh-free ghost spawn, and no cross-check against what `spawn_precombined_meshes` actually emitted for the cell.
- **Impact**: Switchboard alone is cited in the new comment as carrying 141 such REFRs. If their geometry is in the bake — which XPRI asserts — that is 141 duplicated meshes in one cell, z-fighting and doubled BLAS entries on precisely the interactive props the player inspects closest. If it is *not* in the bake, the walker and loader doc comments are wrong and actively mislead future work on this path.
- **Related**: #1188, #2593, FO4-D1-01.
- **Suggested Fix**: Settle the contract from measured data (compare a Switchboard XPRI `FURN`/`MSTT` against the decoded CSG object set), then either retain the entity but strip the renderable — Bethesda's own "hide 3D, keep ref" model — or correct the comments if the bake genuinely excludes non-STAT records. Either way the two comments must stop contradicting each other.

---
**Source**: `docs/audits/AUDIT_FO4_2026-08-12.md` (finding `FO4-D1-02`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

