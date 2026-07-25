# Skyrim SE Compatibility Audit — 2026-07-25

Run as one leg of a `comprehensive` audit-suite sweep. Executed directly,
single session, no sub-agent delegation. Repo: `/mnt/data/src/gamebyro-redux`,
HEAD `ca7a4e0e`.

## Executive Summary

Skyrim SE is the engine's renderer **control bench** (Whiterun BanneredMare,
6 named equipped NPCs) — both loose-mesh and cell rendering already work.
This audit is regression coverage over the seven highest-risk Skyrim-specific
surfaces: BSTriShape packed geometry + SSE skinned reconstruction,
BSLightingShaderProperty/BSEffectShaderProperty shader-type dispatch, NPC
equip/FaceGen, multi-master load order, BSA v105 (LZ4), specialty blocks, and
the NIFAL canonical material boundary.

**Result: every regression guard checked is intact. Zero new CRITICAL/HIGH/
MEDIUM findings.** One new LOW finding (stale path reference inside the audit
skill itself, not production code). All seven dimensions verified with a mix
of direct code reads, `nif.xml` cross-reference, targeted `cargo test`
invocations (including `--ignored` real-game-data tests), a live headless
engine run against the actual Skyrim SE installation, and a full
`cargo test --workspace --lib` sweep (0 failures).

One pre-existing, already-tracked issue materially affects the Skyrim control
bench and is called out under Cell-Load Regression Status below:
**PERF-REGRESSION-6c56e311** (HIGH, ROADMAP Known Issues, not yet a filed GH
issue) — a ~2.2–3× frame-time regression in the main geometry fragment shader
that predates this audit and is not Skyrim-specific (it also hits Prospector/
FO4 MedTek). Re-confirmed live during this audit's Dimension 4 smoke run; not
re-filed as a new finding.

## Verification Method

- Read and cross-checked parser/import/render code against
  `/mnt/data/src/reference/nifxml/nif.xml` (authoritative format spec) for the
  shader-type numeric mapping.
- Ran targeted `cargo test` invocations per dimension (unit + `--ignored`
  real-archive/real-ESM integration tests) plus a full
  `cargo test --workspace --lib` (0 failures, no `FAILED` lines).
- Drove the real, already-built release engine headless against the on-disk
  Skyrim SE install (`docs/smoke-tests/m41-equip.sh skyrim`) — no other
  engine instance was running, so this did not conflict with a user session.
- Re-ran `crates/nif/examples/nif_stats` against `Skyrim - Meshes0.bsa`
  (18,862 files) with `RUST_LOG=warn` to confirm the 100%-clean / zero-WARN
  baseline live, not just by ROADMAP transcription.
- Deduplicated against `gh issue list` (29 open issues fetched) — none
  overlap this audit's findings.

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction

All checklist items **PASS**:

- `VF_*` flag bits (`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:194-243`)
  match `nif.xml`'s `BSVertexDesc.VertexAttribute` bitfield. Half-float decode
  (`crates/nif/src/import/mesh/decode.rs::half_to_f32`) is pinned against
  IEEE-754 binary16 edge classes (±0, denormals, ±Inf, NaN-with-payload,
  smallest/largest normal) in
  `crates/nif/src/import/mesh/decode_half_float_tests.rs` — all pass.
- `extract_bs_tri_shape` / `_local`
  (`crates/nif/src/import/mesh/bs_tri_shape.rs`) handles every flag
  combination; triangle indices are u16 per the packed-format contract
  (`stream.read_u16_triple_array`); skinned `bone_indices`/`bone_weights`
  flow through `renormalize_skin_weights` matching the SSE-buffer twin.
