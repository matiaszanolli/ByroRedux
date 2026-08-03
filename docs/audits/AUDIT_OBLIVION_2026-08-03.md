# Oblivion (TES4) Compatibility Audit — 2026-08-03

**Scope**: NIF v20.0.0.5 + the v10.x NetImmerse tail, BSA v103, the live ESM
path, rendering/material translation, NIFAL canonical translation for
Oblivion, real-data validation, and the exterior blocker chain. Run as one
leg of a `comprehensive` audit-suite sweep. All 7 dimensions were delegated
to sub-agents (max 3 concurrent), each verifying its checklist against live
source, live `cargo test` runs, and live data pulled from
`/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.

## Executive Summary

Oblivion remains the most mature per-game slice of the compat matrix short
of exterior rendering. This sweep re-verified all standing regression guards
intact and found the surface essentially unchanged and healthy since the
2026-07-25 audit: the NIF parser (99.93% clean, 8 026/8 032, 6 residual
NetImmerse markers, 0 hard failures — byte-identical to the checked-in
`oblivion_truncations.tsv` baseline), BSA v103 archive extraction (147,629/
147,629 files, 0 errors, matching #699), the live ESM path (631 plugin tests
passing, both real-vanilla-data parity tests green), the NIFAL material
boundary, and the exterior blocker chain (wiring done, on-device render
bench is the sole remaining step) all check out clean.

Two new findings emerged this cycle, both in Dimension 1 (NIF version
handling) and both MEDIUM/LOW severity gaps in obscure v10.x sub-version
bands that do not affect any vanilla Oblivion content (confirmed via a fresh
full-corpus version census — see Dimension 1). One informational
contribution to the already-open HIGH issue #2193 rules out the "inverted
collision winding" hypothesis and points at a translation/scale unit
mismatch instead. Two LOW documentation-staleness items were also found
(a stale test-doc pointer post-#2059 split, and README.md still describing
the exterior wiring as an open gate when ROADMAP.md has long since closed
it).

- **NIF parse (incl. v10.x tail)**: 99.93% clean (8 026 / 8 032,
  `Oblivion - Meshes.bsa`, verified live via `nif_stats`) — byte-identical to
  the checked-in baseline. Matches `ROADMAP.md`'s cited row exactly — no
  drift.
- **BSA v103 archive**: regression guard intact — version/folder-size/hash
  logic unchanged and correct; full 17-archive/147,629-file sweep re-run
  clean.
- **ESM parse**: live path (not a stub); all Oblivion-specific branches
  (16-byte ACBS, CONT 4-byte DATA, CLMT 3-entry WLST, XCLL 3-size band,
  single-byte DIAL/INFO DATA, MGEF-by-code map, RCLR) verified correct; both
  real-data parity tests pass live against vanilla `Oblivion.esm`.
- **Render / NIFAL**: Disney-BSDF gate confirmed to stay unreachable for the
  all-legacy Oblivion material universe; `EmissiveSource::Material` tagging,
  `resolve_pbr` NaN-sentinel resolve-once path, `#869` wireframe/flat-shading
  guards, the full 11-value `AlphaFunction` blend mapping, and the typed
  particle-emitter parse→ECS→render hookup all hold — 0 new findings across
  Dimensions 4 and 5.
- **Cell loading**: Interior renders end-to-end (unchanged). Exterior:
  TES4 worldspace + LAND parse/load is implemented and game-agnostic; only
  the on-device exterior render bench remains pending (same shape FO3
  was — *not* a BSA v103 problem, that framing stays dead per #699).
- **Top blockers in priority order**: (1) on-device Oblivion exterior render
  bench (infrastructure exists, unexercised) — unchanged from prior sweeps;
  (2) the still-open `is_grounded` grounding bug at Oblivion interior spawns
  (#2193, HIGH, pre-existing — this audit contributes negative evidence
  against its "inverted winding" hypothesis, redirecting investigation
  toward a translation/scale unit mismatch).

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.5 + v10.x NetImmerse Tail)

**2 new findings (1 MEDIUM, 1 LOW), plus 1 LOW informational contribution to
open issue #2193. All 12 regression-guard checklist items confirmed intact
except as noted.**

A fresh independent version census of every `.nif` in `Oblivion - Meshes.bsa`
(new measurement) established that the dominant retail Oblivion version is
actually **v20.0.0.4** (7,282 files), not v20.0.0.5 (100 files) — every gate
written as `<= V20_0_0_5` or `>= V20_0_0_4` covers both, so this doesn't
change any conclusion, but it corrects a long-standing framing assumption in
this audit series' own scope description. The census also pins the exact
v10.1.0.x band present in vanilla content ({10.1.0.101, 10.1.0.106} only) and
the v10.2.0.0 bsver fan-out ({6,7,8,9,11}) that the #1509 gate discriminates
on.

