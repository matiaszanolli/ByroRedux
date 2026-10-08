---
description: "Audit compatibility gaps between Gamebryo 2.3 and Redux — what's mapped, what's missing"
---

# Legacy Compatibility Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol (layout, game-data + legacy-source locations, severity, NIFAL severity rows, dedup, report format).

## Purpose and scope

Compare Gamebryo 2.3 / Creation behaviour with Redux and surface **mapping gaps** — content the source engines render or simulate that Redux drops, mis-maps or diverges on — in the seams **no layer or per-game owner covers**. Each abstraction layer has its own deep audit; findings about a layer's internals go there, not here:

| Layer / seam | Owner | Spec |
|---|---|---|
| NIFAL (NIF → canonical ECS, material boundary) | `/audit-nifal` | `docs/engine/nifal.md` |
| EXAL / SKYAL / WATAL (exterior, sky, water translation) | `/audit-exterior` | `docs/engine/{exal,skyal,watal}.md` |
| PHYSAL (Havok → Rapier; constraint CInfo decode is the only per-game seam) | `/audit-physics` (+ `/audit-nif` for the decode) | `docs/engine/physal.md` |
| CHARAL (rulesets) | `/audit-character` | `docs/engine/charal.md` |
| Per-record / per-block decode divergence | `/audit-esm`, `/audit-nif` | — |

This skill keeps three dimensions: the coordinate/placement math (no other owner), the legacy-subsystem cross-check (the only audit that reads the 2.3 headers) and the cross-game translation-pattern spot-check. The former NIFAL, material-boundary, PHYSAL and EXAL dimensions were re-homed to the layer owners above: PHYSAL and the material boundary yielded only duplicates of `/audit-physics` / `/audit-nifal`; EXAL/WATAL yielded real boundary-completeness finds, which is exactly `/audit-exterior`'s charter.

