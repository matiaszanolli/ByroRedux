# Oblivion (TES4) Compatibility Audit — 2026-09-11

Orchestrator audit covering 7 dimensions: NIF version handling (v20.0.0.4 +
v10.x NetImmerse tail), BSA v103 archive, ESM record coverage, the rendering
path for Oblivion shaders, NIFAL canonical material translation, real-data
validation against vanilla game files, and the exterior blocker chain /
game-specific quirks. Each dimension was run as an independent agent reading
live source and, where applicable, real vanilla Oblivion + DLC data at
`/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.

## Executive Summary

**Compatibility level, as measured live during this audit (not carried over
from prior claims):**

- **NIF parse (incl. v10.x tail)**: 100% clean, 100% recoverable, **0
  truncations** across all 9 vanilla archives (base + 8 DLC), 9,612/9,612
  NIFs (`per_block_baseline_oblivion`, `oblivion_block_count_parity`,
  `oblivion_stream_drift_corpus` — all PASS, live-run during this audit).
  120 block types in the corpus, all dispatched, 0 unknown. Full
  `byroredux-nif` suite: 1,270 lib tests, 0 failed. The #1506/#1507/#1508/#1509
  stride-drift regression-guard family holds; no new drift found.
- **BSA v103 archive extraction**: 100% clean — 147,629 files across all 17
  vanilla + DLC `.bsa` files, 0 extraction failures (live full-corpus sweep
  run during this audit). The "v103 is broken" premise remains dead (#699).
- **ESM parse (live path, not a stub)**: All Oblivion-specific decode
  branches (16-byte ACBS #1650, MGEF-by-code map, CONT 4-byte payload, CLMT
  three-entry WLST, RACE/CLAS Oblivion arms, XCLL 3-size family, DIAL/INFO
  GetIsID speaker fallback) verified correct against source and, where
  testable, against real `Oblivion.esm`. Both previously-`#[ignore]`d
  real-data parity tests (`clas_oblivion_knight_against_vanilla`,
  `race_oblivion_data_and_subs_against_vanilla`) still pass unmodified
  against vanilla data.
- **Render end-to-end**: Interior renders end-to-end (Anvil Heinrich Oaken
  Halls). **Exterior cells already render on-device** — Tamriel `(0,0)`
  radius 1 measured 6,043 entities / 2,355 draws (2026-08-12 EX-01/EX-05,
  image-health + environment-value gates both clean). This audit found
  `docs/feature-matrix.md` incorrectly still says this bench is "pending" —
  see MEDIUM finding below; `ROADMAP.md` and
  `docs/engine/exterior-readiness-plan.md` carry the accurate, current state.

**No CRITICAL findings.** **1 HIGH finding** — a real, measured visual defect
affecting ~7.8% of Oblivion's `APPLY_HILIGHT2` parallax population (100 of
1,274 properties use a BC1 normal map with no alpha channel, and the height
slot is still bound, so the POM marcher ray-marches the normal map's X
channel instead of a height field). **2 MEDIUM findings** (a 67%-affected
particle-emitter mis-attribution bug, and a month-old documentation-drift bug
in `feature-matrix.md`). **11 LOW findings**, mostly test-coverage gaps,
dormant/unconsumed fields that are correctly documented as dormant, and one
doc-pointer correction.

**Top blockers in priority order:**
1. (HIGH) Fix the APPLY_HILIGHT2 binding/alpha-gate mismatch — Dimension 4.
2. (MEDIUM) Fix scene-level particle-emitter first-match attribution for
   multi-emitter Oblivion NIFs (67% of the emitter-bearing corpus) —
   Dimension 4.
3. (MEDIUM) Correct `docs/feature-matrix.md`'s stale "bench pending" Oblivion
   exterior cells — Dimension 7.
4. Everything else is LOW / hardening / regression-guard maintenance.