- `user_version` gate at `V10_0_1_8` (`header.rs:114-118`) — **CONFIRMED**,
  and shown to be live-dependency-tested: Oblivion's 41 v10.0.1.0 + 23
  v10.0.1.2 files all carry `user_version = 0` and read `num_blocks` in its
  place.
- BSStreamHeader dual-band guard (`header.rs:137-161`, `#170`) — **CONFIRMED**
  byte-for-byte against nif.xml; regression test
  `bs_stream_header_not_read_for_off_spec_version` passes. The
  `version == V10_0_1_2` first clause is proven load-bearing on real data (23
  vanilla files hit it).
- v10.x gate-boundary constants (`version.rs:71-116`) — **CONFIRMED**, all
  present including `V10_1_0_108`/`.109`/`.110`/`.111`/`.112`/`.113`/`.114`.
- `#1509` `NiGeomMorpherController` `bsver > 9` gate
  (`blocks/controller/morph.rs:103`) — **CONFIRMED**, 3 dedicated tests pass
  in `blocks/controller/path_lookat_tests.rs`; census confirms the gate is
  discriminating on real data (v10.2.0.0 spans bsver {6,7,8,9,11}).
- `#1506` (`NiInterpController.Manager Controlled`, `NiQuatTransform.TRS
  Valid`) and `#1507` (`NiPSysData` + emitter) — **CONFIRMED** intact, both
  with passing dedicated tests.
- `#1508` (`NiBlendInterpolator` + `ControlledBlock`) — **PARTIAL**: the
  three-band routing and the `ControlledBlock` blend-field gates are
  correct, but a genuine 8-byte gap exists at the 108–109 sub-window — see
  **NIF-OBL-D1-01** below.
- `NiTexturingProperty` raw `u32` shader-texture count, no bool gate
  (`blocks/properties.rs:330-382`) — **CONFIRMED**, regression test passes.
- Pre-v3.3.0.13 inline-string block-type fallback (`lib.rs:378-413`) —
  **CONFIRMED** degrades (logs `warn!` + keeps blocks parsed so far), does
  not hard-fail. Verified live on `marker_radius.nif`.
