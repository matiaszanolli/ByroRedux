# Oblivion (TES4) Compatibility Audit — 2026-08-07

**Scope**: NIF v20.0.0.5 retail body + the v10.x NetImmerse tail, BSA v103,
the live ESM path, rendering/material translation, NIFAL canonical
translation for Oblivion, real-data validation, and the exterior blocker
chain. All 7 dimensions were delegated to sub-agents (max 3 concurrent),
each verifying its checklist against live source, live `cargo test` runs,
and live data pulled from `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.

## Executive Summary

Oblivion remains the most mature per-game slice of the compat matrix. This
sweep found the sizeless NIF parser **improved** since the last audit — the
v10.x stride-drift family (#1506/#1507/#1508/#1509) is not only intact but
has *reduced* the residual truncation count from 6 files to 1 — and it
root-caused that last file to a specific 4-byte gap (`NiKeyframeController.Data`,
OBL-D1-01). Archive extraction, the live ESM path, and the NIFAL material
boundary all check out clean or with only latent/doc findings. The one
piece of material news this cycle is on the render side: a legacy
(pre-`NiPSys`) particle stack that a prior audit had marked "confirmed dead
code" turns out to still be live in vanilla Oblivion content and drops all
its particles silently (OBL-D4-01, HIGH) — this is now the top concrete,
Oblivion-specific content gap in the compat matrix.

- **NIF parse (incl. v10.x tail)**: **99.99% clean (8,031 / 8,032,
  `Oblivion - Meshes.bsa`)** — up from the 99.93% (8,026/8,032) baseline
  cited in prior audits and in `ROADMAP.md`. Five of the six previously
  truncating NetImmerse marker files (`marker_arrow`, `marker_divine`,
  `marker_temple`, `marker_travel`, and the corrupt-by-design `marker_radius`,
  #698) now parse to full block count; only `marker_map.nif` still truncates,
  and its root cause is now identified (OBL-D1-01). `ROADMAP.md`'s Oblivion
  compat-matrix row and the checked-in truncation/per-block baselines have
  not yet caught up to this improvement (OBL-D1-03 / OBL-D6-01).
- **BSA v103 archive**: regression guard intact — version/folder-size/hash
  logic unchanged and correct; full 17-archive / 147,629-file sweep
  (9,612 NIFs) re-run at 100.0000% extraction, 0 failures.
- **ESM parse**: live path (not a stub); every Oblivion-specific branch
  (16-byte ACBS #1650, CONT 4-byte DATA, CLMT 3-entry WLST #540, XCLL
  3-size band, single-byte DIAL/INFO DATA, MGEF-by-code map #969, RCLR
  #970) re-verified correct; both real-data parity tests
  (`clas_oblivion_knight_against_vanilla`, `race_oblivion_data_and_subs_against_vanilla`)
  and the CELL/worldspace integration tests pass live against vanilla
  `Oblivion.esm`. 635/635 plugin-crate tests green. One coverage gap found
  downstream of parsing: placed creatures (`ACRE`/`ACHR`→`CREA`) never
  reach the animated-actor spawn pipeline and render as frozen static
  geometry (OBL-D3-01, MEDIUM).
- **Render / NIFAL**: Disney-BSDF gate confirmed to stay unreachable for the
  all-legacy Oblivion material universe (0/0 across `is_pbr`/`MAT_FLAG_PBR_BSDF`);
  `EmissiveSource::Material` tagging, the `resolve_pbr` NaN-sentinel
  resolve-once path, `#869` wireframe/flat-shading guards, and the typed
  `NiPSysEmitter` parse→ECS→render hookup all hold. Two new issues: the
  legacy pre-`NiPSys` particle stack silently drops all its emitters
  (OBL-D4-01, HIGH — see above), and the no-cluster directional Lambert
  fallback is ~π× dimmer than the clustered per-light path for 100% of
  Oblivion's legacy (non-PBR) material universe (OBL-D4-02, MEDIUM). NIFAL
  boundary tracing found three raw-tier fields (`texture_clamp_mode`,
  `src_blend_mode`, `dst_blend_mode`) that bypass the canonical `Material`
  and are hand-duplicated at four spawn sites — currently byte-identical,
  but a latent-drift risk and invisible to live `mat.*` diagnostics
  (OBL-D5-01, MEDIUM).
