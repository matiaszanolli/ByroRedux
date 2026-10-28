**HEAD**: fa6be4a6b · **Baseline**: docs/audits/AUDIT_STARFIELD_2026-10-21.md (HEAD 4a3e89d56; Dims 2, 3, 4, 6 audited there) · **Audited**: Dims 2, 3, 4, 6 (delta-touched) · **Unchanged since baseline (skimmed)**: Dims 1 and 5 (no logic commits on their paths; guards and the corpus gate were run)

# Starfield Compatibility Audit — 2026-10-27

This report is one leg of `/audit-suite --preset comprehensive`. One auditor ran every dimension in sequence, with no sub-agents.

The window is `4a3e89d56..fa6be4a6b` (395 commits). The Starfield-relevant ones are:

- **Fixes for the last report:**
  - 31b946170 (#6260): pack-in template children keep their own material swaps.
  - 6ca136f6a (#6330): the loose `Data` tier now covers `geometries` and loose CDBs.
  - a1b72625f (part of #6259): Starfield bleedout data is captured; no recovery rule is wired yet.
- **CDB:**
  - 988402d25 (#5665): a build that panics is memoised.
  - 04036a6fb (#5663): cache-lifetime docs.
- **Archives:**
  - f26eb657f (#6308): one Unicode fold for loose keys.
  - 120b7df86 (#6243): loose walk depth cap and visited set.
  - 3ad6289c2 (#5664).
- **ESM:**
  - e04caa7e8 (#6367): reflection and WTHS decode through `SubReader`.
  - ce5796431 (#6272): reflection decoder moved.
  - 0d7e13e6c (#6395).
  - 559a76627 (#5534): XCLL docs.
  - 2e5e25b01 (refs #5509): `nifal.md` parked row.
- **Cell spawn:** 1efe39722 makes VFX effect-shader sheets stop synthesizing colliders. It is the source of the one new finding.

Constraints honoured:
- No engine launch, no `--sf-smoke`, no smoke script, and no blanket `--ignored` run. Every real-data test ran alone (`--exact`) under `systemd-run --scope -p MemoryMax=4G..6G`.
- The full 9 GB CDB `parse` was not attempted.
- No source, skill or issue edits.
- The repo `target/` was used throughout. A read-only scratch probe crate, built into the repo `target/` with a copied `Cargo.lock` and `--offline`, lives at `/tmp/claude-1000/-mnt-data-src-gamebyro-redux/360ebc6e-3fb4-48c0-a282-267e2a566614/scratchpad/sfprobe`. There is no private target dir.

## Guard runs and real-data gates (all green)

| Suite | Result |
|---|---|
| bsa `--lib ba2` | 44/44 |
| nif `--lib` (full) / `bs_geometry` / `shader::tests::starfield` / `starfield` / `skin` | 1446 (1 ign) / 72 / 11 / 32 / 118 (1 ign) |
| sfmaterial lib + header_smoke + integration | 51 + 6 + 1 |
| plugin `--lib` `starfield` / `lgtm` / `xcll` / `spatial_units` / `pkin` / `dat2` / `txst` / `lmsw` / `acbs` / `small_master` / `reflection` / `wths` / `bleedout` / `sf_esm_baseline` | 30 (3 ign) / 5 / 25 (2 ign) / 10 / 12 (1 ign) / 8 / 18 / 11 / 9 (1 ign) / 3 (1 ign) / 1 / 2 / 2 / 1 |
| byroredux bin: `starfield_mat` 19, `asset_provider::material` 30, `material_translate` 78, `loose` 21, `cdb` 37, `light_anim` 31, `normalize_mesh_path` 8, `pkin` 18, `lmsw` 1, `starfield` 36, `bgsm_merge` 79, `pack_imported_material_flags` 8, `merge_external_material_is_the_only_exported_fn_in_this_file` 1, `documented_texture_role_list_matches_the_struct` 1 | all pass |
| **Dim 1 corpus** `parse_rate_starfield_all_meshes` (release, MemoryMax=5G, 8.4 s) | **13/13 archives, 120,543 NIFs, 100.00% clean, 0 truncated, 0 failed** |
| **`real_cdb.rs`**, each `--ignored --exact` alone (release, MemoryMax=4G) | 5/5: `parse_vanilla_materialsbeta_cdb` (streaming validate; 97 classes / 1,438,780 values pinned), `material_index_resolves_a_vanilla_material`, `…_inherited_glass_and_emissive_settings`, `…_dlc_cdb_resolves_the_base_corpus`, `…_inherited_texture_slots` (0.7–2.1 s each) |
| `cell_loader::pkin_expansion_tests::starfield_cydonia_pack_ins_instance_their_template_cells` (alone, MemoryMax=6G, 7.6 s) | pass. 370 pack-in REFRs → 12,372 children; **2,191 carry their own swaps**; 0 left to the default path |
| `sf_smoke --tsv --recurse` on `Starfield.esm` (release, MemoryMax=5G) | **byte-identical** to `.claude/audit-baselines/sf-esm/Starfield.tsv` (non-comment rows) |

No Starfield `Data` file is newer than 2026-10-12. The installed `Data` holds only `video\` and `data.bin` besides plugins and archives, so there is no loose root.

## Executive Summary

Starfield is a first-class `GameKind`. This window brought no regressions, and both fixable findings from the 2026-10-21 report are closed and verified in code and on real data.

- **NIF + BA2**: unchanged. Re-measured at 100.00% over 120,543 NIFs.
- **BSGeometry `.mesh`**: the loose tier now indexes `geometries` (#6330).
  - The NULL-ref skin solver's skeleton composition is NIF-D4-2026-10-27-01, cited and not re-filed. Its failure mode on Starfield is a *decline* (coverage), never a wrong name.
- **CDB Phase 2 is partial** and unchanged in scope. Colour, normal and emissive roles are translated, as are the alpha, glass and blend flags, with settings and texture-set inheritance. Still parked:
  - metal/rough scalars;
  - slot-6 `_height`;
  - the five single-channel kinds;
  - emissive settings;
  - the additive blend family (#5509).

  New this window: a CDB build that panics is now contained and memoised (#5665), and loose CDBs are discovered (#6330).
- **Cydonia bring-up**: pack-in children carry their own `XLMS`/`XMSP` (#6260, 2,191 children on real data).
  - The bleedout data decodes, but recovery stays unwired (#6259, open, by design).
  - The `sf_smoke` byte coverage is identical to the baseline.
  - Walkable Cydonia is not re-verified, because no engine launch is allowed.
- **New finding (LOW)**: the 1efe39722 VFX-collider gate cannot see Starfield's `BSEffectShaderProperty` material-reference stubs.

Counts: **0 CRITICAL / 0 HIGH / 0 MEDIUM / 1 LOW**.

## Dimension Summaries

- **Dim 1 (BA2 + corpus)**: unchanged. f26eb657f only made `ba2::normalize_path` `pub` (same body). Guards and the corpus are green.
- **Dim 2 (BSGeometry)**: SF-2026-10-21-D2-01 (#6330) is verified fixed:
  - `LOOSE_ASSET_ROOTS` includes `geometries` (`archive.rs:50`).
  - `open_shared` re-opens a directory source as the loose tier (`archive.rs:271-275`).
  - `discover_starfield_cdbs` runs on the loose tier, registered last (`provider.rs`).

  #6243's `MAX_LOOSE_DEPTH = 32` and canonical-path visited set leave Starfield's two-level `geometries\<dir>\<hash>.mesh` shape alone. No commits on `bs_geometry.rs` (block or import) or skin logic.
- **Dim 3 (CDB)**:
  - #5665: `contain_cdb_build_panic` runs inside the slot initializer with `CDB_BUILD_GATE` held (`cdb.rs:156-195`). `[profile.release] panic = "unwind"` (`Cargo.toml:345-346`) keeps the catch live in release.
  - A loose CDB's index build re-opens through `open_shared(dir)`. The 1 GB `MAX_CHUNK_BYTES` extract cap is far above the ~105 MB blob.
  - The clippy commits are struct-literal init plus allow reasons only.
- **Dim 4 (ESM + bring-up)**:
  - #6260 is verified: the `SynthPlacement` 5th element is set at `refr.rs:486-504`, and `build_refr_texture_overlay_over` puts child swaps beneath the outer REFR's (`refr.rs:524-623`).
  - The #6367 `SubReader` port of the reflection decoder preserves every width and offset.
  - #6395 kept `warn_unmodelled_txst_slot` (`support.rs:775`).
  - The bleedout capture (a1b72625f) is in place. The one-shot "recovery rule not sourced" warn is at `bleedout.rs:236`.
  - LC-2026-10-27-D2-01 (first `NiControllerSequence` plays from spawn) has **no Starfield leg**: 0 Starfield NIFs in Meshes01/02/Patch carry a `NiControllerSequence` (scratch probe).
  - One new finding: SF-2026-10-27-D4-01.
- **Dim 5 (shader blocks)**: unchanged; 11/11.
  - The probe's stub census agrees with the #4439 discriminator: 88 effect stubs, all `.bgem`-suffixed, plus 783 empty-name full-body blocks.
- **Dim 6 (material flow)**:
  - #6396 deleted `_`-prefixed parameters that were never read (no behaviour change).
  - #6141 moved `ImportedMaterial` to `import/types/material.rs`; the vocabulary pin resolves.
  - #6214 is Skyrim-only.
  - No commits on `material_flags.rs`, the merge logic or the BGSM crate. All boundary guards are green.

## Findings

### LOW

### SF-2026-10-27-D4-01: 1efe39722's VFX-collider gate keys on `ImportedMaterial.effect_shader`, which a Starfield `BSEffectShaderProperty` material-reference stub never fills — 88 stubs in 66 NIFs keep a synthesized collider
- **Severity**: LOW
- **Dimension**: 4 (cell bring-up: synthesized colliders). The physics side belongs to `/audit-physics`.
- **Location**:
  - `byroredux/src/cell_loader/spawn.rs:83-90` (`render_geometry_can_collide`)
  - `crates/nif/src/import/material/dedicated_shader.rs:539-570` (`apply_bs_effect_shader`: the stub arm does `material_kind = 101; return;` before `info.effect_shader = Some(effect)` at :692)
  - `byroredux/src/cell_loader/spawn/mesh_instance.rs:1525-1531` (the `ArchitectureTriMesh` call site)
- **Status**: NEW. This is a partial of 1efe39722 (2026-10-26, no issue number). It is related to FNV-2026-10-27-02, which hits the same predicate through a different block (`BSShaderNoLightingProperty`); fix the two together.
- **Description**:
  - 1efe39722 made the missing-collision fallback skip VFX sheets by requiring `material.effect_shader.is_none()`.
  - On Starfield, a `BSEffectShaderProperty` with a non-empty name is parsed as a material-reference stub (`effect.rs:170-181`). `apply_bs_effect_shader` then returns early after tagging `material_kind = 101`, so `effect_shader` stays `None`. The early return is deliberate (#2617): the stub's fields are placeholders.
  - The FO76 analogue is repaired downstream, because a resolved `.bgem` inserts `effect_shader` (`merge.rs:1797-1809`). Starfield ships zero `.bgem` files, and its CDB arms never insert one.
  - Every Starfield static collider is synthesized from render geometry: collision is an opaque `BhkSystemBinary` (`import/collision/mod.rs:11`). So these shapes still become `Fixed` trimesh colliders.
  - The 783 full-body Starfield effect blocks *are* excluded now. The rule therefore depends on which of the two encodings a mesh happens to use.
- **Evidence** (scratch probe over `Starfield - Meshes01/02/MeshesPatch.ba2` + `ShatteredSpace - Main01.ba2`):
  - 871 `BSEffectShaderProperty` blocks: 783 full-body and **88 stubs in 66 NIFs**.
  - The full-body blocks are almost all editor geometry: `meshes\markers\…` 392, the gizmos, `marker_*.nif`, and empty-name cave/architecture helpers.
  - The stubs sit in content: architecture 24, setdressing 33, ships 15, starstations 6, furniture 6, animobjects 2, actors 1, markers 1.
  - Stub names: `newatlantis\glowwhite.bgem` 12, `sciextledspot.bgem` 11, `opiextlightglow03.bgem` 8, `minerss_helmglass.bgem` 8, `common\lights\glowwhite.bgem` 8, `glassfrostedpatterntri01.bgem` 6, `ships\discovery\glowy.bgem` 6, `glasscleantransparent01.bgem` 4, …, `consolescreen.bgem`, `miningposter*_glow01.bgem`.
  - Example NIFs: `sciintrmsmwallmid_plugc_r00.nif`, `shpgenintaddon_glasspane_opaque01.nif`, `floodlamp_tripodlights_on01.nif`, `neondrink01.nif`.
  - `material_kind` is never rewritten after import. Its only writers are `dedicated_shader.rs:486/514/569/716` and `legacy_properties.rs:740/850`, so the stub's 101 reaches the gate intact.
- **Impact**:
  - Glow strips, LED spots, console screens and poster glow cards keep invisible colliders. Most are flush with the wall they decorate, so gameplay impact is small. That is why this is LOW, unlike FNV-02's free-standing beams.
  - About 15 of the 88 are glass panes, where a collider is arguably correct. Today that outcome is accidental rather than decided.
  - Starfield's real VFX (987 VFX-named NIFs, 883 `BSLightingShaderProperty` `.mat`-only, e.g. `meshes\effects\weaponfx\*beam*.nif`) carry their effect-ness only in the CDB `EffectSettingsComponent` blend mode. No import-side gate can see it until #5509 forwards the additive family. Cited, not re-filed.
- **Related**: FNV-2026-10-27-02 (same gate), #5509 (CDB additive-family blend modes), #2617 (stub placeholder guard), 1efe39722.
- **Suggested Fix**:
  - Key `render_geometry_can_collide` on effect-shader provenance that survives the stub arm: `material_kind == MATERIAL_KIND_EFFECT_SHADER`, or a provenance bit set by the stub arm.
  - Decide glass panes explicitly; keep a collider for glass-named or `glass_enabled` materials if that is the intent.
  - Add a stub case (`effect_shader: None`, `material_kind: 101`) to `effect_shader_sheets_do_not_synthesize_collision`.

---

## Checked and not filed

- **NIF-D4-2026-10-27-01** (`SkeletonBones::from_scene` composes through bare `NiNode` parents only and drops scale) is the composition behind Starfield's NULL-ref skin solver (`skeleton.rs:76-131`, `resolve_external_bone_names` :281). On Starfield it can only cost coverage: `solve_bone_names_with_offset` declines unless a unique anchor fits every bone. Cited, not re-filed.
- **LC-2026-10-27-D2-01** (cell spawn plays the first sequence): there are 0 Starfield NIFs with any `NiControllerSequence`, so this game has no leg.
- **#6260 per-child overlays**: one `build_refr_texture_overlay_over` per swapped child (2,191 on Cydonia) is load-time work under the reference budget. The order (child swaps beneath the outer REFR's) is documented as a decision in `refr.rs:581-585`. Not filed.
- **#5665**: catching inside the `OnceLock` initializer, with the gate held by the caller, is sound. The release profile unwinds.
- **#6330 loose CDB**: `open_shared(dir)` returns `(loose, true)`, and its registry is keyed on the canonical path, matching `loose.path()`. Not filed.

## CRC32 Flag Table

There is no change since the 2026-10-12 report: 0 commits on `crates/nif/src/shader_flags.rs` or `crates/nif/src/blocks/shader/` in this window.

## Remaining-Work Chain

Per `starfield-esm-roadmap.md`, Phases 0 and 1 are done, and Phases 2–4 were invalidated by the 99.9%-parity measurement.

1. **CDB Phase 2 remainder (#3398).**
   - Shipped: the indexed reader, `XMCOLOR` offset order, `IsGlass`, AlphaBlend and emissive-settings capture, the loose `.mat` decoder, settings and texture-set `Parent` inheritance, a panic-safe memoised build, and loose CDB discovery.
   - Left:
     - canonical roles for the five parked kinds (#4429);
     - a slot-6 height-blend consumer (#5381);
     - the `MaterialParamFloat` → roughness/metalness mapping;
     - a `UseSSS` colour source;
     - the inherited-glass coverage signal;
     - the additive-family blend modes (#5509). #5509 also gates VFX collider exclusion for `.mat`-authored effects (see SF-2026-10-27-D4-01).
2. **Material swaps.** LMSW / XLMS (#5757) and the pack-in child swaps (#6260) are shipped.
3. **PDCL ahead of GBFM**, then exterior worldspace tiles, then space-cell / planet / GBFM / SFTR / PNDT records. `sf_smoke` coverage is unchanged at the baseline.
4. **Actor behaviour.**
   - Bleedout data is decoded, but the recovery rule is unsourced (#6259, open).
   - The NPC stat model is #4453.
   - FaceGen path and tint (#6096 / #6099) are open.
   - Corpse ragdoll pose is out of scope, because no Starfield rig is in `pose_skeleton`'s verified set.

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-10-27.md`. Label SF-2026-10-27-D4-01 `game:starfield` + `legacy-compat` + `physics` + `low` + `bug`.
