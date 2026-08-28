# Oblivion (TES4) Compatibility Audit — 2026-08-27

**Run mode**: Solo (no sub-agent fan-out — every dimension read, traced, probed
and test-run directly in one session, per the explicit run constraint; nested
agent result relay is unreliable in this repo).
**Scope**: Full comprehensive sweep, all 7 dimensions, no `--focus` filter.
**Game data**: `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/` — present
and used for every real-data claim below. No engine instance launched, no GPU
device exercised (static analysis + parser/importer harnesses only).
**Prior report**: `docs/audits/AUDIT_OBLIVION_2026-08-24.md` (its findings are
reconciled in "Prior-Report Reconciliation" below rather than re-filed).

## Executive Summary

Oblivion's own parse/import surface is in the best measured state it has ever
been in, and every regression guard this skill asks to re-verify still holds.
The five findings below are all NEW; **none of them is an Oblivion parse or
render regression**. Two of them were found *through* the Oblivion-owned legacy
`NiProperty`-chain code and turn out to mis-fire on FO3/FNV while being
**correct on Oblivion** — that asymmetry is exactly why they survived four
prior FO3/FNV sweeps, and it is the most valuable thing this pass produced.

Live-measured this session (not carried from ROADMAP prose):

| Measurement | Result |
|---|---|
| `Oblivion - Meshes.bsa` NIF parse (`nif_stats`) | **8032 / 8032 clean (100.00 %)**, 0 truncated, 0 failed, 0 recovered |
| `DLCShiveringIsles - Meshes.bsa` NIF parse | **1438 / 1438 clean (100.00 %)**, 0 truncated, 0 failed |
| Oblivion `.kf` import (`import_kf`, 1843 files) | 1843 parsed / 0 failed / **0 files yielding zero clips**; 1845 clips, 121 107 transform channels, 0 clips with no TRS channel |
| Shivering Isles `.kf` import (614 files) | 614 parsed / 0 failed / 0 zero-clip; 620 clips, 40 149 channels |
| `per_block_baseline_oblivion` opt-in gate | **PASS** — "per-block baseline OK (120 types matched)", 0 unknown blocks |
| `parse_real_esm` Oblivion suite (`--ignored`) | 4 / 4 pass (`parse_rate_oblivion_esm`, `clas_oblivion_knight_against_vanilla`, `race_oblivion_data_and_subs_against_vanilla`, `oblivion_spawn_time_global_decodes_float_payload_before_narrowing`); 55 317 records, 37 weathers / 19 climates / 142 trees |
| Crate suites | `byroredux-nif` 1118 pass / 0 fail · `byroredux-plugin` 814 pass / 0 fail · `byroredux-bsa` pass |
| Oblivion runtime baseline (`ICMarketDistrictTheGildedCarafe`) | 705 entities, `tex_missing_unique_paths 0`, `mesh_cache_failed_count 0` (regenerated 2026-08-26 under #3288) |
| Vanilla `Oblivion.esm` `CREA` ACBS census (direct byte scan) | 914 records, **all ACBS exactly 16 bytes** — reconfirms the #1650 gate empirically |

**Findings: 5 total — 0 CRITICAL, 1 HIGH, 2 MEDIUM, 2 LOW.**

**Top items in priority order**

1. `OBL-2026-08-27-01` (HIGH) — the legacy `NiTexturingProperty` clamp-mode
   decode reads the wrong nibble of `TexDesc.flags` on every ≥ 20.1.0.3 file.
   Measured: **2258 / 2258** FNV base `TexDesc`s resolve to `CLAMP_S_CLAMP_T`
   when 2236 of them author `WRAP_S_WRAP_T`. Oblivion is unaffected and correct.
2. `OBL-2026-08-27-02` (MEDIUM) — the same branch is the only legacy
   clamp-mode writer that neither reads nor sets `texture_clamp_mode_consumed`,
   so it breaks the documented shape-before-inherited precedence (#208) in both
   directions.
3. `OBL-2026-08-27-03` (MEDIUM) — `parse_placement_lod` (the Oblivion-only
   `DistantLOD\*.lod` reader) pre-allocates from an unvalidated `u32` count,
   the one file-driven allocation in the LOD/archive path that skips the
   project's own `checked_entry_count` / `allocate_vec_sized` doctrine.
4. `OBL-2026-08-27-04` (LOW) — `ROADMAP.md` contradicts itself about `#2193`.
5. `OBL-2026-08-27-05` (LOW) — 57 committed `_tmp_*` audit-scratch example
   targets in `crates/nif/examples/` (the `crates/plugin` sibling, `#3150`, is
   already open at 3 files).

The documented **Blocker Chain is unchanged**: interiors render end-to-end, TES4
worldspace + LAND wiring is implemented and game-agnostic (#1556), and the one
remaining step is an on-device exterior render bench, which needs a live Vulkan
device and is out of a source-only pass's scope.

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.4 + the v10.x NetImmerse tail)

**No findings.** Every guard re-read against live source and cross-checked
against `/mnt/data/src/reference/nifxml/nif.xml`:

- `crates/nif/src/header.rs:114` — `user_version` gated `>= V10_0_1_8`, with the
  in-code note naming `meshes/creatures/minotaur/horn*.nif` as the Oblivion
  content that needs it.
- `crates/nif/src/header.rs:137-143` — `has_bs_stream_header` implements the
  full nif.xml dual band verbatim: `VER == 10.0.1.2 OR (USER >= 3 AND (VER ∈
  {20.2.0.7, 20.0.0.5} OR (10.1.0.0 <= VER <= 20.0.0.4 AND USER <= 11)))`. #170
  holds.
- `crates/nif/src/blocks/controller/morph.rs:109` / `:219` — the #1509 morph gate
  is `bsver >= MORPH_LEGACY_CUTOFF` with the complementary `<` half at `:219`,
  both keyed on the single constant `crates/nif/src/version.rs:358`
  (`MORPH_LEGACY_CUTOFF = 10`, i.e. nif.xml's `#GT# 9`).
- `crates/nif/src/blocks/properties.rs:328-350` — `NiTexturingProperty` still
  reads the shader-texture count as a raw `u32` with no leading bool gate, with
  the Gamebryo-2.3-over-nif.xml rationale in place. #149 holds.
- `crates/nif/src/import/collision/mod.rs:223-231` — `havok_motion_type` maps the
  full 8-arm `hkMotionType` enum (`1..=5 | 8 => Dynamic`, `6 => Keyframed`,
  `7 => Static`, `9 => CharacterKinematic`, `_ => Static`). The pre-#1652
  `4 => Keyframed` / `_ => Static` collapse is gone.
- `crates/nif/src/import/collision/shape.rs` — `BhkMultiSphereShape` and
  `BhkConvexListShape` both retain live `resolve_shape_inner` arms.
- **#2345 (`2695e4fe`, 2026-08-26) reviewed field-by-field against nif.xml**, as
  the newest and least-reviewed code in the Oblivion-unique v10.x band.
  `NiSequence`'s `until="10.1.0.103"` `Accum Root Name` / `Text Keys` prologue,
  `ControlledBlock`'s `Target Name` (`until=10.1.0.103`), `Interpolator`
  (`since=10.1.0.106`), the `Blend Interpolator` + `Blend Index` pair
  (`10.1.0.104`–`10.1.0.110`), the `Priority` double gate
  (`since=10.1.0.106` **and** `#BSSTREAM#` = `BSVER > 0`), the two disjoint IDTag
  string bands (`10.1.0.104`–`10.1.0.113` inline, `>= 20.1.0.1` string-table),
  the `10.2.0.0`–`20.1.0.0` string-palette band and the whole
  `since="10.1.0.106"` `NiControllerSequence` field group all match nif.xml
  exactly, and the `Controller` ref's true gate is `until="20.5.0.0"` — above
  every version this engine targets, so reading it unconditionally is correct.
  The KF harness result above (1843 / 1843 Oblivion `.kf`, 0 zero-clip files) is
  the real-data confirmation that this landed without disturbing the Oblivion
  mainline.
- `crates/nif/src/blocks/properties.rs:710` — `NiStencilProperty` splits on
  `<= V20_0_0_5` (expanded fields for Oblivion, packed flags for FO3+), matching
  nif.xml's `until="20.0.0.5"` / `since="20.1.0.3"` field pair.
- `crates/nif/src/blocks/properties.rs:664` — `NiVertexColorProperty` reads
  `Flags` unconditionally (nif.xml declares it with no version gate on this
  block) then the `until="20.0.0.5"` `Vertex Mode` / `Lighting Mode` pair.
  Correct; the 4968 Oblivion instances parse clean.
- `crates/nif/src/blocks/extra_data.rs:1133` — `BSFurnitureMarker` splits on
  `bsver <= FO3_FNV`, which coincides with nif.xml's `until="20.0.0.5"` /
  `since="20.2.0.7"` version split for every title this engine targets.

### Dimension 2 — BSA v103 Archive

**No findings** — regression guard, holding, and notably the *strictest*
untrusted-input path in the Oblivion chain (which is what makes
`OBL-2026-08-27-03` stand out).

`crates/bsa/src/archive/open.rs:39-44` accepts only `{103, 104, 105}`;
`:100` sizes the folder record `if version == BSA_V_SKYRIM_SE { 24 } else { 16 }`
(v103 and v104 both 16 B, per the corrected doctrine); `:73` and `:126` route
both the header counts and the per-folder file count through
`checked_entry_count` (#586); `:74` deliberately excludes v103's "Xbox archive"
`0x100` bit from `embed_file_names` (`version >= BSA_V_FO3_SKYRIM &&`), which is
what makes vanilla v103 archives extract at 100 %.
`crates/bsa/src/archive/extract.rs:100,122,169` cap every decompression target
and payload through `checked_chunk_size`. `byroredux-bsa` suite green.
Round-trip evidence: both the base `Oblivion - Meshes.bsa` (20 182 files) and
`DLCShiveringIsles - Meshes.bsa` opened and extracted every `.nif` and `.kf`
this session with zero extraction failures.

### Dimension 3 — ESM Record Coverage (live path)

**No findings.** Re-verified, with one new piece of real-data evidence:

- `crates/plugin/src/esm/records/actor/mod.rs:899` — the 16-byte Oblivion `ACBS`
  arm (`matches!(game, GameKind::Oblivion) && sub.data.len() >= 16`) is gated
  ahead of the FO4 / Skyrim / FNV arms, exactly as #1650 requires.
  **Independently confirmed by a direct byte-scan of vanilla `Oblivion.esm` this
  session**: 914 `CREA` records, ACBS length histogram `{16: 914}` — no other
  length appears, so the FNV `>= 24` arm can never fire on Oblivion.
- A hypothesis that Oblivion `CREA` ACBS bit 0 (`Biped`) would be mis-read as
  `Gender::Female` by `crates/plugin/src/equip.rs:56` was **investigated and
  disproved**: the same scan found **zero** vanilla Oblivion `CREA` with bit 0
  set, and `resolve_armor_meshes` (`equip.rs:169-178`) ignores `gender` entirely
  on the pre-Skyrim branch anyway. Recorded here so it is not re-derived.
- `effective_actor_level` (`actor/mod.rs:96-102`) is the single source of truth
  and reads `ACBS_PC_LEVEL_MULT = 0x0080`, which is also Oblivion's
  "PC Level Offset" bit; multiplier/offset actors resolve to `calc_min.max(1)`,
  and Oblivion's `calc_min` is decoded at offset 12 (`:905`, pinned by
  `acbs_calc_min_decodes_on_every_layout`). `calc_max` is decoded nowhere on any
  game and is read nowhere — a symmetric, deliberate omission, not an
  Oblivion gap.
- `esm/cell/walkers.rs` — the per-game `XCLL` canonical-size table and `RCLR`
  handling stay distinct from the FO/Skyrim/FO4/76/SF tables; `ATXT`/`VTXT`
  pairing and `BTXT` (`:1081-1110`) are game-agnostic and correct for TES4.
- Both previously-`#[ignore]`d Oblivion real-data parity tests pass against
  vanilla `Oblivion.esm` (see Executive Summary table).

### Dimension 4 — Rendering Path for Oblivion Shaders

**One HIGH + one MEDIUM finding**, both in the Oblivion-owned legacy property
chain, both currently mis-firing on FO3/FNV rather than on Oblivion. See
`OBL-2026-08-27-01` and `OBL-2026-08-27-02`.

Everything else on the checklist holds:

- The full legacy property set is honoured in
  `crates/nif/src/import/material/legacy_properties.rs`
  (`apply_legacy_property_chain`, `:64-96`): alpha, z-buffer, material,
  texturing, PP-lighting, no-lighting, misc shader, base-only shader, stencil,
  flag (wireframe/dither/shade/specular), vertex-colour. `NiFogProperty` is the
  one deliberate, documented non-dispatch (`:83-95`, #1224).
- The `NiTexturingProperty` → `MaterialInfo` slot map (`:172-247`) covers base,
  normal-from-bump (#131), glow, detail, gloss, **dark** (slot 1, `albedo *=
  dark` — traced end-to-end to `crates/renderer/shaders/triangle.frag:1202-1203`
  via `GpuMaterial.dark_map_index`), parallax (20.2.0.5+, unreachable on
  Oblivion) and the four decal slots.
- Disney-BSDF gate: `crates/nif/tests/data/per_block_baselines/oblivion.tsv`
  contains **zero** `BSShader*` / `BSLightingShaderProperty` /
  `BSEffectShaderProperty` rows — the Oblivion material universe is entirely
  `NiMaterialProperty` + `NiTexturingProperty`, so `MAT_FLAG_PBR_BSDF` is
  structurally unreachable, not merely unset. (Cross-referenced with Dim 5.)
- Typed particle-emitter chain intact: Oblivion authors 543 `NiParticleSystem`
  / 277 `NiPSysBoxEmitter` / 105 `NiPSysSphereEmitter` / 84 `NiPSysMeshEmitter`
  / 80 `NiPSysCylinderEmitter` / 547 `NiPSysEmitterCtlr`, all parsing clean, and
  `extract_emitter_params` reaches `apply_emitter_params`.

### Dimension 5 — NIFAL Canonical Material Translation for Oblivion

**No new findings** beyond `OBL-2026-08-27-01`/`-02` (filed under Dim 4 since
their origin is the importer, not the boundary). The three still-open
Dim-5 issues from prior sweeps (`#2571`, `#2572`, `#2573`) were spot-checked and
still describe live code; they are not re-filed.

`Material::resolve_pbr`'s resolve-once invariant is intact —
`byroredux/src/render/static_meshes.rs` reads `m.roughness` / `m.metalness`
directly with the explicit "no per-draw keyword scan / `classify_pbr` fallback"
comment, and no per-draw classifier has reappeared. The Oblivion
`NiMaterialProperty → EmissiveSource::Material` arm remains in
`crates/nif/src/import/material/walker.rs`, pinned by
`emissive_source_tests.rs`.

### Dimension 6 — Real-Data Validation

**One LOW tooling-hygiene finding** (`OBL-2026-08-27-05`). This is the dimension that most improved since 2026-08-24:

- The `per_block_baseline_oblivion` gate, which the prior report filed as
  failing (`OBL-D6-NEW-01`, `NiSingleInterpController 3034 -> 0`), now **passes**
  after `2d7a6f02` (#3326, key the baseline on wire RTTI) and `19b844bf`
  (#3175/#2574, re-baseline). Live: 120 distinct types matched, 0 unknown.
- `oblivion_truncations.tsv` and the ROADMAP Oblivion compat row
  (`ROADMAP.md:577`) both now agree with live measurement at 100 % / 0 truncated
  — the `#2564` doc-rot is closed.
- Block-type histogram cross-check for the three representative content
  families is unchanged and healthy: architecture (25 244 `NiNode`, 33 778
  `NiTriStrips`, 40 553 `NiTriStripsData`), skinned creatures (1596
  `NiSkinData`/`NiSkinInstance`/`NiSkinPartition` triples), collision (7929
  `bhkCollisionObject`, 4521 `bhkNiTriStripsShape`, 4504 `bhkMoppBvTreeShape`).
  No new block types since the last sweep; no `unknown` anywhere.
- New this pass: an end-to-end **`.kf` import health probe** over both Oblivion
  archives (results in the Executive Summary). This exercises the whole
  Oblivion animation chain — `NiControllerSequence` → `ControlledBlock`
  string-palette layout → `NiStringPalette::get_string` → `AnimationClip` — and
  is the direct real-data regression guard for #2345 and #402 that the
  checklist previously had no harness for.

### Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks

**One MEDIUM finding** (`OBL-2026-08-27-03`) in the Oblivion-only placement-LOD
reader. Otherwise:

- `byroredux/src/cell_loader/placement_lod.rs`'s module doc was **correctly
  updated** by #3321 (`e23a9908`) — it no longer claims FO3/FNV have no distant
  object LOD, and now scopes the `DistantLOD\*.lod` *placement-list* scheme to
  Oblivion while pointing at `object_lod` for the FO3/FNV
  `meshes\landscape\lod\` family. No doc-rot here.
- `far_nif_path` / `full_model_path` / `placement_lod_cells_in_radius`
  (`:195-247`) still derive `<stem>_far.nif`, prefix `meshes\`, and gate the
  ring on `radius_unload` (not `radius_load`) per #1866. `placement_lod_supported`
  is `game == GameKind::Oblivion`.
- `#3385`'s new availability memo (`c7a70d45`) was checked for a worldspace-key
  hazard: the memo is keyed `(level, qx, qy)` without the worldspace, but
  `WorldStreamingState` is torn down and rebuilt on every worldspace crossing
  (`byroredux/src/app_step.rs:838-870` drains before
  `assemble_exterior_streaming`), and `drain_streaming_state` clears both maps.
  **Not a bug** — recorded so it isn't re-derived.
- The pre-5.0.0.1 inline-name path still logs at `debug!` per file and only
  escalates to `warn!` on the rare mid-file read failure — confirmed by the
  clean 8032-file sweep producing no per-block warn spam (the one warning
  emitted over the whole archive is the unrelated `NifVariant::detect`
  ambiguity note for `(V20_0_0_4, user_version=11, user_version_2=11)`, tracked
  as `#1219`, which is CLOSED and whose log line is informational).

## New Findings

### OBL-2026-08-27-01: `TexDesc.flags & 0xF` decodes clamp mode from the wrong nibble on every ≥ 20.1.0.3 file — correct on Oblivion, wrong on all 2258 FNV base `TexDesc`s

- **Severity**: HIGH
- **Dimension**: 4 — Rendering Path (legacy `NiProperty` chain) / NIFAL material boundary
- **Location**: `crates/nif/src/import/material/legacy_properties.rs:272-276`
  (consumer) · `crates/nif/src/blocks/properties.rs:417-418` and `:462-464`
  (the two producers)
- **Status**: NEW
- **Description**: `TexDesc` has two *disjoint* on-disk layouts, and the parser
  stores them in the **same** `flags: u16` field with **different meanings**:
  - `properties.rs:440-464` (v < 20.1.0.3, i.e. Oblivion) reads nif.xml's
    separate `Clamp Mode` / `Filter Mode` / `UV Set` `uint`s and *synthesizes*
    a packed word with clamp in **bits 0-3**.
  - `properties.rs:417-418` (v >= 20.1.0.3, i.e. FO3 / FNV / Skyrim) stores the
    **raw on-disk** `Flags` word, where nif.xml states "clamp and filter mode
    stored in **upper byte** with `0xYZ00` = clamp mode Y, filter mode Z" —
    clamp lives in **bits 12-15**.

  The single consumer, `legacy_properties.rs:274`, applies the low-nibble
  decode unconditionally. On Oblivion that is right. On FO3/FNV it silently
  returns 0 — `CLAMP_S_CLAMP_T` — for essentially every legacy-chain material.
  The in-code comment at `legacy_properties.rs:394-396` records the belief that
  drove this ("the NiTexturingProperty path mirrored the per-slot `flags & 0xF`
  (#761) — only this PPLighting site was missing"), which is exactly the
  premise the measurement below falsifies.
- **Evidence**: Census over both vanilla archives with a throwaway
  `NiTexturingProperty.base_texture.flags` histogram probe (since removed):

  ```
  === FNV  (Fallout - Meshes.bsa) ===
  base TexDescs: 2258
  raw flags histogram: {512: 21, 8704: 1, 12800: 2236}
  flags & 0xF      : {0: 2258}          <-- what the code reads
  (flags>>12)&0xF  : {0: 21, 2: 1, 3: 2236}   <-- the real clamp mode

  === OBLIVION (Oblivion - Meshes.bsa) ===
  base TexDescs: 30120
  raw flags histogram: {3: 17, 19: 43, 32: 111, 34: 1, 35: 29948}
  flags & 0xF      : {0: 111, 2: 1, 3: 30008}  <-- correct (synthesized layout)
  (flags>>12)&0xF  : {0: 30120}
  ```

  `12800 = 0x3200` → clamp 3 (`WRAP_S_WRAP_T`), filter 2. 2236 of 2258 FNV base
  descriptors author WRAP/WRAP and every one of them resolves to
  `texture_clamp_mode = 0`. Reachability is not theoretical: the checked-in
  per-block baselines record `NiTexturingProperty 2077` for Fallout 3 and
  `3018` for Fallout NV (0 for Skyrim SE / FO4 / FO76 / Starfield).

  The value is consumed as a real sampler selection:
  `crates/renderer/src/texture_registry.rs:171-183` indexes
  `samplers: [vk::Sampler; 4]` directly by this code, with `0 =
  CLAMP_S_CLAMP_T` and `3 = WRAP_S_WRAP_T`.
- **Impact**: Every FO3/FNV mesh that binds its diffuse through a
  `NiTexturingProperty` chain samples with `VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE`
  on both axes instead of `REPEAT`. Any UV outside `[0,1]` — i.e. all tiled
  architecture, terrain trim and repeated detail — smears the border texel
  across the surface. It fails silently and identically on every draw, so it
  reads as "the texture looks wrong", not as an error. Oblivion, Skyrim SE,
  FO4, FO76 and Starfield are unaffected (Oblivion because the synthesized
  layout genuinely puts clamp in the low nibble; the rest because they carry no
  `NiTexturingProperty`). Severity is HIGH per the "wrong/divergent `Material`
  out of the canonical boundary" rule — one producer, whole-game blast radius,
  no per-draw fallback to mask it.
- **Related**: `#2565` (OBL-D1-04) covers *reader-side* `TexDesc` version gaps
  and the PS2 L/K divergence — a different defect in the same function; it does
  not touch the flags-semantics mismatch. `#761` is the commit whose reasoning
  the comment at `legacy_properties.rs:394-396` preserves. Sibling of
  `OBL-2026-08-27-02` (same four lines).
- **Suggested Fix**: Stop overloading one `u16` with two encodings. Either
  (a) give `TexDesc` an explicit `clamp_mode: u8` decoded at *parse* time in
  both branches (low nibble in the synth branch, `(flags >> 12) & 0xF` in the
  raw branch) and have `legacy_properties.rs` read that, or (b) normalise the
  raw ≥ 20.1.0.3 word into the same synthesized bit layout at `properties.rs:418`
  so exactly one encoding ever leaves the parser. (a) is preferable — it removes
  the ambiguity rather than hiding it. Pin it with a two-case unit test built
  from the measured real values (`0x3200` → 3, `0x0023` → 3).

### OBL-2026-08-27-02: the `NiTexturingProperty` clamp writer is the only legacy clamp writer that ignores `texture_clamp_mode_consumed`, inverting #208 shape-before-inherited precedence

- **Severity**: MEDIUM
- **Dimension**: 4 — Rendering Path (legacy `NiProperty` chain)
- **Location**: `crates/nif/src/import/material/legacy_properties.rs:272-276`
  vs. its four siblings at `:405-408`, `:525-528`, `:596-599`, `:610-613`
- **Status**: NEW
- **Description**: `apply_legacy_property_chain` (`:64-96`) walks
  `direct_properties` **then** `inherited_props`, so the shape's own property
  must win (#208). #2328 / FO3-D1-06 implemented that for clamp mode with a
  `texture_clamp_mode_consumed` latch, and all four `BSShader*` writers
  (`apply_pp_lighting_property`, `apply_no_lighting_property`,
  `TileShaderProperty`, `SkyShaderProperty`) read *and* set it. The
  `NiTexturingProperty` writer does neither — it gates on the value shape
  (`if info.texture_clamp_mode == 3`) instead:

  ```rust
  // legacy_properties.rs:272-276
  if info.texture_clamp_mode == 3 {
      if let Some(base) = tex_prop.base_texture.as_ref() {
          info.texture_clamp_mode = (base.flags & 0xF) as u8;
      }
  }
  ```

  This is precisely the pattern the sibling `apply_legacy_alpha_property`
  documents as wrong three functions above (`:104-108`, #1201: "gate on
  `alpha_property_consumed`, not on the `!alpha_blend && !alpha_test`
  value-shape").
- **Evidence**: The asymmetry breaks precedence in both directions:
  - A **shape-level** `NiTexturingProperty` writes the clamp mode without
    latching consumption, so an **inherited** `BSShaderPPLightingProperty`
    later in the same walk sees `!consumed` and overwrites it — inherited beats
    shape, the exact inversion #2328 was written to prevent.
  - A **shape-level** `BSShader*` that legitimately authors clamp mode 3 sets
    `consumed = true`, but the `== 3` gate does not consult that latch, so an
    **inherited** `NiTexturingProperty` overwrites it anyway.
- **Impact**: Wrong sampler address mode on FO3/FNV meshes that mix a legacy
  `NiTexturingProperty` with a `BSShader*` property across the
  shape/parent-node boundary — FNV ships 58 706 `BSShaderPPLightingProperty` and
  3018 `NiTexturingProperty` blocks, so the mixed shape exists. Structurally
  latent on Oblivion (`oblivion.tsv` contains **zero** `BSShader*` rows, so only
  one writer can ever run), which is why this survived the FO3/FNV sweeps that
  introduced the latch. Filed at MEDIUM rather than HIGH because it needs the
  mixed-chain shape, where `-01` needs nothing at all.
- **Related**: `OBL-2026-08-27-01` (same four lines; fixing both together is one
  edit). `#2328` / FO3-D1-06 introduced the latch. `#1201` is the identical
  value-shape-vs-latch bug already fixed for `NiAlphaProperty`.
- **Suggested Fix**: Replace the `== 3` value gate with
  `if !info.texture_clamp_mode_consumed { … ; info.texture_clamp_mode_consumed =
  true; }`, matching the four siblings. Add a chain-order unit test with a
  direct `NiTexturingProperty` plus an inherited `BSShaderPPLightingProperty`
  asserting the shape's value survives.

### OBL-2026-08-27-03: `parse_placement_lod` pre-allocates from an unvalidated `u32` group count — the one untrusted-input allocation in the Oblivion LOD/archive chain that skips the project's own guard doctrine

- **Severity**: MEDIUM
- **Dimension**: 7 — Exterior Blocker Chain & Game-Specific Quirks
- **Location**: `byroredux/src/cell_loader/placement_lod.rs:119-122`
- **Status**: NEW
- **Description**: The Oblivion-only `DistantLOD\*.lod` reader takes its group
  count straight from the first four bytes of an archive file and hands it to
  `Vec::with_capacity` before any validation:

  ```rust
  // placement_lod.rs:119-122
  pub(crate) fn parse_placement_lod(bytes: &[u8]) -> io::Result<Vec<PlacementGroup>> {
      let num_groups = u32_at(bytes, 0)?;
      let mut off = 4usize;
      let mut groups = Vec::with_capacity(num_groups as usize);
  ```

  `PlacementGroup` is `{ base_form_id: u32, placements: Vec<Placement> }`
  (≥ 32 B with padding), so a header word of `0xFFFFFFFF` requests roughly
  137 GB in one allocation. Rust's allocation-failure path is
  `handle_alloc_error` → **`abort`**, which is neither the `Err` this function's
  own doc comment promises nor an unwind the caller could contain:

  ```rust
  /// Errors (rather than panics) on any out-of-bounds read, so a malformed /
  /// degenerate file (e.g. `toddland`) is skipped by the caller rather than
  /// crashing.
  ```

  The per-group `Vec::with_capacity(count)` at `:138` is **fine** — it sits
  after the `end > bytes.len()` check at `:132-137`. Only the outer count is
  unguarded.
- **Evidence**: The surrounding code has an explicit, documented doctrine for
  exactly this, and this is the site that doesn't follow it:
  - `crates/bsa/src/archive/open.rs:50-56` — "Cap folder / file counts before
    the downstream `Vec::with_capacity` / `HashMap::with_capacity` allocations
    … catches the `u32::MAX` attack from a single corrupted header word. See
    #586" → `checked_entry_count`.
  - `crates/bsa/src/ba2.rs:180-181` — same cap for BA2 `file_count`.
  - `crates/nif/src/stream.rs:270-283` (`allocate_vec`) and `:321-323`
    (`allocate_vec_sized`, #2523) — bound the count against remaining bytes and
    `MAX_SINGLE_ALLOC_BYTES` before allocating.

  Reachability: `spawn_placement_lod_cell` (`:434-436`) pulls the bytes with
  `tex_provider.extract_mesh(&lod_path)` from whatever BSA set is open, so any
  installed Oblivion mod archive containing a `distantlod\<world>_<x>_<y>.lod`
  entry reaches this parser during exterior streaming. The scheme is
  Oblivion-only (`placement_lod_supported`, `:307-309`) — no other title has
  this exposure.
- **Impact**: A single corrupt or hostile 4-byte word in a mod-supplied `.lod`
  aborts the process during exterior streaming, bypassing the module's own
  documented "skip the file" recovery. Vanilla is unaffected (the reader is
  validated against all 9889 real files). MEDIUM: recoverable path with missing
  error handling, in an Oblivion-only module, on attacker/mod-controlled input.
- **Related**: `#586` (the BSA/BA2 count caps this mirrors), `#2523` /
  PERF-D8-NEW-01 (`allocate_vec_sized`). Not covered by `#3150`.
- **Suggested Fix**: Bound `num_groups` by the file's own smallest legal
  encoding before allocating — each group costs at least 8 bytes
  (`base_form_id` + `count`), so `num_groups > (bytes.len() - 4) / 8` is
  provably corrupt and should return `Err`. One line, plus a synthetic
  `u32::MAX`-header unit test alongside the existing `parse_placement_lod`
  tests.

### OBL-2026-08-27-04: `ROADMAP.md` contradicts itself about `#2193` — one line records the fix, the next paragraph still calls it open

- **Severity**: LOW
- **Dimension**: 7 — Blocker Chain / doc accuracy
- **Location**: `ROADMAP.md:1102` (contradicting `ROADMAP.md:1100`)
- **Status**: NEW
- **Description**: `ROADMAP.md:1100` correctly records that the Oblivion
  interior-spawn grounding issue was **"Closed 2026-08-04 (`195fbb28`) …
  live-verified on `ICMarketDistrictTheGildedCarafe` grounded from frame 0
  through a 120-frame run."** The very next paragraph, `:1102`, still ends:

  > The Oblivion inverted-normal residue remains separately open as #2193
  > pending a real-data retest; the Skyrim result does not by itself close that
  > game-specific path.

  `#2193` is **CLOSED** (`gh issue view 2193` → `CLOSED`,
  "OBL-2026-07-25-01: is_grounded stays false at Oblivion interior spawn").
- **Evidence**: Both lines are in the same "Known Issues" block of the same
  file; `:1100` names the closing commit and the live verification, `:1102`
  asks for a "real-data retest" that `:1100` already reports as done. The
  2026-08-26 runtime baseline
  (`.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv`)
  is that retest.
- **Impact**: Doc-rot only, but of the actively-misleading kind: `ROADMAP.md` is
  declared by `CLAUDE.md` to be an authoritative source, and this line is a
  standing invitation for a future Oblivion audit to reopen a fixed
  physics-grounding investigation. It is the same failure mode the memory note
  *tes_grounding_zero_mass_dynamic_fix* exists to prevent.
- **Related**: `#2193` (closed), `#2419` (the previous ROADMAP-staleness item in
  the same neighbourhood).
- **Suggested Fix**: Replace the trailing sentence of `ROADMAP.md:1102` with a
  pointer to `:1100`'s closure — e.g. "The Oblivion residue this paragraph once
  tracked (`#2193`) closed independently on 2026-08-04; see the row above."

### OBL-2026-08-27-05: 57 committed `_tmp_*` audit-scratch example targets in `crates/nif/examples/`

- **Severity**: LOW
- **Dimension**: 6 — Real-Data Validation (tooling hygiene)
- **Location**: `crates/nif/examples/_tmp_*.rs` (57 tracked files)
- **Status**: NEW — same class as the OPEN `#3150`, different crate and ~19× the
  count, so `#3150`'s scope (`crates/plugin`, 3 files) does not cover it
- **Description**: `git ls-files crates/nif/examples | grep -c _tmp_` returns
  **57**. Each is a throwaway probe from a past audit (`_tmp_obl_d4_props.rs`,
  `_tmp_sf_d2_dump.rs`, `_tmp_sky27b_bto.rs`, …), each is a real Cargo example
  target, and every one is compiled by `cargo build --examples` and by
  `cargo test -p byroredux-nif`. Seven more are currently untracked from
  concurrent audits (`_tmp_fo3_0828_probe.rs`, `_tmp_sky27b_*.rs`).
- **Evidence**: `cargo build --release -p byroredux-nif --examples` this session
  emitted `unused_mut` warnings from `_tmp_obl_d4_mat` and `_tmp_sk_slotflags` —
  scratch code contributing warning noise to a normal build of the crate.
- **Impact**: Build time and warning noise on every examples build, plus a
  discoverability cost (the 20 genuine, documented examples — `nif_stats`,
  `recovery_trace`, `trace_block`, `probe_lod_corpus`, … — are outnumbered
  nearly 3:1 by scratch). No runtime impact.
- **Related**: `#3150` (ESM-2026-08-20-D4-01, the `crates/plugin` sibling).
- **Suggested Fix**: Delete them, or move the handful with lasting value under
  an explicit `examples/probes/` with a `required-features` gate so they are not
  built by default. Worth folding into whatever commit closes `#3150`.

## Blocker Chain (interiors already render end-to-end)

1. ~~BSA v103 decompression~~ — closed (`#699`); re-confirmed live this session
   (two archives, every `.nif` and `.kf` extracted, zero failures).
2. ~~TES4 worldspace + LAND wiring~~ — closed (`#1556`), implemented and
   game-agnostic. Unchanged.
3. **On-device exterior render bench** — the one remaining step. Needs a live
   Vulkan device; out of scope for a source-only pass. No new evidence either
   way this session.
4. Any placement/LOD gaps that bench surfaces — not knowable without step 3.
   `OBL-2026-08-27-03` hardens the Oblivion placement-LOD reader that step 3
   will exercise, so it is worth fixing before the bench rather than after.

## Regression Guard List (re-verified this session)

| Guard | Where | Result |
|---|---|---|
| v10.x stride-drift family (#1506/#1507/#1508) | `version.rs` predicates + unit tests + 100 % archive sweep | HOLD |
| #1509 morph `bsver > 9` gate | `crates/nif/src/blocks/controller/morph.rs:109,219` | HOLD |
| #2345 pre-10.1.0.106 `NiSequence`/`ControlledBlock` layout | `crates/nif/src/blocks/controller/sequence.rs` vs nif.xml, field by field | HOLD (new) |
| #402 Oblivion `ControlledBlock` string-palette resolution | `crates/nif/src/anim/controlled_block.rs` + 1843/1843 KF probe | HOLD |
| `NiTexturingProperty` raw u32 shader-map count (#149) | `crates/nif/src/blocks/properties.rs:328-350` | HOLD |
| BSStreamHeader dual-band guard (#170) | `crates/nif/src/header.rs:137-143` | HOLD |
| `user_version >= V10_0_1_8` threshold | `crates/nif/src/header.rs:114` | HOLD |
| #1652 full Havok motion-type enum | `crates/nif/src/import/collision/mod.rs:223-231` | HOLD |
| `BhkMultiSphereShape` / `BhkConvexListShape` resolve arms | `crates/nif/src/import/collision/shape.rs` | HOLD |
| #1650 16-byte Oblivion ACBS gate ordering | `crates/plugin/src/esm/records/actor/mod.rs:899` + 914-record byte census | HOLD |
| BSA v103 open/extract + count caps (#699 / #586) | `crates/bsa/src/archive/open.rs`, `extract.rs` | HOLD |
| Disney-BSDF gate stays 0 for Oblivion (#1248-#1252, #2570) | zero `BSShader*` rows in `oblivion.tsv` | HOLD |
| `NiWireframeProperty` / `NiShadeProperty` wiring (#869) | `legacy_properties.rs` flag-property arm | HOLD |
| `_far.nif` distant-LOD placement scheme | `byroredux/src/cell_loader/placement_lod.rs:195-247,307-309` | HOLD |
| Pre-5.0.0.1 inline-name `debug!`-not-`warn!` logging | 8032-file sweep, no per-block warn spam | HOLD |
| `Material::resolve_pbr` resolve-once (no per-draw classifier) | `byroredux/src/render/static_meshes.rs` | HOLD |

## Prior-Report Reconciliation (`AUDIT_OBLIVION_2026-08-24.md`)

| Prior item | Status now |
|---|---|
| `OBL-D6-NEW-01` — `oblivion.tsv` gate fails on `NiSingleInterpController 3034 -> 0` | **CLOSED** — gate passes live (120 types matched) after `2d7a6f02` (#3326) + `19b844bf` (#3175/#2574) |
| `#2574` — oblivion.tsv baseline stale | **CLOSED** |
| `#2564` — truncation baseline / ROADMAP stale | **CLOSED**; `ROADMAP.md:577` now reads 100 % (8032/8032), matching live |
| `#3084` — creature-asset corpus guard not `#[ignore]`d | **CLOSED** |
| `#2347` — `nif_stats --tsv` header drift | **CLOSED** |
| `#2346` — stale doc-comment line numbers | **CLOSED** (`f9437a35`) |
| `#2419` — ROADMAP TES-grounding row stale | **CLOSED** (but see `OBL-2026-08-27-04` — a *different* line in the same block is still wrong) |
| `#1219` — `NifVariant::detect` ambiguity warning | **CLOSED**; the log line is informational and still emitted once per sweep |
| `#2565` — TexDesc version gaps + PS2 L/K divergence | **STILL OPEN**, unregressed, not re-filed. Distinct from `OBL-2026-08-27-01`: `#2565` is about *which bytes are read*, `-01` is about *what the stored word means* |
| `#2571` — raw-tier `ImportedMaterial` fields bypass the NIFAL boundary | **STILL OPEN**, spot-verified, not re-filed |
| `#2572` — `resolve_normal_alpha_spec_roughness` post-mutates canonical roughness | **STILL OPEN**, not re-filed |
| `#2573` — `resolve_pbr` backstop hardcodes `specular_authored: false` | **STILL OPEN**, not re-filed |

## Methodology Note

Static analysis plus parser/importer harnesses only — no engine instance was
launched, no GPU device exercised, no `pkill`. "Confirmed" in Dimensions 4 and 5
means source-level trace plus a passing test or a real-data census, never an
on-device visual check; `OBL-2026-08-27-01`'s rendering impact is inferred from
the sampler table it indexes, not observed in a frame capture. Two throwaway
`crates/nif/examples/_tmp_*` probes were built for the `TexDesc.flags` census
and the `.kf` import sweep and **have been deleted**; their outputs are quoted
verbatim above. The census of vanilla `Oblivion.esm` `CREA` ACBS lengths was run
with an independent Python GRUP walker, deliberately not through this
repository's own parser, so it is an external check rather than a tautology.

---

Suggested follow-up:

```
/audit-publish docs/audits/AUDIT_OBLIVION_2026-08-27.md
```

Label every finding `game:oblivion` + `legacy-compat`, plus its own domain
label (`-01`/`-02` → `nif-parser` + `import-pipeline` + `game:fo3` + `game:fnv`;
`-03` → `terrain-exterior` + `safety`; `-04` → `doc-rot`; `-05` → `tech-debt`).
