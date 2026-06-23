---
description: "Audit compatibility gaps between Gamebryo 2.3 and Redux — what's mapped, what's missing"
---

# Legacy Compatibility Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol (project layout,
game-data + legacy-source locations, severity scale, NIFAL severity rows, dedup, report format).

## Purpose

Compare per-game Gamebryo 2.3 / Creation-engine behaviour against Redux's current
implementation and surface **translation/mapping gaps** — content the source engines
render or simulate that Redux drops, mis-maps, or diverges on.

This is *not* a "does a parser exist" bootstrap audit (it does). The framing is the three
**canonical-translation abstraction layers** — every per-game quirk must be folded in at
**one** explicit boundary, with **no** per-game branch, `Option` resolve-later leak, or
render-time heuristic downstream:

- **NIFAL** — NIF geometry/material/skin/lights/nodes/particles/collision → ECS (`docs/engine/nifal.md`)
- **EXAL** — ESM exterior environment (terrain/sky/sun/weather/water/LOD) → renderer (`docs/engine/exal.md`)
- **PHYSAL** — per-game Havok articulation → one solver-agnostic physics spec (`docs/engine/physal.md`)

A gap is either (a) a leak *inside* a layer (a translatable input silently dropped / a
second producer site / a downstream per-game branch), or (b) a subsystem the layers do not
yet cover. Both are findings; documented limitations (FO4+ packed Havok, phantoms) are
**not**. The per-layer specs maintain leak inventories — cross-check against them so closed
leaks are not re-filed.

Dimensions are ordered by compat-gap blast radius. NIFAL/EXAL/PHYSAL each have a deep
sibling — **`/audit-nifal`** (and the per-game audits) drill the contents; this audit owns
the *cross-layer mapping shape* and subsystem coverage.

## Dimensions

### 1. Coordinate-system correctness (Z-up → Y-up)

The most catastrophic mapping bug class: a wrong transform mis-places or mirrors **all**
content silently. Reference: `docs/engine/coordinate-system.md` (verified against the tree).

- **Single source of truth**: the `(x, z, -y)` axis swap + quaternion conversions live in
  `crates/core/src/math/coord.rs` (`zup_to_yup_pos`, `zup_to_yup_quat_wxyz`,
  `euler_zup_to_quat_yup`, `normalize_quat`, `cell_grid_to_world_yup`). The NIF-typed flavour
  (`zup_point_to_yup`, `zup_matrix_to_yup_quat`) wraps them in `crates/nif/src/import/coord.rs`.
  **Regression guard**: any new duplicated `(x, z, -y)` swap or matrix→quat path that
  bypasses these (the pre-`#1044` five-site duplication, where only one carried the `#333`
  unit-quaternion fix) is a finding.
- **REFR Euler convention**: Bethesda CW-positive, **ZYX** product
  (`Quat::from_rotation_y(-rz) * from_rotation_z(ry) * from_rotation_x(-rx)`). The pre-`#386aabb4`
  XYZ-product variant only matched on Z-only REFRs and skewed multi-axis architecture. Pins:
  `euler_multi_axis_matches_openmw_objectpaging` / `euler_zyx_order_pinned_by_rx_then_rz`.
  REFR placement routes through the `--rotation-mode` A/B dispatcher
  (`byroredux/src/cell_loader/euler.rs::euler_zup_to_quat_yup_refr`, default mode 1); XCLL
  lighting calls the canonical helper directly. Flag any caller that hardcodes a mode or
  re-derives the formula.
- **Winding**: NIF CW → projection Y-flip (`camera.rs::projection_matrix`) → CCW front face.
  Strip de-stitch (`tri_shape/ni_tri_shape.rs::to_triangles`) swaps the **last two** verts on
  odd triangles (CCW). A D3D-style (first-two) swap re-introduces inside-out geometry.
- **Exterior grid**: `EXTERIOR_CELL_UNITS = 4096.0` + `cell_grid_to_world_yup` are the sole
  source (`#1112` collapsed six divergent literals, one with a Z-flip sign bug). New literal
  `4096.0` cell math is a regression.

### 2. NIFAL — canonical NIF→ECS translation contract

The Gamebryo→Redux semantic mapping for everything inside a NIF. Audits the *shape* of the
three-tier model (raw `Imported*` → `translate()` → canonical ECS); **`/audit-nifal`** owns
the per-slice contents. Reference: `docs/engine/nifal.md` (§1 tiers, §2 per-category leak
inventory, §3 material reference realisation).

