# Oblivion (TES4) Compatibility Audit — 2026-08-24

**Run mode**: Solo (no sub-agent fan-out — all dimensions read/traced/tested
directly in one session per explicit run constraint).
**Scope**: Full comprehensive sweep, all 7 dimensions, no `--focus` filter.
**Game data**: `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/` (present, used for every real-data check below).

## Executive Summary

Oblivion compatibility is in a **mature, heavily-regression-guarded state**.
Every documented fix and invariant this skill's checklist asks to re-verify
(the v10.x stride-drift family #1506/#1507/#1508, the #1509 morph bsver gate,
BSStreamHeader dual-band #170, `NiTexturingProperty` raw-u32-count, the #1652
Havok motion-type enum, the #1650 16-byte Oblivion ACBS gate, the #1239
particle-emitter version gate through to `apply_emitter_params`, the #2570
Disney-BSDF-gate-stays-0 invariant, the Oblivion `NiMaterialProperty →
EmissiveSource::Material` arm, and the `_far.nif` distant-LOD placement
scheme) was re-traced against live source **and** re-run as a test where a
test exists. All held. **Zero regressions found** in NIF version handling,
BSA v103 archive extraction, the ESM path, material/NIFAL translation, or the
exterior LOD blocker chain.

Live-measured against the current repo + vanilla data (not carried over from
ROADMAP prose):

- `Oblivion - Meshes.bsa`: **8032/8032 NIFs parse clean (100.00%), 0
  truncated, 0 failed** (`nif_stats`, this session). This is *better* than
  ROADMAP.md's stated 99.93% (8,026/8,032, "6 residual truncated") — that row
  is stale prose, already tracked as `#2564` (OBL-D1-03), and the checked-in
  `oblivion_truncations.tsv` baseline has already been regenerated to match
  (`truncating=0 parsed=8032`) even though the ROADMAP text hasn't caught up.
- The two previously-ignored real-data parity tests
  (`clas_oblivion_knight_against_vanilla`, `race_oblivion_data_and_subs_against_vanilla`)
  both pass against vanilla `Oblivion.esm`.
- Full test suites for every Oblivion-relevant crate are green:
  `byroredux-nif` (1092+ unit tests + all integration suites, 0 failed),
  `byroredux-plugin` (779 unit tests, 0 failed), `byroredux-bsa` (63 unit +
  4 real-archive integration tests, 0 failed, including the live
  `oblivion_meshes_bsa_v103_extracts_nif_with_gamebryo_magic` real-data test),
  and the `byroredux` binary (1514 tests, 0 failed, including the live
  `installed_oblivion_creature_assets_resolve_from_their_records` corpus
  guard and the `placement_lod`/`far_nif_derivation` LOD suite).
- One genuinely new, low-severity finding surfaced: the checked-in
  `per_block_baselines/oblivion.tsv` opt-in regression gate **currently
  FAILS** on a live run, for a *different* reason than the one already
  tracked under `#2574` (see OBL-D6-NEW-01 below).

**10 pre-existing OPEN issues** from the last Oblivion audit sweep remain
open (`#3084`, `#2574`, `#2573`, `#2572`, `#2571`, `#2565`, `#2564`, `#2419`,
`#2347`, `#2346` — all LOW/MEDIUM, mostly doc-rot / NIFAL-boundary
tech-debt / stale-baseline items). They were spot-checked, not
re-litigated line-by-line; nothing found this session contradicts their
descriptions except where noted below (`#2574`'s specific evidence is
partially stale — see OBL-D6-NEW-01).

**Top blockers, in priority order**: none newly identified. The only
remaining item on the documented Blocker Chain (below) is unchanged from
the prior audit sweep: an on-device exterior render bench pass, which needs
a live Vulkan device and is out of this audit's scope.

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.4 + v10.x NetImmerse tail)

No new findings. Every regression guard re-verified directly against source:

- `header.rs:114` — `user_version` gated on `>= V10_0_1_8`, exactly as
  documented.
- `header.rs:137-143` — `has_bs_stream_header` implements the full
  documented dual-band formula: `VER == 10.0.1.2 OR ((VER ∈ {20.2.0.7,
  20.0.0.5} OR (10.1.0.0 <= VER <= 20.0.0.4 AND USER <= 11)) AND USER >= 3)`.
  Matches the Known Quirks doctrine exactly.
