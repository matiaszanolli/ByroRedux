# SKY-2026-08-27-D7-01: the `ice` classifier arm is exactly inverted on Skyrim — 0 real ice surfaces reach it, 269 Imperial-fort stone walls do

Labels: high,nifal,renderer,bug,game:skyrim,legacy-compat

- **Severity**: HIGH
- **Confidence**: CONFIRMED (code read + full-archive census of vanilla Skyrim SE meshes)
- **Location**: `crates/core/src/ecs/components/material.rs:772` (classifier ice/gem arm),
  `crates/core/src/ecs/components/material.rs:716` (`is_glass_keyword_path`),
  `crates/core/src/ecs/components/material.rs:1136` (`contains_any_ci_word`)
- **Description**:
  `classify_pbr_keyword`'s glass arm matches `ice`/`gem` through
  `contains_any_ci_word` (word-boundary), not the plain substring matcher used
  for `glass`/`crystal`. #2009 introduced that boundary to stop FO3/FNV English
  collisions (`office`, `notice`, `justice`, …). Its own in-source comment
  already notes the tension — *"Bethesda's own concatenated-compound naming
  convention (`brokenglasssheet*`) relies on the mid-word match still firing"* —
  and then applies the boundary to `ice` anyway.

  Skyrim names ice assets exactly that concatenated way: `icefrozen01`,
  `icecavewall01`, `icerock01`, `icecavesnowtrim01`, `icelakesurface`,
  `icewall01`, `icefloes`, `icevine01`, `iceberglargelod`. In every one the
  character after `ice` is alphabetic, so `after_ok` is false and the arm is
  skipped. They then fall through to the `cave`/`rock` stone arm or to the
  default-matte arm, both of which resolve **roughness 0.85, metalness 0.0**.

  In the other direction, `contains_any_ci_word` treats a *digit* as a word
  boundary (`before_ok = i == 0 || !hs[i-1].is_ascii_alphabetic()`,
  material.rs:1147). Skyrim's Imperial-fort snow-variant textures are named
  `impextwall01ice.dds` / `impwall05ice.dds` / `impextrubble01ice.dds` /
  `impextdecals01ice.dds` — `ice` preceded by a digit and followed by `.` — so
  they *do* match, and rough masonry resolves to **roughness 0.10** (glass-smooth),
  which also makes `Material::path_indicates_glass` / `is_glass_keyword_path`
  true for them.
- **Evidence**: throwaway probe over `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa`,
  running the real `import_nif` path and printing each material's
  `roughness_override` (the value `translate_material` seeds and `resolve_pbr`
  clamps through unchanged):

  ```
  materials=76934  'ice' substring: word-bounded(reaches glass arm)=269 in 4 paths;
                                    NOT word-bounded(misses)=1928 in 67 paths

  -- word-bounded (glass arm fires) --
       56  textures\dungeons\imperial\impextdecals01ice.dds  rgh=Some(0.1) met=Some(0.0) kind=0
       25  textures\dungeons\imperial\impextrubble01ice.dds  rgh=Some(0.1) met=Some(0.0) kind=0
       83  textures\dungeons\imperial\impextwall01ice.dds    rgh=Some(0.1) met=Some(0.0) kind=0
      105  textures\dungeons\imperial\impwall05ice.dds       rgh=Some(0.1) met=Some(0.0) kind=0

  -- NOT word-bounded (glass arm suppressed), genuine-ice excerpt --
      283  textures\dungeons\caves\icefrozen01.dds          rgh=Some(0.85) met=Some(0.0)
      170  textures\dungeons\caves\icefrozen02.dds          rgh=Some(0.85) met=Some(0.0)
      241  textures\dungeons\caves\icecavesnowtrim01.dds    rgh=Some(0.85) met=Some(0.0)
      187  textures\dungeons\caves\icecavewall01.dds        rgh=Some(0.85) met=Some(0.0)
      100  textures\dungeons\caves\icecavewall04.dds        rgh=Some(0.85) met=Some(0.0)
      178  textures\dungeons\caves\icerock01.dds            rgh=Some(0.85) met=Some(0.0)
       88  textures\dlc01\landscape\icewall01.dds           rgh=Some(0.85) met=Some(0.0)
       77  textures\dlc01\landscape\icelakesurface.dds      rgh=Some(0.85)/0.55 (env arm)
       14  textures\dungeons\caves\icecaverocks01.dds       rgh=Some(0.85) met=Some(0.0)
       11  textures\landscape\frozenmarshice01.dds          rgh=Some(0.85) met=Some(0.0)
        8  textures\dlc01\landscape\icelakesnowcracks.dds   rgh=Some(0.85) met=Some(0.0)
        4  textures\dlc01\lod\dlc01icewalllod.dds           rgh=Some(0.85) met=Some(0.0)
        3  textures\lod\iceberglargelod.dds                 rgh=Some(0.85) met=Some(0.0)
  ```
  The 67 suppressed paths do contain genuine false positives the boundary
  correctly rejects (`riftenlattice01`, `wrwoodlattice01`, `mageapprentice\*`,
  `blacksmithnovice*`, `practicedummy01`, `sanspicedwine`, `dlc01chalice`,
  `birthsignapprentice01`, `sbitsandpices` — 145 instances). Netting those out
  leaves **1,783** instances of real ice/frozen surface, i.e. **~92% of the
  suppressed set is genuine ice, and 100% of the matched set is not ice.**
- **Impact**: Every ice cave, glacier wall, frozen lake surface, ice floe and
  Forgotten Vale / Soul Cairn ice asset in Skyrim + Dawnguard + Dragonborn
  shades as fully matte dielectric (roughness 0.85, well above `triangle.frag:2549`'s
  `roughness < 0.6` RT-reflection gate), so it receives no environment
  reflection and never reaches the glass/IOR path — ice reads as grey plaster.
  Conversely 269 Imperial-fort exterior wall/rubble/decal draws are shaded as
  mirror-smooth (0.10) and are additionally flagged as glass-keyword paths by
  `is_glass_keyword_path`, which is the alpha-gated promotion input to
  `classify_glass_into_material`. This is a wrong `Material` out of the NIFAL
  boundary, which the severity table pins at HIGH minimum.
- **Suggested Fix**: split the two matchers. `ice` needs a Skyrim-aware rule, not
  a symmetric word boundary — e.g. accept `ice` when it is a *path-component
  prefix* (`\ice…`, `dlc1ice…` filename-initial after the digits) or when it is
  followed by a known ice noun (`cave`, `wall`, `rock`, `frozen`, `lake`, `berg`,
  `floe`, `vine`, `snow`), while keeping the current boundary for the trailing
  case so `lattice` / `apprentice` / `novice` / `practice` stay rejected. Tighten
  the leading side so a digit no longer opens the boundary
  (`impwall05ice` must not match). Pin both directions with the exact vanilla
  paths above as regression cases — the current test suite has no Skyrim ice case
  at all.
- **Related**: #2009 (CLOSED, introduced the boundary), #3315 (CLOSED, the same
  class of collision on the skin arm), #3335 (OPEN — *unbounded* collisions in
  the same classifier; this is the opposite direction and is not covered by it).
  No OPEN issue mentions the ice arm.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