- Each per-category slice (material / geometry / skinning / lights / nodes / particles /
  collision / animation / shader-flags) has **exactly ONE `translate()` boundary** — no
  second site re-derives the canonical form.
- No `Option` "resolve-later" leak downstream of the boundary (the raw tier may be messy;
  the canonical tier is the source of truth).
- No per-game `if game == …` branch downstream of the boundary. The renderer/shader side is
  verified clean (`per-game-translation-survey.md` §1, "zero `if (game == …)`"); leaks live
  **upstream** in parser/importer/cell-loader — that is where to look.
- **Cross-check newly-closed leaks against `nifal.md` §2** so audits stop re-filing them
  (converged: materials, geometry, skinning, lights, animation, shader-flags; triaged/parked:
  the four `ImportedNode` fields + the passthrough table — those are *bounded known gaps*,
  not findings).

### 3. Material translation boundary (NIFAL reference slice)

The reference realisation and the highest-blast-radius single boundary (a wrong `Material`
is silently wrong for **every** game — `Material::metalness`/`roughness` are plain resolved
`f32`, no per-draw fallback to mask it; hence the HIGH severity floor in `_audit-severity.md`).

- `byroredux/src/material_translate.rs::translate_material` is the **sole** populated-`Material`
  producer — both spawn paths delegate (`byroredux/src/cell_loader/spawn.rs` +
  `byroredux/src/scene/nif_loader.rs`, both calling `material_translate::translate_material`).
  No other site constructs a populated `Material`.
- `metalness`/`roughness` are resolved `f32`, filled from a NaN sentinel by
  `crates/core/src/ecs/components/material.rs::resolve_pbr` (via `classify_pbr_keyword`, then
  clamped to `[0,1]` / `[0.04,1]`). The deleted `Option`-override + render-time `classify_pbr`
  path must NOT reappear (it survives only in the explanatory comments in
  `byroredux/src/render/static_meshes.rs`).
- Glass / cloth / metal classified **once** at the boundary, alpha-aware
  (`material_translate::classify_glass_into_material`, after `resolve_pbr` so forced glass
  roughness wins) — never re-classified per draw.
- **Emissive (regression guard, do not re-file)**: the three `EmissiveSource` variants
  (`Material` / `Lighting` / `Effect`, `material.rs::EmissiveSource`) were measured across
  Oblivion/FNV/Skyrim/FO4 to share a ~1.0 scale (`nifal.md` §4). No normalization is correct;
  an "unify emissive scale" finding has a false premise.
- **`NiFogProperty` (regression guard)**: parsed but **intentionally not dispatched**
  (`#1224` / D4-NEW-02; see the deliberate-skip comment near the end of
  `crates/nif/src/import/material/walker.rs::extract_material_info`). Per-node fog has no
  `Material` landing site; the renderer's fog path reads cell-scope `CellLighting` only.
  Observed corpus is 1 vanilla FO3 block. Do **not** re-file.

### 4. PHYSAL — per-game Havok articulation → solver

Ragdoll/physics translation; **double-ended** (per-game source axis + per-solver sink axis).
The classic-chain slice landed 2026-06-14. Reference: `docs/engine/physal.md` (§1 double-ended,
§2 tiers, §3 ragdoll reference realisation, §5 per-concern inventory).

- **The per-game seam is ONLY the constraint CInfo decode** — everything else is game-agnostic
  by construction. Audit that the seam stays that narrow: the two typed decoders
  (`crates/nif/src/blocks/collision/constraints.rs::RagdollCInfo` / `LimitedHingeCInfo`, with
  `parse_oblivion` / `parse_fo3` arms) read only the **common subset** (era-only fields like
  FO3+ motors / `Perp Axis In B1` are decoded-or-zeroed, never reaching canonical). Byte
  advancement is asserted per era in `crates/nif/src/blocks/collision/bhk_constraint_tests.rs`.
- **Extract is already game-agnostic**: `crates/nif/src/import/collision.rs::extract_ragdoll`
  switches on `BhkConstraintData`, never on game; emits `ImportedRagdoll` in Y-up,
  `havok_scale`-applied units. A `game ==` branch creeping in here is a finding.