- u16 vs u32 flag width (`blocks/base.rs:82-86`, `FLAGS_U32_THRESHOLD = 26`)
  — **CONFIRMED**, keyed on raw per-file `bsver` (the #1331 doctrine); every
  Oblivion bsver in the census is ≤ 11, so all take the u16 path.
- Oblivion/legacy block dispatch coverage (`blocks/mod.rs`) — **CONFIRMED**,
  0 unknown types across all 81 distinct block types in the fresh corpus
  sweep.
- `BhkMultiSphereShape`/`BhkConvexListShape` → `CollisionShape` resolution
  (`import/collision/shape.rs`) — **CONFIRMED**, both translate correctly,
  not dropped.
- `havok_motion_type` full canonical Havok enum (`#1652`,
  `import/collision/mod.rs:156-165`) — **CONFIRMED, not regressed**. The
  pre-fix `4 => Keyframed`/`_ => Static` collapse is gone; the #1832
  zero-mass-Dynamic-reclassification companion guard is also intact.

#### NIF-OBL-D1-01: `NiBlendInterpolator` legacy band drops `Single Interpolator` + `Single Time` at v ∈ [10.1.0.108, 10.1.0.109]
- **Severity**: MEDIUM
- **Location**: `crates/nif/src/blocks/interpolator.rs:877-1013`
- **Status**: NEW
- **Description**: `NiBlendInterpolator::parse` routes `version <=
  V10_1_0_109` to `parse_legacy(int_priority = true)` and
  `V10_1_0_110..=V10_1_0_111` to `parse_legacy(int_priority = false)`. Only
  the second branch reads `Single Interpolator` (Ref, 4B) + `Single Time`
  (f32, 4B). nif.xml gates both fields on `since="10.1.0.108"
  until="10.1.0.111"` — an 8-byte window that overlaps the *first* branch at
  exactly v10.1.0.108 and v10.1.0.109. The code's own doc-comment even
  records the correct bound but the read is only wired into the 110..111
  arm.
- **Impact**: An 8-byte under-read per `NiBlendInterpolator` on any file at
  exactly v10.1.0.108 or v10.1.0.109 — these bands predate the `block_sizes`
  recovery table, so the drift cascades through the rest of the file (same
  failure shape as the #1301/#1310/#1337/#1508 truncation family). **Blast
  radius on vanilla content is zero** — this audit's fresh version census
  confirms `Oblivion - Meshes.bsa` has no file in the 10.1.0.102–10.1.0.114
  range other than 10.1.0.106. Exposure is limited to third-party Gamebryo /
  Oblivion mod content authored on the 10.1.0.108–109 toolchain.
- **Suggested Fix**: Gate the `Single Interpolator`/`Single Time` pair on
  `version >= V10_1_0_108 && version <= V10_1_0_111` independently of the
  `int_priority` branch selector (read them in both arms when in-band); add
  a `V10_1_0_108` constant to `version.rs`; add a byte-exact regression test
  at v10.1.0.108 alongside the existing #1508 tests.

#### NIF-OBL-D1-02: `ControlledBlock` has no pre-10.1.0.106 layout — three fields mis-gated
- **Severity**: LOW
- **Location**: `crates/nif/src/blocks/controller/sequence.rs:124-227`
- **Status**: NEW
- **Description**: `NiControllerSequence::parse` implements only the
  ≥10.1.0.104 `ControlledBlock` layout. Three nif.xml gates are missing:
  `Target Name` (`until="10.1.0.103"`, never read), `Interpolator`
  (`since="10.1.0.106"`, read unconditionally — an over-read below that
  version), and `Priority` (`since="10.1.0.106" vercond="#BSSTREAM#"`, gated
  only on `bsver > 0` with the `since` half missing). The inherited
  `NiSequence` fields `Accum Root Name`/`Text Keys` (`until="10.1.0.103"`)
  are likewise absent.
- **Impact**: Any `NiSequence`/`NiControllerSequence` below v10.1.0.106
  mis-advances the stream in a band with no recovery anchor. Empirically
  unreached on vanilla content: Oblivion's 23 v10.0.1.2 + 8 v10.1.0.101
  files (the only sub-10.1.0.106 content with `bsver > 0`) all parse clean
  in the fresh corpus run, so vanilla Oblivion does not put controller
  sequences in those bands. Exposure is mod/non-Bethesda Gamebryo content.
- **Suggested Fix**: Add the three version gates plus the `NiSequence`
  `until=10.1.0.103` prologue pair, with a synthetic byte-exact test at
  v10.1.0.101/bsver=4.

#### NIF-OBL-D1-03: `#2193` inverted-collision-normal premise unsupported; real asymmetry is a unit mismatch
- **Severity**: LOW (informational contribution to an open issue)
- **Location**: `crates/nif/src/import/collision/shape.rs:361-407`,
  `crates/nif/src/import/collision/mod.rs:265-284,430-432`
- **Status**: Existing: #2193 (OPEN, HIGH)
- **Description**: Investigated whether the NiTriStrips-based Oblivion
  collision path could systematically invert a normal, per #2193's own
  suspicion. It cannot, as written: `havok_to_engine` delegates to
  `zup_to_yup_pos`, a determinant **+1** basis change (pinned by
  `havok_to_engine_matches_coord_sot`) that preserves handedness/winding; and
  since #2193's partial fix, the collision path de-strips through the exact
  same `NiTriStripsData::to_triangles()` call the render path uses, so
  collision and render winding agree by construction. The real asymmetry
  found in this code is dimensional, not orientational: `extract_from_classic`
  scales the rigid body's translation by `scene.havok_scale` (7.0 for
  Oblivion) while `resolve_tri_strips_data_refs` deliberately leaves the
  shape's vertices unscaled (#1744, since `NiTriStripsData` is already in
  game units) — for a `bhkRigidBodyT` with non-zero translation, the shape
  and its offset end up expressed in different unit systems.
- **Impact**: Redirects #2193's investigation away from "flip the winding"
  (which would introduce a *real* mesh/collision divergence where none
  exists today) toward the translation-vs-vertex-scale mismatch as the
  concrete candidate mechanism.
- **Suggested Fix**: (For #2193's investigator.) Instrument
  `extract_from_classic` to log `body.translation` magnitude for
  `bhkNiTriStripsShape`-rooted bodies in `ICMarketDistrictTheGildedCarafe`;
  if non-zero, drop the `× havok_scale` on the translation for
  unscaled-vertex shape families (or scale the shape instead) so both sides
  of the Compound share a unit system. Do not touch the winding.

**Verification**: `cargo test -p byroredux-nif` — 0 failures across the full
suite; fresh `nif_stats` sweep over `Oblivion - Meshes.bsa` reproduces the
post-#1611 baseline exactly (8,026/8,032 clean, 6 known marker truncations,
0 failures, 0 partial-unknown types).

### Dimension 2 — BSA v103 Archive
**0 findings — regression guard confirmed clean.**