Documented limitations are **not** findings (FO4+ packed Havok blobs, phantoms, `NiFogProperty`'s deliberate non-dispatch — no `Material` landing site, cell-scope `CellLighting` only — and the measured ~1.0 shared emissive scale, `nifal.md` §4). Cross-check the layer specs' leak inventories so closed leaks are not re-filed. If the Gamebryo 2.3 drive is unmounted, `/mnt/data/src/reference/gamebyro-v26` / `gamebyro-v32` and `docs/legacy/` carry the same class shapes.

## Dimensions

### 1. Coordinate + placement fidelity (Z-up → Y-up)
The most catastrophic mapping-bug class: one wrong transform mis-places or mirrors all content silently. Reference: `docs/engine/coordinate-system.md`.
Paths: `crates/core/src/math/coord.rs`, `crates/nif/src/import/coord.rs`, `crates/nif/src/rotation.rs`, `crates/nif/src/blocks/strip.rs`, `byroredux/src/cell_loader/euler.rs`, `crates/core/src/ecs/components/camera.rs`
First step: `git log --since=<last report> --format='%h %cs %s' -- crates/core/src/math crates/nif/src/import/coord.rs byroredux/src/cell_loader/euler.rs` and `grep -rn '(x, z, -y)\|4096.0' byroredux/src crates --include='*.rs'` for new duplicated swaps / cell literals.
Guards: `coord.rs` tests `euler_multi_axis_matches_openmw_objectpaging` + `euler_zyx_order_pinned_by_rx_then_rz`; `cell_loader/euler_zup_to_quat_yup_tests.rs`; `strip.rs::even_odd_winding_matches_opengl_convention`.
- **One implementation** — the `(x, z, -y)` swap and quaternion conversions live in `coord.rs` (`zup_to_yup_pos`, `zup_to_yup_quat_wxyz`, `euler_zup_to_quat_yup`, `normalize_quat`, `cell_grid_to_world_yup`); the NIF-typed flavour (`zup_point_to_yup`, `zup_matrix_to_yup_quat`) wraps them in `import/coord.rs`. Any new duplicated swap or matrix→quat path that bypasses them is a finding (the pre-#1044 five-site duplication kept the #333 unit-quaternion fix at only one).
- **REFR Euler** — Bethesda CW-positive, ZYX product `from_rotation_y(-rz) * from_rotation_z(ry) * from_rotation_x(-rx)` (the XYZ variant only matches Z-only REFRs and skews multi-axis architecture). REFR placement goes through the `--rotation-mode` diagnostic dispatcher `euler_zup_to_quat_yup_refr` (default mode 1); XCLL lighting calls the canonical helper directly. Flag any caller that hardcodes a mode or re-derives the formula. `--rotation-mode` reaches the library unclamped (`boot::cli::rotation_mode_arg`, #4126/#4764), so an out-of-range value falls back to the shipping formula — a CLI-side clamp is the regression.
- **Winding** — NIF CW → projection Y-flip (`Camera::projection_matrix`, Vulkan clip space) → CCW front face. Strip de-stitch has ONE implementation (`strip::destrip`, #2298): odd triangles swap the **last two** indices; a D3D-style first-two swap re-introduces inside-out geometry, and a hand-copied destrip is the regression.
- **Exterior grid** — `EXTERIOR_CELL_UNITS = 4096.0` + `cell_grid_to_world_yup` are the sole source (#1112 collapsed six divergent literals, one with a Z-flip sign bug); new literal 4096 *cell* math is a regression (distance literals are not). The one sanctioned re-declaration is `crates/bsa/src/uvd.rs` (the bsa crate cannot depend on core), re-exported as `UVD_CELL_UNITS` and const-asserted equal to the core constant in `byroredux/src/cell_loader/precombined.rs` (`27002ed99`, #4939). Also check frame composition, not just the literal: FO4 exterior `_oc.nif` precombine instances are already world-absolute, so bakes spawn at a `Vec3::ZERO` root — composing `cell_grid_to_world_yup` on top placed every exterior bake one cell out (#5228, `f3e1bba62`).
- **Transform model** — Gamebryo's `NiTransform` scale is **uniform** by format (`docs/legacy/api-deep-dive.md`), so Redux's `Transform.scale: f32` is not a collapse — a "non-uniform scale is dropped" finding is a false premise. Matrix3→Quat at import (Shepperd + SVD repair for degenerate rotations) and `local * parent` propagation are the fidelity surface. Non-finite input is neutralised at the boundary: `rotation.rs::sanitize_rotation` (identity) + the `read_ni_transform*` readers (translation 0, scale 1, #4549); animation rotation keys go through the one sanitizer `anim/keys.rs::normalized_rotation_sample` (#4396). Core `normalize_quat` is deliberately lenient — a new caller using it as a sanitizer is a finding.
- **Unit scale** — Starfield authors metric; `crates/plugin/src/esm/records/spatial_units.rs::normalize` lifts ESM placement/light/fog/weather/water/navmesh distances to 70 units/m once per plugin walk (FO76 is BU-native; its only lift is the shared-decoder WATR per-metre absorption lane, #5169), and NIF import does the matching mesh/bone conversion. A second game-dependent distance multiplier downstream (lighting, skinning, camera) is the regression; per-field coverage is `/audit-esm` Dim 5.

### 2. Legacy subsystem coverage
What the 2.3 engine drives that Redux maps incompletely or not at all: cross-check `SDK/Win32/Include/`, `CoreLibs/Ni*/` (see `_audit-common.md` Legacy Source; `/legacy-lookup`) against the Redux side. Audit **fidelity**, not existence — the import-critical set exists.
Paths: `docs/legacy/api-deep-dive.md` (mapping table), `crates/core/src/ecs/components/`, `crates/core/src/animation/`, `byroredux/src/anim_convert.rs`, `crates/nif/src/import/{material,walk}/`
First step: read the mapping table against `crates/core/src/ecs/components/`; `gh issue list --search 'SUBSYS' --state open` for what is already tracked.
- **Property → pipeline** — for each `NiProperty` type: does its authored effect land in dynamic pipeline state (cull, blend, stencil two-sided) or a `Material`/per-object component? Flag any property whose authored effect is parsed and dropped.
- **Animation model** — `NiTimeController` → `NiInterpolator` → keys converge at import (`nifal.md` Animation; B-spline / Euler / TBC → quaternion keys; KF + embedded through `convert_nif_clip`; text keys → `AnimationTextKeyEvents`). Only per-light **ambient** colour channels (`ColorTarget::LightAmbient`, no light-buffer slot, #983) are parked; morph-weight channels reach `AnimatedMorphWeights` and deform on the GPU in `skin_vertices.comp` (#3231/#4399). The timing envelope (cycle type, frequency, phase) and the `NiControllerManager` / KFM sequence state machine are the known-fragile spots (#4128 open).
- **Scene graph** — each `NiAVObject` field → Redux component (Parent, Children, GlobalTransform, WorldBound, Name, flags) with fidelity, not existence.
- **String interning** — `NiFixedString` + `GlobalStringTable` ↔ `StringPool` + `FixedString`: semantic equivalence; flag any gap that breaks bone-name → entity resolution (load-bearing for skinning and PHYSAL bone binding).
- Typical yield is the shape "authored input parsed, no canonical sink" — weapon reach/speed, `XLOC` lock state, the controller timing envelope and Oblivion's parallax mode were each MEDIUM in past reports and are since fixed (#3096–#3098). Confirm against `/audit-gameplay` scope before filing gameplay-facing gaps.

### 3. Cross-game translation-pattern spot-check
Reference: `docs/engine/per-game-translation-survey.md` §4 (per-layer inventory), §5 (Patterns A/B/C), §7 (why Fallout is the stress case). **Verify a survey claim against code before citing it** — the doc last changed 2026-10-06 (a comment-drift fix) and has carried stale examples before.
Paths: `crates/nif/src/version.rs`, `crates/plugin/src/esm/reader.rs` (`GameKind`), `crates/plugin/src/esm/records/`
First step: `grep -rnE 'bsver(\(\))? *[<>=]+ *[0-9]{2,3}\b' crates/nif/src --include='*.rs' | grep -v test` then drop `//` comment lines (`grep -vE '^[^:]+:[0-9]+: *//'`) — literal thresholds; at 2026-10-08 the only non-comment hits are three test asserts in `blocks/base.rs` and `grep -rn 'game == \|GameKind::' byroredux/src/cell_loader byroredux/src/scene --include='*.rs'`.
- The renderer/shader side is clean; per-game branches live **upstream** (parser, importer, cell loader). Look there.
- **Pattern A** — hardcoded BSVER threshold literals that should be a named `version::bsver::*` constant. This is **not** a bypassed `NifVariant` helper: those feature-flag predicates were removed deliberately (#938/#1511/#1840/#1897) and `version.rs` records the raw-bsver doctrine as fully enforced (search `raw-bsver doctrine`). Do not flag a raw `bsver()` comparison as "should call a helper".
- **Pattern B** — feature flag on an enum where the wire format already discriminates the game (the correct shape; flag mis-dispatch, not "needs a trait").
- **Pattern C** — divergent decode of one wire structure across games/eras (same record, different offsets or sizes): FO4 `WEAP` with no `DATA` and WATR simulator blocks decoded one field off were the HIGH finds. Depth belongs to `/audit-esm` / `/audit-nif`; this dimension checks the *cross-game* shape (a new per-game size/offset split with no test per era).
- Scattered new `if game == …` in the cell loader / exterior assembly is a finding (one `GameKind`-keyed decision or a table-shaped `match` returning data is fine).

## Process

1. Read the live spec for the seam under audit; its leak inventory is the baseline — do not re-file what it records as closed or a limitation.
2. Trace each claimed boundary to its callers (grep the symbol) and confirm single-producer.
3. Cross-reference Redux components / translate sites against the legacy headers and the survey.
4. Classify per `_audit-severity.md` (a wrong/divergent canonical out of a `translate()` = HIGH; a translatable input silently dropped = MEDIUM, escalate if it removes visible content). Disprove every finding before keeping it.
5. Finalize per `_audit-common.md` "Report Finalization" (save to `docs/audits/AUDIT_LEGACY_COMPAT_<TODAY>.md`; do not open issues directly). Suggest `/audit-publish` (label `legacy-compat` + the domain label).
