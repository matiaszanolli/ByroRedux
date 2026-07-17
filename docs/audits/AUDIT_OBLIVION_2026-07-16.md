# Oblivion (TES4) Compatibility Audit — 2026-07-16

Repo: `matiaszanolli/ByroRedux` · Branch: `main` @ `c3e09bb5` · Auditor: 7-dimension parallel sweep (audit-oblivion skill)

## Executive Summary

Oblivion is in **excellent, stable shape**. Every one of the 7 audit dimensions
came back clean of CRITICAL/HIGH/MEDIUM findings; the only output is **3 NEW
LOW-severity** doc-rot / dead-field findings, none of which affect runtime
correctness. This audit functioned primarily as a **regression-guard sweep**
across a densely-fixed area (the v10.x NetImmerse stride-drift family,
BSA v103 extraction, the 16-byte ACBS guard, NIFAL material translation) —
every previously-closed issue re-checked against live code still holds.

Current compatibility level (live-verified this session, not carried over
from ROADMAP prose):

| Aspect | State |
|---|---|
| NIF parse rate | **99.93%** (8026/8032 clean) over `Oblivion - Meshes.bsa`, **100% recoverable**, 0 hard failures — matches ROADMAP exactly, re-run live this session |
| Residual truncations | Exactly the expected **6** NetImmerse-era marker files (`marker_arrow`/`divine`/`map`/`radius`/`temple`/`travel`), zero new drift, zero `NiUnknown` growth (81/81 block types match the checked-in baseline) |
| BSA v103 archive | **100% extraction** — live sweep of the two largest vanilla archives (38,222 files, 0 errors) reconfirms the #699 baseline |
| ESM parser (live path) | All Oblivion-specific decode branches (16-byte ACBS #1650, MGEF-by-code, CONT 4-byte guard, CLMT 3-entry WLST, 24-byte CTDA, XCLL sizing) correct and test-covered; 2 real-data parity tests pass against vanilla `Oblivion.esm` |
| Rendering / material path | `NiTexturingProperty`/`NiMaterialProperty` pipeline fully wired end-to-end (base/dark/detail/glow/gloss, normal-from-bump, all 11 AlphaFunction values, particle emitter chain reaches the ECS runtime); Disney BSDF path confirmed unreachable for all-legacy Oblivion content |
| NIFAL canonical translation | Metalness/roughness resolve exactly once (NaN-sentinel pattern), `EmissiveSource::Material` tagging correct, `MAT_FLAG_PBR_BSDF` provably stays 0 |
| Cell loading | Interior renders end-to-end (Anvil Heinrich Oaken Halls, prior sessions). Exterior: **parse + load are done and game-agnostic**; the only remaining step is an **on-device render bench** — no code gap found |

**Top blockers, in priority order:**

1. **On-device Oblivion exterior render bench** (not a code gap — verification
   work). Every code-level prerequisite (worldspace/LAND wiring, CELL REFR
   placement, distant LOD, CLI/asset plumbing) is implemented and tested; no
   bench-of-record, smoke script, or HISTORY.md entry exists confirming an
   actual exterior grid load has been run for Oblivion. Risk surface that's
   unverified: Tamriel-scale REFR counts, LOD ring behavior against real
   neighbor-cell data, BLAS/TLAS build cost for a multi-cell exterior — none
   visible from static review.
2. Three LOW-severity doc-accuracy / dead-field items (below) — no functional
   impact, but two carry a nonzero risk of misleading a future contributor
   into reintroducing a closed bug class (#349/#396-style XESP mishandling).

No CRITICAL, HIGH, or MEDIUM findings were produced by any dimension.

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.5 + v10.x NetImmerse tail)
**0 findings.** All 11 checklist items re-verified and hold (see Regression
Guard List). Additionally, 4 plausible-looking non-findings were actively
investigated and disproved (documented in the dimension report so they aren't
re-derived): `read_short_string`'s missing alloc-guard (not exploitable, u8
length prefix), an apparent `group_id` double-read (mutually exclusive by
version band), `NiQuatTransform` TRS-Valid `bool[3]` stride (version-aware
`read_bool` is correct), and `NiBlendInterpolator`'s item-array skip under
`manager_controlled` (matches nif.xml's `!(Flags & 1)` gate).

### Dimension 2 — BSA v103 Archive
**0 findings.** Regression-guard dimension confirmed clean: version
recognition, 16-byte folder-record sizing (the "v104 = 24B" claim was a
doc-comment-only typo, already fixed under #1545 — the code was never wrong),
Xbox-archive-bit handling, and hash functions all hold. Live sweep: 38,222
files (Meshes.bsa + Textures-Compressed.bsa) extracted with 0 errors,
consistent with the 147,629/147,629 all-archive baseline from #699.
`cargo test -p byroredux-bsa`: 53/53 passed.

### Dimension 3 — ESM Record Coverage (live path)
**2 NEW LOW findings.**

#### DIM3-OBL-01: XESP doc comments mislabel the sub-record "(Skyrim+)" — it is Oblivion-era
- **Severity**: LOW
- **Dimension**: ESM Record Coverage
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:855-856`, `crates/plugin/src/esm/cell/mod.rs:517`
- **Status**: NEW
- **Description**: Both doc comments label `XESP` (REFR/ACHR/ACRE enable-parent
  gating) as a Skyrim+ feature, contradicting the parser's own comment three
  lines above the match arm and a sibling test's docstring, both of which
  correctly state XESP is present on Oblivion (`walkers.rs:788-791`;
  `crates/plugin/src/esm/cell/tests/refr.rs:583-588`).
- **Evidence**: The `b"XESP"` match arm (`walkers.rs:861`) has no `GameKind`
  guard — it already parses unconditionally, so this is a pure doc-label bug,
  not a live parsing defect.
- **Impact**: None today. Risk is a future contributor reading the "(Skyrim+)"
  label and adding an incorrect `if game != GameKind::Oblivion` guard, which
  would reintroduce the #349/#396-class bug (Ayleid ruin / Oblivion gate /
  dungeon creature placements silently skipped).
- **Related**: #349, #396, #471 (all closed, none regressed)
- **Suggested Fix**: Drop the "(Skyrim+)" qualifier from both comments, or
  replace with "present since Oblivion".

#### DIM3-OBL-02: `NpcRecord`/`ClasRecord.flags_oblivion` parsed and real-data-verified but has no downstream consumer
- **Severity**: LOW
- **Dimension**: ESM Record Coverage
- **Location**: `crates/plugin/src/esm/records/actor.rs:467` (field), `:1078` (populated in `parse_clas`)
- **Status**: NEW
- **Description**: `flags_oblivion: Option<u32>` decodes correctly from
  Oblivion's 60-byte CLAS `DATA` and is verified against real `Oblivion.esm`
  (`clas_oblivion_knight_against_vanilla`), but repo-wide grep shows no
  production consumer (e.g. no leveling/spellmaking-eligibility gate).
- **Impact**: None today — reads as intentional sequencing ahead of CHARAL
  (the per-game character-rules abstraction layer, `docs/engine/charal.md`,
  PROPOSED) reaching its Oblivion class-flag pass, not a bug.
- **Related**: CHARAL (`docs/engine/charal.md`)
- **Suggested Fix**: No action needed now; flag for CHARAL's Oblivion pass so
  it isn't rediscovered as a "surprise" gap later.

All other checklist items (TES4 header/GRUP dispatch, MGEF-by-code map, CONT
4-byte guard, CLMT 3-entry WLST, the 16-byte ACBS guard #1650 with correct
Oblivion-before-FNV arm ordering, CELL walker XCLL/RCLR handling, DIAL/INFO
cross-game-safe layout, the two previously-ignored real-data parity tests)
verified correct — see Regression Guard List. Full targeted + real-data test
runs: `cargo test -p byroredux-plugin --lib -- --ignored` → 13 passed;
`cargo test -p byroredux-plugin --test parse_real_esm -- --ignored` → 8 passed
(including `clas_oblivion_knight_against_vanilla` and
`race_oblivion_data_and_subs_against_vanilla` against vanilla `Oblivion.esm`).

### Dimension 4 — Rendering Path for Oblivion Shaders
**0 findings.** All 8 checklist items traced end-to-end and hold: the
`NiTexturingProperty` pipeline (base/dark/detail/glow/gloss, normal-from-bump
#131), all 11 `AlphaFunction` values routed to distinct `vk::BlendFactor`s,
trivial properties honored (or intentionally ignored, e.g. `NiDitherProperty`
— no Vulkan analogue), vertex-color × material-color SourceMode semantics
correct, the #1239 emitter version gate intact, and the typed particle-emitter
chain (`NiPSysEmitter*` → `extract_emitter_params`/`extract_emitter_rate` →
`apply_emitter_params`) reaches the ECS runtime from both spawn paths — not
parse-then-drop. One documented-accepted non-finding: the detail-map sampler
uses a hardcoded 2× tiling instead of the detail slot's own UV transform — an
explicit shared-UV-set approximation, not a defect.

### Dimension 5 — NIFAL Canonical Material Translation for Oblivion
**0 findings.** All 3 checklist invariants verified against the single
`translate_material` boundary: metalness/roughness resolve exactly once via
the `f32::NAN` sentinel pattern (`Material::resolve_pbr`) with no downstream
`classify_pbr` re-appearance; `emissive_source` correctly tagged
`EmissiveSource::Material` via the `NiMaterialProperty` arm (distinct from the
Skyrim `Lighting` / FO4 `Effect` arms); `MAT_FLAG_PBR_BSDF` provably stays 0
for all Oblivion content since `is_pbr` is only ever set `true` on the
BGSM/Starfield `.mat` merge path, which Oblivion never touches.

### Dimension 6 — Real-Data Validation
**0 findings.** Live sweep this session exactly matches the checked-in
baseline on every axis: file-level parse rate (8032/8032, 99.93% clean, 100%
recoverable, 0 failed), the 81-type per-block histogram (byte-identical to
`crates/nif/tests/data/per_block_baselines/oblivion.tsv`), the residual
6-file truncation set (exactly the expected NetImmerse markers, no new
drift), and a 3-mesh interior import trace (book, chandelier, character
head) — all parse cleanly with non-zero mesh/vertex/triangle counts and
resolved texture references.

### Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks
**1 NEW LOW finding.**

#### OBL-D7-01: `legacy_particle.rs` module doc overclaims Oblivion dependency the real corpus contradicts
- **Severity**: LOW
- **Dimension**: Exterior Blocker Chain & Game-Specific Quirks
- **Location**: `crates/nif/src/blocks/legacy_particle.rs:1-17`
- **Status**: NEW
- **Description**: The module doc asserts Oblivion "still serializes" the
  pre-10.1 legacy particle stack (`NiParticleSystemController`,
  `NiAutoNormalParticles`, `NiRotatingParticles`, etc.), directly contradicted
  by `crates/nif/src/import/walk/mod.rs:502-505`'s comment ("the target games
  all author the modern NiParticleSystem stack") and by real corpus data: the
  checked-in per-block-type baselines for **all 7 supported games**,
  including Oblivion's own 8032-NIF sweep, show **zero** occurrences of any
  legacy-stack block type. Oblivion's baseline shows `NiParticleSystem 547 0`
  — 547 correctly-typed modern particle systems, the type that *is* routed to
  the renderer.
- **Evidence**: `crates/nif/tests/data/per_block_baselines/oblivion.tsv` (and
  the 6 sibling per-game TSVs) contain no `AutoNormal`/`Rotating`/
  `ParticleSystemController`/`BSPArray` rows. `git show 23ab46f2` (#1327)
  confirms the legacy-emitter surfacing arm was dead code (never reachable)
  even before its removal.
- **Impact**: Low — doc-rot only. The parser itself may be legitimate
  nif.xml-completeness / defensive coverage (mod content, non-`Meshes.bsa`
  archives, or other NetImmerse-era titles), but as written the doc would
  send a future auditor chasing a "dropped Oblivion particle FX" finding that
  the real data refutes — exactly the stale-premise pattern flagged by
  `feedback_audit_findings.md`.
- **Related**: #1327 (dead-arm removal this doc should have been updated
  alongside)
- **Suggested Fix**: Soften the module doc to state the format support is
  nif.xml-driven/defensive rather than asserting vanilla Oblivion content
  requires it, citing the per-block baseline evidence; or if a non-`Meshes.bsa`
  Oblivion source is later found to ship these blocks, cite that source
  instead.

Also confirmed in this dimension (not findings): the TES4 worldspace + LAND
wiring is real and game-agnostic (`exterior.rs`), the `--bsa`/`--esm`/`--grid`
CLI path is fully generic (no game-branching), no Oblivion-specific record
type is missing from the cell loader's REFR-placement surface, animation/
scene-graph name resolution has no exterior-specific gap, the pre-v3.3.0.13
fallback already logs at `debug!` (not `warn!` — no sweep spam risk), and
`_far.nif` distant-object LOD (#1726/#1745) is implemented, tested, and wired
into both the streaming tick and the bulk `--grid` loader.

## Blocker Chain

Sequential list to reach "Oblivion exterior cell renders on-device" (interiors
already render end-to-end — Anvil Heinrich Oaken Halls):

1. **TES4 worldspace + LAND wiring** — ✅ DONE. `exterior.rs`/`terrain.rs`/
   `water.rs`/`references/*.rs` are generic and already exercise Oblivion's
   worldspace, LAND heightmap, and default-water-height paths.
2. **CELL exterior REFR placement** — ✅ DONE. Shares the same
   `load_references` entry point interiors use; Oblivion-specific record
   shapes (16-byte ACBS, XCLL sizes, RCLR) are gated and tested.
3. **Distant LOD (terrain + object)** — ✅ DONE. `placement_lod.rs`
   (`_far.nif`) + `terrain_lod.rs` are implemented, real-data-verified, and
   wired into both the streaming tick and the bulk `--grid` loader.
4. **CLI / asset plumbing** — ✅ DONE. `--game oblivion` and
   `--esm`/`--bsa`/`--textures-bsa`/`--grid`/`--wrld` all route through the
   generic (non-game-branched) BSA/scene-loading path.
5. **On-device exterior render bench** — ❌ NOT DONE. No commit, smoke
   script, or bench-of-record entry exists for an Oblivion exterior grid
   load. This is a *verification* step against an already-wired pipeline, not
   a known-missing feature. Unverified risk surface: Tamriel-scale REFR
   counts for a `--grid` bulk load, LOD ring behavior against real
   neighboring-cell data, BLAS/TLAS build cost for a multi-cell exterior —
   none of which have failure modes visible from static code review.

Do NOT regenerate the "BSA v103 is broken" framing — confirmed fully dead
(#699, closed, re-verified generically via both Dimension 2's direct sweep
and Dimension 7's CLI-plumbing check).

## Regression Guard List

Previously-fixed items re-verified this session as still holding, with no
regressions found:

- **v10.x stride-drift family** (#1506 NiInterpController/NiQuatTransform,
  #1507 NiPSysData+emitter, #1508 NiBlendInterpolator+ControlledBlock, #1509
  NiGeomMorpherController `bsver > 9` gate) — all 4 confirmed intact,
  `crates/nif/src/blocks/controller/{mod,morph}.rs`, `interpolator.rs`,
  `particle.rs`.
- **`NiTexturingProperty` u32 shader-map count, no leading bool gate** —
  `crates/nif/src/blocks/properties.rs:336-337`.
- **BSStreamHeader dual-band guard (#170)** — `crates/nif/src/header.rs:137-143`,
  regression test still asserts a non-Bethesda out-of-band file skips the
  header read.
- **`user_version` threshold `V10_0_1_8`** — `header.rs:114-118`.
- **u16/u32 NiAVObject flag width via raw `bsver` (#1331)** — `blocks/base.rs:82-86`.
- **BhkMultiSphereShape / BhkConvexListShape collision translation** —
  `crates/nif/src/import/collision/shape.rs:110-136,228-243`, neither falls
  through to the unsupported-log drop.
- **Havok canonical motion-type enum (#1652)** — `import/collision/mod.rs:156-164`,
  the pre-fix `4 => Keyframed`/`_ => Static` collapse has not regressed.
- **BSA v103 extraction (#699)** — 38,222-file live sweep, 0 errors, matches
  the 147,629/147,629 all-archive baseline.
- **v103/v104 16-byte folder-record size, "v104=24B" doc typo (#1545)** —
  `crates/bsa/src/archive/open.rs:100`; code was never wrong.
- **v103 Xbox-archive-bit handling (#700)** — `open.rs:67-75`.
- **16-byte Oblivion ACBS gating, Oblivion-before-FNV arm order (#1650)** —
  `crates/plugin/src/esm/records/actor.rs:638,649`, both `NPC_` and `CREA`
  covered via the shared `parse_npc` call.
- **NiWireframeProperty → LINE pipeline / NiShadeProperty.flat_shading (#869)** —
  `crates/nif/src/import/material/walker.rs:1027-1029,1036-1038`.
- **Disney-BSDF gate stays 0 for Oblivion (#1248-#1252)** — confirmed
  independently by both Dimension 4 (`cell_loader.rs`) and Dimension 5
  (`material_translate.rs`); `is_pbr` is only ever `true` on the
  BGSM/Starfield `.mat` merge path, which Oblivion content never reaches.
- **NiMaterialProperty raw monitor-space colors, no sRGB conversion
  (commit `0e8efc6`)** — `walker.rs:639-645`.
- **`_far.nif` distant-object LOD (#1726/#1745)** — `placement_lod.rs`,
  wired into both load paths, `object_lod.rs` correctly no-ops for
  `GameKind::Oblivion` to prevent double-loading.

## Finding Count Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 3 (DIM3-OBL-01, DIM3-OBL-02, OBL-D7-01) |
| **Total** | **3** |

Suggested next step:
```
/audit-publish docs/audits/AUDIT_OBLIVION_2026-07-16.md
```