There is **no remaining blocker to a first exterior render** — that chain is
already closed (see Blocker Chain below). Remaining exterior work is the
repeatable readiness matrix (#2377/#2368), not a first render.

## Dimension Findings

### Dimension 1: NIF Version Handling — v20.0.0.4 + v10.x NetImmerse Tail

All 10 checklist items confirmed correct against nif.xml and Gamebryo 2.3
source. Full `byroredux-nif` suite: 1,270 lib tests, 0 failed.

**LOW**
- Inline block-type-name inference keys off `header.block_types.is_empty()`
  (`crates/nif/src/lib.rs:404`) rather than the version threshold directly
  (`crates/nif/src/header.rs:231`). Unreachable on any shipping content;
  purely a diagnostic-quality nit on hand-corrupted/fuzzed input.
- `BSFaceGenNiNode` has a dedicated parser (`crates/nif/src/blocks/node.rs:1053-1059`)
  but no arm in the scene walker's `as_ni_node`
  (`crates/nif/src/import/walk/mod.rs:102-143`) — the subtree is silently
  dropped. **Zero impact for Oblivion** (0 occurrences in the 9,612-file
  corpus; this is a FO3-era-and-later type). Flagged for the
  character/NIFAL dimension to investigate on Skyrim/FO4 head meshes, not
  fixed here.

### Dimension 2: BSA v103 Archive (regression guard)

**LOW**
- No permanent brute-force full-archive extraction test for v103, unlike the
  existing v105 test (`crates/bsa/tests/bsa_real.rs`). A live ad hoc sweep run
  during this audit found 147,629 files / 0 failures across all 17 vanilla +
  DLC archives — v103 extraction is provably 100% clean today, this is a
  coverage gap for catching a *future* regression, not a present defect.

Regression guards (version recognition, 16-byte folder records for v103/v104
vs. 24 for v105, the Xbox-archive-flag / embed-name gate, hash functions,
codec dispatch) all confirmed correct by source read + live extraction run.

### Dimension 3: ESM Record Coverage (live path, not a stub)

No CRITICAL/HIGH/MEDIUM defects.

**LOW**
- The audit brief's own checklist pointer for TES4/HEDR handling names
  `crates/plugin/src/esm/records/grup_walker.rs`; the actual detection
  (`EsmVariant::detect`, `GameKind::from_header`) lives in
  `crates/plugin/src/esm/reader.rs:101-239`. Documentation/navigation
  correction only, not a code defect.

All Oblivion-specific decode branches (16-byte ACBS #1650 ordered correctly
before FO4/Skyrim/FNV arms; MGEF-by-code map gated to `GameKind::Oblivion`;
CONT 4-byte payload guard; CLMT three-entry WLST dispatched by `GameKind`
rather than a length heuristic; RACE/CLAS Oblivion `is_oblivion`/
`flags_oblivion` arms; XCLL `[28,32,36]` size family; CELL block/sub-block
grouping; ACRE gated to Oblivion; DIAL/INFO 24-byte CTDA + GetIsID speaker
fallback for the zero-ANAM/PNAM Oblivion convention) verified correct with
passing unit tests, and both previously-ignored real-data parity tests
(`clas_oblivion_knight_against_vanilla`: 111 classes, Knight matches vanilla;
`race_oblivion_data_and_subs_against_vanilla`: 15 races, all sane) still pass
against real `Oblivion.esm` (~2.7s each, no memory pressure).

### Dimension 4: Rendering Path for Oblivion Shaders

**HIGH**
- **APPLY_HILIGHT2 binds the normal map into the height slot
  unconditionally, but the alpha-channel selector is format-gated — BC1
  normals make the POM marcher read `normal.r` as height.**
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:289-294` /
  `byroredux/src/scene/nif_loader.rs:1067` bind
  `textures.height = textures.normal` whenever
  `material.parallax_height_in_alpha` is set, with no format check. The
  format check exists only downstream, at
  `byroredux/src/render/static_meshes.rs:480-486`, gating the
  `PARALLAX_ALPHA_HEIGHT_BIT` — when the format lacks alpha (BC1/DXT1) the
  bit is withheld but **the binding is not undone**, so POM still runs and
  reads the normal map's X channel as a height field. Measured over the real
  Oblivion + Shivering Isles mesh archives: 1,274 `APPLY_HILIGHT2`
  properties total, 100 of them (7.8%) have a DXT1 (BC1, no alpha) `_n.dds`
  sibling and hit this defect — the same visual-artifact class (grazing-angle
  UV swimming) that `#3562` was written to prevent, at lower amplitude.
  **Fix**: when `parallax_height_in_alpha && !normal_has_alpha`, zero
  `parallax_map_index` entirely at the render site rather than only
  withholding the alpha bit.

**MEDIUM**
- **Scene-level first-match particle-emitter attribution is wrong for 67% of
  Oblivion's emitter-bearing NIFs.** `crates/nif/src/import/walk/emitter.rs`'s
  three emitter-parameter extractors scan the whole scene and use the first
  matching block for every emitter instance in the file. Measured: 208 NIFs
  carry a `NiPSysEmitter`; 140 of those (67.3%) carry more than one, and 139
  import multiple `ImportedParticleEmitter`s that all receive identical
  kinematics from the first emitter block. Real Oblivion content hit by
  this: `transformation.nif` (13 emitters), `obgatemini01.nif` (11),
  `restoration.nif` (11), `skeleton.nif` (10), `drain.nif` (9),
  `invisibility.nif` (8) — spell VFX and Oblivion-gate effects lose their
  authored layering (fast core + slow haze collapse to one).

**LOW**
- `gamebryo_to_vk_blend_factor`'s out-of-range fallback (`SRC_ALPHA`) applies
  to the destination factor too, where the engine default is
  `INV_SRC_ALPHA` — defensive-path only, no vanilla exposure observed.
- `NiStencilProperty` state is captured into `MaterialInfo` and never leaves
  the NIF crate — correctly documented as dormant at both ends (#337,
  blocked on depth-format stencil bits); the two-sided promotion Oblivion
  actually needs does reach the renderer via a separate path (#930).
- `NiTexturingProperty` apply modes 1 (`APPLY_DECAL`) and 3
  (`APPLY_HILIGHT`) are decoded but never consumed (681 of 30,121 Oblivion
  properties have non-default intent) — correctly left unconsumed per the
  no-guessing policy (#3625), PC-era semantics are genuinely unsourced.

Regression guards (base/dark/normal-from-bump/glow/detail/gloss slot
pipeline #131/#264/#450; the #3596 fix confirmed correct — `apply_mode ==
APPLY_HILIGHT2 && parallax_map.is_none()`, no `normal_map.is_some()` gate;
raw monitor-space colors, no sRGB linearization; full 11-value AlphaFunction
routing; #869 `NiWireframeProperty`→LINE pipeline and
`NiShadeProperty.flat_shading`→fragment-shader wiring; vertex-color ×
material-color non-double-counting; #1239 Oblivion emitter version gating;
typed-emitter-import → ECS runtime reachability; Disney BSDF gate provably 0
across the all-legacy Oblivion material universe) all confirmed correct.

### Dimension 5: NIFAL Canonical Material Translation for Oblivion

No CRITICAL/HIGH/MEDIUM findings — all 4 checklist invariants hold.

**LOW**
- Oblivion's `APPLY_HILIGHT2` arm (`crates/nif/src/import/material/legacy_properties.rs:303-308`)
  fabricates `Some(4.0)`/`Some(0.04)` parallax scalars as detached literal
  copies of the canonical `DEFAULT_PARALLAX_MAX_PASSES`/
  `DEFAULT_PARALLAX_HEIGHT_SCALE` constants (introduced by #3073 precisely to
  prevent this pattern), converting "unauthored" into "authored" at the raw
  tier. Numerically inert today (values match the canonical defaults, and
  the branch measured 0/35,322 meshes with the flag actually set before
  #3596); becomes a live per-game divergence the moment either default is
  retuned.
- Three exterior spawners (`terrain.rs`, `terrain_lod.rs`,
  `terrain_lod_btr.rs`) attach `MaterialTextureHandles` via the sibling
  `translate_texture_only_material` boundary with hardcoded parallax
  literals and without the module doc's stated Phase-2 resolver invariant
  covering them. Verified structurally inert by construction (none of the
  three resolvers' gates can currently fire on that path), but the doc text
  doesn't say so. Not Oblivion-specific (EXAL territory).

Regression guards (single canonical `translate_material` boundary; NaN-
sentinel PBR resolved exactly once via `Material::resolve_pbr`, no per-draw
`classify_pbr` reintroduced; `EmissiveSource::Material` distinct arm for
Oblivion legacy content; `MAT_FLAG_PBR_BSDF` provably 0 for Oblivion) all
confirmed correct with passing tests.

### Dimension 6: Real-Data Validation

No CRITICAL/HIGH/MEDIUM findings. Live measurement against the full 9-archive
vanilla + DLC corpus matches ROADMAP.md and the checked-in baselines exactly:
`per_block_baseline_oblivion` (8,032/8,032 clean, 120 types, 0 unknown),
`oblivion_block_count_parity` (9,612/9,612 whole, 0 truncating across all 9
archives), `oblivion_stream_drift_corpus` (0 false-positive drift warnings).
3 representative mesh traces (chandelier, book, creature head) all parsed
clean with sane mesh counts and resolved material chains.

**LOW**
- `crates/nif/examples/nif_stats.rs`'s `--tsv` histogram does not actually
  implement the byte-for-byte parity with `tests/common::PerBlockHistogram`
  that its own module doc claims (missing the #3326 wire-name-keying fix) —
  a manual cross-check against the checked-in baseline TSV using this tool
  produces a wall of false "differences" (e.g. `NiTriShape` collapsing
  `NiTriShape`/`NiTriStrips`/`BSSegmentedTriShape`) that look like a severe
  regression but aren't — the actual regression-gate tests are unaffected
  since they use the correctly-keyed common module.

### Dimension 7: Exterior Blocker Chain & Game-Specific Quirks

**MEDIUM**
- **`docs/feature-matrix.md`'s Cell Loading table is stale against a
  real, already-landed Oblivion exterior bench.** It currently reads
  "bench pending" / "device check pending" for Oblivion's exterior grid and
  confirmed-bench rows, and its own explanatory note says "only an on-device
  exterior render bench is pending." This has been false since commit
  `f90e4eec` (2026-08-12, Fix #2368), which recorded a real on-device
  measurement (6,043 entities / 2,355 draws, both image-health and
  environment-value gates clean) and updated `ROADMAP.md` and
  `docs/engine/exterior-readiness-plan.md` — but never touched
  `feature-matrix.md`, which has since been edited four more times for
  unrelated FNV/Skyrim/FO4 bench-number updates in the very same table
  without ever correcting Oblivion's cells. **Fix**: change line 20's
  Oblivion cell to "✓", line 23's to "6,043 ent / 2,355 draws (image-health +
  env-value gates clean, 2026-08-12, #2368)" (entities/draws, not FPS — don't
  fabricate an FPS figure to match the row's other cells), and rewrite
  lines 25-28's note accordingly. FO3 remains genuinely pending and should
  stay as-is.

**LOW**
- Oblivion's animated statics (the `obgate*`/`oblivionarchgate*` gate family,
  plus other machinery) play only their first authored
  `NiControllerSequence` — 423/8,032 Oblivion-Meshes files carry 792
  sequences total, and selecting the correct one (e.g. a gate's Open vs.
  Close) needs an activation-trigger system that doesn't exist yet. Already
  self-documented in code (#3602's own residual scope) and transparently
  logged at `debug`, not silent — visuals-only impact.

Regression guards (`--bsa` CLI path opens/lists/extracts Oblivion v103
end-to-end; ACHR/ACRE/REFR triad needs no extra Oblivion-only record type;
pre-5.0.0.1 inline-name logging stays at `debug` per-file, `warn` only on
rare mid-file failure, keyed per-block not per-file; `placement_lod_supported`
correctly gates on `GameKind::Oblivion` alone, with FO3/FNV's separate
`FalloutLegacyBlocks` object-LOD scheme intact) all confirmed correct.

## Blocker Chain

Interiors already work end-to-end (Anvil Heinrich Oaken Halls). TES4
worldspace + LAND wiring is already implemented and game-agnostic (#1556),
and **exterior cells already render on-device** (Tamriel `(0,0)` radius 1,
6,043 entities / 2,355 draws, 2026-08-12 EX-01/EX-05). **The chain to first
exterior render is already closed.** The remaining chain is the repeatable
readiness matrix (#2377/#2368) plus the specific gaps this audit surfaced:

1. ~~BSA v103 decompression~~ — closed (#699), reconfirmed clean this audit
   (147,629/147,629 files).
2. ~~TES4 worldspace + LAND wiring~~ — closed (#1556).
3. ~~First on-device exterior render~~ — closed (2026-08-12 EX-01/EX-05,
   6,043 ent / 2,355 draws).
4. **Documentation correction**: `docs/feature-matrix.md` still frames step 3
   as pending — fix so the readiness state is discoverable from either doc
   (Dimension 7 MEDIUM finding).
5. **Visual-quality gap**: the APPLY_HILIGHT2/BC1 POM defect (Dimension 4
   HIGH) and the particle-emitter mis-attribution (Dimension 4 MEDIUM) are
   both visible-content-affecting but do not block rendering.
6. Repeatable readiness matrix (#2377/#2368) — ongoing, not blocked by
   anything this audit found.

## Regression Guard List

Previously-fixed items this audit re-verified still hold, with live
measurements taken during this audit run:

- **v10.x stride-drift family** (#1506 `NiQuatTransform.TRS Valid` /
  `NiInterpController.Manager Controlled`, #1507 emitter base + `NiPSysData`,
  #1508 `NiBlendInterpolator` + `ControlledBlock`, #1509
  `NiGeomMorpherController` `bsver > 9` gate) — all four still land exactly
  on the block boundary; no new truncation growth.
- **`NiTexturingProperty` raw u32 shader-texture count**, no leading bool
  gate (`crates/nif/src/blocks/properties.rs:428-431`) — confirmed, the
  highest-stakes Oblivion gate (30,121 occurrences, no block-size table to
  recover from a miss).
- **BSStreamHeader dual-band guard (#170)** and **`user_version` >= V10_0_1_8
  threshold** — confirmed exact nif.xml transcription, regression test
  present and passing.
- **BSA v103 extraction (#699)** — reconfirmed 100% clean, 147,629/147,629
  files, live full-corpus sweep.
- **16-byte ACBS guard (#1650)** — confirmed correctly ordered ahead of
  later-game arms; both named regression tests pass.
- **`havok_motion_type` full-enum mapping (#1652)** — the pre-fix
  `4=>Keyframed`/`_=>Static` collapse has not returned.
- **APPLY_HILIGHT2 reachability fix (#3596)** — confirmed the #3530 bug
  (gating on `normal_map.is_some()`) is genuinely fixed; 92.2% of the
  `APPLY_HILIGHT2` population now reaches the intended alpha-channel path
  (the residual 7.8% is this audit's new HIGH finding, a different bug in
  the same area).
- **Disney BSDF gate (#1248-#1252)** — `MAT_FLAG_PBR_BSDF` provably 0 across
  the entire Oblivion material universe; only reachable via BGSM/BGEM/CDB
  paths Oblivion cannot enter.
- **`#869` `NiWireframeProperty` / `NiShadeProperty.flat_shading` wiring** —
  both confirmed wired end-to-end to the renderer.
- **No sRGB linearization of legacy colors (0e8efc6)** — reconfirmed, exactly
  one unrelated hit for `srgb_to_linear` repo-wide.

## Statistics

- **Total findings: 14**
- **CRITICAL: 0 · HIGH: 1 · MEDIUM: 2 · LOW: 11**
- Dimensions with zero findings above LOW: Dimension 3 (ESM), Dimension 6
  (real-data validation, aside from a tooling nit).

---

Suggested next step: `/audit-publish docs/audits/AUDIT_OBLIVION_2026-09-11.md`
(label every finding `game:oblivion` + `legacy-compat`, plus its own domain
label: `nif` for Dim 1/6, `bsa` for Dim 2, `esm` for Dim 3, `renderer` for Dim
4, `nifal` for Dim 5, `docs`/`cell-loader` for Dim 7).
