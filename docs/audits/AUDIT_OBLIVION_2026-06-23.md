# Oblivion (TES4) Compatibility Audit — 2026-06-23

Working tree: `/mnt/data/src/gamebyro-redux` (branch `main`, clean).
Oblivion game data present at
`/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/` — real-data validation
was exercised (live `nif_stats` over vanilla `Oblivion - Meshes.bsa`).

Dedup baseline: `/tmp/audit/issues.json` (28 open issues) + all prior
`docs/audits/AUDIT_OBLIVION_*.md` reports scanned (latest 2026-06-18).
All 7 dimensions run inline (no nested sub-agents). Execution was a code/compat
audit: NIF parse, archive extract, and ESM parse were exercised against real
data; the rendering/cell-render dimensions were verified by code inspection and
unit tests, not an on-device bench.

## Executive Summary

**Oblivion compatibility is in a clean state. Zero NEW findings this sweep.**
Every HIGH/MEDIUM from prior Oblivion audits that this run could verify is
**fixed and holding**. The single NEW HIGH from the 2026-06-18 audit (the
16-byte ACBS that never branched on `GameKind`) is now resolved (#1650,
`3d5d0d68`) and verified live.

Current compatibility level (cited from `ROADMAP.md` compat matrix + live
`nif_stats`, 2026-06-23):

- **NIF parse** (incl. the v10.x NetImmerse tail): **8026 / 8032 clean
  (99.93%)**, 6 truncated (38 blocks), **0 unknown / 0 recovered-with-unknown**
  over `Oblivion - Meshes.bsa`. The 6 truncated files are exactly the
  pre-Gamebryo v3.3–v4.2 marker meshes baselined in
  `crates/nif/tests/data/block_coverage_baselines/oblivion_truncations.tsv`
  (`marker_arrow/divine/map/radius/temple/travel.nif`). No drift, no growth.
- **Archive extract**: BSA v103 opens + extracts end-to-end (regression guard,
  #699). No regression.
- **ESM parse**: live shared path with FNV/FO3; all Oblivion-specific decode
  branches present and tested (500 plugin lib tests green).
- **Render end-to-end**: interiors render (Anvil Heinrich Oaken Halls). Exterior
  parse + load wired (TES4 worldspace + LAND, game-agnostic); only an on-device
  exterior render bench remains.

Top blockers in priority order (all are *feature gaps*, not bugs — no
finding-grade defects surfaced):

1. **Oblivion exterior on-device render bench** — TES4 worldspace + LAND wiring
   is implemented and game-agnostic (parse + load ✓). Pending: a Vulkan-device
   exterior bench. Same shape FO3 was; *not* a BSA-v103 problem.
2. Distance-banded LOD for Oblivion (`DistantLOD\*.lod` → `_far.nif`) — quality,
   not gameplay; tracked under M35.

## Severity Counts

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 0 |
| **Total**| **0** |

No finding-grade defects were identified that could not be disproven against the
live code. Everything checked either holds correctly or is an already-tracked
feature gap (exterior bench, distant LOD) — not a regression or new bug.

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.5 + v10.x NetImmerse tail)

All checklist items verified correct:

- **`user_version` threshold** is `version >= NifVersion::V10_0_1_8`
  (`crates/nif/src/header.rs:114`). Older NetImmerse files read `num_blocks`
  where `user_version` would sit — correct.
- **BSStreamHeader dual-band guard** (`crates/nif/src/header.rs:137-143`)
  matches the nif.xml `#BSSTREAMHEADER#` condition exactly:
  `V10_0_1_2 || (user_version >= 3 && (V20_2_0_7 || V20_0_0_5 ||
  (V10_1_0_0..=V20_0_0_4 && user_version <= 11)))`. The #170 regression test
  `bs_stream_header_not_read_for_off_spec_version` is present and asserts a
  non-Bethesda v20.1.0.0 / user_version=4 file does NOT read the header.
