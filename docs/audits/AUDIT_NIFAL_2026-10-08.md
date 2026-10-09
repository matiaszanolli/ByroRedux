**HEAD**: 00f580e09 · **Baseline**: docs/audits/AUDIT_NIFAL_2026-10-05.md (HEAD a2c24b16e) · **Audited**: Dims 1, 2, 3, 4, 7, 8, 9 (delta-touched) · **Unchanged since baseline (skimmed)**: Dims 5, 6 (no commits on their paths; first steps and guards run)

# NIFAL Audit — 2026-10-08

This report is one leg of `/audit-suite --preset comprehensive`. One auditor ran all nine dimensions in sequence, with no sub-agents.
The delta is `a2c24b16e..00f580e09`, 116 commits. Of those, 14 touch NIFAL paths. Scratch notes for each dimension are in
`/tmp/audit/nifal/dim_{1..9}.md`, and this report was reconciled against them.

## Executive Summary

**New findings: 3 (0 CRITICAL · 0 HIGH · 1 MEDIUM · 2 LOW).** One of the LOWs is a regression of #4283, but only on a new code
path. Already-tracked items re-checked: 11 (listed below; none counted as new).

**Headline (MEDIUM, D8-01).** #4277 (`c2f28e06c`) added a loose Starfield `.mat` JSON resolver, but it decodes a JSON layout that
was guessed, not taken from real files. This install has **20** loose `.mat` files across four Creation archives. All of them use
a different layout: the components sit under `Objects[].Components[]` with a `Data` sub-object, the texture key is `FileName`,
and scalars and bools are strings. Because of that, `parse_loose_mat` returns `None` for every real file. Such a file is only
recognised as present, and it no longer reaches the CDB lookup it got before.

The module doc and the commit say that "a re-scan of this install's 129 archives finds zero" loose `.mat` files. That is false:
`merge.rs`'s own census, a few lines above, says 20. The unit tests pin the guessed layout, so they pass on a resolver that
cannot decode any real file.

