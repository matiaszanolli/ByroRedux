# EXT-D3-2026-10-18-01: The authored-model tier squares the authored GRAS density wherever a load order links textures to grass

Labels: medium,terrain-exterior,shaders,renderer,bug,game:skyrim,game:fo3,game:fnv
Source: docs/audits/AUDIT_EXTERIOR_2026-10-18.md

- **Severity**: MEDIUM. Authored rocks, ferns, kelp and grass cards grow at roughly `d²` of their authored rate on every game with `LTEX.GNAM` links (Skyrim, FO3, FNV; any `AuthoredCover::localised` load order).
- **Dimension**: Ground-cover pipeline
- **Location**:
  - `crates/renderer/shaders/groundcover_models.comp:449-455` (`field = clamp(f.affinity * f.slope * ...)`, accept test `pick.accept * field * fade`) and `:293` `gcPickLocalRecord`.
  - Affinity source: `byroredux/src/cell_loader/terrain.rs:1124-1137` (override) and `:1270-1271` (stored in `TerrainCoverInputs`), consumed via `byroredux/src/render/groundcover.rs` `EntityCell::layer_affinity` / `base_affinity`.
- **Status**: NEW (interaction of `c14dd2b39` and `592f31b87`, both 2026-10-14).
- **Tier Violated**: n/a (a double application of one authored value)
- **Game Affected**: Skyrim, FO3, FNV (and any other title whose load order carries `LTEX.GNAM`).
- **Description**:
  - `c14dd2b39` (#4906, 17:56): the model tier's accept rate is `pick.accept` = the lane-weighted union `Σ share · (1 − Π(1 − d_r))` of the authored densities of the records the ground lists. It then multiplies the §3 field, which still contains `f.affinity` (`byroGcAffinity` of the cell's affinity lanes).
  - `592f31b87` (20:34): when `AuthoredCover::localised`, each splat layer's and the BTXT base's affinity is overwritten with `union_density(grasses)` — the *same* union. The cell record is shared by both tiers, so the model tier's `f.affinity` is now that union mixed by the same ordered alpha chain.
  - Result on pure ground whose texture lists one record of density `d`: `pick.accept = d`, `f.affinity = d`, so the accept probability is `d · d · slope · shelter · clump · fade`. `exal-groundcover.md:1664-1667` promises the opposite ("a lone record keeps its authored density on pure ground").
- **Evidence**:
  - `592f31b87` touched no shader (`git show --stat`): the model tier's `byroGcDensityFactors(..., cell.affinity0, cell.affinity1, cell.baseAffinity, ...)` call is unchanged from `c14dd2b39`.
  - `byroGcAffinity` (`groundcover_density.glsl:129-152`) is `mix(a, aff_i, w_i)` over the lanes, i.e. exactly the lane-share-weighted mix of the overridden affinities.
  - The `c14dd2b39` measurement ("demanded model plants 8789 -> 1656") predates `592f31b87` by 2.6 hours, so it does not include the second factor. No test composes the accept rate: `local_selection_follows_the_ground_and_the_authored_union` (`render/groundcover.rs:1519`) checks the pick only.
  - The blade tier is not affected (it uses `f.affinity` once).
- **Disproof attempted**: is the model tier's field meant to carry the keyword heuristic as a spatial taper? Only before `592f31b87`; the commit message itself says both tiers "read one authored truth", so the multiplication is unintended. The non-localised path (all-ones masks, keyword affinity) is not squared.
- **Impact**: Every authored-model plant in a localised load order is thinned by another factor of the authored density (0.3 -> 0.09, 0.5 -> 0.25). Species mix is unaffected (the pick weights are unchanged); only the total rate is wrong. Not confirmed on screen (no capture allowed).
- **Related**: #4906 (closed), `592f31b87`, `c14dd2b39`.
- **Suggested Fix**: The model tier must not multiply the affinity the host already folded into `pick.accept`. Smallest change: evaluate the field with a neutral affinity for the model tier (pass `1.0` for `f.affinity` when `AuthoredCover::localised`, keep the keyword affinity otherwise, via a push-constant flag), or give `GroundCoverCell` a separate model-tier affinity. Pin it with a CPU mirror test: pure ground, one record of density `d` -> accept `d`.

## Completeness Checks
- [ ] **SHADER**: GLSL change recompiled with plain `glslangValidator -V` (no `-g0`); `.spv` committed; `scripts/check-shader-artifacts.sh` green; shader-struct lockstep tests still pass
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
