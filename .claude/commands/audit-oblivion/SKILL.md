---
description: "Per-game audit of Oblivion (TES4) compatibility — NIF v20.0.0.5 + the v10.x NetImmerse family, BSA v103, live ESM path"
argument-hint: "--focus <dimensions>"
---

# Oblivion Compatibility Audit

Deep audit of ByroRedux readiness for **The Elder Scrolls IV: Oblivion** content.

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, game-data locations,
key reference docs, methodology, the path-reference convention, deduplication
rules, and the base finding format. See `.claude/commands/_audit-severity.md`
for the severity scale. Do not restate any of it here.

## What Makes Oblivion Different (the audit surface)

Oblivion is the **oldest** title in the lineage and exercises code paths no
other game reaches. Two NIF eras coexist in vanilla `Oblivion - Meshes.bsa`:

1. **The retail body** — NIF **v20.0.0.5** (`bsver=11`): inline strings, u16
   flags, **no per-block size table**. This is what most clutter / architecture
   / creature meshes are.
2. **The NetImmerse tail** — a long tail of **v10.x** sub-versions (down to
   pre-Gamebryo v3.3.0.13) authored years earlier. These have subtly different
   field layouts gated by tight version bands in nif.xml, and — like v20.0.0.5 —
   **no block_size table**, so a single N-byte under-read truncates the entire
   downstream subtree.

