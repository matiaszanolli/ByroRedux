# Oblivion (TES4) Compatibility Audit — 2026-09-05

**Command**: `/audit-oblivion` (as part of `/audit-suite --preset per-game-all`)
**HEAD**: `6fba2b0a` ("Enhance audit documentation for Starfield and related subsystems")
**Game data**: `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/` — present, 9 vanilla mesh-bearing archives (`Oblivion - Meshes.bsa` + 8 DLC), 9,612 NIFs total.
**Method**: static analysis + offline corpus parsing (`nif_stats`, `recovery_trace`, release build). No engine launch. One deliberate one-line patch to `crates/nif/src/blocks/skin.rs` for causality proof, **reverted** — `git status` shows no modified tracked files.

## Scope caveats

- **ESM real-data validation was NOT run.** `crates/plugin`'s Oblivion parity
  tests (`clas_oblivion_knight_against_vanilla`,
  `race_oblivion_data_and_subs_against_vanilla`) parse the whole 277 MB
  `Oblivion.esm`; the machine had 11 GB available and 28 GB of swap already in
  use with sibling audit agents running, and the known failure mode is an
  OOM-kill of the session. Dimension 3 is therefore **static-only** this run.
- Dimensions 4/5 were verified by code reading + the corpus histogram, not by a
  rendered frame (standing rule against parallel engine launches).

---

## Executive Summary

**Oblivion's NIF clean-parse rate has regressed from 100 % to 92.41 %, and this
audit has proven the cause by patch-and-revert bisection.** The regression
landed in `d49cd88b` ("Fix #3691: reserve skinning parse buffers", 2026-09-03),
a one-line pre-reservation in the strip branch of `NiSkinPartition::parse`.

Measured across all nine vanilla archives:

| | HEAD (`6fba2b0a`) | With the one-line fix | Checked-in baseline |
|---|---|---|---|
| NIFs parsed | 9,612 | 9,612 | 9,612 |
| Clean | **8,882 (92.41 %)** | **9,612 (100.00 %)** | 9,612 (100 %) |
| Truncated / recovered | 730 | 0 | 0 |
| `NiSkinPartition` parsed / unknown (`Oblivion - Meshes.bsa`) | 1,016 / 74 | **1,596 / 0** | **1,596 / 0** |
| `NiNode` parsed / unknown (same) | 22,200 / 464 | **25,244 / 0** | **25,244 / 0** |

The fixed build reproduces the checked-in `per_block_baselines/oblivion.tsv`
**byte-for-byte on every affected row**, and the checked-in
`block_coverage_baselines/oblivion_truncations.tsv` (`truncating=0
parsed=9612`) exactly. That is as tight as attribution gets: nothing else
changed, and the baseline is the pre-regression ground truth.