- **SSE skinned-geometry reconstruction tangent path**
  (`crates/nif/src/import/mesh/sse_recon.rs::decode_sse_packed_buffer`):
  positions/normals are Z-up→Y-up converted via the canonical
  `byroredux_core::math::coord::zup_to_yup_pos` helper, and the on-disk
  "bitangent" triplet (`bitangent_x`/`_y`/`_z`) is reassembled and routed as
  the Y-up tangent slot (∂P/∂U) exactly as the regression guard requires —
  confirmed by reading the full reconstruction loop end-to-end. 7/7 tests in
  `sse_skin_geometry_reconstruction_tests.rs` pass, including the dedicated
  "tangents without normals keeps stride aligned" collapse-gap regression
  test (#1559).
- **Alpha-property cascade** gated on `alpha_property_consumed`: intact.
  `apply_alpha_flags` (`crates/nif/src/import/material/mod.rs:1146`) sets the
  flag unconditionally on consumption; the two `!info.alpha_property_consumed`
  gate sites are in `crates/nif/src/import/material/dedicated_shader.rs:488`
  (Skyrim+ dedicated-ref implicit-blend write) and
  `crates/nif/src/import/material/legacy_properties.rs:65` (legacy
  NiAlphaProperty cascade) — logic verified correct, 14/14
  `alpha_flag_tests.rs` pass. **See LOW-1 below**: the SKILL.md text still
  names `walker.rs` as the gate-site location; the code moved during a
  module split and the skill wasn't updated.

### Dimension 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch

All checklist items **PASS**, cross-checked directly against
`/mnt/data/src/reference/nifxml/nif.xml`:

- `parse_shader_type_data` (Skyrim, BSVER < 130,
  `crates/nif/src/blocks/shader.rs:1309`) dispatches type 1→EnvironmentMap,
  5→SkinTint(Color3), 6→HairTint, 7→ParallaxOcc, 11→MultiLayerParallax,
  14→SparkleSnow, 16→EyeEnvmap — this is a byte-for-byte match against
  `nif.xml:6619-6636`'s `BSLightingShaderType` `cond="Shader Type == N"`
  gates. Types 0,2,3,4,8,9,10,12,13,15,17,18,19,20 correctly fall through to
  `ShaderTypeData::None` (14 no-trailing-data types + 7 with-data types = the
  full 21-value enum).
  - Skyrim-specific field count matches: `parse_skyrim`
    (`crates/nif/src/blocks/shader.rs:885`) is a bit-for-bit reproduction of
    the documented BSVER 83-129 layout (u32 shader flags, no CRC32,
    `lighting_effect_1/2` present, no `root_material_path`/wetness/FO76 tail).
  - FO76's distinct `BSShaderType155` numeric mapping (4=Fo76SkinTint Color4,
    5=HairTint Color3) is verified against `nif.xml:1423-1431` and does not
    share code paths with the Skyrim/FO4 arm — no cross-contamination.
    `blocks::shader::tests::fo76::*` (18 tests) and
    `shader_type_data_tests::fo76_skin_tint_*` all pass.
  - 44/44 tests in `crates/nif/src/blocks/shader_tests/` pass (per-era
    parsers: skyrim, fo4, fo76, starfield, legacy) plus 18/18
    `shader_type_data_tests.rs` plus 6/6 `sky_water_shader_tests.rs`.
- Flag-bit vocabularies (`crates/nif/src/shader_flags.rs`) are documented per
  game with explicit cross-game "do NOT reuse this bit" warnings
  (`skyrim_slsf2::ANISOTROPIC_LIGHTING` vs FO3/FNV `Alpha_Decal` at the same
  bit position, etc.) — Decal/Dynamic_Decal/ZBuffer_Test align at bits
  26/27/31 across all three vocabularies per the `fo4_slsf1` doc comment; no
  drift found.