**D8-02 (LOW).** The new loose arm's fallback branches set `is_pbr` but not `external_material_resolved`. The CDB-miss fallback
they replaced did set it (#4283), so the glass classifier's effect-carrier promotion is locked out on this path.

**D1-01 (LOW).** The Phase-2 resolver table describes `resolve_unresolved_gloss_neutral_roughness`'s trigger backwards, in both
the module doc and `nifal.md`.

Per-category status against spec §2:

| Category | This delta |
|---|---|
| Material (D1) | Converged. #5230 and #5252 verified; 178/178 `byroredux` NIFAL guard tests pass. 1 LOW (the inverted resolver-table row) |
| Geometry/Transform (D2) | #4268 (BSGeometry skin bone-index bound) and #5081 verified. No findings |
| Skinning / Lights (D3) | #5264 (one spawnable-light predicate) and #5262 (doc) verified. No findings; #5282 still open |
| Nodes (D4) | Unchanged in substance: the seven parked fields still have 0 canonical consumers |
| Particles (D5) | No commits; exactly 2 overlay callers; guards green |
| Collision (D6) | No commits; 16 resolve arms (counted fresh); guard green |
| Animation (D7) | Only the #5100 guard hardening (`production_text`); guards green |
| Shader flags / roles / merge (D8) | #4277 is new: 1 MEDIUM and 1 LOW. #5319 verified. The `if game` shader grep is empty |
| Completeness (D9) | Harness 2/2 green on all 8 games. Small fill-rate shifts on SkyrimLE and FO4 come from the sample change in #5259 |

Count of new findings per invariant:

| Invariant | Findings |
|---|---|
| single-boundary | D8-02 (the loose arm re-implements the CDB-miss fallback in part, instead of calling it) |
| no-fabrication | D8-01 (the dialect is unsourced; the guessed defaults are listed under D8-01) |
| no-leak | 0 |
| no-render-time-fallback | 0 |
| parked-not-leak / translation gap | D8-01 (authored loose `.mat` data is dropped for 20/20 real files) |
| doc / record-keeping | D1-01 |

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | PASS: 4 production `translate_material{,_with_provenance}` callers (`nif_loader`, `mesh_instance`, `placement_lod`, `object_lod`) | PASS | PASS | PASS | `translate_material_with_provenance` / `translate_texture_only_material*` |
| Geometry/Transform | PASS | PASS: #4268 declines and never clamps | PASS | PASS | `import/units.rs`, `import/transform.rs`, `rotation.rs` |
| Lights | PASS: one predicate shared by the count and the spawn (#5264) | PASS | PASS | PASS | `walk/lights.rs` → `spawn_nif_lights` |
| Particles | PASS | PASS | PASS | PASS | `systems/particle.rs::apply_emitter_overlays` |
| Collision | PASS | PASS | PASS | PASS | `import/collision/` |
| Animation | PASS | PASS | PASS | PASS | `convert_nif_clip` + `convert_hkx_clip` |
| Shader flags / roles / merge | PARTIAL: D8-02, plus #5284 now also diverging on provenance | **FAIL**: D8-01 | PASS | PASS | `slot_role.rs::slot_to_role`, `merge.rs::{fill, apply_cdb_material, apply_loose_mat}` |

## Findings

### MEDIUM

#### NIFAL-D8-2026-10-08-01: #4277's loose `.mat` decoder was written for an invented JSON layout, so it decodes none of the 20 real loose `.mat` files installed and now skips their CDB lookup
- **Severity**: MEDIUM. This is the "translatable data silently dropped" row: the authored loose-material textures and settings
  are never captured. The content is mod (Creation) content, not vanilla, so it is not HIGH.
- **Dimension**: Shader-flags/Effects (the merge boundary, the Starfield `.mat` arm)
- **Tier Violated**: no-fabrication (the layout is unsourced), and parked-not-leak (the translation is wired but cannot fire)
- **Game Affected**: Starfield (Creation/mod archives)
- **Location**:
  - `byroredux/src/asset_provider/material/loose_mat.rs:52-136` (`parse_loose_mat`) and `:153-182` (`decode_rgba`);
  - `byroredux/src/asset_provider/material/merge.rs:479-490` (the loose arm short-circuits before `lookup_cdb_material`);
  - `merge.rs:1683-1720` (`apply_loose_mat`);
  - `merge.rs:436-440` and `:573-575` (stale comments).
- **Status**: NEW. This is an incomplete fix of #4277, which was closed by `c2f28e06c`. It subsumes the renderer audit's routed
  note (`AUDIT_RENDERER_2026-10-08.md:287`, `Index as u8` wrap and `f64 as f32` overflow).
- **Description**: The decoder looks for a top-level `Components` array whose objects carry their properties directly:
  `File`/`Path`, a JSON-bool `Enabled`, a numeric `Value`, and an array or hex `Color`. The real files differ on every one of
  those points:
  1. **Root**: the top-level keys are `Filename`, `Import`, `Objects`, `Summary`, `Version`, and components live at
     `Objects[i].Components[j]`. `find_array(&value, &["Components", "components"])?` therefore returns `None`. That routes the
     file to `apply_loose_mat`'s `None` arm, which only recognises the file as present.
  2. **Properties** sit under each component's `Data` object, not on the component object itself.
  3. **Texture key**: `MRTextureFile` stores `"FileName" : "Data\\Textures\\QOG\\…\\TerminalCase_color.dds"`. The decoder reads
     `File`/`Path`, and it does not strip the `Data\` prefix.
  4. **Typed values are strings**: for example `"Enabled" : "false"` and `"MaterialOverallAlpha" : "0.750000"`. So `as_bool()`
     and `as_f64()` return `None`. A disabled `TextureReplacement` would be treated as **enabled**, because the code tests
     `enabled != Some(false)`, and it would push a fabricated flat colour.
  5. **Colour** is nested: `BSMaterial::Color → Data.Value → XMFLOAT4 {x,y,z,w}`, also as strings.

  `decode_rgba` also invents values when it cannot read one:
  - a missing channel becomes 1.0;
  - for a 6-digit hex value, alpha is read from the blue byte (`channel(6.min(len-2))` evaluates to `channel(4)`);
  - `Index as u8` wraps around.

  The loose arm also returns before `lookup_cdb_material`. So a loose file that cannot be decoded now hides a CDB row that
  previously resolved. That is latent: no installed loose file shadows a vanilla path (`portablegreenhouse02.mat` has no vanilla
  counterpart). Finally, the arm sits inside `starfield_cdb_gate` (`merge.rs:434`, `has_starfield_cdb()`), even though decoding a
  loose file does not need a CDB.
- **Evidence**:
  - `ba2_grep` over all 129 `Starfield/Data/*.ba2` found `.mat` entries in four archives: `qog-pawnshop - main.ba2` (12),
    `sp2_factionrequisitionkiosks - main.ba2` (5), `starfieldresourcerevival - main.ba2` (2) and `avontechshipyards - main.ba2`
    (1). That is 20 files, which matches `merge.rs:437-438` ("20 JSON `.mat` exports measured across 129 installed archives").
  - Two were extracted and read, `galacticpawnshopterminal_terminalcase.mat` and `lasersight_white.mat` (saved in
    `/tmp/audit/nifal/matsamples/`). Both have the `Objects`/`Components`/`Data`/`Type` layout and the string-typed values
    described above.
  - The doc claims "a re-scan of this install's 129 archives with `ba2_grep` finds **zero**" (`loose_mat.rs:10-11`) and
    "0 across this install's 129 archives" (`merge.rs:483`). Both are contradicted by that measurement.
  - The test `decodes_the_cited_component_spellings` builds its fixture in the invented layout, so it passes.
- **Impact**:
  - Every installed loose `.mat` contributes no textures or settings. These Creation surfaces render exactly as they did before
    #4277 (recognised as present only), while #4277 is closed and the code claims a working Stage A.
  - If the root lookup alone were fixed, the string-bool handling would *enable* authored-disabled replacements.
  - Once a loose override of a vanilla path exists, it would lose the CDB textures it gets today.
- **Related**: #4277 (closed), #762, #3398 (open, CDB per-field data), #5210 (open, CDB Phase-2 doc rot), #5284 (open, CDB tail
  copied at four sites), D8-02.
- **Suggested Fix**:
  - Re-derive the decoder from the installed samples: walk `Objects[].Components[]`, read `Data`, match `FileName`, strip the
    `Data\` prefix, parse string scalars and bools explicitly, and decode the `BSMaterial::Color` nesting.
  - Change the fixtures to real extracted files (or verbatim excerpts) and add a test that decodes one real-layout file end to
    end.
  - If a file does not decode, fall through to `lookup_cdb_material` rather than returning `PresenceOnly`.
  - Correct the "zero" census claims and the stale comments at `merge.rs:439-440` and `:573-575`.

### LOW

#### NIFAL-D8-2026-10-08-02: The loose `.mat` arm's `Undecodable`/`None` branches set `is_pbr` but not `external_material_resolved`, so the #4283 provenance the replaced CDB-miss fallback set is lost on this path
- **Severity**: LOW · **Dimension**: Shader-flags/Effects → Material · **Tier Violated**: single-boundary (a partial
  re-implementation of `apply_cdb_pbr_fallback`) · **Game Affected**: Starfield (Creation/mod)
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:1690` (`material.is_pbr = true;` is the only routing write) and
    `:1703-1718` (the `Undecodable` and `None` arms);
  - compare `byroredux/src/asset_provider/material/cdb.rs:361-370` (`apply_cdb_pbr_fallback` sets both flags);
  - consumer: `byroredux/src/helpers.rs:140-142` (`effect_glass_carrier = … && (bgem_glass || (keyword_match &&
    external_material_resolved))`).
- **Status**: Regression of #4283, confined to the new loose-file path introduced by `c2f28e06c`.
- **Description**: Before #4277, a Starfield `.mat` path with no CDB row went to `apply_cdb_pbr_fallback`. That sets
  `external_material_resolved = true`, the format-agnostic provenance #4283 added so that Starfield effect-shader glass can take
  the keyword-promotion route. `apply_loose_mat` handles the same "external material present, nothing decoded" state, but writes
  only `is_pbr`. Only the `Decoded` branch gets the flag, indirectly, through `apply_cdb_material` (`merge.rs:217`).

  Per D8-01, every real file currently lands in the `None` branch. A loose file that cannot be decoded will always be possible,
  even after D8-01 is fixed.
- **Impact**: A Creation effect-shader material whose path or name matches a glass keyword is no longer promoted to glass. None
  of the 20 installed files is known to hit this (for example, `lasersight_white.mat` has no glass keyword), so the visible
  impact today is nil.
- **Related**: #4283 (closed), #5099 (the two provenance signals), D8-01, #5284.
- **Suggested Fix**: In the two fallback branches, call `apply_cdb_pbr_fallback(material, path)` (or set
  `external_material_resolved` beside `is_pbr`), so that "a `.mat` is present but carries no data" has one implementation.

#### NIFAL-D1-2026-10-08-01: The Phase-2 resolver table describes `resolve_unresolved_gloss_neutral_roughness`'s trigger backwards, in both the module doc and `nifal.md`
- **Severity**: LOW · **Dimension**: Material · **Tier Violated**: doc / record-keeping · **Game Affected**: FO4 / FO76 (BGSM
  content)
- **Location**: `byroredux/src/material_translate.rs:42` and `docs/engine/nifal.md:797`. The predicate itself is at
  `material_translate.rs:1409-1433`.
- **Status**: NEW. The text was introduced by `c9b02ba4a` on 2026-09-12. It is not covered by #4246 (the `placement_lod`
  exemption rationale) or by #5279 (the "backstop" wording).
- **Description**: Both table rows say the resolver writes `Material::roughness` "when a gloss/smoothness map **resolved** with
  **no authored BGSM PBR scalars** to interpret it". The code does the opposite on both counts:
  - It returns `None` unless `bgsm_pbr_scalars_authored` is true.
  - It returns `None` unless `gloss_map_index == 0`, meaning no gloss map resolved.
  - It acts only when roughness is at or below the near-mirror floor.

  `nifal.md:823-824` states the gate correctly ("only acts on authored BGSM PBR scalars"), so the spec contradicts itself 26 lines
  apart. The function's own rustdoc (`:1383-1386`) is also correct.
- **Evidence**: `unresolved_gloss_neutral_roughness` contains the line `if !bgsm_pbr_scalars_authored { return None; }`, then
  `if gloss_map_index != 0 { return None; }`, then the floor test.
- **Impact**: A reader of the table would scope the resolver to legacy, non-BGSM content with a working gloss map. That is
  exactly the population it must never touch. Widening the gate to match the table would neutralise legacy roughness.
- **Related**: #3905, #3639, #4246, #5230.
- **Suggested Fix**: Reword both rows to: "when authored BGSM PBR scalars sit at the near-mirror floor and no gloss map
  *resolved* to modulate them (#3905; forced mirror panes exempt, #5230)". Fold this into the #4246 / #5279 doc pass.

## Existing open issues re-checked (not counted as new)
- **#5284** (OPEN), with an addendum: `c2f28e06c` added `record_external_texture_sources(…, ImportedTextureSource::Mat)` only at
  the `.mat`-arm CDB hit (`merge.rs:494-501`). The three other CDB fills still leave CDB-filled roles labelled `NifTextureSet`:
  unknown kind at `:607`, BGSM miss at `:718` and BGEM miss at `:1390`. The BGSM-miss fill is live for Starfield, since 17 of 57
  sampled `.bgsm`/`.bgem` names resolve to CDB rows (`cdb.rs:356-358`). So the four-copy divergence now covers provenance as well
  as telemetry. This affects diagnostics only: `MaterialTextureDebugInfo` is not saved (`registry_completeness_tests.rs:556`).
- **#4246** (OPEN): unchanged. `resolve_unresolved_gloss_neutral_roughness` still reads `.unwrap_or(0)` (`material_translate.rs:1449-1452`)
  and does not return early when the handles are missing.
- **#5279** (OPEN): `nifal.md:858` still contains the "backstop" claim. #5252 fixed only the two rustdoc copies (`46f59e1d2`).
- **#5282, #5285** (OPEN): `nifal.md` had no commit in the window, so the light-scope ledger row and the seventh beam converter
  are still missing.
- **#5281** (OPEN): the `_with_authored_msn` → `_with_authored_msn_and_clamp` forward is still present (`material_translate.rs:1079`,
  `:1091`).
- **#5277, #5283** (OPEN): `apply_cdb_material` still forwards no blend state and captures no emissive scalars. The loose decoder
  does not capture them either (`EmissiveSettingsComponent` is ignored, and `EffectSettings` reads only `IsGlass`).
- **#5210, #4256, #3398** (OPEN): unchanged. `merge.rs:476-477` ("Phase 2 should return `Merged`") is still there.

## Verified fixed in this window (for the next run's dedup)
- **#5230** (`9f0a8a7cc`):
  - `classify_glass_into_material_with_provenance` returns whether the mirror branch fired.
  - `translate_material_with_provenance` returns a `MaterialTranslateProvenance { mirror_pane_forced }`.
  - Both resolver-calling spawn sites use it (`scene/nif_loader.rs:1301,1510`, `cell_loader/spawn/mesh_instance.rs:1114,1373`).
  - `normal_alpha_spec_roughness` cannot reach a mirror pane, because metalness 1.0 fails its `< 0.3` gate.
- **#5252** (`46f59e1d2`): the rustdoc in `translate_material` and `resolve_pbr` scopes the classifier arm to BGEM and CDB misses.
- **#4268** (`0abc86c8b`): `convert_bs_geometry_skin_weights` declines, and never clamps, when a bone index is at or above
  `bone_refs.len()`. One observation, not filed: the check also covers weight entries that the top-4 truncation would discard.
- **#5081** (`2be7c7c0b`): the `tangent.rs` doc now matches the parse-Z-up / import-swap contract.
- **#5262 / #5264**: the `affected_node_names` doc states the real version gate, and `is_spawnable_nif_light` now includes the
  exporter-artifact skip.
- **#5259** (`44986c6ef`): the harness sample and the dark census filter through `corpus::is_nif_entry`.
- **#5100** (`655b317c9`): the `anim_convert` scan uses `byroredux_core::source_scan::production_text`.
- **#5319** (`8be8c3d3a`): a failed CDB fetch now logs a warning before the failure is memoized. No change to translation.

## Documented-limitation ledger (parked, not leaks)
The baseline ledger (`AUDIT_NIFAL_2026-10-05.md` and earlier) still stands. Unchanged rows:
- the seven parked node fields have 0 canonical consumers (the grep is empty);
- the BhkNP blob, phantoms and undecoded constraint kinds;
- `BhkPlaneShape` → `None`;
- the particle size-over-life curve and `initial_color`;
- ambient and morph animation channels;
- `NiAmbientLight` scoping, plus the D3 affected-node scope gap (#5282);
- `NiSpotLight` inner angle and exponent;
- the structural 0% `tex`/`nrm` fill on FO76 and Starfield;
- the CDB emissive role is zero-weighted (#5283);
- Starfield CDB metalness and roughness are parked (#3398).

New row: the **loose `.mat`** path is effectively recognition-only for the whole installed population until D8-01 is fixed.

## Dimension 9: harness table (release, `--ignored`, all 8 games; 2 passed in 62.98 s)

| game | meshes | tex% | mat_path% | m_kind% | metO% | spec% | gloss% | env% | nrm% | tan% |
|---|---|---|---|---|---|---|---|---|---|---|
| Oblivion | 567 | 91.4 | 0.0 | 0.0 | 100.0 | 100.0 | 0.0 | 0.0 | 0.0 | 84.0 |
| FO3 | 687 | 92.6 | 0.0 | 10.9 | 94.3 | 16.7 | 0.0 | 4.1 | 79.2 | 99.1 |
| FNV | 806 | 95.3 | 0.0 | 17.6 | 96.7 | 9.8 | 0.0 | 4.0 | 78.9 | 99.3 |
| SkyrimLE | 529 | 92.1 | 0.0 | 42.9 | 92.1 | 67.5 | 0.0 | 7.0 | 67.5 | 92.4 |
| SkyrimSE | 515 | 93.8 | 0.0 | 35.5 | 93.8 | 76.1 | 0.0 | 6.8 | 76.1 | 94.8 |
| FO4 | 633 | 91.8 | 56.7 | 56.2 | 99.4 | 83.1 | 73.5 | 46.6 | 82.9 | 94.6 |
| FO76 | 773 | 11.3 | 83.8 | 33.1 | 15.8 | 7.8 | 0.0 | 7.1 | 8.8 | 96.9 |
| Starfield | 811 | 0.0 | 94.9 | 3.7 | 5.1 | 1.4 | 0.0 | 3.7 | 0.0 | 100.0 |

- SkyrimLE and FO4 moved slightly against 2026-10-05 (FO4 644 → 633 meshes; SkyrimLE tan 94.0 → 92.4). #5259 widened the sample's
  NIF filter to the renamed `.bto`/`.btr` LOD meshes, and no extractor changed in the window, so this is a change in which meshes
  were sampled, not in how they are translated.
- Every floor and ceiling passes.
- The dark-role census found 8 hits, all in Oblivion.
- The harness measures the pre-merge tier, so it cannot see D8-01 or D8-02.

## Method notes
- **Tests run at HEAD**:
  - `cargo test -p byroredux --bin byroredux` on toolchain 1.96.0, filtered to `material_translate`, `merge::`, the animation
    harness, the HKX sample, overlay, secondary-role walk, tint-alpha and spawner/marker guards: **178 passed**
    (`/tmp/audit/nifal/test_byroredux.log`).
  - `cargo test -p byroredux-nif --lib`, filtered to dispatch coverage, role/value counts, light dispatch, satellite
    independence, `slot_role`, decal, tangent and units: **51 passed** (`test_nif.log`).
  - The D9 harness: 2 passed (`d9_harness.log`).
- **D8-01 measurement**: I built `crates/bsa`'s existing `ba2_grep` / `ba2_extract_one` examples (read-only, nothing added to the
  repo), ran them over all 129 Starfield BA2s, and extracted 2 samples to `/tmp/audit/nifal/matsamples/`.
- **No engine launch**, no plugin `--ignored` tests, and no repo edits besides this file.
- **Dedup sources**:
  - `/tmp/audit/issues.json` (113 open), plus `gh issue list --state closed --search "NIFAL in:title"`.
  - Issues #4277, #4246, #3905 and #4283.
  - Today's sibling reports. RENDERER: only the routed `parse_loose_mat` note, now subsumed; REN-D5-01/-02 are DDS parse issues
    with no NIFAL-boundary angle, because the glow role is translated correctly and the collapse happens at texture decode.
    NIF: #4277 is deferred to NIFAL. PERFORMANCE: the probe cost of the `.mat` lookup only.
- **Skill staleness** (route to `/audit-sync`): Dim 8 still says the unproduced `Mat` label "was not re-added". #4277 re-added it,
  with producers at the `.mat` CDB hit and the loose arm only. Dim 8 should also list `loose_mat.rs` among its paths.

**Suggested publish labels** (domain `nifal` on all):
- **D8-01**: `medium` `bug` `import-pipeline` `game:starfield`
- **D8-02**: `low` `bug` `import-pipeline` `game:starfield`
- **D1-01**: `low` `documentation` `doc-rot`

## Next Step

```
/audit-publish docs/audits/AUDIT_NIFAL_2026-10-08.md
```