- **v10.x sub-version constants** all present in `crates/nif/src/version.rs`:
  `V10_0_1_2`, `V10_0_1_8`, `V10_1_0_0`, `V10_1_0_114`, `V10_2_0_0`, `V20_0_0_4`,
  `V20_0_0_5`, `V20_2_0_5`, `V20_2_0_7`.
- **#1509 morph gate** (`crates/nif/src/blocks/controller/morph.rs:90-93`):
  `version >= V10_2_0_0 && version <= V20_0_0_5 && bsver > 9`. The `bsver > 9`
  comparator (not the stale `bsver != 0 && bsver <= 11`) is in place; the
  `doghead.nif` v10.2.0.0/bsver-9 case correctly drops the trailing field.
- **#1506/#1507/#1508 stride-drift family**: `parse_interp_controller_base`
  (Manager-Controlled bool, #1506), `NiPSysData`/emitter (#1507, the
  `life_span_variation` un-bundling in `particle.rs:104-112`), and
  `NiBlendInterpolator`/`ControlledBlock` (#1508) are all in place. Live
  `nif_stats` shows 0 truncation growth — the family holds.
- **`NiTexturingProperty`** reads a raw `u32` count (`properties.rs:211`,
  shader-map count `properties.rs:337`) with **no leading `Has Shader Textures:
  bool`** — the Gamebryo-2.3-authoritative layout. Regression tests
  `parse_ni_texturing_property_retains_oblivion_decal_slots` and
  `parse_ni_texturing_property_with_zero_shader_maps` present.
- **Inline-string pre-v3.3.0.13 handling** (`crates/nif/src/lib.rs:344-374`):
  detected via empty block-type table; logs at `debug` (not `warn`), truncates
  rather than hard-fails on the corrupt-by-design markers.
- **Collision import** (`crates/nif/src/import/collision.rs`):
  `BhkMultiSphereShape` → `Ball`/`Compound` (`:566-592`) and
  `BhkConvexListShape` → `ConvexHull`/`Compound` (`:684-699`) both translate; no
  silent drop. **Dispatch↔resolve parity verified**: all 17 `bhk*Shape` dispatch
  arms in `blocks/mod.rs` have matching `downcast_ref` resolve arms in
  `collision.rs`.
- **#1652 motion type** (`collision.rs:145-153`): `havok_motion_type` maps the
  full canonical enum (1–5/8→Dynamic, 6→Keyframed, 7→Static, 9→
  CharacterKinematic, 0/other→Static). The pre-fix `4 => Keyframed / _ => Static`
  collapse is gone; test `havok_motion_type_maps_full_enum` present.

`cargo test -p byroredux-nif` green.

### Dimension 2 — BSA v103 Archive

Regression guard — holds:

- Version gate accepts only {103, 104, 105} (`crates/bsa/src/archive/open.rs:40`).
- **Folder-record size**: `if version == BSA_V_SKYRIM_SE { 24 } else { 16 }`
  (`open.rs:100`) — v103 AND v104 are 16 bytes, only v105 is 24. Correct (the
  old "v104 = 24" skill text was wrong; code is right).
- **Xbox bit 0x100** ignored for v103: `embed_file_names = version >=
  BSA_V_FO3_SKYRIM && archive_flags & 0x100 != 0` (`open.rs:75`) — the bit only
  means "embed names" on v104+; on v103 it is the Xbox flag and is not consulted.
- Hash validation (`hash::genhash_folder/_file`) wired in debug-build asserts.
- Live extraction over `Oblivion - Meshes.bsa` round-trips 8032 NIFs through the
  v103 path with 0 read failures.

### Dimension 3 — ESM Record Coverage (live path)

All Oblivion-specific branches present, correct, and tested:

- **16-byte ACBS (#1650)** — `crates/plugin/src/esm/records/actor.rs:576`:
  `b"ACBS" if matches!(game, GameKind::Oblivion) && sub.data.len() >= 16`
  arm precedes the `len >= 24` FNV arm (`:587`). Reads flags@0, level@10.
  Confirms the prior-audit HIGH is fixed; match-arm ordering guarantees the
  16-byte arm fires first on Oblivion. Tests
  `oblivion_16byte_acbs_parses_level_and_gender` + `fnv_ignores_16byte_acbs`.
- **MGEF 4-char-code map (#969)** — `magic_effects_by_code: HashMap<[u8;4],u32>`
  (`records/index.rs:140`), Oblivion-only secondary index populated from EFID raw
  bytes (`records/mod.rs:616-627`). Tested in `records/tests.rs:1006+`.
- **CONT 4-byte Oblivion DATA** — `records/container.rs:111`: Oblivion is 4 bytes
  (no flags trailer), FNV/FO3/Skyrim is 5 (weight f32 + flags u8).
- **CLMT WLST** — `records/climate.rs:43-45`: Oblivion ships 8-byte
  `(form_id, chance)` entries; later games ship 12-byte
  `(form_id, chance, global)` (#540).
- **DIAL/INFO TES4 layout (#1304)** — `grup_walker.rs` walks DIAL→INFO via the
  type-7 Topic-Children sub-GRUP; the TES4 TRDT (emotion@0, response#@12) is
  handled distinctly (test `records/tests.rs:87-181`).
- **TES4 20-byte record/GRUP header** — `EsmVariant` detector triggers Oblivion
  mode; builders in `records/tests.rs:1013+` exercise the 20-byte header
  (4-byte vc_info / stamp vs 8 on Tes5+).
- The two ignored Oblivion real-data parity tests
  (`clas_oblivion_knight_against_vanilla`,
  `race_oblivion_data_and_subs_against_vanilla`) remain in tree, gated `#[ignore]`
  pending vanilla `Oblivion.esm` — ROADMAP records them re-running green when
  un-ignored (last confirmed R2 Phase B sweep).

`cargo test -p byroredux-plugin --lib` = 500 passed / 0 failed.

### Dimension 4 — Rendering Path for Oblivion Shaders

- `NiTexturingProperty` → `MaterialInfo` slot routing (base/dark/detail/gloss/
  glow/bump + version-gated normal/parallax @ ≥20.2.0.5, decals) verified in
  `crates/nif/src/blocks/properties.rs:213-313`.
- `NiMaterialProperty` color is raw monitor-space (no `srgb_to_linear`, per
  0e8efc6) — `material/walker.rs:624-639`.
- **#1239 NiPSysEmitter gate** is version-based (`since=10.4.0.1` for
  `radius_variation`, `life_span_variation` unconditional;
  `blocks/particle.rs:104-112`), correctly including Oblivion (v20.0.0.5,
  bsver=11) and the v10.x sub-versions.
- **Typed emitter → runtime path**: `extract_emitter_params` /
  `extract_emitter_rate` (`import/walk/mod.rs:518-519`) feed
  `apply_emitter_params` (`byroredux/src/systems/particle.rs:29`) — the import
  reaches the ECS authoring path, not parses-then-drops.
- **Disney-BSDF gate stays 0**: no Oblivion material authors BGSM/`.mat`;
  `mesh.metalness_override/roughness_override` arrive `None` → `f32::NAN` →
  `MAT_FLAG_PBR_BSDF` 0. The Disney lobe is unreachable for the all-legacy
  Oblivion universe. (Cross-referenced with Dim 5.)

### Dimension 5 — NIFAL Canonical Material Translation

- Single boundary `translate_material` (`byroredux/src/material_translate.rs`)
  carries metalness/roughness as plain `f32` with the `f32::NAN` sentinel
  (`:157-158`); resolved once by `Material::resolve_pbr`
  (`crates/core/src/ecs/components/material.rs:638`). The per-draw
  `Material::classify_pbr` is **deleted** — `static_meshes.rs:301-310` reads
  `m.roughness`/`m.metalness` directly with **no render-time keyword scan**
  (the surviving `classify_pbr_keyword` runs only inside `resolve_pbr`,
  resolve-once).
- `emissive_source = EmissiveSource::Material` for the Oblivion `NiMaterialProperty`
  arm (`material/walker.rs:638`), distinct from the Skyrim/FO4
  `BSLightingShaderProperty` arm. Test file
  `crates/nif/src/import/material/emissive_source_tests.rs` present.
- Disney-gate-stays-0 cross-referenced with Dim 4 (flagged once).

### Dimension 6 — Real-Data Validation

- `nif_stats` over `Oblivion - Meshes.bsa`: total 8032, clean 8026 (99.93%),
  truncated 6 (38 blocks), unknown 0 — matches ROADMAP Oblivion row and the
  checked-in baselines (`per_block_baselines/oblivion.tsv`,
  `block_coverage_baselines/oblivion_truncations.tsv`). No `unknown` growth, no
  `parsed` shrinkage.
- The 6 truncated files are the expected pre-Gamebryo marker tail
  (`marker_arrow/divine/map/radius/temple/travel.nif`); byte-exact match against
  the truncation baseline. No new drift.
- Block-type histogram: 81 distinct types, all `parsed` with `unknown=0`. No new
  types since the last sweep.

### Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks

- The real exterior blocker is the **TES4 worldspace + LAND on-device render
  bench**, *not* BSA v103 (dead framing, closed via #699 — not regenerated).
- `byroredux/src/cell_loader/exterior.rs` carries per-worldspace context with
  Oblivion-specific NAM2 default-water handling (Tamriel sea level Z=0; `:42-49`)
  — parse + load are wired and game-agnostic.
- The `--bsa` CLI path opens + lists + extracts Oblivion archives end-to-end
  (Dim 2 + Dim 6 live extraction).
- Pre-v3.3.0.13 fallback logs at `debug`, not `warn` — no full-sweep spam.
- No Oblivion-only record type is missing from the cell-loader set beyond the
  FNV-aligned set for exterior REFR placement (covered by Dim 3 coverage).

## Blocker Chain (to "Oblivion exterior cell renders")

Interiors already render end-to-end (Anvil Heinrich Oaken Halls). Remaining:

1. TES4 worldspace + LAND wiring — **implemented, game-agnostic, parse+load ✓**.
2. CELL exterior REFR placement — shares the FNV-aligned cell-loader path.
3. **On-device exterior render bench** (Vulkan device + game data) — the one open
   step. Same shape FO3 was. No code defect blocking it.

(The stale "BSA v103 is broken" framing is dead — not regenerated.)

## Regression Guard List (verified still holding this sweep)

| Guard | Location | Status |
|-------|----------|--------|
| v10.x stride-drift family #1506/#1507/#1508 | `controller/`, `particle.rs`, `interpolator.rs` | Hold (0 truncation growth) |
| #1509 `NiGeomMorpherController` `bsver > 9` | `controller/morph.rs:90-93` | Hold |
| `NiTexturingProperty` raw u32 count, no bool gate | `blocks/properties.rs:211,337` | Hold |
| BSStreamHeader dual-band (#170) | `header.rs:137-143` + test | Hold |
| `user_version` threshold V10_0_1_8 | `header.rs:114` | Hold |
| BSA v103 extract (#699) | `archive/open.rs:40,75,100` | Hold (8032 NIFs) |
| #1652 `havok_motion_type` full enum | `import/collision.rs:145-153` | Hold |
| 16-byte ACBS (#1650) | `records/actor.rs:576` | Hold (prior HIGH now fixed) |
| MGEF 4-char-code map (#969) | `records/index.rs:140` | Hold |
| CONT 4-byte / CLMT 8-byte WLST (#540) | `container.rs`, `climate.rs` | Hold |
| TES4 24-byte CTDA (#1548) | `condition.rs` | Hold |
| Disney-BSDF gate stays 0 | `material_translate.rs:157-158` | Hold |
| dispatch↔resolve shape parity (17 arms) | `blocks/mod.rs` ↔ `collision.rs` | Hold |

---

This audit produced **0 findings**. No `/audit-publish` action is required.