**This confirms the FO3 audit's PERF-D6-NEW-01 attribution and closes
PERF-D6-NEW-02.** The Oblivion clean-rate drop is not documentation rot — it
is a live parser regression, and Oblivion is the **worst-hit** game in the
lineage because it is the only shipped title with no per-block size table:
where FO3/FNV/Skyrim recover one block via `block_size` seek, Oblivion
**discards the entire remainder of the file**. That is the `_audit-severity.md`
CRITICAL row verbatim ("Data loss — corrupted NIF parse state affecting
subsequent blocks").

Blast radius on `Oblivion - Meshes.bsa` alone: **4,693 blocks lost** across 578
files, concentrated in worn armour / clothing / creature bodies and their
Havok ragdoll chains (226 `bhkRigidBody`, 226 `bhkCollisionObject`, 220
`bhkBoxShape`, 141 `bhkLimitedHingeConstraint` — 24 % of Oblivion's entire
limited-hinge population).

Everything else this audit checked is **clean**: BSA v103 open + extract
(9,612 / 9,612 NIF extractions, 0 failures), the whole v10.x stride-drift
regression-guard family, the `#170` BSStreamHeader dual-band, the
`user_version >= V10_0_1_8` threshold, `NiTexturingProperty`'s raw `u32` count,
the `havok_motion_type` canonical enum, the `PARALLAX_ALPHA_HEIGHT_BIT`
masking in both POM marchers, the Disney-BSDF gate staying unreachable, and
the Oblivion-only `_far.nif` / `distantlod\` placement-LOD route.

### Top blockers, priority order

1. **OBL-2026-09-05-D6-01** (CRITICAL) — the `NiSkinPartition` strip-branch
   reservation regression. Root cause owned by `/audit-fo3` (PERF-D6-NEW-01);
   this report supplies the Oblivion measurement and blast radius.
2. **OBL-2026-09-05-D1-01** (MEDIUM) — the sizeless runtime-size-cache recovery
   applies an unvalidated median skip; on HEAD it silently converted 74
   hard failures into "recoverable" garbage and cascaded 464 more.
3. **#3567** (MEDIUM, already open) — Oblivion's `APPLY_HILIGHT2` normal-map
   alpha is consumed as both parallax height and specular mask. Re-verified
   this run: still true.

---

## Findings

### CRITICAL

#### OBL-2026-09-05-D6-01 — Oblivion loses 730 of 9,612 vanilla meshes to `d49cd88b`'s skin-partition reservation, and 4,693 blocks with them
- **Severity**: CRITICAL
- **Dimension**: 6 — Real-Data Validation (with Dimension 1 mechanism)
- **Location**: `crates/nif/src/blocks/skin.rs` — the `num_strips > 0` branch of `NiSkinPartition::parse` (`stream.allocate_vec_sized::<[u16; 3]>(num_triangles as u32)?`), introduced 2026-09-03 in `d49cd88b`. Failure surfaces through `Stream::check_alloc` (`crates/nif/src/stream.rs`, the `bytes > remaining` arm) and is amplified by the sizeless-recovery path in `crates/nif/src/lib.rs` (`parse_nif`'s `Err` arm, the `truncated = true; break` fall-through).
- **Status**: NEW **for the Oblivion measurement**. Root cause is **Existing (cross-audit): PERF-D6-NEW-01** in `docs/audits/AUDIT_NIF_2026-09-04.md`, re-raised today by `/audit-fo3`. Also a **Regression of the fix for `#3691`** (CLOSED). Do not double-file the root cause — file the Oblivion consequence against the same issue, or add this measurement to it.
- **Description**: `allocate_vec_sized::<[u16; 3]>(n)` bounds `n * 6` bytes against the stream's remaining bytes. In the strip branch those triangles are *generated* by `strip::destrip` from u16 index arrays that cost ~2 B per emitted triangle, so the bound over-demands by ~3×. On Oblivion, where `NifHeader.block_sizes` is empty, the resulting `Err` has no `block_size` recovery: `parse_nif` sets `truncated = true` and **discards every subsequent block**.
- **Evidence** (all measured today, release build, all nine archives):

  Per-archive, HEAD:

  | Archive | NIFs | clean | truncated | stopped at `NiSkinPartition` |
  |---|---|---|---|---|
  | `Oblivion - Meshes.bsa` | 8,032 | 7,454 | 578 | 504 |
  | `DLCShiveringIsles - Meshes.bsa` | 1,438 | 1,302 | 136 | 118 |
  | `Knights.bsa` | 75 | 60 | 15 | 15 |
  | `DLCHorseArmor.bsa` | 4 | 3 | 1 | 1 |
  | 5 remaining DLC archives | 63 | 63 | 0 | 0 |
  | **Total** | **9,612** | **8,882 (92.41 %)** | **730** | **638** |

  In `Oblivion - Meshes.bsa`, **504 of 509** parse-stopping blocks are
  `NiSkinPartition` (the other 5 are `NiNode`, downstream cascade). All **505**
  `check_alloc` rejections request a byte count that is an **exact multiple of
  6** — `size_of::<[u16; 3]>()` — e.g. `NIF requested 468-byte read at position
  56258, only 377 bytes remaining` (78 triangles × 6 B demanded; the strip
  payload actually costs ≈ 156 B, well inside the 377 available).

  **Causality proof.** Single-line patch, strip branch only:
  `allocate_vec_sized::<[u16; 3]>(n)` → `allocate_vec_min_bytes::<[u16; 3]>(n, 2)`.
  Re-measured all nine archives: **9,612 / 9,612 clean, 0 truncated, 0
  `NiUnknown`**, and the per-block histogram matches
  `crates/nif/tests/data/per_block_baselines/oblivion.tsv` exactly on every
  affected row (`NiSkinPartition 1596 0`, `NiNode 25244 0`, `NiSkinData 1596 0`,
  `NiSkinInstance 1596 0`). Patch reverted; tree clean.

  **Block loss in `Oblivion - Meshes.bsa` (HEAD vs patched), 4,693 total:**

  | Lost | Type | HEAD parsed / unknown | Fixed parsed |
  |---:|---|---|---:|
  | 3,044 | `NiNode` | 22,200 / 464 | 25,244 |
  | 580 | `NiSkinPartition` | 1,016 / 74 | 1,596 |
  | 231 | `NiExtraData` | 52,101 / 0 | 52,332 |
  | 226 | `bhkRigidBody` | 8,504 / 0 | 8,730 |
  | 226 | `bhkCollisionObject` | 8,504 / 0 | 8,730 |
  | 220 | `bhkBoxShape` | 1,235 / 0 | 1,455 |
  | 141 | `bhkLimitedHingeConstraint` | 451 / 0 | 592 |
  | 6 | `bhkConvexVerticesShape` | 2,085 / 0 | 2,091 |
  | 19 | `NiTriShape` / `NiTriShapeData` / `NiTriStripsData` / `NiMaterialProperty` / `NiSkinData` / `NiSkinInstance` / `NiTransformController` / `NiTexturingProperty` / `NiSourceTexture` / `NiAmbientLight` | — | — |

  **Named vanilla casualties** (`nif_stats` truncated-file examples, and direct
  `recovery_trace` runs): `meshes\creatures\troll\troll.nif`,
  `meshes\creatures\goblin\shamanchest.nif` (39 blocks dropped),
  `meshes\creatures\goblin\handrberserker.nif` (15),
  `meshes\armor\daedric\m\cuirass.nif`, `meshes\armor\elven\m\greaves.nif` (10),
  `meshes\armor\fur\f\helmet.nif` (5),
  `meshes\armor\townguardcho\m\cuirass_gnd.nif` (12),
  `meshes\clothes\middleclass\04\m\shirt_gnd.nif` (18),
  `meshes\clothes\robelcgrey\m\robelcgreym_gnd.nif` (12),
  `meshes\clothes\robemcblack\m\robemcblack_gnd.nif` (12),
  `meshes\clothes\lowerclass\{08,12,15}\f\shirt.nif`,
  `meshes\clothes\amulet\{amuletgold,thornblademedallion,amuletjadejeweled}.nif`,
  `meshes\oblivion\clutter\containers\clawstandcontainer.nif`.
  The population is dominated by worn ARMO/CLOT meshes, their `_gnd` ground
  models, and creature bodies — i.e. the equipment/outfit rendering surface.
- **Impact**:
  - **Oblivion-specific amplification.** 580 `NiSkinPartition` blocks actually
    fail; **4,693** blocks are lost — an **8.1×** amplification that exists
    only because Oblivion ships no `block_sizes` table. FO3/FNV/Skyrim lose one
    block per failure and stay 100 % clean; Oblivion loses the file's tail.
  - **Physics.** 24 % of Oblivion's `bhkLimitedHingeConstraint` population and
    ~2.6 % of its rigid bodies vanish. The PHYSAL ragdoll articulation for the
    affected creatures/actors is built from a truncated constraint chain — a
    silent behavioural regression with no parse-level error.
  - **Rendering.** 3,044 `NiNode` blocks — whole scene-graph subtrees on
    equipment meshes — never reach `import_nif_scene`.
  - **Silent.** `MIN_RECOVERABLE_RATE = 1.0` gates *recoverable*, not *clean*,
    and stays green. The one gate that would go red
    (`crates/nif/tests/per_block_baselines.rs`) is `#[ignore]`d, needs game
    data, and has no CI runner — so the regression shipped invisibly.
  - **Cross-game blast radius confirmed.** The FO3 finding's severity should be
    escalated on this evidence: it is not a latent bound-tightening nit, it is
    a live 7.6-point content-loss regression on a shipped title.
- **Related**: PERF-D6-NEW-01 / PERF-D6-NEW-02 (`docs/audits/AUDIT_NIF_2026-09-04.md`); `#3691` (CLOSED, whose fix this is); `#2523` (the `allocate_vec_sized` / `allocate_vec_min_bytes` split this violates); `#1549` (de-strip); `#324` (the sizeless recovery path this cascades through); `ROADMAP.md:605` (documents 100 % / 8,032 of 8,032, now wrong).
- **Suggested Fix**: `stream.allocate_vec_min_bytes::<[u16; 3]>(num_triangles as u32, 2)?` — 2 B is the honest per-triangle minimum for strip-derived faces (verified: restores 9,612 / 9,612 and reproduces the checked-in baselines). Add a regression test whose partition authors a strip of ≥ 5 indices with < 3 × `len` bytes trailing. Separately, add a **clean**-rate floor to `run_game` in `crates/nif/tests/parse_real_nifs.rs` so a clean-rate slide cannot hide behind a green recoverable gate again.

---

### MEDIUM

#### OBL-2026-09-05-D1-01 — the sizeless runtime-size-cache recovery applies a median skip with no plausibility check, converting hard failures into silent mis-alignment
- **Severity**: MEDIUM
- **Dimension**: 1 — NIF Version Handling (Oblivion-only code path)
- **Location**: `crates/nif/src/lib.rs`, `parse_nif`'s `Err` arm — the `parsed_size_cache` branch (`stream.set_position(start_pos)` then `stream.skip(median_size)`), reachable only when `header.block_sizes.is_empty()`.
- **Status**: NEW. Hardening of `#324` (CLOSED, "M2: Oblivion synthetic skip-table to prevent cascading parse failure").
- **Description**: When a block fails to parse on a game with no per-block size table, the recovery takes the **median of previously-observed consumed sizes for the same type in the same file** and skips that many bytes. `NiSkinPartition` is a variable-size block whose length scales with vertex/triangle/bone counts, so the median of one partition is a poor estimate of another. Nothing validates the post-skip position — no magic-number check, no "does the next block's type index look sane" test, no bound on how far the median may differ from the failed block's own partial consumption. The block is replaced with `NiUnknown`, `recovered_blocks` is bumped, and parsing continues from a position that may be arbitrarily wrong.
- **Evidence**: On HEAD, `Oblivion - Meshes.bsa` reports `recovered: 538 (2 types with partial unknown)`: `NiSkinPartition` 74 and `NiNode` **464**. With the D6-01 fix applied, both drop to **0** — so all 464 `NiNode` substitutions are *downstream cascade* from a mis-aligned stream, not independent parse failures. The mechanism turns one upstream parser bug into hundreds of `NiUnknown` blocks that the recoverable-rate gate still scores as a success. Worse than the counted case: a median skip that happens to land on a *plausible* boundary produces a block that parses "successfully" from the wrong offset and is never counted at all.
- **Impact**: Oblivion (and pre-Gamebryo NetImmerse content) only — no other shipped title reaches this branch. Defense-in-depth: any future parser bug in a variable-size Oblivion block gets its damage silently multiplied instead of stopping at one truncation, and the multiplication is invisible to every current gate. The 464:74 ratio measured today is the observable size of that multiplier.
- **Related**: `#324`; `#568` (clean-vs-recoverable split); OBL-2026-09-05-D6-01 (the bug that exercised it); PERF-D6-NEW-02.
- **Suggested Fix**: Bound the median skip — reject it when `median_size` deviates from the failed block's own `consumed` by more than some factor, or when the resulting position does not land on a boundary consistent with the next block's expected type. At minimum, promote the per-type recovery rollup from `warn!`-summary to a value on `NifScene` so a caller can distinguish "recovered cleanly" from "recovered by guessing".

---

### Existing — verified still true this run

#### #3567 — Oblivion `APPLY_HILIGHT2` normal-map alpha is consumed as both parallax height and specular mask
- **Status**: Existing: **#3567** (OPEN). Premise re-verified against current code.
- **Evidence**: `byroredux/src/render/static_meshes.rs` sets
  `PARALLAX_ALPHA_HEIGHT_BIT` when `parallax_map_index != 0 && normal_has_alpha
  && material.parallax_height_in_alpha`. The gloss-slot rebind gate,
  `normal_alpha_spec_binding_applies`
  (`byroredux/src/material_translate.rs`), takes `material: Option<&Material>`
  but consults only `env_map_scale`, `material_kind`, `normal_map_index`,
  `gloss_map_index` and `normal_has_alpha` — it **never reads
  `Material::parallax_height_in_alpha`**. An Oblivion `APPLY_HILIGHT2` material
  with a normal map, no gloss map and an alpha-bearing normal therefore sets
  both `PARALLAX_ALPHA_HEIGHT_BIT` and `NORMAL_ALPHA_SPEC_BIT` on the same
  channel. Unchanged since the issue was filed.

#### #3848 — `oblivion_ruleset` is production-unreachable
- **Status**: Existing: **#3848** (OPEN). Verified, **not** re-filed.
- **Evidence**: `CharacterRulesProfile::OBLIVION`
  (`crates/core/src/character/profile.rs`) carries `ruleset:
  RulesetBuilder::None`, and `build_ruleset`'s `RulesetBuilder::None` arm
  `return None`s before touching `oblivion_ruleset`
  (`crates/core/src/character/tes.rs`), which is otherwise complete and
  unit-tested end to end.
- **Additional Oblivion-specific consequence (for #3848, not a new issue)**:
  the same profile also carries `npc_stats: NpcStatModel::None` **and**
  `creature_stats: NpcStatModel::None`, and
  `derive_npc_actor_values`
  (`crates/plugin/src/esm/records/actor_value_derive.rs`) returns `Vec::new()`
  for that arm. So `#1650`'s Oblivion 16-byte-ACBS recovery of `level` and
  `acbs_flags`, and the `is_oblivion`-gated `ATTR` / `DNAM` / `VNAM` / `PNAM` /
  `UNAM` / `XNAM` decode in
  `crates/plugin/src/esm/records/actor/mod.rs`, currently have **no consumer**
  on the population side either — the data is parsed correctly and then
  discarded. This is deliberate and pinned by
  `oblivion_creatures_select_no_stat_model`, and is already listed in
  `docs/feature-matrix.md`, so it is a scope note rather than a defect; it
  belongs on #3848 as the second half of what "unwired" costs.

#### Oblivion ESM findings from the 2026-08-30 sweep — still open, not re-measured
`#3617` (LVSP has a `RecordType` constant but no parser, 306 leveled-spell
lists), `#3616` (only the last response of a multi-response INFO survives,
4,617 segments lost), `#3614` (TCLF / NAME / CTDT dropped on INFO). All three
require an `Oblivion.esm` parse to re-measure, which this run deliberately
skipped (see Scope caveats). No code change touching them was observed in the
static pass.

---

## Regression Guard List — verified holding

| Guard | Where | Verdict |
|---|---|---|
| `user_version` only read for `version >= V10_0_1_8` | `crates/nif/src/header.rs`, the `user_version` binding | ✅ exact |
| BSStreamHeader dual-band matches nif.xml `#BSSTREAMHEADER#` (`#170`) | `crates/nif/src/header.rs`, `has_bs_stream_header` | ✅ `V10_0_1_2` OR (`user >= 3` AND (`V20_2_0_7` \| `V20_0_0_5` \| (`V10_1_0_0..=V20_0_0_4` AND `user <= 11`))) |
| v10.x sub-version constants present as gate boundaries | `crates/nif/src/version.rs` | ✅ `V3_3_0_13`, `V4_2_2_0`, `V5_0_0_1`, `V10_0_1_2`, `V10_1_0_0`, `V10_1_0_106`, `V10_1_0_114`, `V10_2_0_0`, `V20_0_0_4`, `V20_0_0_5` all live |
| `NiGeomMorpherController` gates on `bsver > 9` (`#1509`) | `crates/nif/src/blocks/controller/morph.rs` + `MORPH_LEGACY_CUTOFF` | ✅ `MORPH_LEGACY_CUTOFF == 10`, gate is `bsver >= MORPH_LEGACY_CUTOFF` |
| v10.x stride-drift family (`#1506`/`#1507`/`#1508`) | whole-corpus outcome | ✅ 0 truncations across 9,612 NIFs once D6-01's regression is removed; 638 of the 730 HEAD truncations are D6-01, and the residual 92 are its `NiNode` cascade + recovered-only files — **no member of the v10.x family reappeared** |
| `NiTexturingProperty` reads a raw `u32` count, no `Has Shader Textures` bool | `crates/nif/src/blocks/properties.rs`, `texture_count` | ✅ raw `read_u32_le`; corpus still shows `texture_count == 7` on all 30,121 instances |
| Pre-5.0.0.1 inline block-type names log at `debug`, not per-block `warn` | `crates/nif/src/lib.rs`, the `inline_type_names` branch | ✅ one `debug!` per file; `warn!` only on a mid-file inline-name read failure |
| `NifVariant::detect` `(V20_0_0_4, uv=11)` ambiguity warning is one-shot | `crates/nif/src/version.rs`, the `std::sync::Once` block | ✅ fires once per process, not per file — no sweep spam |
| BSA v103 recognised; rejection only outside {103,104,105} | `crates/bsa/src/archive/open.rs` | ✅ |
| Folder-record size is 16 B for v103 **and** v104, 24 B only for v105 | `crates/bsa/src/archive/open.rs`, `folder_record_size` | ✅ `if version == BSA_V_SKYRIM_SE { 24 } else { 16 }` |
| v103 "Xbox archive" flag ignored for embedded names | `crates/bsa/src/archive/open.rs`, `embed_file_names` | ✅ gated `version >= BSA_V_FO3_SKYRIM` |
| BSA v103 extraction stays at 100 % (`#699`) | live sweep | ✅ 9,612 / 9,612 NIF extractions across 9 archives, **0 extract failures** |
| `havok_motion_type` maps the full nif.xml `hkMotionType` enum (`#1652`) | `crates/nif/src/import/collision/mod.rs` | ✅ 1–5/8→Dynamic, 6→Keyframed, 7→Static, 9→CharacterKinematic, 0/other→Static; pinned by `havok_motion_type_maps_full_enum` |
| `BhkMultiSphereShape` / `BhkConvexListShape` resolve, don't fall out silently | `crates/nif/src/import/collision/shape.rs` | ✅ both have downcast arms in `resolve_shape_inner` (16 arms total) |
| Oblivion 16-byte ACBS arm precedes the ≥24-byte arm (`#1650`) | `crates/plugin/src/esm/records/actor/mod.rs` | ✅ `b"ACBS" if matches!(game, GameKind::Oblivion) && sub.data.len() >= 16` sits before the Skyrim and generic ≥24 arms |
| Oblivion-specific ESM decode branches still present | `actor/mod.rs` (`is_oblivion` ATTR/DNAM/VNAM/PNAM/UNAM/XNAM/DATA), `climate.rs` (3-entry WLST), `items.rs` (Oblivion ARMO/WEAP/AMMO/CLOT arms), `cell/tests/cell.rs` (`parse_oblivion_xcll`) | ✅ present |
| `NiMaterialProperty` tags `EmissiveSource::Material` (legacy arm) | `crates/nif/src/import/material/legacy_properties.rs` | ✅ pinned by `emissive_source_tests.rs` |
| No per-draw `classify_pbr`; `Material::resolve_pbr` resolves once | `crates/core/src/ecs/components/material.rs` | ✅ `Material::classify_pbr` deleted; only the free `classify_pbr_keyword`, called from `resolve_pbr` |
| `MAT_FLAG_PBR_BSDF` unreachable across the all-legacy Oblivion universe | `crates/nif/src/import/material/mod.rs` (the `has_material_data` gate) | ✅ zero Oblivion materials author BGSM/`.mat`; Disney lobe unreachable |
| `PARALLAX_ALPHA_HEIGHT_BIT` (bit 31) masked by **both** POM marchers | `crates/renderer/shaders/include/material_sampling.glsl` (raster) and `include/ray_hit.glsl` (secondary ray) | ✅ both `& ~PARALLAX_ALPHA_HEIGHT_BIT` before indexing, both read the flag separately; `triangle.frag` masks at both of its test sites |
| `normal_has_alpha` gate on the parallax bit (`#3562`) | `byroredux/src/render/static_meshes.rs` | ✅ bit set only when the bound normal texture's DDS format actually carries alpha |
| Typed particle emitter blocks parse on Oblivion (`#1239`) | `crates/nif/src/blocks/particle.rs` + corpus | ✅ `NiPSysEmitter` 547 / 0 unknown, `NiPSysEmitterCtlr` 547 / 0, `NiPSysGrowFadeModifier` 449 / 0, `NiParticleSystem` 547 / 0, `NiPSysBlock` 4,232 / 0 in the fixed build; `extract_emitter_params` / `extract_emitter_rate` → `apply_emitter_params` chain intact |
| `_far.nif` placement LOD is Oblivion-only | `byroredux/src/cell_loader/placement_lod.rs`, `placement_lod_supported` | ✅ `game == GameKind::Oblivion`, pinned by `placement_lod_supported_is_oblivion_only`; the vanilla data backs it — `Oblivion - Meshes.bsa` carries **130** `_far.nif` and **9,944** `distantlod\` entries, and `archive_path_matches_vanilla_filenames` asserts the exact vanilla naming |

---

## Blocker Chain

Interiors already render end-to-end (Anvil Heinrich Oaken Halls) and exterior
cells already render on-device (Tamriel `(0,0)` radius 1, 6,043 entities /
2,355 draws, 2026-08-12 EX-01/EX-05). The chain to *first render* is closed —
do not regenerate the stale "BSA v103 broken" or "TES4 worldspace wiring
missing" framings.

The live chain today is:

1. **Restore the 730 lost meshes** — OBL-2026-09-05-D6-01. Until this lands,
   any readiness matrix run on Oblivion is measuring a corpus with 7.6 % of its
   meshes truncated, and 24 % of its ragdoll constraints missing. **This now
   gates the readiness matrix, not the other way round.**
2. **Re-baseline and re-gate** — regenerate nothing (the checked-in baselines
   are already correct and are what proves the regression); instead add the
   *clean*-rate floor so the next slide is caught automatically.
3. **Repeatable readiness matrix** (`#2377` / `#2368`) — the pre-existing
   remaining chain, now unblocked by (1).
4. Any placement / LOD gaps that matrix surfaces.

---

## Verified-Clean Areas (no finding)

- **Dimension 2 — BSA v103.** Regression guard holds in full. All nine
  archives opened; 9,612 of 9,612 NIF entries extracted with zero failures.
  Version acceptance, folder-record sizing, archive-flag semantics and the
  hash function are all unchanged and correct.
- **Dimension 1 — version handling.** Every guard in the checklist verified
  (table above). No new version-gate drift found. The one live defect on this
  dimension is D1-01's recovery-path hardening gap, filed above.
- **Dimension 4/5 — render + NIFAL.** No new finding. The `APPLY_HILIGHT2`
  route reaches the GPU as designed post-`#3596`/`#3562`, both marchers mask
  the flag bit, `MAT_FLAG_PBR_BSDF` is unreachable for Oblivion, PBR resolves
  exactly once, and the legacy `NiMaterialProperty` emissive arm is intact.
  The single open defect is `#3567`, re-verified rather than re-filed.
- **Dimension 7 — exterior + quirks.** `placement_lod_supported` is
  Oblivion-only and backed by real vanilla data; the pre-Gamebryo inline-name
  path logs at `debug`; the `NifVariant` ambiguity warn is one-shot. The
  `#1219` `(V20_0_0_4, uv=11)` ambiguity remains harmless
  (`havok_scale_for` maps Oblivion and Fallout3 to the same 7.0 scale) —
  unchanged, no action.
- **Dimension 3 — ESM.** Static verification only (see Scope caveats). All
  Oblivion-specific decode branches named in the checklist are present and
  structurally correct. Real-data parity was **not** re-run; #3614/#3616/#3617
  remain open and unmeasured this cycle.

---

## Documentation drift observed (fold into the fix, don't file separately)

- `ROADMAP.md:605` records Oblivion at "**100%** (8 032 / 8 032) · recover
  100%". Live measurement is 7,454 / 8,032 (92.80 %) for that archive and
  8,882 / 9,612 (92.41 %) across the full corpus. The row becomes correct again
  the moment D6-01 is fixed — it should be re-verified, not rewritten.
- `crates/nif/examples/nif_stats.rs`'s `--tsv` histogram still keys parsed
  blocks on the parsed struct's `block_type_name()`, while
  `crates/nif/tests/per_block_baselines.rs` keys on wire RTTI since `#3326`.
  The two therefore emit different type-name sets for the same corpus
  (`NiPSysBlock` / `NiExtraData` collapses vs. per-wire-type rows). Already
  reported by `/audit-nif` as D3-03; noted here only because it makes an
  auditor's first instinct — diff the tool's TSV against the checked-in
  baseline — misleading on every row except the non-aliased ones.

---

## Suggested next step

```
/audit-publish docs/audits/AUDIT_OBLIVION_2026-09-05.md
```

Label OBL-2026-09-05-D6-01 `critical` · `bug` · `nif-parser` · `nif` ·
`legacy-compat` · `game:oblivion` (and cross-link the FO3/NIF root-cause
issue rather than opening a second one for the same line of code).
Label OBL-2026-09-05-D1-01 `medium` · `bug` · `nif-parser` · `nif` ·
`legacy-compat` · `game:oblivion`.