- **One translate, one build**: `byroredux/src/ragdoll.rs::template_from_imported` (bone-name
  → `EntityId` resolution into `RagdollTemplate`) + `activate_ragdoll` (world-space
  `RagdollSpec` seed) are the single translate; `crates/physics/src/ragdoll.rs::build_ragdoll`
  is the single solver boundary (Rapier types appear nowhere else). `RagdollSpec` /
  `RagdollJointSpec` carry no Rapier in their signatures.
- **Writeback rides existing skinning**: `byroredux/src/ragdoll.rs::ragdoll_writeback_system`
  (Stage::Late) copies stepped poses onto bone `GlobalTransform`s — no renderer change. A
  writeback that touches the render path is out of contract.
- **Converged games**: Oblivion (NIF ≤ 20.0.0.5) / FO3 / FNV / Skyrim LE/SE all funnel
  through one `ImportedRagdoll` → one `RagdollSpec` → one Rapier multibody. Skyrim's
  `havok_scale` ×69.99 is applied via `havok_scale_for(header)`; its version gate is
  test-pinned but real-data validation is pending — flag any *new* per-game ragdoll branch.
- **Documented limitations, NOT findings**: FO4 / FO76 / Starfield ragdolls are blocked on the
  `BhkNPCollisionObject → BhkSystemBinary` blob decoder (multi-day RE project); the Havok
  cone+2-plane → Rapier per-axis limit mapping is a known approximation; motors are captured
  but unused (`physal.md` §3 "Known approximation", §5). Do not re-file these.

### 5. EXAL — per-game exterior environment → renderer

Outdoors translation (terrain/sky/sun/weather/water/LOD). The boundary skeleton + WTHR/sky/
sun/weather/water slices landed; the LOD slice has a first cut + deferred follow-ups.
Reference: `docs/engine/exal.md` (§2 per-category leak inventory, §3 boundary, §4 GameVariant
table, §5 LOD, §7 rollout status).

- **Single boundary**: `byroredux/src/env_translate.rs` is the sole exterior-translate site
  (`default_water_for_worldspace`, `resolve_water_material`, `translate_sky`,
  `translate_weather`, `translate_exterior_cell_lighting`, and the `procedural_fallback_*`
  canonical constructors that replaced the old hardcoded-Mojave render-setup block). Both the
  bulk `--grid` loader and the streaming bootstrap call these — a second `SkyParamsRes` /
  `WeatherDataRes` / `WaterMaterial` construction site is a finding.
- **No render-time fallback**: the "no climate/weather" case is an explicit canonical default
  (`procedural_fallback_*`), not a branch in the render loop. A reintroduced inline hardcoded
  sky/lighting block is a leak.
- **GameVariant table (§4)**: per-game exterior quirks route through one `GameKind`-keyed
  decision (`crates/plugin/src/esm/reader.rs::GameKind`). The water-default decision in
  `env_translate::default_water_for_worldspace` is the prototype/only current exterior branch;
  scattered new `if game == …` exterior logic is a finding. (DALC/XCLL-tail/XCWT quirks live
  correctly in the parser tier or as canonical `Option` sentinels — not leaks.)
- **Canonical `Option` sentinels are not leaks**: `WeatherDataRes::skyrim_dalc_per_tod` =
  `None` on FNV/FO3/Oblivion, `default_water_height` = `None`, encode real game distinctions.
- **LOD — the largest open gap**: distant **object** LOD has a first cut (Skyrim/FO4 baked
  `.bto` quads via `byroredux/src/cell_loader/object_lod.rs::stream_object_lod_blocks`,
  spawned as `IsLodTerrain`, live-verified on Tamriel). Distant **terrain** now prefers the
  prebaked `.btr` mesh on Skyrim+/FO4 (M35, #1685 — `byroredux/src/cell_loader/terrain_lod_btr.rs::spawn_btr_block`,
  dispatched from `cell_loader/terrain_lod.rs`), falling back to heightmap synthesis for older
  games and missing `.btr` blocks. The Oblivion/FO3/FNV placement scheme (`DistantLOD\*.lod` →
  `_far.nif`) is still **unimplemented**.
  Per `exal.md` §5.4: runtime LOD is asset-driven — neither NIF LOD nodes nor STAT `MNAM`
  unblock it; the **VWD / "Has Distant LOD" record-header flag** (to cull full models) is the
  small parser gap. Findings here are real coverage gaps, not premise errors.
