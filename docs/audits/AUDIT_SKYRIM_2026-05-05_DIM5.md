# Skyrim SE Audit — Dimension 5: Real-Data Validation & Rendering
Date: 2026-05-05

## Baseline reproduction

| BSA | total | clean % | truncated | failures | recovered | Notes |
|-----|-------|---------|-----------|----------|-----------|-------|
| `Skyrim - Meshes0.bsa` | 18862 | **100.00%** | 0 | 0 | 0 | matches stated baseline |
| `Skyrim - Meshes1.bsa` | 3185 | **99.81%** | 6 | 0 | 7 | **GATE FAIL** — undispatched `bhkBallSocketConstraintChain`×6, `bhkPlaneShape`×1 |

The audit prompt's stated baseline ("100.00% / 18862 / 18862 on Skyrim Meshes BSAs") is accurate
for **Meshes0** but **not for Meshes1**, where 7 blocks across 6 NIFs drop to NiUnknown.
`nif_stats` exits non-zero on Meshes1 — pinned by the 100 % gate.

Sweetroll demo path: not launched (no display); traced on paper via
`byroredux/src/scene.rs::load_nif_bytes_with_skeleton` →
`byroredux_nif::import::import_nif_scene_with_resolver` →
`crates/nif/src/import/mesh.rs::extract_bs_tri_shape`. Sweetroll
(1 BSTriShape + 1 BSLightingShaderProperty + 1 BSShaderTextureSet,
9 blocks total) takes the unified material extraction path
(`material::extract_material_info_from_refs`, line 784-791) and
should emit one mesh handle and one MaterialInfo with diffuse + normal
(Sweetroll uses 2 of the 9 BSShaderTextureSet slots).