- `BSA_V_OBLIVION = 103` recognized (`mod.rs:32-36`); rejection outside
  `{103, 104, 105}` with a descriptive error (`open.rs:40-48`).
- Folder-record size: `if version == BSA_V_SKYRIM_SE { 24 } else { 16 }`
  (`open.rs:100`) — v103 **and** v104 both 16 bytes, only v105 is 24. The
  "v104 = 24B" claim referenced in some prior audit framing is confirmed
  **false and absent** from the current tree — not perpetuated.
- `embed_file_names` gated `version >= BSA_V_FO3_SKYRIM` (`open.rs:75`), so
  v103's "Xbox archive" bit (0x100) can never be misread as embed-names —
  confirmed both by code inspection and by the fresh sweep succeeding on
  archives that set that bit.
- `genhash_folder`/`genhash_file` (`hash.rs`) implement the correct
  Bethesda pack+rolling-multiply hash shape; debug/test-build gated only, no
  release-path impact.
- Fresh full-archive sweep: **147,629/147,629 files extracted with zero
  errors** across all 17 vanilla Oblivion BSAs — exact match to the #699
  baseline, no regression.
- **Verification**: `cargo test -p byroredux-bsa` (53 passed) +
  `--ignored` real-data suite (4 passed, incl. v103/v104/v105 cross-version
  tests).