- **Sun model (regression guard)**: the canonical sun inputs are `tod_hours` +
  `weather::SUN_SOUTH_TILT` (engine-defined; `exal.md` §9 Q1 verified **no** authored
  latitude field exists in CLMT/WRLD — `#1019`'s "read a latitude field" premise is false).
  An "implement worldspace latitude parsing" finding is a false premise.

### 6. Per-game translation-survey gaps (upstream branches)

The structural inventory of where the abstraction is still incomplete. Reference:
`docs/engine/per-game-translation-survey.md` (§4 findings by layer, §5 cross-cutting
patterns, §7 "why Fallout is worse than Skyrim").

- **The renderer is clean; the gaps are upstream** — parser/importer/cell-loader carry the
  per-game branches. Audit by the survey's three leak patterns:
  - **Pattern A** — hardcoded BSVER constants where a named helper already exists but call
    sites bypass it (`per-game-translation-survey.md` §5 Pattern A).
  - **Pattern B** — feature-flag-on-an-enum where the wire format already discriminates the
    game (the correct shape; flag mis-dispatch, not "needs a trait").
  - **Pattern C** — variant-enum struct shapes for divergent records.
- **Fallout is the stress case** (§7): widest BSVER span (FO3 24 → FO76 155), format-breaking
  changes at every major version; FO-only paths (BGSM, `bhkNPCollisionObject`, CRC32 shader
  flags, half-float verts, inline tangents) are where silent wrong-default fallbacks cluster.
  Use the per-game audits (`/audit-fnv`, `/audit-fo3`, `/audit-fo4`, `/audit-oblivion`,
  `/audit-skyrim`, `/audit-starfield`) for depth; this dimension owns the cross-game pattern.

### 7. Subsystem coverage vs legacy (gaps with no layer yet)

Subsystems the legacy engine drives that Redux maps incompletely or not at all. Cross-check
the legacy headers (`SDK/Win32/Include/`, `CoreLibs/Ni*/`) against the Redux side.

- **Scene-graph decomposition** (`docs/legacy/api-deep-dive.md` mapping table): each
  `NiAVObject` field → does the Redux ECS component exist? (Parent, Children, GlobalTransform,
  WorldBound, Name, flags — the import-critical set is present; audit *fidelity* gaps, not
  existence.)
- **Transform model**: Gamebryo `NiTransform` (Matrix3 + Point3 + scale) → Redux `Transform`
  (Quat + Vec3 + f32). Matrix3→Quat at import (Shepperd + SVD repair), world propagation
  (`local * parent`) — covered by Dimension 1; flag fidelity gaps (non-uniform scale is
  collapsed to uniform `f32`).
- **Property → pipeline mapping**: the 12 `NiProperty` types — which map to dynamic pipeline
  state (cull mode, blend, stencil two-sided) vs `Material`/per-object components? Flag any
  property whose authored effect is dropped (NiFogProperty is a *documented* skip — D3 above).
- **Animation model**: `NiTimeController` → `NiInterpolator` → keys, converged at import
  (`nifal.md` §"Animation"; B-spline/Euler/TBC all resolved to quaternion keys, KF + embedded
  through `anim_convert::convert_nif_clip`). Parked: per-light ambient + morph-weight channels.
- **String interning**: Gamebryo `NiFixedString` + `GlobalStringTable` vs Redux `StringPool` +
  `FixedString` — semantic equivalence; flag any interning gap that breaks bone-name → entity
  resolution (load-bearing for skinning and PHYSAL bone binding).

## Process

1. Read the live spec for the layer under audit (`nifal.md` / `exal.md` / `physal.md`) — its
   leak inventory is the baseline; do not re-file what it already records as closed/limitation.
2. Trace each claimed boundary to its callers (grep the symbol) — confirm single-producer.
3. Cross-reference Redux components/translate sites against the legacy headers + the
   per-game-translation-survey for upstream branches.
4. Classify per `_audit-severity.md` (NIFAL/translation rows: wrong/divergent canonical out of
   a `translate()` = HIGH; translatable block silently dropped = MEDIUM, escalate if it removes
   visible content). Disprove each finding before keeping it.
5. Finalize per `_audit-common.md` "Report Finalization" (save to
   `docs/audits/AUDIT_LEGACY_COMPAT_<TODAY>.md`; do not open issues directly).
