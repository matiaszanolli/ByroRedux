# #4941: REN-D6-2026-09-27-01: A specular-disabled BGSM hands metalness to the NIF keyword classifier — 33 vanilla FO4 shapes (Sanctuary houses included) now render as zero-specular conductors

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4941
- **Labels**: high,nifal,renderer,bug,game:fo4

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D6-2026-09-27-01**._

- **Severity**: HIGH. This is the floor for a wrong `Material` out of the NIFAL boundary. It is also a visible regression introduced by `853b2180e` on content that was a dielectric the day before.
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/asset_provider/material/merge.rs:651-681` (`merge_bgsm_arm`, the `if leaf.specular_enabled {` block and the unconditional `material.bgsm_pbr_scalars_authored = true;`). The keyword arm it falls back to is `crates/core/src/ecs/components/material.rs:1191` (`classify_pbr_keyword`, the path-only metal arm).
- **Status**: NEW. It is residual of CLOSED #4836 and #4654: the #4836 fix introduced it.
- **Description**: #4836 made the BGSM leave `metalness_override` untouched when `specular_enabled == false`, on the premise that "left untouched, all three keep whatever the NIF side classified … which zeroes the specular colour before `classify_legacy_pbr` runs and so classifies the surface as a dielectric." That premise holds only for the non-keyword arms. `classify_pbr_keyword`'s first arm returns `{metalness: 0.9, roughness: 0.55}` for any path containing `metal`/`steel`/`chainmail`/`iron`, and its second arm returns `{0.95, 0.25}` for `gold`/`silver`/`bronze`/`copper`, before specular is consulted. An FO4 NIF that inlines its diffuse in `BSShaderTextureSet` (15 of a 60-NIF DiamondCity sample; 17,980 of 34,995 vanilla NIFs carry both a `.bgsm` name and inline `.dds` strings) is keyword-classified at import. The spec-off merge then:
  - keeps that 0.9;
  - zeroes `specular_color` and `specular_strength` (#4654);
  - sets `bgsm_pbr_scalars_authored = true` anyway.

  The flag's own doc (`crates/nif/src/import/types.rs:722`) and the adjacent comment ("set at the exact site that merges them") say it means scalars were merged. It therefore also disables the overlay re-classification in `translate_material` and the normal-alpha heuristic. Shading then does `diffuseBrdf * (1 - metalness)` with the specular term × 0 (`lighting.glsl` `shadowableLightRadiance`), which leaves 10% of the diffuse and no highlight.
- **Evidence** (vanilla FO4, `material_dump` NIF-side values, which the spec-off merge now leaves final):
  - `Res01Modern01`–`06`, `Res01PlayerHouse`, `Res01PlayerHouseRoof`, `Res01PlayerHouseInterior` (`Intewall:11`), `Res01ModernCarport01`, `Res01ModernDormer01`: tex `trimmetalresidential01_d.dds`, metO 0.90 / rghO 0.55. Their BGSM `trimmetalresidential01.bgsm` has `spec_enabled=false`.
  - `Gate_ParsonsAsylum:1`, `FenceWIBollardPost01`, `FenceWIBollardStr01`, `CastleWallOutWedge02`: tex `wroughtiron01_d.dds`, metO 0.90. `wroughtiron01.bgsm` has `spec_enabled=false`, smoothness 1.0, so roughness takes the #3639 neutral 0.5.
  - DiamondCity `dextmetalrailings01` / `dextmetaldetails02`, `metalpanelslong01` (garage shell), gravel walls, and 4 SCOL precombines.
  - Total: 33 shapes in 26 NIFs.
  - Of the 467 spec-off FO4 BGSMs, 14 have a diffuse path that hits a metal/gold arm. That includes substring collisions such as `awesometales4_d` (`…someTALes` contains `metal`) and `comicbackgold_d`, but those magazine meshes were not found inlining the path.
  - Before `853b2180e`, the same shapes took `bgsm_metalness(specular_color, false)`, which is about 0 for white or near-white spec (a dielectric).
  - The #4836 fixture `merge_and_translate` seeds `NIF_METALNESS = 0.2` (`tests/bgsm_merge.rs:1473`), so no test reaches the keyword-metal input.
- **Impact**: Visibly dark, specular-less metal trim on every pre-war Sanctuary and Concord residential building and on wrought-iron fences and gates. The legacy sibling (a disabled `NiSpecularProperty` with a metal-keyword texture on Oblivion/FO3/FNV) has the same shape through walker.rs #696, but its population was not measured.
- **Related**: #4836, #4654, #696, #2609 (flag meaning), `classify_pbr_keyword` substring collisions (NIFAL scope).
- **Suggested Fix**:
  - When the leaf authors `specular_enabled = false`, resolve metalness to the dielectric the disabled block implies (`Some(0.0)`: FO4's spec-gloss model expresses a conductor only through specular), rather than deferring to a keyword guess.
  - Keep `bgsm_pbr_scalars_authored` truthful for that case (set it only where scalars are actually written, or document the new meaning).
  - Add a fixture with `NIF_METALNESS = 0.9` (keyword-metal) and a spec-off leaf.


### MEDIUM

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the BGSM merge (`byroredux/src/asset_provider/material/merge.rs`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