The v10.x tail is the Oblivion-unique risk. Retail FO3/FNV/Skyrim are all
BS202 / 20.x and never touch these bands; a regression here is **silent and
Oblivion-only**. The v10.x stride-drift family (#1506 NiInterpController /
NiQuatTransform, #1507 NiPSysData + emitter, #1508 NiBlendInterpolator +
ControlledBlock, #1509 NiGeomMorpherController `bsver > 9` gate) is **resolved**
as of 2026-06-13; Oblivion-Meshes went from 56 truncated → ~10. This audit
treats that family as a **regression-guard set**, not open work.

| Aspect            | Current state (verify, don't trust this table blindly)              |
|-------------------|--------------------------------------------------------------------|
| NIF format        | v20.0.0.5 retail + v10.x NetImmerse tail (both sizeless)            |
| BSA format        | v103 — opens AND extracts cleanly across all vanilla archives (regression guard, #699) |
| ESM parser        | **Live** — `crates/plugin/src/esm/` with `parse_esm_cells` walker + ~25 record types, several with Oblivion-specific decode branches. NOT a stub (the per-game *legacy/tes4.rs* stub was removed under #390). |
| Parse rate        | See ROADMAP.md Oblivion compat-matrix row (drifts after each sweep; do NOT hardcode a number here). Post-v10.x-family: ~10 residual NetImmerse truncations + 1 corrupt-by-design hard-fail (`#698` closed). |
| Cell loading      | Interior renders end-to-end (Anvil Heinrich Oaken Halls). Exterior blocked on TES4 worldspace + LAND wiring (same shape FO3 was — *not* BSA v103) |
| Reference data    | `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`           |

### Known Quirks (do NOT re-derive — verify still hold)

- **`user_version` only exists for files ≥ v10.0.1.8.** Older NetImmerse files
  have `num_blocks` where `user_version` would be. Confirm the
  `version >= NifVersion::V10_0_1_8` guard in `crates/nif/src/header.rs`.
- **BSStreamHeader presence is the nif.xml dual-band condition** (post-#170),
  NOT the old `version == 10.0.1.2 || user_version >= 3`. The live guard is
  `version == V10_0_1_2 || (user_version >= 3 && (version ∈ {V20_2_0_7,
  V20_0_0_5} || (V10_1_0_0 <= version <= V20_0_0_4 && user_version <= 11)))`.
  A v20.0.0.5 Oblivion file with `user_version >= 3` reads the header; a
  non-Bethesda file outside the band must NOT (regression of #170).
- **`NiTexturingProperty` reads a `uint` count directly** — do NOT add a
  leading `Has Shader Textures: bool`. nif.xml is wrong here; the Gamebryo 2.3
  source is authoritative. Regressing this breaks every Oblivion clutter / book
  / furniture mesh. Test lives in `crates/nif/src/blocks/properties_tests.rs`.
- **Oblivion has no per-block size table.** A single mis-aligned read poisons
  every subsequent block — there is no `block_size`-advance recovery path
  (unlike 20.2.0.7+). This is why every v10.x stride-drift bug truncated whole
  subtrees rather than one field.
- **Pre-Gamebryo v3.3.0.13 files** (e.g. `meshes/marker_*.nif`) inline type
  names as sized strings instead of a global type table. Parser returns an
  empty `NifScene` with a debug log rather than failing.
- **NetImmerse v10.x leading group_id.** For versions in [10.0.0.0,
  10.1.0.114), each block is preceded by a 4-byte group_id (`00 00 00 00`);
  block content starts AFTER it. Mixing stream-relative vs file offsets is the
  classic false-trail when chasing stride drift (see memory note
  `nif_v10x_stride_drift_resolved`).
- **`as_ni_node` walker must unwrap every NiNode subclass** (`BSOrderedNode`,
  `NiBillboardNode`, `NiSwitchNode`, `NiLODNode`, …) so scene-graph walks
  descend correctly.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 7.

## Phase 1: Setup

1. Parse `$ARGUMENTS`.
2. `mkdir -p /tmp/audit/oblivion`.
3. Fetch dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/issues.json`.
4. Confirm `Oblivion/Data/` exists; if not, note which dimensions lose real-data validation.
5. Read the current Oblivion row in `ROADMAP.md` (compat matrix) and
   `docs/feature-matrix.md` so every status claim cites live numbers, not this file.

## Phase 2: Launch Dimension Agents (parallel)

Dimensions are ordered by Oblivion-specific risk: NIF version handling first
(the v10.x tail is the unique surface), then archive, ESM, render, real-data.

### Dimension 1: NIF Version Handling — v20.0.0.5 + the v10.x NetImmerse Tail
**Subagent**: `legacy-specialist`
**Entry points**: `crates/nif/src/header.rs`, `crates/nif/src/version.rs`, `crates/nif/src/stream.rs`, `crates/nif/src/blocks/`, `docs/legacy/`
**Checklist**:
- `user_version` threshold (`V10_0_1_8`) and the BSStreamHeader dual-band guard
  in `header.rs` match nif.xml (see Known Quirks). The #170 regression test in
  `crates/nif/src/header.rs` (tests module) must still assert a non-Bethesda
  out-of-band file does NOT read the header.
- The v10.x sub-version constants exist and are used as gate boundaries:
  `V10_0_1_2`, `V10_1_0_0`..`V10_1_0_114`, `V10_2_0_0`, `V20_0_0_4`,
  `V20_0_0_5` in `crates/nif/src/version.rs`.
- **#1509 regression guard** (`crates/nif/src/blocks/controller/morph.rs`):
  `NiGeomMorpherController` gates its trailing field on `bsver > 9` (NOT the old
  `bsver != 0 && bsver <= 11`). `doghead.nif` is v10.2.0.0 **bsver 9** and must
  keep the field; an off-by-band gate restarts `NiMorphData` 24 B late and
  truncates the file. Tests: `crates/nif/src/blocks/controller/path_lookat_tests.rs`.
- **#1506/#1507/#1508 regression guards** — the resolved stride-drift family.
  Each was a `since`/`until` field gated on the wrong comparator dropping N
  bytes. Confirm `NiInterpController`/`NiQuatTransform`, `NiPSysData` + emitter,
  and `NiBlendInterpolator` + ControlledBlock all still land exactly on the next
  block boundary in the v10.x bands. Any new truncation growth on Oblivion-Meshes
  is a regression of this family — escalate, don't re-derive.
- `NiTexturingProperty` reads u32 count raw, no bool gate (regression guard).
- Inline-string block-type handling for pre-v3.3.0.13.
- u16 vs u32 flag width per block in the v20.0.0.5 vs v10.x layouts.
- Oblivion-only / legacy block types dispatched in `crates/nif/src/blocks/mod.rs`:
  NiKeyframeController, NiSequenceStreamHelper, NiBillboardNode + NiNode
  subclasses, NiLight hierarchy, NiUVController, NiCamera, NiTextureEffect, the
  legacy particle stack, the BSShader*Property aliases.
- **Collision import** (`crates/nif/src/import/collision.rs`): `BhkMultiSphereShape`
  and `BhkConvexListShape` translate into `CollisionShape` (the former to
  `Ball`/`Compound`, the latter to `ConvexHull`/`Compound`) via
  `resolve_shape_inner` in the `extract_collision` chain — they must not fall
  out silently. Verify against the `BhkMultiSphereShape` / `BhkConvexListShape`
  downcast arms.
- **bhk motion_type via the canonical Havok enum (#1652, `dc33ec7d`)**:
  `collision.rs::havok_motion_type` maps the raw `hkMotionType` byte per the full
  nif.xml enum (1–5/8 → Dynamic, 6 KEYFRAMED → Keyframed, 7 FIXED → Static, 9
  CHARACTER → CharacterKinematic, 0/other → Static); the pre-fix
  `4 => Keyframed` / `_ => Static` collapse froze BOX_INERTIA (4) clutter.
  Shared with FNV/FO3 — re-introducing the collapse is the regression.
**Output**: `/tmp/audit/oblivion/dim_1.md`

### Dimension 2: BSA v103 Archive
**Subagent**: `general-purpose`
**Entry points**: `crates/bsa/src/archive/` (`mod.rs`, `open.rs`, `extract.rs`, `hash.rs`)
**Checklist**: This is a **regression guard** — v103 decompression has worked
end-to-end since 2026-04-17 (#699); the "v103 is broken" premise is dead, do not
regenerate it.
- `BSA_V_OBLIVION = 103` recognised in `open.rs`; rejection only outside {103,104,105}.
- **Folder-record size**: v103 AND v104 are **16 bytes**; only v105 (Skyrim SE)
  is 24. The live code is `if version == BSA_V_SKYRIM_SE { 24 } else { 16 }` in
  `open.rs`. (The older skill text claiming "v104 = 24 B" was wrong — verify
  against the constant, do not perpetuate it.)
- v103 archive-flag semantics (e.g. the "Xbox archive" bit several vanilla v103
  archives set, ignored for embedded names — `embed_file_names` gates on
  `>= BSA_V_FO3_SKYRIM`).
- Folder/file hash function in `hash.rs` still produces correct hashes.
- Full-archive sweep stays at 100% extraction. Only escalate to an open finding
  if `meshes/*.nif` extraction starts failing on a previously-clean archive.
**Output**: `/tmp/audit/oblivion/dim_2.md`

### Dimension 3: ESM Record Coverage (live path, not a stub)
**Subagent**: `general-purpose`
**Entry points**: `crates/plugin/src/esm/` (`mod.rs`, `reader.rs`, `cell/`, `records/`)
**Checklist**: TES4 records share the live ESM path with FNV/FO3 — there is no
per-game stub (removed under #390). The parser already carries Oblivion-specific
decode branches; the audit's job is correctness + coverage gaps, not "does it
exist".
- TES4 header (`HEDR` version 1.0 vs 0.94) and GRUP structure handled by the
  walker (`crates/plugin/src/esm/records/grup_walker.rs`).
- Oblivion-specific branches already present — verify they're correct, not
  regressed: `flags_oblivion` + `is_oblivion` in
  `crates/plugin/src/esm/records/actor.rs`; MGEF-by-code map (Oblivion 4-char
  effect codes) and the CONT 4-byte-payload guard in
  `crates/plugin/src/esm/records/tests.rs` / `container.rs`; CLMT three-entry
  WLST in `crates/plugin/src/esm/records/climate.rs`.
- **16-byte ACBS guard (#1650, `3d5d0d68`)**: Oblivion `NPC_`/`CREA` ship a
  **16-byte** ACBS (flags u32 @0, level i16 @10) — distinct from the ≥24-byte
  FNV/FO3/Skyrim layout, so `parse_npc` (`actor.rs`) needs a `GameKind::Oblivion`
  arm gated on `len >= 16` *before* the FNV arm. Pre-fix the ≥24 arm never fired
  on Oblivion: `record.level` defaulted to 1 (high-level NPCs resolved
  lowest-tier inventory) and `acbs_flags` defaulted to 0 (every actor read Male
  via `Gender::from_acbs_flags`). Tests: `oblivion_16byte_acbs_parses_level_and_gender`
  + `fnv_ignores_16byte_acbs` in `actor.rs`/`tests.rs`. Per-game layout must stay
  gated at the parser→record boundary, never re-derived at spawn/equip time.
- The two ignored Oblivion real-data parity tests
  (`clas_oblivion_knight_against_vanilla`, `race_oblivion_data_and_subs_against_vanilla`)
  still pass against vanilla `Oblivion.esm` when un-ignored.
- CELL walker: does `parse_esm_cells` (`crates/plugin/src/esm/cell/mod.rs`)
  handle Oblivion's CELL group layout (XCLL lighting, RCLR, interior vs
  exterior block grouping)?
- DIAL/INFO format differs between Oblivion and later titles — does the
  conversation-tree decode (M24.2) account for it, or silently mis-read?
- What is the minimum record set the cell loader needs to place an Oblivion
  exterior REFR? (Feeds the Blocker Chain.)
**Output**: `/tmp/audit/oblivion/dim_3.md`

### Dimension 4: Rendering Path for Oblivion Shaders
**Subagent**: `renderer-specialist`
**Entry points**: `crates/nif/src/import/material/` (`mod.rs`, `walker.rs`, `shader_data.rs`), `crates/nif/src/import/walk/`, `byroredux/src/systems/particle.rs`, `crates/renderer/shaders/triangle.frag`
**Checklist**:
- `NiTexturingProperty` → `MaterialInfo` pipeline (base slot 0, dark slot 1,
  normal-from-bump per #131, detail, glow, gloss).
- `NiMaterialProperty` color mapping is raw monitor-space (per 0e8efc6 — do NOT
  `srgb_to_linear` legacy colors).
- `NiAlphaProperty` blend-factor extraction routes every Gamebryo AlphaFunction
  enum value.
- `NiStencilProperty` / `NiZBufferProperty` / `NiVertexColorProperty` /
  `NiSpecularProperty` / `NiWireframeProperty` / `NiDitherProperty` /
  `NiShadeProperty` — honored or dropped silently? **#869 guards**:
  `NiWireframeProperty` wires to the LINE pipeline variant;
  `NiShadeProperty.flat_shading` is consumed in the fragment shader.
- Vertex-color interaction with material color.
- **#1239 Oblivion `NiPSysEmitter` version gating**: Oblivion's pre-Skyrim
  emitter field layout is routed via nif.xml's version gate — a regression
  silently misparses emitter authoring.
- **Typed particle-emitter import → runtime path**: `NiPSysEmitter` /
  `NiPSysEmitterCtlr` / `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier` are
  TYPED blocks (`crates/nif/src/blocks/particle.rs`) decoded by
  `extract_emitter_params` + `extract_emitter_rate`
  (`crates/nif/src/import/walk/mod.rs`) and fed into
  `apply_emitter_params` (`byroredux/src/systems/particle.rs`). Verify an
  Oblivion emitter that parses (per the #1239 gate) actually reaches the ECS
  authoring path and animates — not just parses-then-drops.
- **Disney BSDF gating regression guard (#1248-#1252)**: zero Oblivion materials
  author BGSM/`.mat`, so `MAT_FLAG_PBR_BSDF` (`crates/renderer/shaders/include/shader_constants.glsl`)
  must be 0 across the entire Oblivion material universe — the Disney lobe
  (`crates/renderer/shaders/include/pbr.glsl`) is
  unreachable for Oblivion. Any Oblivion scene activating Burley / anisotropic
  GGX is a gate regression.
**Output**: `/tmp/audit/oblivion/dim_4.md`

### Dimension 5: NIFAL Canonical Material Translation for Oblivion
**Subagent**: `renderer-specialist`
**Entry points**: `byroredux/src/material_translate.rs`, `crates/nif/src/import/material/walker.rs`, `crates/core/src/ecs/components/material.rs`, `byroredux/src/render/static_meshes.rs`, `docs/engine/nifal.md`
**Checklist**: Trace an Oblivion `NiTexturingProperty`/`NiMaterialProperty`
`MaterialInfo` through the single canonical boundary `translate_material`
(`byroredux/src/material_translate.rs`) into the ECS `Material`.
- Metalness/roughness arrive as plain `f32` carrying the `f32::NAN` sentinel
  (`mesh.metalness_override`/`roughness_override` `.unwrap_or(f32::NAN)`) and are
  resolved exactly once by `Material::resolve_pbr`
  (`crates/core/src/ecs/components/material.rs`). Confirm no per-draw
  `classify_pbr` reappears and that `static_meshes.rs` still reads
  `m.roughness`/`m.metalness` directly with no render-time keyword scan.
- `emissive_source` is tagged `EmissiveSource::Material` for Oblivion legacy
  meshes via the `NiMaterialProperty` arm in
  `crates/nif/src/import/material/walker.rs` (distinct from the Skyrim/FO4
  `BSLightingShaderProperty` arm). Test:
  `crates/nif/src/import/material/emissive_source_tests.rs`.
- `MAT_FLAG_PBR_BSDF` stays 0 across the all-legacy Oblivion universe (shared
  with Dim 4's Disney-gate guard; flag this once, cross-reference).
See `/audit-nifal` for the cross-game canonical-tier audit; this is the
Oblivion-specific slice.
**Output**: `/tmp/audit/oblivion/dim_5.md`

### Dimension 6: Real-Data Validation
**Subagent**: `general-purpose`
**Entry points**: `crates/nif/examples/nif_stats.rs`, `crates/nif/examples/recovery_trace.rs`, `crates/nif/tests/parse_real_nifs.rs`, `crates/nif/tests/per_block_baselines.rs`
**Checklist**:
- Run `nif_stats` (and `nif_stats --tsv` for the per-type histogram) over
  `Oblivion - Meshes.bsa`; compare clean/recovered/truncated counts against the
  current ROADMAP Oblivion row AND the checked-in Oblivion baseline in
  `per_block_baselines.rs`. Any `unknown` growth or `parsed` shrinkage is a
  regression.
- Run `recovery_trace` to enumerate the residual truncated files (~10 NetImmerse
  v10.x after the family fix). Confirm they're the expected NetImmerse tail and
  the single corrupt-by-design hard-fail, not new drift.
- Cross-check the block-type histogram for any new types appearing since the last
  sweep.
- Pick 3 representative interior meshes (Anvil Heinrich Oaken Halls chandelier, a
  book, a creature head) and trace them through `import_nif_scene` → verify
  expected mesh count + material chain.
**Output**: `/tmp/audit/oblivion/dim_6.md`

### Dimension 7: Exterior Blocker Chain & Game-Specific Quirks
**Subagent**: `general-purpose`
**Entry points**: `ROADMAP.md` (Known Issues + compat matrix), `docs/feature-matrix.md`, `byroredux/src/cell_loader/`, `docs/audits/`
**Checklist**:
- The real exterior blocker is **TES4 worldspace + LAND wiring** (same shape FO3
  was pre-cell-loader) — *not* BSA v103 decompression, which has worked
  end-to-end since 2026-04-17 (#699). Do not regenerate the dead "v103 is
  broken" framing.
- Does the `--bsa` CLI path open + list + extract Oblivion archives end-to-end?
- Are there Oblivion-specific record types the cell loader
  (`byroredux/src/cell_loader/`) needs beyond the FNV-aligned set to place
  exterior REFRs?
- Animation blocks that parse but can't play because scene-graph name resolution
  is missing?
- Does the pre-v3.3.0.13 fallback log at `warn` or `debug` (spam risk on
  full-archive sweeps)?
- Any 100%-parse NIFs that would still render wrong (legacy particle emitters
  that parse but don't route to the renderer — cross-check Dim 4)?
**Output**: `/tmp/audit/oblivion/dim_7.md`

## Phase 3: Merge

1. Read all `/tmp/audit/oblivion/dim_*.md` files.
2. Combine into `docs/audits/AUDIT_OBLIVION_<TODAY>.md` with structure:
   - **Executive Summary** — Current compatibility level (NIF parse incl. v10.x
     tail, archive extract, ESM parse, render end-to-end), top blockers in
     priority order. Cite ROADMAP/feature-matrix numbers, not this skill.
   - **Dimension Findings** — Grouped by severity per dimension.
   - **Blocker Chain** — Sequential list to reach "exterior cell renders".
     Interiors already work end-to-end (Anvil Heinrich Oaken Halls). Real chain:
     TES4 worldspace + LAND wiring → CELL exterior REFR placement → exterior
     bench. Do NOT regenerate the stale BSA-v103 framing.
   - **Regression Guard List** — Previously-fixed items this audit verified still
     hold: the v10.x stride-drift family (#1506/#1507/#1508/#1509),
     `NiTexturingProperty` u32 count, BSStreamHeader dual-band (#170),
     `user_version` threshold, BSA v103 extraction (#699), Disney-gate stays 0.
3. Remove cross-dimension duplicates.

Suggest: `/audit-publish docs/audits/AUDIT_OBLIVION_<TODAY>.md`