- `properties.rs:328-350` — `NiTexturingProperty` reads the shader-texture
  count as a raw `u32` with no leading bool gate, with the #149 regression
  explained in-line. Still correct.
- `version.rs` — all v10.x sub-version constants (`V10_0_1_2`,
  `V10_1_0_0..V10_1_0_114`, `V10_2_0_0`, `V20_0_0_4`, `V20_0_0_5`) present
  and used as documented; `has_interp_controller_manager_controlled` (#1506),
  `has_quat_transform_trs_valid` (#1506), `has_skin_data_partition_ref` /
  `has_skin_data_vertex_weights_flag` (#2168) all carry both bounds and are
  unit-tested.
- `blocks/controller/morph.rs` — `NiGeomMorpherController` gates on
  `bsver >= MORPH_LEGACY_CUTOFF` (10), i.e. `#GT# 9`; `Morph.Legacy Weight`
  gates on the complementary `< MORPH_LEGACY_CUTOFF`. #1509 still holds.
- `import/collision/mod.rs:222-229` — `havok_motion_type` maps the full
  8-arm `hkMotionType` enum (#1652); the pre-fix `4 => Keyframed / _ =>
  Static` collapse is gone.
- `import/collision/shape.rs` — `BhkMultiSphereShape` (line 110) and
  `BhkConvexListShape` (line 235) both have live `resolve_shape_inner` arms;
  neither falls through to a silent drop.
- `lib.rs:379-384` vs `404-407` — the pre-5.0.0.1 inline-name path still logs
  at `debug!` per-file and only escalates to `warn!` on the rare mid-file
  read failure (the `marker_radius.nif` #698 case). No spam-risk drift.
- Block dispatch coverage: `NiKeyframeController`, `NiSequenceStreamHelper`,
  `NiBillboardNode`, the full `NiLight` hierarchy, `NiUVController`,
  `NiCamera`, `NiTextureEffect` all have live dispatch arms in
  `blocks/mod.rs`.

Real-data confirmation: `nif_stats` over `Oblivion - Meshes.bsa` — **8032/8032
clean (100.00%), 0 truncated, 0 failed, 85 distinct block types, 0 unknown
blocks**. See Executive Summary for the ROADMAP-vs-reality delta (tracked as
`#2564`).

### Dimension 2 — BSA v103 Archive

Regression guard, confirmed still holding. `BSA_V_OBLIVION = 103` recognized;
rejection only outside `{103, 104, 105}` (`open.rs:40-44`). Folder-record
size is `if version == BSA_V_SKYRIM_SE { 24 } else { 16 }` — v103/v104 both
16 bytes, matching the corrected doctrine (`open.rs:100`). Live-verified via
`cargo test -p byroredux-bsa --test bsa_real -- --ignored`:
`oblivion_meshes_bsa_v103_extracts_nif_with_gamebryo_magic` passes against
the real archive.

### Dimension 3 — ESM Record Coverage (live path)

No new findings. Re-verified:

- `esm/records/actor/mod.rs:834` — the 16-byte Oblivion `ACBS` arm
  (`GameKind::Oblivion`, `len >= 16`) is gated **before** the FNV/Skyrim/FO4
  arms, exactly as #1650 requires.
- `esm/records/misc/dialogue.rs` — `parse_dial`'s `DATA` byte-0 read is
  cross-game safe (Oblivion single-byte, FO3+ wider) and unit-tested
  (`parse_dial_captures_dialogue_type_byte`); `parse_info`'s `TRDT` layout
  (emotion @0, response number @12) matches #1304.
- `esm/records/container.rs` — the 4-byte Oblivion `CONT DATA` payload
  (vs. 5-byte FO3+) is guarded and unit-tested
  (`cont_data_handles_oblivion_4byte_payload_without_overrun`).
- `esm/records/index.rs` / `dispatch_misc_gameplay_b.rs` — the Oblivion
  4-char `EFID`→MGEF code map (`magic_effects_by_code`) is live.
- Both previously-`#[ignore]`d real-data parity tests
  (`clas_oblivion_knight_against_vanilla`,
  `race_oblivion_data_and_subs_against_vanilla`, in
  `crates/plugin/tests/parse_real_esm.rs`) pass against vanilla
  `Oblivion.esm` (verified this session with `BYROREDUX_OBL_DATA` set).
- `esm/cell/walkers.rs` — the per-game `XCLL` canonical-size table
  (`XCLL_SIZES_OBLIVION = [28, 32, 36]`) and `RCLR` handling are live and
  distinct from the FO/Skyrim/FO4/76/SF tables.
- Full `byroredux-plugin` crate test suite: **779 passed, 0 failed**.

### Dimension 4 — Rendering Path for Oblivion Shaders

No new findings. All the "honored or dropped" property list is honored:
`NiStencilProperty` (two-sided + full stencil state), `NiZBufferProperty`
(depth write/func), `NiVertexColorProperty` (source/lighting mode),
`NiSpecularProperty` (#220), `NiWireframeProperty` (routes to the LINE
pipeline via `static_meshes.rs:643`, #869), `NiDitherProperty`, and
`NiShadeProperty` (`flat_shading` consumed at `static_meshes.rs:647`, #869) —
all live in `legacy_properties.rs`, none silently dropped.

`NiPSysEmitter`'s #1239 version-gate fix (`blocks/particle.rs:81-89`, `bsver
>= FO3_FNV` replaced with the correct version-based gate) is intact, and the
typed emitter chain (`extract_emitter_params`/`extract_emitter_rate` in
`import/walk/mod.rs`) reaches both live consumers
(`cell_loader/spawn.rs:994`, `scene/nif_loader.rs:604` →
`apply_emitter_params`) — parses-then-animates, not parses-then-drops.

The Disney-BSDF gate-stays-0 invariant (#1248-#1252, cross-referenced with
#2570 below) is enforced by a direct unit test
(`legacy_material_and_texturing_properties_never_yield_a_pbr_material`,
passing) — a bare `NiMaterialProperty` + `NiTexturingProperty` Oblivion shape
never sets `is_pbr`, `from_bgsm`, or `bgsm_pbr_scalars_authored`.

### Dimension 5 — NIFAL Canonical Material Translation

No new findings beyond the 5 already-tracked OBL-D5-* issues (`#2571`,
`#2572`, `#2573`, `#2346`, and the corpus-guard tech-debt item `#3084`) — all
still describe real, unfixed but LOW/MEDIUM gaps and were spot-verified to
still target current code (e.g. `texture_clamp_mode` / `src_blend_mode` /
`dst_blend_mode` are still read directly off `mesh.material.*` at multiple
spawn sites — `cell_loader/spawn/mesh_instance.rs:333,615,822,863,893`,
`scene/nif_loader.rs:897,941,1047,1088` — matching `#2571`'s description,
even though the file/line layout has since moved from
`cell_loader/spawn.rs` into `cell_loader/spawn/mesh_instance.rs`).

`emissive_source_tests.rs::nimaterial_tags_emissive_source_as_material` and
`legacy_is_pbr_tests`'s two tests pass, confirming both the Oblivion
`EmissiveSource::Material` arm and the Disney-gate-stays-0 invariant
(`#2570`) hold. `static_meshes.rs:334-343` reads `m.roughness`/`m.metalness`
directly with the explicit in-code comment "no per-draw keyword scan /
classify_pbr fallback" — the `Material::resolve_pbr` single-resolution
invariant is intact.

### Dimension 6 — Real-Data Validation

**New finding** (see OBL-D6-NEW-01 below): the opt-in per-block-type
regression gate genuinely fails on a live run today, for a reason distinct
from the one `#2574` already documents.

Otherwise: `nif_stats` census (8032/8032 clean, 0 truncated, 0 failed, 85
types) confirms the parser is in a *better* state than any checked-in
baseline or ROADMAP prose currently claims. Traced three representative
interior meshes' block-type presence via the histogram (skinned creature
content: 1596 `NiSkinData`/`NiSkinInstance`/`NiSkinPartition` triples;
architecture: 25244 `NiNode` + 35916 `NiTriShape`; particle-authored content:
547 `NiParticleSystem`/`NiPSysEmitter` pairs) — all present in expected
proportion, no unexplained zero-counts apart from the one flagged below.

### Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks

No new findings. `_far.nif` distant-LOD (`cell_loader/placement_lod.rs`):
`far_nif_path` derives `<stem>_far.nif`, confirmed against the real
`Oblivion - Meshes.bsa` naming (130 `*_far.nif` entries per module doc); all
10 `placement_lod` unit tests pass, including the real-file parse
(`parses_real_single_placement_file`) and the Oblivion-only gate
(`placement_lod_supported_is_oblivion_only`). `--bsa` end-to-end open/extract
confirmed via the live BSA v103 integration test (Dimension 2). Exterior
TES4 worldspace + LAND wiring remains implemented and game-agnostic per the
prior sweep — nothing in this session's code reading contradicts that.

## Blocker Chain (interior already renders end-to-end)

1. ~~BSA v103 decompression~~ — closed (#699), re-confirmed live this session.
2. ~~TES4 worldspace + LAND wiring~~ — closed (#1556), unchanged this session.
3. **On-device exterior render bench** — the one remaining step, needs a live
   Vulkan device; out of scope for a source-only audit pass. No new evidence
   either way this session.
4. Any placement/LOD gaps the bench surfaces — not yet knowable without step 3.

## Regression Guard List (re-verified this session, all holding)

| Guard | Where | Result |
|---|---|---|
| v10.x stride-drift family (#1506/#1507/#1508) | `version.rs` predicates + unit tests | HOLD |
| #1509 morph `bsver > 9` gate | `blocks/controller/morph.rs` | HOLD |
| `NiTexturingProperty` raw u32 count (no bool gate) | `properties.rs:328-350` | HOLD |
| BSStreamHeader dual-band guard (#170) | `header.rs:137-143` | HOLD |
| `user_version` `>= V10_0_1_8` threshold | `header.rs:114` | HOLD |
| #1652 full Havok motion-type enum | `import/collision/mod.rs:222-229` | HOLD |
| `BhkMultiSphereShape`/`BhkConvexListShape` resolve arms | `import/collision/shape.rs` | HOLD |
| #1650 16-byte Oblivion ACBS gate ordering | `esm/records/actor/mod.rs:834` | HOLD |
| CONT 4-byte Oblivion payload guard | `esm/records/container.rs` | HOLD |
| DIAL/INFO byte-0 cross-game dialogue-type read (#1307) | `esm/records/misc/dialogue.rs` | HOLD |
| BSA v103 extraction (#699) | `crates/bsa/src/archive/open.rs` + live archive test | HOLD |
| #1239 `NiPSysEmitter` version gate → runtime | `blocks/particle.rs` + `systems/particle.rs` | HOLD |
| Disney-BSDF gate stays 0 for Oblivion (#1248-#1252, #2570) | `legacy_is_pbr_tests.rs` | HOLD |
| `NiWireframeProperty`/`NiShadeProperty` render wiring (#869) | `render/static_meshes.rs:643,647` | HOLD |
| `_far.nif` distant-LOD placement scheme | `cell_loader/placement_lod.rs` | HOLD |
| Pre-5.0.0.1 inline-name debug-not-warn logging | `lib.rs:379-407` | HOLD |

## New Findings

### OBL-D6-NEW-01: oblivion.tsv per-block baseline gate fails live — NiSingleInterpController reclassification, not the drift #2574 describes
- **Severity**: LOW
- **Dimension**: 6 — Real-Data Validation
- **Location**: `crates/nif/tests/data/per_block_baselines/oblivion.tsv:43`, `crates/nif/tests/per_block_baselines.rs`
- **Status**: NEW (related to `#2574`, but distinct evidence — see below)
- **Description**: Running the opt-in gate
  (`cargo test -p byroredux-nif --release --test per_block_baselines
  per_block_baseline_oblivion -- --ignored`) fails today with: `PARSED
  shrank NiSingleInterpController 3034 -> 0`. This is **not** the drift
  `#2574` (OBL-D6-01) describes — that issue's cited evidence
  (`bhkCollisionObject` 8784→8730 baseline-vs-live, missing
  `bhkPCollisionObject` row) has **already been fixed**: the checked-in
  baseline now correctly reads `bhkCollisionObject 8730` and
  `bhkPCollisionObject 54`, matching live exactly. So `#2574`'s specific
  reproduction steps no longer reproduce that failure — but the gate still
  fails, now for an unrelated reason that was apparently never folded into
  the same regeneration pass.
  `NiSingleInterpController` is an **abstract** nif.xml base class (never a
  concrete on-disk block-type name). The comments in
  `crates/nif/src/blocks/controller/mod.rs:282,290-292` explain that
  pre-`#2562`/`#2563` several concrete controller subclasses parsed as a
  bare `NiSingleInterpController` (RTTI-erased); `#2562`/`#2563` restored
  their real block-type-name dispatch. Live-summing the four types that
  absorbed the reclassified count exactly accounts for the baseline's 3034:
  `NiTransformController` (2645) + `NiVisController` (361) +
  `NiAlphaController` (27) + `NiKeyframeController` (1) = **3034**. This is
  a **benign, already-shipped precision improvement** (more specific block
  typing, not data loss) — the baseline simply was never regenerated to
  drop the now-dead `NiSingleInterpController` row and add the four real
  rows in its place.
- **Evidence**:
  ```
  [Oblivion] 1 per-block regression(s) vs .../oblivion.tsv:
    PARSED shrank       NiSingleInterpController  3034 -> 0  (filter or dispatch loss?)
  ```
  Live histogram (`nif_stats --tsv --all`) confirms: `NiTransformController
  2645`, `NiVisController 361`, `NiAlphaController 27`,
  `NiKeyframeController 1` — sum 3034, none of the four present in the
  checked-in baseline.
- **Impact**: Same as `#2574`'s stated impact — the gate is opt-in, not
  CI-wired, so nothing is silently broken in production, but anyone who
  actually runs `per_block_baseline_oblivion -- --ignored` today hits a
  false "parser regression?" panic, for a different root cause than the one
  `#2574`'s own reproduction steps describe.
- **Related**: `#2574` (OBL-D6-01, same file, same gate, partially-stale
  evidence — its `bhkCollisionObject` symptom is now fixed, this
  `NiSingleInterpController` symptom is not). `#2564` (OBL-D1-03, sibling
  baseline in the same family, `oblivion_truncations.tsv`, confirmed this
  session to already match live at `truncating=0 parsed=8032` — only
  `ROADMAP.md`'s prose is stale there).
- **Suggested Fix**: Regenerate `oblivion.tsv` with
  `BYROREDUX_REGEN_BASELINES=1` in the same commit that closes `#2574`
  (its own suggested fix already proposes a full regeneration pass) —
  folding this `NiSingleInterpController` split in avoids re-discovering a
  third drift the next time someone runs the gate.

## Pre-Existing Open Issues (not re-litigated, spot-checked)

| Issue | Title | Note |
|---|---|---|
| `#3084` | REG-2026-08-16-D5-03: Oblivion creature-asset corpus guard not `#[ignore]`d | Confirmed still not ignored; test passes live (1.81s) |
| `#2574` | OBL-D6-01: oblivion.tsv baseline stale | Partially stale itself now — see OBL-D6-NEW-01 |
| `#2573` | OBL-D5-03: `resolve_pbr` backstop hardcodes `specular_authored: false` | Not re-verified beyond confirming location exists |
| `#2572` | OBL-D5-02: `resolve_normal_alpha_spec_roughness` post-mutates canonical roughness | Not re-verified beyond confirming location exists |
| `#2571` | OBL-D5-01: raw-tier `ImportedMaterial` fields bypass NIFAL boundary | Confirmed still reproduces — same fields still read raw at spawn sites (paths shifted into `cell_loader/spawn/mesh_instance.rs`) |
| `#2565` | OBL-D1-04: TexDesc version-gap + PS2 L/K divergence | Not re-verified; latent on live corpus per its own description |
| `#2564` | OBL-D1-03: truncation baseline / ROADMAP stale | `oblivion_truncations.tsv` now matches live (0 truncated); `ROADMAP.md` prose confirmed still stale this session |
| `#2419` | TD3-212: ROADMAP TES-grounding row stale | ROADMAP line (now ~1088) already reads "Closed 2026-08-04" with the fix noted — the requested edit appears to already be applied; issue may be closeable |
| `#2347` | OBL-D6-01: `nif_stats --tsv` header drift (cosmetic) | Not re-verified |
| `#2346` | OBL-D5-01: doc comment stale line numbers | Not re-verified |

## Methodology Note

This run touched real Vulkan-adjacent code paths (material/render property
wiring) by static read only — no GPU device was exercised (no `cargo run`,
no RenderDoc capture). "Confirmed" in Dimensions 4/5 means source-level
trace + passing unit test, not an on-device visual check. `cargo test
--workspace` (bare) still fails to build on the pre-existing unrelated
`crates/scripting/examples/fragment_coverage.rs:59` E0004 noted in the task
brief; every crate actually touched by this audit was checked/tested
per-crate instead, and all were green.