- **Cell loading**: Interior renders end-to-end (unchanged, Anvil Heinrich
  Oaken Halls). Exterior: TES4 worldspace + LAND wiring is implemented and
  game-agnostic (#1556) — and unlike prior sweeps, **the on-device exterior
  render bench has now actually run and passed**: Tamriel `(0,0)` radius-1
  static profile (5,709 entities / 2,355 draws, 0 missing textures,
  image-health pass) and a 3-boundary grid-cross traversal (908 ms / 1.50 s
  full-detail/LOD max, no device loss), landed via the EX-01..EX-08
  exterior-readiness workstream (epic #2377, 2026-08-04/05). Oblivion is
  currently the best-behaved of the five primary profiles in that matrix.
- **Top blockers in priority order**:
  1. **OBL-D4-01 (HIGH)** — legacy pre-`NiPSys` particle stack (fire, dust,
     blood, magic FX) parses fully but emits zero `ImportedParticleEmitter`s;
     the only concrete Oblivion-specific content gap left in the render path.
  2. **OBL-D3-01 (MEDIUM)** — placed creatures (`ACRE`/`ACHR`→`CREA`) never
     route through the actor spawn pipeline; dungeons/wilderness render
     creatures as frozen static meshes.
  3. **OBL-D4-02 (MEDIUM)** — Lambert diffuse normalization mismatch (~π×)
     between the clustered and no-cluster lighting paths, systematically
     dimming Oblivion exteriors on the fallback path.
  4. **OBL-D1-01 (MEDIUM)** — the one remaining NIF truncation
     (`marker_map.nif`), root-caused to a missing `NiKeyframeController.Data`
     read; a small, well-scoped parser fix.
  5. **OBL-D5-01 (MEDIUM)** — NIFAL boundary leak for three material fields;
     latent today, real diagnostic/drift risk.
  6. Baseline/doc hygiene (OBL-D1-03/04/05, OBL-D6-01, OBL-D7-02/03) — no
     functional risk, but keeps regression gates trustworthy and audit
     framing accurate for future sweeps.

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.4/.5 retail + v10.x NetImmerse Tail)

**5 findings: 1 MEDIUM, 4 LOW. All 10 checklist items PASS; the v10.x
stride-drift regression-guard family is intact and net-improved.**

Real-data verification: `cargo test -p byroredux-nif` (960 lib + 76
integration/doc tests, 0 failed); `block_coverage_baselines -- --ignored`
(7/7 pass, 8031/8032 Oblivion NIFs whole); a live version census over all
11 Oblivion BSAs (16 distinct `(version, user_version, user_version_2)`
tuples, every one resolving correctly against the header guards).

#### MEDIUM

- **OBL-D1-01 — `NiKeyframeController.Data` (`until="10.1.0.103"`) is never
  read — the sole remaining Oblivion truncation.** `crates/nif/src/blocks/mod.rs:748-758`,
  `crates/nif/src/blocks/controller/mod.rs:253-267`. NEW. nif.xml defines a
  complementary pair (`NiSingleInterpController.Interpolator since="10.1.0.104"`
  / `NiKeyframeController.Data until="10.1.0.103"`) so the block always
  carries exactly one 4-byte ref; the dispatcher routes `NiKeyframeController`
  straight to `NiSingleInterpController::parse`, which only reads the
  `>= 10.1.0.104` field. Below that version nothing is read and the block
  ends 4 bytes early. Byte-traced on `meshes\marker_map.nif` (v4.2.1.0):
  the parser drops 8 of 13 blocks, including both `NiTriShape` subtrees —
  the Oblivion world-map marker imports with no geometry. This is the only
  file in `Oblivion - Meshes.bsa` (1/8032) that still truncates; fixing it
  takes Oblivion sizeless parity to 8032/8032. The helper this needs already
  exists (`NifVersion::has_keyframe_controller_data()`) but its only caller
  is `BsKeyframeController`. Related: #2345 (sibling `ControlledBlock`
  mis-gate, still open). Fix: give `NiKeyframeController` its own parser
  calling the existing gate helper, split `NiVisController`/`NiAlphaController`/
  `NiTransformController` out of the shared arm with the same gate.

#### LOW