One test-coverage observation (not filed as a finding): Oblivion v103 lacks
a dedicated brute-force zero-errors sweep test in `cargo test` (only Skyrim
SE v105 has one) — the 147,629-file sweep is exercised ad hoc, not pinned.
Nice-to-have parity item for a future `audit-tech-debt` pass, not a
correctness risk (the extraction code path is shared with the pinned v105
test's branches).

### Dimension 3 — ESM Record Coverage (live path, not a stub)
**0 new bugs. 1 carried-forward LOW (pre-existing, closed-but-relevant).**

- TES4 header/GRUP dispatch (`reader.rs:51-65`) — **CONFIRMED**, offset-20
  `HEDR` vs offset-24 FO3+ detection, variant-aware `group_content_end`.
- `flags_oblivion`/`is_oblivion` branches — **CONFIRMED** correct, not
  regressed.
- MGEF-by-code map (`records/index.rs:132-144`,
  `dispatch_misc_gameplay_b.rs:30-48`) — **CONFIRMED** implemented and
  gated to `GameKind::Oblivion` (fixed since a prior 2026-05-11 sweep flagged
  it open).
- CONT 4-byte-payload guard (`container.rs:142-148`) — **CONFIRMED**,
  dedicated test passes.
- CLMT 3-entry WLST (`climate.rs:53-94,190-221`) — **CONFIRMED**, per-game
  dispatch (not size-autodetect), dedicated test passes.
- 16-byte ACBS guard (`#1650`, `actor/mod.rs:713-730`) — **CONFIRMED**, the
  `GameKind::Oblivion` arm (`len >= 16`) sits before the wider FNV/FO4 arms
  and is exclusive on `game`; all 4 dedicated tests pass.
- Real-data parity tests (`clas_oblivion_knight_against_vanilla`,
  `race_oblivion_data_and_subs_against_vanilla`) — **re-run green** against
  real `Oblivion.esm` (111 classes, 15 races, Knight's primaries verified).
- CELL walker (`cell/walkers.rs:13-97`) — **CONFIRMED**, canonical XCLL size
  set `[28, 32, 36]`, warns (not silently accepts) on off-canonical sizes;
  RCLR now parsed on both interior and exterior paths.
- DIAL/INFO — **CONFIRMED** Oblivion's 1-byte DATA / TRDT split handled
  correctly, cross-game walker unaffected.
- Minimum exterior-REFR record chain (item 10): WRLD → CLMT → exterior CELL
  → REFR → base-object → LAND → (LTEX/WATR) — all seven already wired
  end-to-end for Oblivion; this is a dependency-chain description, not a
  gap list.

One item carried forward, not a regression: `flags_oblivion` and sibling
CLAS fields (`specialization`/`major_skills`/`primary_attributes`) parse
correctly and are real-data-verified but have **zero production consumers**
(Existing: #2089, closed COMPLETED — the closure documented the gap as
intentional pending CHARAL's Oblivion ruleset, it did not add a consumer).
Flagged only because "COMPLETED" could misleadingly read as "fixed" rather
than "documented as deferred."

**Verification**: `cargo test -p byroredux-plugin` — 631 passed, 0 failed,
13 ignored. Real-data parity: 2 passed against vanilla `Oblivion.esm`.

### Dimension 4 — Rendering Path for Oblivion Shaders
**0 findings — all 8 checklist items confirmed correct against live source.**

- **`NiTexturingProperty` → `MaterialInfo` pipeline** — **CONFIRMED**
  (`crates/nif/src/import/material/legacy_properties.rs::apply_texturing_property`).
  Base slot 0, dark/lightmap slot (#264), normal-from-bump fallback
  (`normal_texture.or_else(bump_texture)`, #131 — Oblivion has no dedicated
  normal slot, only `bump_texture`), detail, glow, and gloss slots all
  mapped, plus decal slots, UV transform, and clamp-mode propagation
  (#219/#435/#610).
- **`NiMaterialProperty` legacy color stays raw monitor-space** —
  **CONFIRMED**. Repo-wide grep for `srgb_to_linear`/`to_linear` across
  `crates/nif/src/import/material/`, `byroredux/src/material_translate.rs`,
  and every shader/include file returns zero hits anywhere near legacy
  material-color handling — no regression of `0e8efc6`.
- **`NiAlphaProperty` blend-factor routing** — **CONFIRMED**, all 11
  Gamebryo `AlphaFunction` values (0=ONE .. 10=SRC_ALPHA_SATURATE) mapped
  in `gamebryo_to_vk_blend_factor` (`crates/renderer/src/vulkan/pipeline.rs:166-180`),
  out-of-range defensively falls back to `SRC_ALPHA`; regression test
  `gamebryo_to_vk_blend_factor_covers_all_11_values` (#392) passes.
- **`NiStencilProperty`/`NiZBufferProperty`/`NiVertexColorProperty`/
  `NiSpecularProperty`/`NiWireframeProperty`/`NiDitherProperty`/
  `NiShadeProperty`** — **CONFIRMED**, all handled deliberately:
  `NiStencilProperty` promotes two-sided + captures full stencil state
  (`apply_stencil_property`); `NiZBufferProperty` captures z-test/write/
  function (`apply_zbuffer_property`); `NiVertexColorProperty` drives
  `VertexColorMode` (`apply_vertex_color_property`); the shared
  `NiFlagProperty` helper (`apply_flag_property`) distinguishes
  `NiSpecularProperty` (disables specular, #220), `NiWireframeProperty`
  (`#869`: sets `info.wireframe`, consumed by
  `byroredux/src/render/static_meshes.rs:591` into `PipelineKey::{Opaque,
  Blended}{wireframe}` → `vk::PolygonMode::LINE`), and `NiShadeProperty`
  (`#869`: sets `info.flat_shading`, consumed at `static_meshes.rs:595` →
  `INSTANCE_FLAG_FLAT_SHADING` (bit 7) → read live in both
  `crates/renderer/shaders/triangle.frag:151` (raster path) and
  `crates/renderer/shaders/include/ray_hit.glsl:154` (RT reflection/GI hit
  path) — confirmed genuinely consumed, not dropped after tagging).
  `NiDitherProperty` is deliberately ignored (documented: "legacy hint with
  no Vulkan analogue, safe to ignore") — not a translatable-block-dropped
  finding, since dithering has no destination in this pipeline.
- **Vertex-color/material-color interaction** — **CONFIRMED** correct.
  `diffuse_color` (from `NiMaterialProperty`) is the documented fallback
  when `vertex_color_mode == Ignore` or the mesh has no vertex-color array;
  the shader multiplies `texColor.rgb * fragColor.rgb` for the lit path and
  routes to emissive via `MAT_FLAG_VERTEX_COLOR_EMISSIVE` when
  `vertex_color_mode == Emissive` — matches the documented split in
  `crates/nif/src/import/material/mod.rs:530-533`.
- **`#1239` `NiPSysEmitter` version gating** — **CONFIRMED still correct**
  (`crates/nif/src/blocks/particle.rs:69-132`, `read_emitter_base`). Only
  `Radius Variation` is gated `>= V10_4_0_1`; `Life Span Variation` is
  unconditional (the `#1507` follow-up fix, cross-references Dimension 1's
  regression-guard item 5c). Oblivion's v10.x sub-versions correctly read
  `Life Span Variation` without the erroneous pre-#1239/#1507 bundling that
  once dropped 4-8 bytes on 219 Oblivion NIFs.
- **Typed emitter parse → ECS → render (not parse-then-drop)** —
  **CONFIRMED**. Traced the full chain: `extract_emitter_params`/
  `extract_emitter_rate` (`crates/nif/src/import/walk/mod.rs:721,820`)
  populate `ImportedParticleEmitter`/`ImportedParticleEmitterFlat`, consumed
  at **both** spawn sites —
  `byroredux/src/scene/nif_loader.rs:513` and
  `byroredux/src/cell_loader/spawn.rs:642` — which both call
  `crate::systems::apply_emitter_overlays` (`byroredux/src/systems/particle.rs:64`),
  which calls `apply_emitter_params` to overlay the authored
  kinematic/lifetime/size values onto the ECS `ParticleEmitter` preset. An
  Oblivion emitter that parses reaches a live, animating ECS component on
  both load paths, not just a parsed-and-discarded intermediate.
- **Disney-BSDF gate (`MAT_FLAG_PBR_BSDF`) stays 0 for Oblivion** —
  **CONFIRMED independently** (cross-references Dimension 5's identical
  conclusion). `material.is_pbr = true` has exactly 3 production set-sites,
  all in `byroredux/src/asset_provider/material.rs` (FO4 BGSM-authored-pbr
  flag, Starfield `.mat`+CDB, BGEM merge) — none reachable from any
  Oblivion `NiTexturingProperty`/`NiMaterialProperty` NIF-import content,
  which never populates `root_material_path`/`.bgsm`/`.bgem`. The Disney
  lobe in `pbr.glsl` stays unreachable for the entire Oblivion material
  universe.

**Verification**: `cargo test -p byroredux-nif --lib import::material` —
154 passed, 0 failed. `cargo test -p byroredux-renderer --lib
gamebryo_to_vk_blend_factor` — 1 passed (the #392 all-11-values regression
test).

### Dimension 5 — NIFAL Canonical Material Translation for Oblivion
**0 findings — all 3 checklist items PASS. 1 LOW documentation-only finding.**

Traced the full Oblivion chain: NiProperty chain → `extract_material_info_
from_refs` → `MaterialInfo` → `into_imported_material` (`classify_legacy_pbr`
sets `metalness_override`/`roughness_override`) → `ImportedMaterial` →
`translate_material` (the single boundary) → `Material::resolve_pbr` (clamp
only, NaN arm not reached for NIF content) → `classify_glass_into_material`
→ canonical ECS `Material` → `static_meshes.rs` reads `m.roughness`/
`m.metalness` verbatim.

- **PBR sentinel resolved exactly once** — **PASS**. No per-draw
  `classify_pbr` survives anywhere (`Material::classify_pbr` is gone,
  referenced only in prose); `static_meshes.rs:300-322` reads canonical
  scalars directly with no keyword scan.
- **`EmissiveSource::Material` on the Oblivion legacy arm** — **PASS**.
  Set-site `legacy_properties.rs:89-108` (`apply_material_property`),
  guarded by `!info.has_material_data` so a Skyrim+ mesh is never demoted;
  distinct, tested arm from the BSLighting/BSEffect paths.
- **`MAT_FLAG_PBR_BSDF` stays 0 for Oblivion** — **PASS** (independently
  verified; cross-references Dimension 4). Single production set-site
  (`cell_loader.rs:229-231`) gated on `ImportedMaterial.is_pbr`, which the
  NIF importer writes `false` unconditionally and only external BGSM/BGEM/
  Starfield `.mat` paths flip `true` — none reachable from Oblivion content.

#### OBL-D5-01: `emissive_source_tests.rs` module doc still points at pre-#2059-split line numbers in `walker.rs`
- **Severity**: LOW (documentation/audit-navigation only)
- **Location**: `crates/nif/src/import/material/emissive_source_tests.rs:1-12`
- **Status**: NEW
- **Description**: The module header cites `walker.rs:~292/~347/~578` for
  the three `EmissiveSource` set-sites. Post-#2059, `walker.rs` is a 157-line
  orchestrator containing none of those arms — BSLighting/BSEffect moved to
  `dedicated_shader.rs`, `NiMaterialProperty` to `legacy_properties.rs:89-108`.
- **Impact**: Audit/navigation friction only (this audit's own dimension
  brief inherited the stale pointer from the same source). No runtime
  effect.
- **Suggested Fix**: Repoint the header table to
  `dedicated_shader.rs::apply_dedicated_shader_property` and
  `legacy_properties.rs::apply_material_property` by function name (survives
  future file splits), not line number.

Non-findings verified deliberately not filed: `NiMaterialProperty.shininess`
correctly does not drive roughness absent a gloss-map slot (documented
"wet floor" fix behavior, not divergence); `env_map_scale` defaults to `0.0`
so the metalness env-map arm never spuriously fires on plain Oblivion
content; `NiSpecularProperty`-disabled zeroing happens before
`classify_legacy_pbr` runs (correct ordering); `NiFogProperty` non-dispatch
is intentional (1 vanilla block measured).

**Verification**: `cargo test -p byroredux-core --lib material` (24 passed),
`cargo test -p byroredux-nif --lib material::` (154 passed), `cargo test -p
byroredux --bin byroredux material_translate` (8 passed).

### Dimension 6 — Real-Data Validation
**0 findings requiring action — 1 LOW cosmetic observation. Baseline matches
exactly, no drift.**

- `nif_stats` full sweep over `Oblivion - Meshes.bsa` (8,032 files): **8,026
  clean (99.93%), 6 truncated (38 blocks dropped), 0 failures, 0
  recovered/unknown types**, 81 distinct block types all with `unknown=0`.
  Byte-identical to `crates/nif/tests/data/per_block_baselines/oblivion.tsv`
  on every data row. Matches `ROADMAP.md`'s Oblivion row verbatim.
- `recovery_trace`: the 6 residual truncated files
  (`marker_arrow`/`divine`/`map`/`radius`/`temple`/`travel`) exactly match
  the checked-in `oblivion_truncations.tsv` baseline (#1611), same per-file
  dropped-block counts, 0 hard failures (confirming `marker_radius.nif`'s
  truncate-not-error behavior, #698, still holds).
- Block-type histogram: no new/unexpected types since the last sweep.
- Three representative interior meshes traced through `import_nif_scene`:
  `lights\chandelier01.nif` (39 blocks, 5 meshes), `clutter\books\
  octavo01.nif` (18 blocks, 2 meshes), `creatures\goblin\goblinhead.nif`
  (26 blocks, 1 mesh) — all parsed clean, non-truncated, every submesh's
  material chain resolved a valid base-color texture; `is_pbr=false`
  throughout as expected.

#### OBL-D6-01: `nif_stats --tsv` header line drifted from the test-harness `to_tsv` (cosmetic)
- **Severity**: LOW
- **Status**: Confirmed, informational — zero functional impact (both
  implementations agree on every one of 81 data rows; only the `#`-prefixed
  header comment differs, which both parsers skip).
- **Suggested Fix**: Have `nif_stats.rs`'s `--tsv` path reuse
  `PerBlockHistogram::to_tsv` instead of maintaining a second header format.

**Verification**: fresh live `nif_stats`/`recovery_trace` runs (release
build) against real vanilla Oblivion data; no `cargo test` regressions.

### Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks
**1 finding (LOW, documentation). All 6 other checklist items confirmed
clean, several reconfirming fixes from prior sessions.**

- `--bsa` CLI path — **CONFIRMED** game-agnostic end-to-end (auto-detects
  BSA/BA2 by magic, no per-game branch; Oblivion's non-suffixed archive
  names correctly skip the numeric-sibling probe).
- No Oblivion-specific record types missing from the cell loader's REFR
  placement surface beyond the FNV-aligned baseline — confirmed by zero
  `GameKind::Oblivion`/`is_oblivion` hits in the spawn path (by design: NIFAL/
  EXAL absorb per-game differences upstream).
- Animation scene-graph name resolution — **CONFIRMED** no live gap;
  `build_subtree_name_map` + `attach_animation_sinks` (fixed under #2221)
  cover both Oblivion's loose-`.kf` path and the embedded-clip path
  identically.
- The alleged pre-v3.3.0.13 "empty NifScene + log" fallback **does not exist
  in the current tree** — searched exhaustively; the only fallback of that
  shape (`NifHeader::detached`) is an unrelated FO4-precombine construct.
  Flagged explicitly as a non-finding so a future audit doesn't re-chase it.
- Legacy particle emitters — **CONFIRMED** non-issue; vanilla Oblivion's 547
  particle systems are 100% modern `NiParticleSystem` stack, 100%
  renderer-routed (legacy pre-NiPSys stack confirmed dead code, removed
  under #1327).
- `_far.nif` distant-object LOD (#1726/#1745) — **CONFIRMED** working,
  10/10 `placement_lod` tests pass including real-data placement-file
  parsing.

#### OBL-D7-01: `README.md` still frames Oblivion exterior as wiring-gated, contradicting `ROADMAP.md`
- **Severity**: LOW
- **Location**: `README.md:129-130`
- **Status**: NEW
- **Description**: README reads (present tense) "Oblivion exterior gated on
  TES4 worldspace + LAND wiring" — implying the wiring is still the blocker.
  `ROADMAP.md` (more recently touched) is explicit the wiring is done and
  only the on-device render bench remains. This dimension's own
  re-verification (items above) reconfirms the wiring is real and
  functional.
- **Impact**: Documentation friction only — risk of a future contributor or
  audit re-opening "wiring missing" as if it were still live, duplicating
  already-confirmed work.
- **Suggested Fix**: Reword `README.md:129-130` to match `ROADMAP.md`'s
  framing: "Oblivion exterior: worldspace/LAND wiring implemented, on-device
  render bench pending."

## Blocker Chain

Interiors already render end-to-end (Anvil Heinrich Oaken Halls, unchanged).
The remaining chain to "Oblivion exterior renders on screen":

1. **TES4 worldspace + LAND wiring** — implemented and game-agnostic
   (re-confirmed this session across Dimensions 3 and 7; no
   `GameKind::Oblivion` special-casing needed beyond the shared CELL/WRLD
   walkers).
2. **CELL exterior REFR placement** — no Oblivion-specific record gap found;
   shares the FNV-aligned baseline (Dimension 3 item 10, Dimension 7 item 2).
3. **On-device exterior render bench** — infrastructure exists (same shape
   as the completed FO3/Skyrim SE/FO4 benches); execution is the sole
   remaining step. Suggested invocation:
   `cargo run --release -- --esm Oblivion.esm --grid <x,y> --bsa "Oblivion - Meshes.bsa" --textures-bsa "Oblivion - Textures - Compressed.bsa" --bench-frames 300 --bench-hold`
4. **Triage whatever the bench surfaces** — candidates already visible in
   open issues: #2193 (grounding/inverted-normal-or-scale bug, so far
   confirmed only on an interior cell; an exterior bench would be the first
   real test against outdoor terrain collision) and #2215 (RT indirect
   draw-call grouping regression — a perf confound for the bench's FPS
   number, not a correctness blocker).
5. **Refresh `ROADMAP.md`'s Oblivion row + fix `README.md:129-130`**
   (OBL-D7-01) once the bench closes.

Do **not** regenerate the stale "BSA v103 is broken" framing (dead since
#699) or the stale "wiring missing" framing (dead since #1556) — both were
re-confirmed dead this session.

## Regression Guard List

All items below were checked against live code + live test runs this
session and confirmed still intact:

- v10.x stride-drift family: `#1506` (`NiInterpController`/`NiQuatTransform`),
  `#1507` (`NiPSysData` + emitter), `#1508` (`NiBlendInterpolator` +
  `ControlledBlock` — **partial**, see NIF-OBL-D1-01 for a newly-found
  gap in an unreached sub-band), `#1509` (`NiGeomMorpherController`
  `bsver > 9` gate).
- `NiTexturingProperty` raw `u32` texture-count read (no `Has Shader
  Textures: bool` gate).
- BSStreamHeader dual-band guard (`#170`).
- `user_version` threshold at `V10_0_1_8`.
- BSA v103 extraction end-to-end (`#699`) — fresh 147,629-file sweep, 0
  errors.
- 16-byte Oblivion ACBS guard ahead of the FNV/FO3 24-byte arm (`#1650`).
- `havok_motion_type` full-enum mapping, no `4 => Keyframed`/`_ => Static`
  collapse (`#1652`).
- `BhkMultiSphereShape` / `BhkConvexListShape` → `CollisionShape` resolution.
- Disney-BSDF gate (`MAT_FLAG_PBR_BSDF`) stays 0 across the entire Oblivion
  material universe.
- `#1239` `NiPSysEmitter` version gating + full emitter-to-ECS runtime
  hookup.
- `#1611` residual truncation baseline (6 NetImmerse markers, 0 hard
  failures) — byte-identical to the checked-in TSV.
- `placement_lod_supported` stays `GameKind::Oblivion`-only (`_far.nif` LOD,
  `#1726`/`#1745`/`#2086`).
- TES4 worldspace + LAND wiring (`#1556`) — implemented, game-agnostic.
- Animation scene-graph name resolution (`#2221`) — intact for both `.kf`
  and embedded-clip paths.
- Legacy particle non-routing (`#1327`) — intact, confirmed dead for real
  Oblivion content.

## Finding Count Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 0 new (1 pre-existing open, #2193, receives new negative evidence this session) |
| MEDIUM   | 1     |
| LOW      | 5     |

**Total new findings this session: 6** (NIF-OBL-D1-01 MEDIUM,
NIF-OBL-D1-02 LOW, NIF-OBL-D1-03 LOW/informational to #2193, OBL-D5-01 LOW,
OBL-D6-01 LOW, OBL-D7-01 LOW). All 7 dimensions are complete; Dimension 4
(rendering path for Oblivion shaders) was independently verified against
live source and contributed 0 new findings — every checklist item
(texture-slot mapping, legacy color space, alpha blend-factor routing, the
`#869` wireframe/flat-shading guards, vertex-color/material-color
interaction, `#1239` emitter version gating, the emitter parse→ECS→render
hookup, and the Disney-BSDF gate) checked out clean.

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-08-03.md`