- `BSEffectShaderProperty` fields (`soft_falloff_depth`, `greyscale_texture`,
  `lighting_influence`, `env_map_min_lod`) all parsed
  (`crates/nif/src/blocks/shader.rs:1730-1836`); 5/5
  `lighting_shader_pbr_tests.rs` pass (#1241 PBR scalars).
- **Disney/Burley lobe pin**: `MAT_FLAG_PBR_BSDF` (renderer
  `material_flag::PBR_BSDF`) is only ever set via `merge_bgsm_into_mesh`
  (`byroredux/src/asset_provider/material.rs:644`), gated on
  `mesh.material_path` being `Some` — which only happens when the NIF shader
  property references an external `.bgsm`/`.bgem`/`.mat` file. Vanilla Skyrim
  SE ships no such files (BGSM/BGEM is FO4+); `extract_bs_tri_shape`
  (Skyrim's own extraction path) hardcodes `is_pbr: false` unconditionally
  (`crates/nif/src/import/mesh/bs_tri_shape.rs:241`). The flag is
  structurally unreachable for vanilla Skyrim.esm content — confirmed by
  code-path analysis, not sampling (the gate makes a false positive
  impossible without an external material file on disk).

### Dimension 3 — NPC Equip + FaceGen (M41)

All checklist items **PASS**, **live-verified against real game data**:

- Ran `docs/smoke-tests/m41-equip.sh skyrim` against the actual Skyrim SE
  Whiterun BanneredMare cell (no other engine instance was active). Result:
  **PASS on all hard assertions** — `entities=3406`, `draws=1505`,
  `Inventory=46 entities`, `EquipmentSlots=6 entities`, `tex.missing=0`.
  `byro-dbg`'s `find EquipmentSlots` confirmed exactly the 6 named NPCs the
  checklist names: `saadia`, `brenuin`, `mikael`, `sinmir`,
  `amaundmotierreend`, `hulda`.
- `resolve_armor_mesh` (`crates/plugin/src/equip.rs:122`) implements the
  documented two-pass ARMO→ARMA→worn-mesh resolution (race-match first,
  first-non-empty fallback second) exactly as specced.
  `armor_covers_main_body` pre-scan skips `upperbody.nif`
  (`byroredux/src/npc_spawn.rs:871`) to avoid z-fight/double-palette cost.
- `expand_leveled_form_id` LVLI flattening (`crates/plugin/src/equip.rs:304`)
  is gated on `LVLI_MAX_DEPTH = 8` with single-pick/multi-pick semantics
  intact.
- `BsDismemberSkinInstance` resolves correctly on both the inline skin path
  (`crates/nif/src/import/mesh/skin.rs:40,141,326,402`) and the SSE
  reconstruction path (`crates/nif/src/import/mesh/sse_recon.rs:76`).
- FaceGen: `BSDynamicTriShape` parse (`parse_dynamic`,
  `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:515`) correctly handles
  the `data_size == 0` / trailing-Vector4-array case per #157/#341 — parse,
  not pixel-match, is the expected fidelity bar and that's what's delivered.

### Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression

All checklist items **PASS**:

- ESL/light-master FormID decode (#1554) intact:
  `0xFE00_0000 | ((sub & 0x0FFF) << 12) | (raw & 0x0FFF)`
  (`crates/plugin/src/esm/reader.rs:303`), driven by the TES4 `0x0200` flag
  (`reader.rs:720-723`); regression tests
  (`read_file_header_reads_localized_and_light_master_flags`) pass.
- Deleted-REFR tombstone skip (#1660,
  `crates/plugin/src/esm/cell/walkers.rs:62,809`,
  `RECORD_FLAG_DELETED = 0x0000_0020`) intact; the `mod.rs` doc comment
  (`crates/plugin/src/esm/cell/mod.rs:959`) correctly describes the tombstone
  skip with no stale "not captured" language (#1781 regression watch — still
  clean).
- **`.STRINGS` loader wiring**: `install_strings_guard`
  (`byroredux/src/cell_loader/load_order.rs:103`) is called inside the
  per-plugin loop in `parse_record_indexes_in_load_order`
  (`load_order.rs:161`) — every plugin in the load order gets its own guard
  installed and dropped before the next plugin, not just the last `--esm`.
  Verified by reading the loop structure directly.
- `parse_real_skyrim_esm` (`crates/plugin/src/esm/cell/tests/integration.rs:254`)
  passes against the real `Skyrim.esm` (`cargo test --lib -- --ignored`),
  confirming `SolitudeWinkingSkeever` resolves through the unified walker.
- Minimum interior-render record set (CELL, REFR, STAT, LIGH, WEAP, ARMO,
  LAND, LTEX, TXST, ADDN) and the out-of-scope-but-must-parse set (NAVM,
  HDPT) all dispatch without error — confirmed via `record.rs` FourCC table
  + `esm/records/dispatch_misc_gameplay_a.rs` + `esm/cell/walkers.rs`.
- **Control-bench guard**: the live smoke run above measured
  `entities=3406`, matching the most recent ROADMAP FSR-matrix figure
  (`e153b50c`, 3406 ent) exactly — entity count is flat, so there is no new
  collider-gate or record-parsing regression on the control bench. FPS in
  that run (67.9 wall / `gpu_main_render`-dominated) is depressed, but that
  is the pre-existing, already-tracked **PERF-REGRESSION-6c56e311** (ROADMAP
  Known Issues, HIGH, root-caused to `triangle.frag` shadow-ray + GI-path
  cost added in commit `6c56e311`), which also hits Prospector and FO4
  MedTek — it is not new, not Skyrim-specific, and not re-filed here.

### Dimension 5 — BSA v105 (LZ4)

All checklist items **PASS**, **live-verified against real archives**:

- `cargo test -p byroredux-bsa --lib -- --ignored` (11/11 pass) against the
  actual `Skyrim - Meshes0.bsa` / `Meshes1.bsa` / `Textures0.bsa`:
  `skyrim_meshes0_opens_and_counts_match_baseline`,
  `skyrim_meshes1_dlc_overflow_opens_and_counts_match_baseline`,
  `skyrim_textures0_opens_and_first_dds_decodes`,
  `skyrim_meshes0_extracts_sweetroll_with_exact_size` all green.
- Zero-based sibling auto-load (`open_with_numeric_siblings` /
  `numeric_sibling_paths`, `byroredux/src/asset_provider/archive.rs:280-379`)
  correctly special-cases the Skyrim `…0` series start vs the FNV
  no-suffix series vs the Starfield two-digit series vs a mid-series
  explicit member — 8/8 sibling tests pass
  (`siblings_skyrim_zero_start_offers_1_through_9`, etc.).

### Dimension 6 — Specialty Blocks + Real-Data Rendering

All checklist items **PASS**:

- `BSLODTriShape` routes to `NiLodTriShape::parse`
  (`crates/nif/src/blocks/mod.rs:453`), NOT `BsTriShape` — #838 regression
  guard intact. `BSMeshLODTriShape` correctly routes through
  `BsTriShape::parse_meshlod` (`mod.rs:454`).
- `BsLagBoneController` + `BsProceduralLightningController` (#837) have
  dedicated parsers wired at `crates/nif/src/blocks/mod.rs:844,846`.
- M35 `.btr` distant-terrain LOD (`byroredux/src/cell_loader/terrain_lod_btr.rs`)
  — 5/5 tests pass. M-series `.bto` object LOD
  (`byroredux/src/cell_loader/object_lod.rs`) — 4/4 tests pass, including the
  hysteresis-band ring-exclusion regression test.
- VWD full-model culling (#1731) — confirmed still documented as forward
  scope in `object_lod.rs`'s own doc comment (no z-fight possible today by
  construction, since object LOD only loads outside the full-detail ring);
  not a regression, correctly not re-filed.
- **Meshes0 sweep baseline re-verified live**: ran
  `cargo run -p byroredux-nif --example nif_stats --release` against
  `Skyrim - Meshes0.bsa` — **18,862/18,862 clean (100.00%), 0 truncated, 0
  failures, 0 recovered**. Re-ran with `RUST_LOG=warn` piped through the same
  sweep: **zero WARN-level log lines emitted** — the "0 realignment WARNs"
  baseline is confirmed live, not just transcribed from ROADMAP.

### Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice)

All checklist items **PASS**:

- `translate_material` (`byroredux/src/material_translate.rs:73`) remains the
  single canonical boundary; no second translation path found.
- **Ordering invariant verified directly**: `material.resolve_pbr()`
  (`material_translate.rs:164`) executes immediately before
  `crate::helpers::classify_glass_into_material`
  (`material_translate.rs:165`) — forced-glass roughness wins over the
  keyword default as designed. 7/7 `material_translate::tests` pass; 5/5
  `resolve_pbr_*` tests in `crates/core/src/ecs/components/material.rs`
  pass.
- **`EmissiveSource` discriminator (#1280)** verified in code and tests:
  Skyrim `BSLightingShaderProperty` sets
  `EmissiveSource::Lighting` (`dedicated_shader.rs:293`);
  `BSEffectShaderProperty` sets `EmissiveSource::Effect`
  (`dedicated_shader.rs:360`) — 5/5 `emissive_source_tests.rs` pass,
  including the explicit
  `bslighting_tags_emissive_source_as_lighting` /
  `bseffect_tags_emissive_source_as_effect` pair.
- The deleted per-draw `Material::classify_pbr` fallback stays gone — no
  render-time classification path found in `crates/core` or the shaders.

## New Findings

### LOW-1: `.claude/commands/audit-skyrim/SKILL.md` names a stale location for the alpha-cascade gate sites
- **Severity**: LOW
- **Dimension**: 1 (BSTriShape / material import)
- **Location**: `.claude/commands/audit-skyrim/SKILL.md` (Dimension 1 checklist) vs actual code at `crates/nif/src/import/material/dedicated_shader.rs:488` and `crates/nif/src/import/material/legacy_properties.rs:65`
- **Status**: NEW (doc-rot in the audit skill, not in production code)
- **Description**: The skill's Dimension 1 checklist says the two
  `!info.alpha_property_consumed` gate sites are "consulted at the two gate
  sites in `crates/nif/src/import/material/walker.rs`". A module split moved
  this logic into `dedicated_shader.rs` (Skyrim+ dedicated-ref path) and
  `legacy_properties.rs` (legacy NiAlphaProperty cascade) — `walker.rs` no
  longer contains either gate. The underlying logic itself is verified
  correct (see Dimension 1 findings); only the audit-skill's path reference
  is stale.
- **Evidence**: `grep -n "alpha_property_consumed" crates/nif/src/import/material/walker.rs` returns only a stale comment referencing the field, not a gate; the two live `if !info.alpha_property_consumed` sites are in the two files named above.
- **Impact**: A future audit following the skill's literal instructions would search the wrong file and could wrongly conclude the guard regressed. No runtime/parse impact — this is audit-infrastructure hygiene only.
- **Related**: None (first time this drift is flagged).
- **Suggested Fix**: Update the Dimension 1 entry point list in `audit-skyrim/SKILL.md` to cite `dedicated_shader.rs` and `legacy_properties.rs` instead of `walker.rs`, matching the path-reference convention in `_audit-common.md`.

No other new findings. No CRITICAL, HIGH, or MEDIUM findings this cycle.

## Shader-Type Coverage Matrix

`ShaderTypeData` variant × parse-complete / import-complete / render-complete
(Skyrim SE numeric mapping, `BSLightingShaderType`, BSVER 83-129):

| Type # | Name | Variant | Parse | Import (`ImportedMesh`/`MaterialInfo`) | Render (`triangle.frag`) |
|---|---|---|---|---|---|
| 0 | Default | `None` | ✓ | ✓ (defaults) | ✓ (base lit path) |
| 1 | Environment Map | `EnvironmentMap` | ✓ | ✓ (`env_map_scale`) | ✓ |
| 2 | Glow Shader | `None` | ✓ | ✓ (via emissive fields, not shader-type data) | ✓ |
| 3 | Parallax | `None` | ✓ | ✓ | — (no dedicated POM path for legacy Parallax) |
| 4 | Face Tint | `None` | ✓ | ✓ | ✓ (detail/tint slots, not shader-type data) |
| 5 | Skin Tint | `SkinTint` | ✓ | ✓ (`skin_tint_color`, +alpha on FO4) | ✓ |
| 6 | Hair Tint | `HairTint` | ✓ | ✓ | ✓ |
| 7 | Parallax Occ | `ParallaxOcc` | ✓ | ✓ (`max_passes`,`scale`) | marked "Unimplemented" by nif.xml itself — engine parses/imports, no dedicated POM shader path (matches upstream spec note, not a gap) |
| 8 | Multitexture Landscape | `None` | ✓ | ✓ (via landscape splat path, not shader-type data) | ✓ |
| 9 | LOD Landscape | `None` | ✓ | ✓ | ✓ |
| 10 | Snow | `None` | ✓ | ✓ | ✓ (base path) |
| 11 | MultiLayer Parallax | `MultiLayerParallax` | ✓ | ✓ (4 fields) | ✓ |
| 12 | Tree Anim | `None` | ✓ | ✓ | ✓ (SpeedTree wind, separate from shader-type data) |
| 13 | LOD Objects | `None` | ✓ | ✓ | ✓ |
| 14 | Sparkle Snow | `SparkleSnow` | ✓ | ✓ (4 params) | ✓ |
| 15 | LOD Objects HD | `None` | ✓ | ✓ | ✓ |
| 16 | Eye Envmap | `EyeEnvmap` | ✓ | ✓ (scale + 2 reflection centers) | ✓ |
| 17 | Cloud | `None` | ✓ | ✓ | ✓ (sky path) |
| 18 | LOD Landscape Noise | `None` | ✓ | ✓ | ✓ |
| 19 | Multitexture Landscape LOD Blend | `None` | ✓ | ✓ | ✓ |
| 20 | FO4 Dismemberment | `None` | ✓ | n/a (FO4+ only) | n/a |

All 21 numeric values verified against `nif.xml:1400-1421`
(`BSLightingShaderType`) — no drift found between the Rust dispatch and the
authoritative spec.

## Cell-Load Regression Status

- TES5 cells parse through the unified `esm/cell/` walker; compressed record
  groups decompress correctly; `parse_real_skyrim_esm` passes against the
  real `Skyrim.esm`.
- **Whiterun BanneredMare control-bench**: live re-run this session measured
  `entities=3406`, `draws=1505`, `Inventory=46`, `EquipmentSlots=6`,
  `tex.missing=0` — entity/draw counts and equip counts are flat against the
  most recent ROADMAP figures, so the control bench shows **no new
  regression**. FPS is currently well below historical baselines
  (`67.9` in this run vs `335.0` at `8a668eff`), but this is the **existing,
  already-tracked** `PERF-REGRESSION-6c56e311` (ROADMAP Known Issues →
  Open — Performance), root-caused to `triangle.frag` shadow-ray rewrite +
  GI path depth added in commit `6c56e311`, and it affects Prospector (FNV)
  and FO4 MedTek identically — it is a cross-game renderer regression, not a
  Skyrim-specific defect, and is intentionally **not** re-filed by this
  audit.
- Meshes0 sweep: 100.00% clean (18,862/18,862), 0 truncated, 0 recovered, 0
  WARN-level log lines — baseline confirmed live.

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 new (1 pre-existing, already tracked: PERF-REGRESSION-6c56e311) |
| MEDIUM | 0 |
| LOW | 1 new (audit-skill path drift, LOW-1) |

Every Skyrim-specific regression guard named in the audit-skyrim checklist —
#838 (BSLODTriShape routing), #837 (BsLagBoneController/BsProceduralLightningController),
#559 (SSE skin reconstruction), #795/#796 (bitangent-triplet tangent
convention), #1201/#1202 (alpha-cascade gate), #1554 (ESL FormID decode),
#1660 (deleted-REFR tombstones), #1553 (multi-plugin `.STRINGS`), #890 zero-based
sibling auto-load, #1731 (VWD forward-scope), #1280 (EmissiveSource), and the
NIFAL `resolve_pbr` → `classify_glass_into_material` ordering — is intact and,
where practical, was re-verified live against real Skyrim SE game data rather
than by code inspection alone.

Suggested: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-07-25.md`