Every recent perf-fix touched ECS / NIF parser counters (#823, #824,
#828, #832, #834, #835); no ground-truth FPS measurement collected
this session. Stale ROADMAP claim of ~3000–5000 FPS is not invalidated
by code review but is also not verified by this audit.

## Content-type traces

| Class | Sample path | Block count | Root type | Shader prop | Expected handles |
|---|---|---|---|---|---|
| Sweetroll (clutter) | `meshes\clutter\ingredients\sweetroll01.nif` | 9 | NiNode | BSLightingShaderProperty | 2 textures (diffuse + normal) |
| Creature skeleton | `meshes\actors\bear\character assets\skeleton.nif` | 161 | NiNode | none (78 NiNode + 19 ragdoll bhkRigidBody) | 0 mesh, 19 collision shapes |
| Tree LOD (DLC2) | `meshes\dlc02\landscape\trees\treepineforestashl02.nif` | 35 | **BSTreeNode** | 4 × BSLightingShaderProperty | 2 BSLODTriShape + 2 BSTriShape, 2 BSShaderTextureSet |
| Magic effect | `meshes\magic\absorbspelleffect.nif` | 88 | NiNode | 2 × BSEffectShaderProperty + 1 × BSLightingShaderProperty | 2 BSTriShape + 38 NiPSysBlock particle systems |

All four parse `clean=1`. The `BSTreeNode`-rooted tree LOD parses correctly
through `is_ni_node_subclass` (lib.rs:616-623, fix #611 verified in real data).
Skeleton-only NIFs (no shape, all bones+ragdoll+constraints) parse cleanly.
Magic effect with BSEffectShaderProperty + 38 NiPSysBlock particle blocks
parses cleanly — no dispatch gaps.

## Findings

(severity-sorted; appended as confirmed)

### SK-D5-NEW-01: Skyrim Meshes1.bsa parse rate is 99.81 %, not 100 % — `bhkBallSocketConstraintChain` + `bhkPlaneShape` undispatched

- **Severity**: HIGH
- **Dimension**: Real-Data Validation & Rendering
- **Location**: `crates/nif/src/blocks/mod.rs` dispatch table (no entry); fall-through to `NiUnknown` recovery; `crates/nif/examples/nif_stats.rs:50` exits non-zero
- **Status**: Existing: #766 (open, currently labelled `low`)
- **Description**: A fresh `nif_stats --release` run against
  `/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim - Meshes1.bsa`
  reports `clean: 3179 (99.81%)`, `truncated: 6`, `recovered: 7`, dropping
  `bhkBallSocketConstraintChain × 6` + `bhkPlaneShape × 1`. The audit-prompt
  baseline ("Parse rate: 100.00% (18862 / 18862) on Skyrim Meshes BSAs") only
  holds for `Meshes0.bsa`. The repository's own `NIF_STATS_MIN_SUCCESS_RATE`
  gate (default 1.0) is currently failing on Meshes1 — anyone running
  the example locally hits a non-zero exit.
- **Evidence**:
  ```
  ─── Parse stats ──────────────────────────────────────────────
    total:       3185
    clean:       3179  (99.81%)
    truncated:      6  (7 blocks dropped)
    recovered:      7  (0 types with partial unknown)
  ─── Unparsed types (no dispatch entry) ───────────────────────
     unknown  type
           6  bhkBallSocketConstraintChain
           1  bhkPlaneShape
  parse success rate 99.81% is below the 100.00% threshold
  ```
  Affected NIFs: `meshes\traps\macetrap\trapmace01.nif`,
  `meshes\traps\skullram\trapskullram01.nif`,
  `meshes\traps\bonealarm01\trapbonealarmhavok01.nif`,
  `meshes\plants\switchnodechildren\slaughterfisheggcluster01_1.nif`,
  `meshes\traps\tripwire\traptripwire01.nif`,
  `meshes\traps\bonealarm02\trapbonealarmbhavok.nif`.
- **Impact**: Trap meshes and certain plant collision payloads drop their
  Havok constraint chain (rope-physics tripwire, ball-and-socket joint
  arrays) — the visible mesh still renders but the trap's physics-driven
  swing motion is silenced. Also: parse-rate-gate regression test will
  fail on every CI Skyrim run that includes Meshes1, masking future
  100 %→99 % drops on Meshes0 once someone disables the gate to "fix"
  the false positive.
- **Suggested Fix**: Bump #766 from `low` to `medium`/`high` and ship
  the two parsers. `bhkPlaneShape` is a single-block bhkShape (8 floats
  for plane normal+distance + 4 unknowns per nif.xml). `bhkBallSocketConstraintChain`
  is a Havok constraint with N pivot-point pairs — see `bhk*Constraint`
  family pattern in `crates/nif/src/blocks/havok/`. Both are FNV-shared
  by Gamebryo lineage so the fix lands across multiple games.

### SK-D5-NEW-02: BSTriShape `data_size mismatch (irrational)` warning floods on every Skyrim parse — false positive on the SSE skin reconstruction path

- **Severity**: MEDIUM
- **Dimension**: Real-Data Validation & Rendering
- **Location**: `crates/nif/src/blocks/tri_shape.rs:467-501`
- **Status**: NEW
- **Description**: When the BSTriShape header reports `data_size > 0` but
  `vertex_size_quads == 0 && num_vertices == 0 && num_triangles == 0`, the
  sanity check at line 468-501 logs a WARN every time. This is exactly
  the **legitimate Skyrim SE skinned-body case** — the shape carries
  `VF_SKINNED` and the real packed vertex buffer lives on the linked
  `NiSkinPartition` (the `try_reconstruct_sse_geometry` path at
  `import/mesh.rs:718-722`, fix #559). The descriptor on the `BsTriShape`
  block legitimately ships `0/0/0` because the geometry is elsewhere.
  The warning's "irrational" branch is taken because `derived_stride` is
  `None` when `num_vertices == 0`, so it falls back to the (also zero)
  descriptor stride and the per-vertex loop iterates zero times — which
  is correct, because `data_size > 0` here is just the persisted size of
  data that lives on a sister block.
- **Evidence**: 67 occurrences in a single Meshes0 parse (sample sizes
  `70144 / 71936 / 98304 / 145920 / 220160 / 364032 / 7680 / 15360 /
  229888` — all powers-of-2-ish, all consistent with packed SSE skin
  vertex buffers). Excerpt:
  ```
  WARN  byroredux_nif::blocks::tri_shape] BSTriShape data_size mismatch:
        stored 70144 vs derived 0 (vertex_size_quads=0, num_vertices=0,
        num_triangles=0) — trusting data_size-derived stride
        (irrational; falling back to descriptor stride)
  ```
  The block's `data_size != 0` gate does NOT inspect `vertex_attrs` or
  `VF_SKINNED`. Same trigger appears in
  `dlc02\landscape\trees\treepineforestashl02.nif` (2 occurrences in a
  single 35-block tree NIF — those are the BSLODTriShape distant-LOD
  shapes, also sharing the SSE-skin payload pattern).
- **Impact**: WARN-level log spam every cell load (~tens of warnings
  per cell in Whiterun / dragon-spawn cells), drowns out actual
  parser warnings. Also: the comment block at lines 450-454 explicitly
  exempts the **`data_size == 0`** false-positive case for FaceGen
  facegen content (#341), but doesn't extend the exemption to the
  symmetric **`data_size > 0` + `num_vertices == 0`** SSE-skin case.
- **Suggested Fix**: Extend the gate at line 468 to also skip the warning
  when `vertex_attrs & VF_SKINNED != 0 && num_vertices == 0` (or simply
  when `num_vertices == 0`, since the per-vertex loop won't run anyway).
  One-line fix in `tri_shape.rs:468`:
  `if data_size != 0 && num_vertices != 0 {`. Net behaviour
  identical (per-vertex loop is gated on `num_vertices`); only the spurious
  warning goes away.

### SK-D5-NEW-03: `BSLagBoneController` + `BSProceduralLightningController` dispatch to `NiTimeController` base only — trailing fields intentionally dropped, but logged at WARN

- **Severity**: MEDIUM
- **Dimension**: Real-Data Validation & Rendering
- **Location**: `crates/nif/src/blocks/mod.rs:630-637`
- **Status**: NEW (the partial parsers and WARN emission are intentional;
  the issue is the WARN-level + the missing trailing-field model)
- **Description**: `BSLagBoneController` and `BSProceduralLightningController`
  are dispatched to `NiTimeController::parse` only (the base class) per
  the explicit "we don't model yet" comment at lines 624-636. The trailing
  3 floats (BSLagBoneController: shake amplitude / damping / shake speed)
  and 3 interp refs + strip data (BSProceduralLightningController) are
  consumed by `block_size` recovery and discarded. The under-consumption
  fires the WARN-level `block_size` realignment notice on every block,
  even though the parser is doing exactly what its dispatch comment says.
  Net effect: 42 WARN events / 11 WARN events for these two types per
  full Meshes0 sweep, all benign by design but indistinguishable in
  the log from real per-block drift bugs (the `BSLODTriShape` ones in
  the same run, where the parser DOES claim to model the trailer, are
  actual data-loss).
- **Evidence**: Single Meshes0 parse run produces `BSLagBoneController`
  warnings on 42 NIFs (78 blocks aggregated), `BSProceduralLightningController`
  on 3 blocks, plus `BSLODTriShape=11` realignments (suspect: real drift,
  see SK-D5-NEW-07 below). Source confirmation:
  ```rust
  // crates/nif/src/blocks/mod.rs:624-636
  // Bethesda / Fallout controller types that extend NiTimeController
  // or NiInterpController with additional fields we don't model yet.
  // Dispatch to the NiTimeController base-parse stub...
  "BSLagBoneController"  // base + 3 floats
  | "BSProceduralLightningController"  // base + 3 interp refs + strip data
  ...
  ```
- **Impact**: For `BSLagBoneController` (used on cape, cloak, hair,
  dragon-wing physics) the bone-lag amplitude / damping / shake speed
  fields are unread. Animations driven by these controllers fall back
  to engine defaults — visual difference is small but content authors
  set non-default values intentionally. `BSProceduralLightningController`
  is rarer (storms, magic). Plus: the warnings train the eye to ignore
  ALL `consumed != block_size` warnings, which is bad because real
  drift bugs (e.g. SK-D5-NEW-07 below on `BSLODTriShape`) sit in the
  same channel.
- **Suggested Fix**: Two-part:
  1. Implement the trailing fields (BSLagBoneController is 12 bytes of
     well-documented controller params — straightforward 10-line parser).
  2. Until then, downgrade the by-design under-consumption to
     `log::debug!` for the explicit "we don't model yet" set at
     mod.rs:630-636 by emitting them through a typed-stub path that
     bypasses the realignment WARN (or annotate via a per-type
     `expected_under_consume: bool` flag).

### SK-D5-NEW-07: `BSLODTriShape` parser realignment fires on real Skyrim tree LODs — distant-LOD size triplet may be misread

- **Severity**: MEDIUM
- **Dimension**: Real-Data Validation & Rendering
- **Location**: `crates/nif/src/blocks/tri_shape.rs:797+` (`parse_lod`)
- **Status**: NEW
- **Description**: Unlike the by-design `BSLagBoneController` case in
  SK-D5-NEW-03, `BSLODTriShape` HAS a dedicated parser
  (`tri_shape::BsTriShape::parse_lod`, dispatch at `blocks/mod.rs:275`)
  that explicitly reads the trailing 3 × u32 LOD-size triplet
  (line 8239 of nif.xml-equivalent test at `tri_shape_skin_vertex_tests.rs:719`).
  But on Skyrim Meshes0/1 + the single-NIF dump of
  `dlc02\landscape\trees\treepineforestashl02.nif`, every parse fires
  the per-block `consumed != block_size` realignment for
  `BSLODTriShape`. Either the on-disk SE LOD layout has additional
  trailing fields the parser doesn't model, or the FO4-targeted
  `parse_lod` has a per-version offset gap on Skyrim.
- **Evidence**:
  - Single-tree dump: `BSLODTriShape=2` realigned (treepine NIF
    extracted via `dump_nif` — `nif_stats /tmp/audit/skyrim/treepine.nif`
    output above).
  - Sweep totals on Meshes0: 11 WARN events covering ~14 blocks.
  - Test at `tri_shape_skin_vertex_tests.rs:719` covers the FO4 layout
    with a synthesised 3-u32 trailer — but doesn't cover the SE on-disk
    layout (no real-Skyrim-NIF regression test).
- **Impact**: Distant-LOD0/1/2 size fields are realigned via
  `block_size` — values may be partially or wholly recovered depending
  on alignment, but trust in their values is unverified. Affects
  SpeedTree distant LOD switching (`BSTreeNode` + tree-LOD pyramid).
  Game-visual consequence: tree LODs may pop or pick the wrong tier at
  draw distance. Hard to pin without RenderDoc — file the bug, defer
  visual diagnosis.
- **Suggested Fix**: Pull a Skyrim BSLODTriShape via `trace_block` and
  diff its on-disk byte range against the synthesised test layout at
  `tri_shape_skin_vertex_tests.rs:732`. Likely either (a) extra padding
  pre-LOD trailer on SE or (b) a per-stream-version branch missing in
  `parse_lod`. Add a real-NIF regression once the layout is pinned.

### SK-D5-NEW-04: Aggregator warning text references closed issue #615 — log noise routes investigators to a sealed thread

- **Severity**: LOW
- **Dimension**: Real-Data Validation & Rendering
- **Location**: `crates/nif/src/lib.rs` — search for `tracked in #615`
- **Status**: NEW (depends on SK-D5-NEW-03 disposition)
- **Description**: The per-NIF aggregator emits the literal string
  "_per-block detail at debug level — under-/over-consume bugs tracked
  in #615_" on every NIF that hits the per-block `consumed != block_size`
  path. #615 is closed. Future maintainers chasing the warning
  will land on a closed issue with no actionable child links.
- **Evidence**: 67+ instances of the string in the Meshes0 parse run.
- **Impact**: Cosmetic / docs hygiene; routes investigators to a dead
  ticket.
- **Suggested Fix**: Replace the static `#615` reference with either
  the umbrella tag introduced for SK-D5-NEW-03, or remove the issue
  number altogether (the warning already carries the type name +
  `RUST_LOG=debug` instructions).

### SK-D5-NEW-05: Stream-realignment "warn" for `BSLODTriShape` is invisible to `nif_stats`'s parse-rate gate — silent per-block data loss masked by `clean == total`

- **Severity**: MEDIUM
- **Dimension**: Real-Data Validation & Rendering
- **Location**: `crates/nif/examples/nif_stats.rs::min_success_rate` +
  per-block consumption check in `crates/nif/src/lib.rs`
- **Status**: Existing pattern: previously documented as `SK-D5-06`
  (closed?) in 2026-04-22 audit; current Meshes1 run shows the issue
  re-applies once realignment count > 0
- **Description**: `nif_stats` reports `recovered: 0` (i.e. zero types
  with partial unknown) even when 56+ per-block stream-realignment
  events fire across 18862 NIFs. The gate only inspects `clean` vs
  `total` at the **NIF** level, not the per-**block** level. So
  `BSLODTriShape × 14` + `BSLagBoneController × 78` + `BSProceduralLightningController × 3`
  realignments collectively lose data on **95 blocks per pass**,
  but the gate stays green at `100.00%` for Meshes0.
  This is the same masking pattern the 2026-04-22 audit flagged as
  `SK-D5-06`. Verifying via `clean == total` is necessary but not
  sufficient.
- **Evidence**: Meshes0 run prints 100.00% clean / `truncated: 0` /
  `recovered: 0` while the run log carries 67+ WARN lines.
- **Impact**: Audit gate gives false confidence. Future regressions
  that turn dispatch-clean into per-block-truncated will not trip the
  gate.
- **Suggested Fix**: Pipe the per-block realignment counter into
  `Stats::recovered` (or a sibling `Stats::realigned` field) and
  bump the gate to fail when `realigned > 0` on a known-clean BSA.
  Roughly 10-line change in `nif_stats.rs`.

### SK-D5-NEW-06: `bhkRigidBody` ragdoll warning suppression confirmed restored — bear skeleton parses 19 bhkRigidBody clean

- **Severity**: LOW
- **Dimension**: Real-Data Validation & Rendering
- **Location**: `crates/nif/src/blocks/havok/rigid_body.rs` (positive evidence)
- **Status**: NEW (regression of 2026-04-22 SK-D5-01 / SK-D5-04 — closed and verified)
- **Description**: The 2026-04-22 audit's flagship `SK-D5-01` finding
  ("bhkRigidBody parser misaligned — 14,408 blocks demoted to NiUnknown
  across Skyrim BSAs") and the companion warning-spam SK-D5-04 are
  no longer reproducible: the bear skeleton dump
  (`meshes\actors\bear\character assets\skeleton.nif`) reports
  `bhkRigidBody=19, parsed=19, unknown=0`, and the full Meshes0 sweep
  reports `bhkRigidBody=12866, parsed=12866, unknown=0` with no
  `bhkRigidBody`-specific warnings in the log. Logging this as a
  **positive** finding (not a regression) so the 2026-05-03 baseline
  picks it up in the next pass.
- **Impact**: None — verification.
- **Suggested Fix**: None.

## Summary

- **7 findings**: 1 HIGH, 4 MEDIUM, 2 LOW
  - HIGH: SK-D5-NEW-01 (Meshes1 parse rate 99.81 % — parse-rate-gate failure)
  - MEDIUM: SK-D5-NEW-02 (BSTriShape data_size irrational warning spam),
    SK-D5-NEW-03 (BSLagBoneController/BSProceduralLightningController
    base-only dispatch + WARN noise), SK-D5-NEW-05 (per-block realignment
    invisible to gate), SK-D5-NEW-07 (BSLODTriShape SE realignment on
    real tree LODs)
  - LOW: SK-D5-NEW-04 (warning text references closed #615),
    SK-D5-NEW-06 (positive: bhkRigidBody no longer demoted)

- **Existing-issue confirmations** (not refiled):
  - #570 SK-D3-03 (`material_kind` truncated to u8 at
    `material/walker.rs:279`) — verified still present, untouched
  - #571 SK-D1-02 (BSDynamicTriShape data_size==0) — covered by the
    `data_size==0` skip in `tri_shape.rs:450-454`, paired with FO4
    LOD-chunk fix #711, separate skin-reconstruction path #559
  - #559 SK-D5-02 (NiSkinPartition global vertex buffer) — verified
    fixed via `try_reconstruct_sse_geometry` at `import/mesh.rs:718-722`
  - #611 SK-D5-02 root selector (`is_ni_node_subclass`) — verified
    fixed at `lib.rs:616-623`; tree LOD `BSTreeNode` root resolves
    correctly on real treepine NIF
  - #614 BSBoneLODExtraData parser — verified fixed (782b723)
  - #766 NIF-D5-NEW-01 — re-confirmed (basis for SK-D5-NEW-01 here)

- **Baseline reproduction**:
  - Meshes0: 100.00% (18862/18862) — matches stated baseline
  - Meshes1: 99.81% (3179/3185) — **fails** stated baseline; gate exits non-zero
  - Sweetroll FPS (~3000–5000): not measured this session; not invalidated
    but not verified — no display, and recent perf changes (#823, #824,
    #828, #832, #834, #835) have not been benchmarked yet

- **Not findings, but flagged**:
  - The aggregator's "_tracked in #615_" annotation ships with every
    `consumed != block_size` warning even after the issue closed, training
    investigators to chase a dead ticket. Captured as SK-D5-NEW-04.
  - The `nif_stats` parse-rate gate's per-NIF granularity hides per-block
    realignments — this audit cycle's exact masking pattern from the
    2026-04-22 SK-D5-06 audit. Captured as SK-D5-NEW-05.