- **OBL-D1-02 — Four more controllers miss their `until="10.1.0.103"` Data
  ref (and `NiFlipController` its `Accum Time`/`Delta`).** `crates/nif/src/blocks/controller/shader.rs:55-79,179-203,211-232`,
  `crates/nif/src/blocks/controller/mod.rs:345-368,591-620`. NEW. Same root
  cause as OBL-D1-01 in five sibling parsers
  (`NiTextureTransformController`, `NiMaterialColorController`,
  `NiLightColorController`, `NiFloatExtraDataController`, `NiFlipController`).
  Zero vanilla Oblivion files at `version <= 10.1.0.103` reference any of
  these types today (latent, not live), but it is a real trap for
  mod/legacy NetImmerse content. `NiFlipController`'s own code comment
  asserting "nothing to read here" is wrong for this exact pre-10.1.0.104
  band. Fix alongside OBL-D1-01, same shape.
- **OBL-D1-03 — The Oblivion truncation baseline and the ROADMAP parse-rate
  row are stale by 5 files.** `crates/nif/tests/data/block_coverage_baselines/oblivion_truncations.tsv:1-7`,
  `ROADMAP.md:430`. NEW. The checked-in baseline still lists 6 truncating
  files (`truncating=6 parsed=8032`); a live run reports 8031/8032 whole, 1
  truncating. `ROADMAP.md:430` still says "99.93% (8,026/8,032)"; the true
  figure is 99.99% (8,031/8,032). The gate still catches regressions (it's
  a superset), so nothing is broken, but the stated numbers are wrong.
  **Sibling finding**: see OBL-D6-01 below — a second, independent
  checked-in baseline (`per_block_baselines/oblivion.tsv`) has also drifted
  stale, for a different underlying reason (a benign block-type
  reclassification, not truncation-count change). Fix: regenerate both
  baselines together in one commit (after OBL-D1-01 lands) and update
  `ROADMAP.md` in the same commit.
- **OBL-D1-04 — Two latent `TexDesc` version gaps, plus a PS2 L/K divergence
  between the two `TexDesc` readers.** `crates/nif/src/blocks/properties.rs:349-381,401-462`.
  NEW. `read_tex_desc`'s `else` branch over-reads 12 bytes for the
  unexercised `20.1.0.0`–`20.1.0.2` band; `Unknown Short 1 (until=4.1.0.12)`
  is never read; the shader-map trailer's second `TexDesc` reader omits the
  PS2 L/K shorts the primary reader correctly reads at `<= 10.4.0.1`. Fully
  latent on the live 11-BSA vanilla corpus (no file in the affected bands
  carries a `NiTexturingProperty`). Risk confined to NifSkope-exported
  Oblivion mod content. Fix: make the `TexDesc` version branch explicit
  rather than `else`, add the missing `Unknown Short 1` read, factor a
  shared `TexDesc`-body helper so the two readers can't drift again.
- **OBL-D1-05 — `audit-oblivion/SKILL.md` mis-states the Oblivion retail
  version and the pre-Gamebryo fallback behaviour.** `.claude/commands/audit-oblivion/SKILL.md:22-24,68-70`.
  NEW. The skill's own brief names v20.0.0.5 as the dominant retail body;
  the live census says it's actually v20.0.0.4 (7,282 files vs. 1,680) —
  `version.rs` already documents this correctly, the skill contradicts it.
  The skill also claims pre-v3.3.0.13 files return an empty `NifScene`;
  the parser actually parses inline and only fails (with a `warn`) on a
  mid-file inline-name read error. Documentation-only, but it is the brief
  every Dimension-1 agent reads first. Related: #2348/#2347 (same doc-drift
  class from earlier Oblivion audits).

### Dimension 2 — BSA v103 Archive

**0 findings. Regression guard fully intact.**

`BSA_V_OBLIVION = 103` recognition, v103/v104 16-byte vs. v105 24-byte
folder-record sizing, the Xbox-archive-bit-vs-embed-file-names version
split (empirically verified against all 17 vanilla v103 BSAs, every one
setting bit `0x100`), and the folder/file hash functions all verified
correct against source and real data. A full 17-archive sweep
(`obl_sweep`) reports **147,629/147,629 files (100.0000%), 9,612/9,612
NIFs (100.0000%)**, zero failures. `cargo test -p byroredux-bsa` — 58
passed, 0 failed.

### Dimension 3 — ESM Record Coverage (live path)

**1 finding: MEDIUM. All 6 checklist items PASS.**

`cargo test -p byroredux-plugin` is 635/635 green. Both previously-ignored
real-data parity tests (`clas_oblivion_knight_against_vanilla`,
`race_oblivion_data_and_subs_against_vanilla`) pass live against vanilla
`Oblivion.esm`, alongside the CELL/worldspace integration tests. The
16-byte ACBS guard (#1650), CLMT 3-entry WLST (#540), MGEF 4-char-code map
(#969), CONT length-gated DATA, XCLL/RCLR (#970), and DIAL/INFO byte-offset
decode are all re-verified correct.

#### MEDIUM

- **OBL-D3-01 — Creature placements (Oblivion `ACRE`, and cross-game
  `ACHR`→`CREA`) never route through the actor spawn pipeline — they
  render as frozen static geometry.** `byroredux/src/cell_loader/references/mod.rs:485`,
  `byroredux/src/cell_loader/exterior.rs:174,294,341`,
  `byroredux/src/cell_loader/load.rs:404-418`,
  `crates/plugin/src/esm/records/dispatch_actor.rs:42-49`. NEW — not a
  regression of #396 (which fixed CREA/ACRE *parsing* into the statics
  fallback and explicitly treated the static-mesh render as its acceptance
  bar). NPC_ and CREA are parsed into two disjoint maps
  (`index.npcs`/`index.creatures`), both typed `NpcRecord`. Every runtime
  call site in `byroredux/src/cell_loader/` that decides "is this REFR an
  actor" checks only `index.npcs` — `index.creatures` is never consulted
  anywhere under `byroredux/src/` (confirmed by exhaustive grep). A placed
  `CREA` base form falls through to the generic static-mesh instance path,
  which only animates via an *embedded* NIF controller clip (#544) — never
  via external `.kf` skeletal locomotion/idle, the mechanism NPC_ actors use.
  Impact: Oblivion dungeons, Ayleid ruins, the Arena, and wilderness
  encounters — a large fraction of Oblivion's placed-actor content — render
  creatures frozen in bind pose, easy to mistake for a KF-importer gap
  rather than a spawn-routing gap. Cross-game (any FO3+ master with `ACHR`→`CREA`
  hits the same gap) but Oblivion is highest-density since `ACRE` is
  dedicated and ubiquitous there. Fix: thread `index.creatures` alongside
  `index.npcs` into `load_references`/`load_references_budgeted` and the
  exterior call sites; extend actor-detection to check both maps (both
  already share `NpcRecord`); add a no-race fallback to the runtime FaceGen
  path since creatures typically reference no RACE.

### Dimension 4 — Rendering Path for Oblivion Shaders

**4 findings: 1 HIGH, 1 MEDIUM, 2 LOW. Wide swath of properties
(`NiTexturingProperty` slot coverage, `NiMaterialProperty` raw-color
semantics, `NiWireframeProperty`/`NiShadeProperty`, `NiSpecularProperty`,
`NiZBufferProperty`, `NiVertexColorProperty`, `NiAlphaProperty`, and the
modern `NiPSysEmitter` decode) verified clean.**

#### HIGH

- **OBL-D4-01 — Oblivion legacy particle stack (`NiParticleSystemController`
  / `NiAutoNormalParticles`) is fully parsed but produces no
  `ImportedParticleEmitter`.** `crates/nif/src/import/walk/mod.rs:531-568`,
  `crates/nif/src/blocks/legacy_particle.rs:318-361`,
  `crates/nif/src/blocks/mod.rs:375-400`. NEW. The particle-emission site
  downcasts only to the modern `NiParticleSystem`/`NiPSysEmitter*` shape.
  Oblivion's *other* particle stack — `NiParticleSystemController`/
  `NiBSPArrayController` + `NiAutoNormalParticles`/`NiRotatingParticles`,
  whose own dispatcher comment reads "Oblivion magic FX, fire, dust,
  blood" — dispatches to `legacy_particle::*` and matches neither
  downcast, so it imports with zero emitters. The in-code comment at the
  emission site claims "the target games all author the modern NiParticleSystem
  stack" — true for FO3/FNV/Skyrim+, false for Oblivion, which is exactly
  the title this dimension exists to cover. `legacy_particle::NiParticleSystemController`
  already decodes a superset of the needed spawn parameters (speed,
  declination, birth_rate, lifetime, emitter_dimensions, etc.) — it's
  parsed, just never consumed. `crates/nif/src/import/tests/particle.rs:23,86`
  pins the current gap as intended behaviour ("deliberately not surfaced
  (#1327)"), i.e. it's test-locked in. Impact: every Oblivion FX asset
  using the legacy stack (torch fire, magic-effect shaders, dust, blood,
  smoke) renders as static geometry with zero particles — no parse error,
  no log, looks like "Oblivion has no fire." This is the top concrete
  Oblivion-specific content gap in the render path. Fix: add a second
  emission arm downcasting `legacy_particle::NiParticleSystemController`
  (+ `NiBSPArrayController` alias), map its fields onto
  `ImportedEmitterParams`, reuse the existing NIFAL-S3 finite/positivity
  filter verbatim, and update the #1327 tests to assert the new arm.

#### MEDIUM

- **OBL-D4-02 — Legacy (non-PBR) Lambert diffuse differs by a factor of π
  between the clustered per-light path and the no-cluster directional
  fallback.** `crates/renderer/shaders/include/lighting.glsl:154-166`,
  `crates/renderer/shaders/triangle.frag:2321-2332`. NEW. Both sites branch
  on `MAT_FLAG_PBR_BSDF` and both take the Lambert `else` arm for 100% of
  Oblivion content (cross-referenced with OBL-D4-04/OBL-D5's Disney-gate
  confirmation), but the two Lambert arms use different normalization:
  `lighting.glsl` has no `/PI` (documented as "the legacy non-/PI Lambert
  convention"); `triangle.frag` divides by `PI` then applies an extra
  `* vec3(0.8)`. Impact: an Oblivion surface lit by the no-cluster
  directional fallback is ~π× (≈3.14×) dimmer than the identical surface
  lit through the clustered per-light path — visible as a brightness pop
  crossing the cluster-population threshold, and as systematically dark
  Oblivion exteriors. Also affects FO3/FNV legacy content equally. Fix:
  make the two sites agree (drop `/PI` at `triangle.frag:2331` and re-tune
  the `vec3(0.8)` fudge, per the `lighting.glsl` side being named the
  legacy reference), guarded by a shader-parity unit test; validate with a
  live capture before shipping, per project policy against speculative
  Vulkan/shader fixes.

#### LOW

- **OBL-D4-03 — `NiStencilProperty` state is captured but structurally
  un-consumable (depth format has no stencil bits).** `crates/nif/src/import/material/legacy_properties.rs:654-674`,
  `crates/renderer/src/vulkan/pipeline.rs:399-410`. KNOWN/documented (#337),
  reconfirmed still open, no action recommended — vanilla Oblivion's
  `NiStencilProperty` usage is dominated by the two-sided `draw_mode` case,
  which *is* honoured; true stencil-masked content is a handful of mod
  assets.
- **OBL-D4-04 — `MAT_FLAG_PBR_BSDF` verified always-0 for Oblivion, but
  `is_pbr` has no negative test pinning that invariant.** `byroredux/src/cell_loader.rs:229-232`,
  `byroredux/src/asset_provider/material.rs:719,807,990,1148`,
  `crates/nif/src/import/material/mod.rs:1244`. VERIFIED CORRECT today — no
  defect, but a test-coverage gap. Every `is_pbr = true` writer sits behind
  an external material file merge (`.mat`/BGSM/BGEM); the NIF import path
  hard-writes `is_pbr: false`, and Oblivion authors no BGSM/BGEM/.mat, so
  the Disney lobe is correctly unreachable. Risk is regression, not present
  behaviour: a future "promote legacy specular to PBR" heuristic could
  silently flip every Oblivion surface onto the Disney lobe. Fix: add a
  one-line regression test asserting `NiMaterialProperty`+`NiTexturingProperty`-only
  `MaterialInfo` yields `is_pbr == false`.

Not verified this cycle for lack of an on-device BSA: OBL-D4-01's exact
blast-radius count (a corpus census of `NiParticleSystemController` vs.
`NiParticleSystem` block frequency in `Oblivion - Meshes.bsa`) — argued
from source and the dispatcher's own comment, not measured.

### Dimension 5 — NIFAL Canonical Material Translation for Oblivion

**3 findings: 1 MEDIUM, 2 LOW. All 3 requested confirmations PASS** —
`Material::resolve_pbr` is the sole PBR resolution site for Oblivion (no
per-draw `classify_pbr` reappeared), and Oblivion legacy meshes correctly
tag `EmissiveSource::Material` via the `NiMaterialProperty` arm (distinct
from the Skyrim/FO4 `BSLightingShaderProperty` and FO4+ `BSEffectShaderProperty`
arms), test-pinned at `emissive_source_tests.rs:236`.

#### MEDIUM

- **OBL-D5-01 — Three raw-tier `ImportedMaterial` fields bypass the NIFAL
  boundary and are re-read at each spawn site.** `byroredux/src/cell_loader/spawn.rs:1367,1533,1565`,
  `byroredux/src/scene/nif_loader.rs:786,830,915`. NEW. `texture_clamp_mode`,
  `src_blend_mode`, and `dst_blend_mode` have no canonical `Material` field
  — they are read directly off the raw `ImportedMaterial` at four spawn
  sites instead of through `translate_material`, exactly the
  hand-synced-duplication failure mode the NIFAL boundary's own module doc
  says it was created to eliminate. Oblivion relevance is direct:
  `texture_clamp_mode` (`CLAMP_S_CLAMP_T`) is authored on Oblivion
  architecture trim/signs/banners (#610); `src_blend_mode`/`dst_blend_mode`
  come from `NiAlphaProperty`'s Oblivion-era blend-factor authoring. The
  two sites are byte-identical today (latent, not live), but a third spawn
  path (FO4's `cell_loader/precombined.rs`) already reads the same raw
  fields independently and could silently diverge, and these values are
  invisible to `mat.*`/`material_dump` console diagnostics since those
  inspect the canonical `Material`. Fix: add the three fields to `Material`,
  copy them in `translate_material`, point both spawn sites at the
  canonical component, extend the canonical-completeness harness.

#### LOW

- **OBL-D5-02 — `resolve_normal_alpha_spec_roughness` post-mutates
  canonical roughness outside `translate_material`, with no canonical-tier
  test of the combined result.** `byroredux/src/material_translate.rs` (called from
  `cell_loader/spawn.rs:1553`, `scene/nif_loader.rs:934`). NEW. Both load
  paths call the post-pass consistently (no divergence today), but the
  gate (`normal_alpha_spec_applies`) is live for a meaningful swath of
  non-metal Oblivion content — Oblivion stores tangent-space normals in
  `NiTexturingProperty`'s bump slot with the specular mask in the DDS
  alpha, and `env_map_scale` is 0.0 unless the SLSF1 env bit is authored —
  so the alpha-normal roughness formula (derived from `NiMaterialProperty.shininess`)
  silently overrides the classifier's default matte roughness across
  Oblivion architecture/clutter. Whether the resulting values are correct
  is unmeasured (no Oblivion BSA reachable this session — a ready-made
  census harness exists at `crates/nif/examples/_tmp_obl_d5_nifal.rs`).
  Two concrete test gaps regardless: the unit tests exercise the resolver
  as a pure function only, and the canonical-completeness harness
  deliberately bypasses classifiers so it never covers Oblivion's actual
  roughness population.
- **OBL-D5-03 — `resolve_pbr`'s classifier backstop hardcodes
  `specular_authored: false`, diverging from the real Oblivion signal.**
  `crates/core/src/ecs/components/material.rs:815-829`. NEW. The importer
  already carries the exact signal (`MaterialInfo::specular_authored`, true
  for every Oblivion mesh with a `NiMaterialProperty`) but it is never
  forwarded onto `ImportedMaterial`. Impact today is nil — the backstop is
  unreachable on the Oblivion path since overrides always arrive `Some`
  (per OBL-D5-01's confirmation) — but becomes a live divergence the
  moment any future non-pre-classified source reaches `translate_material`,
  which is the same shape as the closed #1873 chrome-flyer regression.
  Fix: forward `specular_authored` onto `ImportedMaterial`, or delete the
  backstop arm entirely since every live producer already supplies `Some`.

### Dimension 6 — Real-Data Validation

**1 finding: LOW. 3 representative meshes (chandelier, book, troll) traced
end-to-end through `import_nif_scene`/`import_nif` — all clean, sane mesh
counts, correctly differentiated material chains (metal vs. wax on the
chandelier; body vs. alpha overlay on the troll).**

Reused Dimension 1's full-corpus sweep. Confirmed the residual-truncation
count (1, `marker_map.nif`) matches Dimension 1's finding exactly, and
identified one genuinely new block type in the histogram
(`bhkPCollisionObject`) that is correctly dispatched, not an unknown-type
regression.

#### LOW

- **OBL-D6-01 — Checked-in `per_block_baselines/oblivion.tsv` is stale —
  the opt-in regression gate currently FAILS on a benign reclassification.**
  `crates/nif/tests/data/per_block_baselines/oblivion.tsv` (last regenerated
  2026-06-15). NEW. Live-running the opt-in gate
  (`per_block_baseline_oblivion -- --ignored`) fails today:
  `bhkCollisionObject` reads 8,784 in the baseline vs. 8,730 live (−54),
  with a new `bhkPCollisionObject = 54` row appearing only live. The
  arithmetic is exact (8,730 + 54 = 8,784) — a pure reclassification into
  an already-existing, already-correctly-dispatched type
  (`crates/nif/src/blocks/mod.rs:1235`, predates the baseline by ~7 weeks,
  #557), not data loss. Six other types show small increases fully
  explained by Dimension 1's truncation-recovery finding. **Sibling of
  OBL-D1-03** — a second, independent checked-in baseline in the same test
  suite has also gone stale, for a different reason (reclassification vs.
  truncation-count drift). The gate is opt-in (not CI-wired), so nothing
  is silently broken, but anyone who actually runs it — as this dimension
  was asked to — hits a false "parser regression?" panic. Fix: regenerate
  both Oblivion baselines (`per_block_baselines/oblivion.tsv` and
  `block_coverage_baselines/oblivion_truncations.tsv`) together in one
  commit, once OBL-D1-01 (`marker_map.nif`) is fixed too.

### Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks

**1 finding: LOW (OBL-D7-02). One informational housekeeping note
(OBL-D7-03, not a code finding, not counted in severity totals). OBL-D4-01
is cross-referenced here, not double-counted — see Dimension 4 for its
full record.**

Six of seven checklist items reconfirmed clean and unchanged since the
2026-08-03 pass: the `--bsa` CLI path (now backed by the 2026-08-04
on-device exterior bench, not just static inspection), REFR placement
record coverage (no Oblivion-specific gap beyond the FNV-aligned set),
animation scene-graph name resolution (#2221, unchanged), the
non-existent pre-v3.3.0.13 empty-scene fallback (reconfirmed dead), and
`_far.nif` distant-object LOD (#1726/#1745, 10/10 real-data tests still
pass). The seventh item — legacy particle routing — **flips** from the
prior audit's "confirmed non-issue" to a real HIGH gap (OBL-D4-01, filed
this session under Dimension 4); do not reuse the old "legacy pre-NiPSys
stack confirmed dead code" framing going forward.

#### LOW

- **OBL-D7-02 — Doc drift: ROADMAP.md's Oblivion exterior compat-matrix
  entity/FPS figure is stale against the newer, more thorough
  readiness-plan bench.** `ROADMAP.md:430` vs. `docs/engine/exterior-readiness-plan.md`.
  NEW. `ROADMAP.md` still cites "4,886 entities / 150.6 FPS" for Tamriel
  `(0,0)` radius-1; the 2026-08-04 EX-01 sweep re-ran the identical
  profile and recorded 5,709 entities / 2,355 draws with an explicit
  image-health pass — a denser, more validated measurement of the same
  scenario, landed in the same commit window that touched `ROADMAP.md`
  for an adjacent edit but left this figure untouched. Documentation-only;
  risk is a future contributor misreading the delta as a regression. Fix:
  update `ROADMAP.md:430` to cite the 2026-08-04 figures and/or point at
  `docs/engine/exterior-readiness-plan.md` as the live source.

#### Informational (not counted)

- **OBL-D7-03 — Issue #2348 is already fixed in tree but still open on
  GitHub.** `README.md:129-130`. The prior audit's OBL-D7-01 (README
  framing Oblivion exterior as wiring-gated) was fixed by commit `49b14e95`
  (2026-08-04); the tracking issue is still listed OPEN. Not a code
  finding — flagged so a closing pass closes #2348 as already resolved.

## Blocker Chain

**Sequential list to reach "exterior cell renders."** Interiors already
work end-to-end (Anvil Heinrich Oaken Halls, unchanged). TES4 worldspace +
LAND wiring is already implemented and game-agnostic (#1556) — this is not
a pending item, it shipped several audit cycles ago.

1. **TES4 worldspace + LAND wiring** — done, game-agnostic (#1556).
   *(historical step, not a current blocker)*
2. **CELL exterior REFR placement** — done, no Oblivion-specific gap beyond
   the FNV-aligned record set (Dimension 3 confirms; Dimension 7
   reconfirms). *(historical step, not a current blocker)*
3. **On-device exterior render bench** — **done, not pending.** Unlike
   prior audits which listed this as the sole remaining step, the
   EX-01..EX-08 exterior-readiness workstream (epic #2377) actually ran it
   on 2026-08-04/05: Tamriel `(0,0)` radius-1 static profile (5,709
   entities / 2,355 draws, 0 missing textures, image-health pass) and a
   3-boundary grid-cross traversal (908 ms / 1.50 s full-detail/LOD max,
   24.0 ms apply max, 72.3 ms frame max, no device loss) both pass.
   Oblivion is currently the best-behaved of the five primary profiles in
   the readiness matrix — no white-out (unlike historical FO3), no
   device-loss/memory blowup (unlike initial FO4), no multi-second tail
   latency (unlike Skyrim's 8.1 s worst crossing). **Do not re-file this
   step as pending in future sweeps.**
4. **Remaining readiness gates (shared cross-game infrastructure, not
   Oblivion-specific blockers)**: EX-05 (pre-tonemap non-finite pixel
   counter) is unimplemented cross-game, so today's passing image-health
   check is PNG-statistics-based rather than a true HDR NaN guard;
   Tranche B items (typed foreground-readiness result vs. `Vec3::ZERO` on
   ambiguity, full deadline-bounded atomic apply/unload/global-geometry
   work, the EX-08 cancellation/ownership soak) remain open, tracked under
   #2377/#2368/#2374/#2376.
5. **OBL-D4-01 (HIGH)** — the one real Oblivion-specific *content* gap left
   in the chain: legacy pre-NiPSys particle FX (fire, dust, blood, magic
   effects) silently drop on import. Does not block terrain/geometry
   rendering, but the executed bench's "0 missing textures / image-health
   pass" result is blind to this class of missing visual content
   (particles aren't textures and don't fail image-health by being
   absent).
6. **Doc/baseline sync** (no functional blocker): OBL-D7-02 (ROADMAP bench
   figure), closing stale issue #2348, and regenerating the two stale NIF
   baselines (OBL-D1-03 / OBL-D6-01) once OBL-D1-01 lands.

Do **not** regenerate the dead "v103 is broken" framing (dead since #699,
re-verified clean by Dimension 2 this cycle) or the dead "wiring missing"
framing (dead since #1556). Do **not** describe the on-device exterior
bench as still-pending — it ran and passed on 2026-08-04/05.

## Regression Guard List

Previously-fixed items this audit re-verified still hold:

- **v10.x stride-drift family (#1506/#1507/#1508/#1509)** — all four intact
  and, this cycle, **net-improved**: Oblivion sizeless block-count parity
  rose from 8,026/8,032 to 8,031/8,032 as five of the six previously
  truncating NetImmerse marker files now parse to full block count
  (Dimension 1, PASS-2).
- **`NiTexturingProperty` u32 count** — reads a raw `uint` shader-texture
  count directly, no leading bool gate; regression would break every
  Oblivion clutter/book/furniture mesh (Dimension 1, item 5, PASS;
  Dimension 4 confirms downstream slot coverage intact).
- **BSStreamHeader dual-band (#170)** — the exact
  `version == V10_0_1_2 || (user_version >= 3 && (...))` band re-verified
  against 16 distinct live `(version, user_version, user_version_2)`
  tuples across all 11 Oblivion BSAs; the #170 non-Bethesda-out-of-band
  regression test still passes (Dimension 1, item 1, PASS-1).
- **`user_version` threshold (`V10_0_1_8`)** — confirmed still gates
  correctly; older NetImmerse files with `num_blocks` in that field slot
  are unaffected (Dimension 1, item 1).
- **BSA v103 extraction (#699)** — re-run clean: 147,629/147,629 files
  (100.0000%), 9,612/9,612 NIFs, across all 17 vanilla archives including
  base game, all 6 official plugins, and `DLCShiveringIsles - Meshes.bsa`
  (Dimension 2).
- **Disney-BSDF gate stays 0** — `MAT_FLAG_PBR_BSDF`/`is_pbr` confirmed
  unreachable for the all-legacy Oblivion material universe; every
  `is_pbr = true` writer sits behind an external BGSM/BGEM/.mat merge that
  zero vanilla Oblivion content authors (Dimension 4 OBL-D4-04, Dimension 5
  confirmation).

## Summary Table

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 |
| MEDIUM   | 4 |
| LOW      | 10 |
| **Total** | **15** |

By dimension: Dim 1 — 1 MEDIUM + 4 LOW (5). Dim 2 — 0. Dim 3 — 1 MEDIUM (1).
Dim 4 — 1 HIGH + 1 MEDIUM + 2 LOW (4). Dim 5 — 1 MEDIUM + 2 LOW (3).
Dim 6 — 1 LOW (1). Dim 7 — 1 LOW (1; OBL-D4-01 cross-referenced not
double-counted; OBL-D7-03 informational, not counted).

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-08-07.md`
