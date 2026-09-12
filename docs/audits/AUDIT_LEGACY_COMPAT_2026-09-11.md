# Legacy Compatibility Audit — 2026-09-11

**Base:** `b3db49fa` · **Type:** full `/audit-legacy-compat` sweep, all 7
dimensions, run via 6 parallel dimension sub-agents (Dimensions 2 and 3 combined
into one agent per the skill's NIFAL/Material pairing) + this orchestrator merge
pass.

## Scope

All seven dimensions: coordinate-system correctness (Z-up→Y-up), NIFAL
cross-layer mapping shape, the material translation boundary, PHYSAL's source
axis, EXAL, per-game translation-survey patterns (A/B/C), and subsystem
coverage vs the legacy engines.

**Method.** Six dimension sub-agents ran in parallel, each independently
re-reading `docs/engine/{coordinate-system,nifal,exal,physal,per-game-translation-survey}.md`
and `docs/legacy/api-deep-dive.md` against current HEAD (no citation-only
carry-forward), then tracing every cited symbol/call-site by grep + targeted
read. Each dimension deduplicated against the 200 most recent GitHub issues
(`gh issue list --repo matiaszanolli/ByroRedux --limit 200`, fetched once and
shared) and against prior `docs/audits/` reports, including the previous
`AUDIT_LEGACY_COMPAT_2026-09-06.md` sweep. The orchestrator independently
re-requested each agent's verbatim final findings (rather than accepting a
secondhand relay) before merging below. No source file, game file, or GitHub
issue was modified.

**Cross-audit note.** Dimension 2 (NIFAL) overlaps the same-day
`docs/audits/AUDIT_NIFAL_2026-09-11.md` full 9-dimension sweep. The Dimension
2/3 sub-agent read that report and independently re-verified its four open
findings against the cited code rather than re-deriving them; per this
project's dedup protocol they are cross-referenced below (§ "Cross-Referenced,
Not Filed Here") rather than duplicated under new IDs in this report's totals.

## Executive Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 1 |
| **Total (new, owned by this report)** | **3** |

Plus, not counted in the totals:

| Class | Count | Detail |
|---|---:|---|
| Prior sweep findings **verified FIXED** | 2 | `#3956`, `#3957` (EXAL height-fog/fog-power) confirmed closed via `ea3ba609` |
| **Existing** open issues re-verified, not re-filed | 4 | `#3142`, `#3307` (EXAL VWD), `#2440`/`#2441` (NIFAL skinning, cross-referenced by the NIFAL/Material agent), `#2458` (bone-name case-sensitivity, closed via interim mitigation — verified, not reopened) |
| **Existing** open findings, sourced from the same-day sibling `/audit-nifal` report, cross-referenced not re-filed | 4 | `AUDIT_NIFAL_2026-09-11.md`: 1 HIGH (B-spline rotation NaN guard), 1 MEDIUM new (Animation/Particles completeness harness), plus `#3901` and `#4044` re-confirmed open |
| Stale skill-text premises corrected (doc drift, not compat bugs) | 3 | `docs/engine/coordinate-system.md` XCLL call-path (COORD-02, filed below as LOW); skill-text's "morph-weight is parked" claim (now false — fully wired); SCOL era-gate doc-vs-code drift (code is more correct than the survey doc) |

**All three canonical-translation-layer boundaries (NIFAL material, PHYSAL
source axis, EXAL) remain structurally intact under independent re-verification
by five of six sub-agents** — no second producer site, no reintroduced
per-game branch downstream of any boundary, no regression on any of the
skill's named regression guards (emissive scale, `NiFogProperty` skip, sun
model, OFST non-capture, VWD consumption). Dimension 6 (per-game
translation-survey) is likewise clean for a second consecutive sweep, with one
notable correction in the audited codebase's favor: SCOL era-gating is now
*more* correct than the survey doc describes it (FNV is correctly included,
not just FO4+).

**This sweep's two real findings are both process/tooling gaps, not silent
mis-translation.** COORD-01 is a diagnostic CLI flag (`--rotation-mode`)
defeating its own library's documented out-of-range safety fallback — narrow
blast radius (opt-in flag only, never the default game path) but ironic, since
the flag exists specifically for placement-bug triage. SUBSYS-01 is an
architecture gap reopening today: the only Redux glue for the legacy
`NiControllerManager`/KFM sequence state machine (issue #338/AR-09) was
deleted as unreachable dead code in the same commit range this audit covers
(`fd111883`), correctly (it had zero consumers), but with no tracking issue
left recording that AR-09 needs re-solving when an actor-AI milestone needs
sequence-driven blending.

---

## Dimension 1 — Coordinate-system correctness (Z-up → Y-up)

**Verified clean (regression guards held, re-traced against current code, not
carried forward from citation):**

- Single position-swap producer `zup_to_yup_pos` (`crates/core/src/math/coord.rs:90`) — every other `(x, z, -y)` hit in the tree delegates to it or is a doc comment.
- The matrix→quat/matrix quadruplication (`nif::import::coord::zup_matrix_to_yup_quat`, `mesh::skin::ni_transform_to_yup_matrix`, `collision::havok_quat_to_engine`, `collision::decompose_havok_matrix`) is a known, tracked quadruplication (`#2437`/COORD-4) guarded by `crates/nif/src/import/tests/coord_cross_check.rs`, which independently derives `C·R·Cᵀ` via glam and pins all four paths to agree. Re-verified by hand — still agrees.
- `EXTERIOR_CELL_UNITS = 4096.0` (`crates/core/src/math/coord.rs:41`) remains the sole source; every other `4096.0` literal resolves to it, is an unrelated constant, or is test-only.
- Strip de-stitch (`crates/nif/src/blocks/strip.rs::destrip`, delegated to by `NiTriStripsData::to_triangles` post-`#2298`) swaps the **last** two vertices on odd triangles — confirmed, not the D3D first-two variant.
- `Camera::projection_matrix_with` applies the Y-flip exactly as documented.
- REFR placement callers (`cell_loader/refr.rs`, `references/mod.rs`, `placement_lod.rs`, `transition.rs`) all route through `euler_zup_to_quat_yup_refr` — no hardcoded mode, no re-derived formula. `transition.rs`'s prior bypass (`#2435`/COORD-2) remains fixed and self-guarded by a source-text regression test.

### COORD-01: `--rotation-mode` CLI dispatcher defeats its own out-of-range safety fallback
- **Severity**: MEDIUM
- **Dimension**: 1 — Coordinate-system correctness
- **Location**: `byroredux/src/boot/mod.rs:293-303`
- **Status**: NEW
- **Description**: `crates/core/src/math/coord.rs::euler_zup_to_quat_yup_mode` is documented and pinned (`out_of_range_mode_falls_back_to_ship`) to fall back to the shipping ZYX/CW formula (mode 1) for any mode outside `0..=3`, specifically "so a bad `--rotation-mode` argument can't produce garbage placement." The CLI entry point never lets that fallback fire: `crate::cell_loader::set_refr_rotation_mode_diag(mode.min(3))` pre-clamps any out-of-range value to **3** — one of the two explicitly-wrong diagnostic-only conventions (CCW + XYZ-product) — before it reaches the library's own guard. The adjacent comment ("Defaults to 0 (current shipping behavior)") is also stale: the shipping default has been mode 1 since the 2026-05-26 ZYX fix (`REFR_ROTATION_MODE_SHIP = 1`, `cell_loader/euler.rs`).
- **Evidence**:
  ```rust
  // boot/mod.rs:297-301
  // on a known-good cell. Defaults to 0 (current shipping behavior).
  if let Some(idx) = args.iter().position(|a| a == "--rotation-mode") {
      if let Some(mode) = args.get(idx + 1).and_then(|v| v.parse::<u8>().ok()) {
          crate::cell_loader::set_refr_rotation_mode_diag(mode.min(3));
  ```
  vs. `coord.rs`'s contract: `_ => Quat::from_rotation_y(-rz) * Quat::from_rotation_z(ry) * Quat::from_rotation_x(-rx)` for any mode not in `{0,2,3}`, pinned by `out_of_range_mode_falls_back_to_ship` for modes `4, 9, 255`.
- **Impact**: Narrow blast radius — only reachable via the opt-in `--rotation-mode` diagnostic flag, never the default game path (`set_refr_rotation_mode_diag` is only called when the flag is explicitly passed). But an operator using this exact tool to triage a placement bug (e.g. `--rotation-mode 4`) is silently steered into the wrong CCW+XYZ convention instead of the safe ship default, undermining the triage session the flag exists for. The stale "Defaults to 0" comment could also mislead a future engineer about current shipping behavior.
- **Related**: Originated in `196dd67c8` (2026-05-07). Survived the `main.rs → boot.rs` (`#1858`) and `boot.rs → boot/` (`#3855`) moves unchanged. Not caught by `AUDIT_LEGACY_COMPAT_2026-08-27.md`'s Dimension 1 sweep (which checked only for hardcoded modes/re-derived formulas, not this clamp). No matching open/closed GitHub issue found.
- **Suggested Fix**: Replace `mode.min(3)` with a straight pass-through (`set_refr_rotation_mode_diag(mode)`), letting `euler_zup_to_quat_yup_mode`'s own `_ =>` arm provide the fallback; update the stale "Defaults to 0" comment to "Defaults to 1 (current shipping behavior)".

### COORD-02: `docs/engine/coordinate-system.md`'s XCLL call-path claim is stale
- **Severity**: LOW
- **Dimension**: 1 — Coordinate-system correctness (documentation)
- **Location**: `docs/engine/coordinate-system.md:192-194` vs. `byroredux/src/cell_loader/load.rs:218-233` and `byroredux/src/cell_loader/euler.rs:4-6`
- **Status**: NEW
- **Description**: The dimension's own reference doc states: "Non-REFR callers (XCLL directional lighting in `scene.rs`, `#380`) call the canonical `euler_zup_to_quat_yup` directly, bypassing the dispatcher." This is no longer true on two counts: (1) XCLL lighting no longer lives in `scene.rs` — it moved to `cell_loader/load.rs`; (2) it no longer calls `euler_zup_to_quat_yup` at all. Fix `#3313`/`#3314`/`#3315`/`#3316` (`b78749aff`, 2026-08-26) replaced the `#380` quaternion-based routing with a dedicated `xcll_direction_yup(azimuth, elevation)` spherical-to-vector helper, specifically because routing through the shared REFR helper discarded azimuth (an X-axis-only quaternion rotation cannot move the model vector's azimuth component). `cell_loader/euler.rs`'s own module doc already states the opposite of the shared doc: "XCLL directional lighting deliberately does not use these helpers."
- **Evidence**: `xcll_direction_yup`'s trigonometric derivation (`cos_elev·cos_az, -sin_elev, -cos_elev·sin_az`) is a correct direct construction of the `(x,z,-y)`-swapped direction vector — the current code is correct, only the doc is wrong. `docs/engine/coordinate-system.md` was last touched `a5a360f96` (2026-08-26, ~3h before `b78749aff` landed the same day) and was never reconciled afterward.
- **Impact**: Low direct impact (the code itself is correct), but this is the dimension's cited authoritative reference; a future contributor "fixing" `xcll_direction_yup` to route through `euler_zup_to_quat_yup` on the doc's authority would reintroduce the discarded-azimuth bug `#3313` fixed.
- **Related**: Fix `#3313`/`#3314`/`#3315`/`#3316`. No open issue tracks the doc drift itself.
- **Suggested Fix**: Update the "Impact on Euler angles" section of `coordinate-system.md` to point at `cell_loader::load::xcll_direction_yup` and describe the azimuth/elevation-to-direction derivation instead of claiming XCLL uses `euler_zup_to_quat_yup`.

---

## Dimensions 2 & 3 — NIFAL canonical translation contract + Material translation boundary

**Zero new findings from independent investigation.** Both dimensions were
re-traced against HEAD and hold as documented:

- **Material single-boundary contract (Dim 3)**: `translate_material` (`byroredux/src/material_translate.rs`) remains the sole populated-`Material` producer — exactly three production callers (`scene/nif_loader.rs:1106`, `cell_loader/spawn/mesh_instance.rs:837`, `cell_loader/placement_lod.rs:542`) plus the self-contained `--cornell` RT harness. Every other `Material {` construction site is test-only. `resolve_pbr`'s NaN-sentinel gate + clamp, and `classify_glass_into_material` running strictly after PBR resolve ("forced glass roughness wins"), both confirmed by direct read.
- **Emissive/fog regression guards (Dim 3)**: `EmissiveSource` still has exactly three variants with no shared-scale normalization (matches `nifal.md` §4's 2026-08-29 census — FNV `Material`'s 10.0 secondary mode and FO4 `Lighting`'s 0.05 mode are explicitly *not* ~1.0 across the board, correcting this audit's own prior looser framing of that guard). `NiFogProperty`'s deliberate-skip comment (`#1224`/D4-NEW-02) is unchanged, no fog dispatch added.
- **NIFAL skinning/nodes/lights/collision (Dim 2)**: `SkinnedMesh::new_with_global` has one non-test production call site. The four raw-tier-parked `ImportedNode` fields (`bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`) have zero non-test consumers, matching the documented bounded gap. `canonical_light_animation_flags`'s `match game` is itself the translate-time boundary (called only from the three ESM-LIGH producer sites, before any canonical `LightSource` exists) — not a downstream branch on already-canonical data. Collision dispatch (`resolve_shape_inner` and callees) has zero per-game branches.

### Cross-Referenced, Not Filed Here

The same-day `docs/audits/AUDIT_NIFAL_2026-09-11.md` (a full 9-dimension
`/audit-nifal` sweep) covers this exact NIFAL scope in more depth and carries
open findings independently re-verified by this audit's Dim 2/3 agent but
**not re-derived under new IDs**, to avoid double-filing:

- **HIGH** — B-spline rotation-channel quaternion normalize has no post-normalize finiteness guard (`crates/nif/src/anim/bspline.rs:367-401`); sibling float/translation/scale channels got one under `#3765`, rotation didn't. A poisoned/NaN transform can reach the canonical `AnimationClip` → skinned-BLAS refit path.
- **MEDIUM** — Animation and Particles are the only two declared NIFAL boundaries with no completeness/dispatch-coverage test harness (successor to closed `#2532`).
- Existing `#3901` (open) — `TextureFlipEntry.texture_slot` leaks a raw `TexType`, `handle_for_slot(0)` hardcoded.
- Existing `#4044` (open) — greyscale-palette LUT index hand-copied at two particle spawn sites instead of inside the single `apply_emitter_overlays` boundary.

File these against `AUDIT_NIFAL_2026-09-11.md`, not this report, if not already filed.

---

## Dimension 4 — PHYSAL per-game Havok articulation → solver (source axis)

**Zero findings.** Re-verified against `docs/engine/physal.md`:

- The three typed decoders (`RagdollCInfo`/`LimitedHingeCInfo`/`PrismaticCInfo`, `crates/nif/src/blocks/collision/constraints.rs`) each have `parse_fo3`/`parse_oblivion` (and hinge-specific) arms; era-only fields (FO3+ motors, `Perp Axis In B1`) are decoded-or-zeroed and never read downstream by `crates/nif/src/import/collision/ragdoll.rs`'s joint builders. `BallAndSocket`/`StiffSpring` remain `Other` on both eras, matching the documented deferral. A self-checking `physal_seam_doc_tests` module cross-verifies doc and dispatch stay in lockstep.
- Per-era byte-exact coverage confirmed in `crates/nif/src/blocks/collision/bhk_constraint_tests.rs` (dedicated tests per era per type).
- `extract_ragdoll` (`crates/nif/src/import/collision/ragdoll.rs`) switches only on `BhkConstraintData` — zero `game ==`/`GameKind::` hits.
- Exactly one `build_ragdoll` (`crates/physics/src/ragdoll.rs:262`) and exactly one production call site of `template_from_imported` (`byroredux/src/scene/nif_loader.rs:545`, gated only on `imported.ragdoll.is_some()`). `RagdollSpec`/`RagdollJointSpec` carry no Rapier types in their public signatures — repo-wide grep for `rapier3d` outside `crates/physics` returned nothing.
- `ragdoll_writeback_system` only reads ECS transform components and writes `GlobalTransform`/`WorldBound` — no renderer/Vulkan code touched.
- `havok_scale_for` branches on `NifVariant` (a version-detected enum, not a raw game string) for the Skyrim+ ×69.99125 scale — the documented second legitimate source-boundary seam, not a new branch.

Two solver-end issues (`#3968`, `#3964`, collider dirty-marking + `physics_sync_system` access declarations) exist in the open-issue tracker but are sink-axis, owned by `/audit-physics`, and correctly not duplicated here.

---

## Dimension 5 — EXAL per-game exterior environment → renderer

**Clean. No new findings.** Independently re-verified against
`docs/engine/exal.md`:

- `byroredux/src/env_translate.rs` remains the sole producer of `SkyParamsRes`/`WeatherDataRes`/`CellLightingRes`/`WaterMaterial` — every other populated-literal construction site is test-only or the correctly-scoped interior-only `engine_default_interior_lighting` (`cell_loader/load.rs`), which EXAL explicitly defers.
- No scattered new `if game ==` exterior branches: the only production `match game` sites outside `env_translate.rs` are `object_lod.rs:571` and `lod_bands.rs:144`, both the documented LOD trait-style per-game providers, not GameVariant-table leaks.
- `LodBandSelection::coarsen_to_available` (`cell_loader/lod_bands.rs:281`) matches the `#3502` fix description exactly. `#3142` and `#3307` confirmed still OPEN with matching titles — not re-filed. `resident_vwd_refr_cells`/`VisibleWhenDistant`/`LodCoverageStats::vwd_full_model_overlaps` confirmed present and wired. `OFST` confirmed deliberately walked-past-unconsumed.
- Sun-model regression guard holds: `SUN_SOUTH_TILT` + `tod_hours` remain the only canonical sun inputs; no latitude-field parsing exists anywhere in `crates/plugin/src/esm/`.

**Prior findings verified FIXED, not re-filed**: `#3956`/`#3957` (FO4/FO76
authored height-fog + fog-power dropped at the EXAL boundary, from
`AUDIT_LEGACY_COMPAT_2026-09-06.md`) — confirmed fixed by `ea3ba609`.
`env_translate.rs` now builds `fog_medium` via
`FogMedium::from_legacy_ramp(...).with_authored_height_range(...)` and sets
`fog_power: Some(wthr.fog_day_power)`; `context/draw.rs`'s
`height_fog_params` sink now threads the EXAL-resolved scale height instead of
the unconditional `DEFAULT_SCALE_HEIGHT_METERS` constant.

---

## Dimension 6 — Per-game translation-survey gaps (upstream branches)

**Clean. No new findings**, for a second consecutive sweep — independently
re-verified against `docs/engine/per-game-translation-survey.md` rather than
citing the prior sweep:

- **Pattern A**: `NifVariant` (`crates/nif/src/version.rs`) carries zero
  feature-flag predicates (the `has_*`/`uses_*` methods that do exist belong
  to a separate `impl NifVersion` block, a deliberate documented split, `#1337`).
  Every live `bsver()` comparison site checked (10 call sites across
  `node.rs`, `shader.rs`, `skin.rs`, `controller/sequence.rs`,
  `controller/morph.rs`, `collision/rigid_body.rs`, `tri_shape/bs_tri_shape.rs`)
  compares against a named `version::bsver::*` constant, not a bare literal.
- **Pattern B**: No `GameVariant` trait exists anywhere in source (only
  build-artifact hits under `target/`). `NifVariant`/`GameKind` remain flat
  enums — accepted architecture debt per the survey's own §6 proposal, not a
  live bug.
- **Pattern C**: `ShaderFlags` (three-variant enum) confirmed present with
  well-pinned CRC32 constant tables and cross-game bit-collision regression
  tests. No drift.
- **FO-only spot-checks**: `bhkNPCollisionObject`'s opaque `BhkSystemBinary`
  payload fails toward a safe render-geometry-proxy fallback, not a silently
  wrong one. FO4's SSE half-float reconstruction gap (`sse_recon.rs`) returns
  `None` cleanly and is skipped, not misdecoded, for `bsver >= 130`. The
  `FO4_SHADER_GAP` CRC32 band (`bsver == 131`) is a named, tested, explicitly
  documented "carries neither encoding" band, not a silent default.
- **Doc-vs-code drift, corrected in the code's favor**: the survey doc calls
  SCOL "FO4+ only by definition," but current code
  (`crates/plugin/src/esm/records/mod.rs:263-270`) correctly treats SCOL as
  Gamebryo-Fallout-wide (`is_scol_era = is_fo4_plus || matches!(game, GameKind::Fallout3NV)`,
  with 98 FNV records / 1084 refs observed), while PKIN/MOVS/MSWP stay
  strictly FO4+-gated. Not a compat bug; worth a documentation touch-up to
  `per-game-translation-survey.md`.

---

## Dimension 7 — Subsystem coverage vs legacy

### SUBSYS-01: `NiControllerManager`/KFM sequence state machine (AR-09) is open again — its only Redux glue was deleted today as dead code
- **Severity**: MEDIUM
- **Dimension**: 7 — Subsystem coverage vs legacy
- **Location**: deleted `crates/core/src/animation/controller.rs` (`fd111883`), `crates/nif/src/kfm.rs`, `docs/engine/animation.md:235-255`
- **Status**: Regression of `#338` (AR-09: "No `NiControllerManager` sequence state machine equivalent" — closed by `07dc6b16` 2026-04-23, reopened in effect by `fd111883` 2026-09-11)
- **Description**: Gamebryo's `NiControllerManager` drives cross-sequence transitions (idle→attack→walk) via KFM-authored sync groups and blend-duration precedence. Redux's `AnimationStack` supplies only the blend mechanism; `#338`/AR-09 added `AnimationController` as the joining state machine, but it never acquired a consumer — no spawn path attached it, no system read `apply_pending_transition` — and was deleted today under `#3886` (closed, same commit `fd111883`) as unreachable dead code. Correct call for that specific code (tested-but-never-driven for 5 months), but the net effect is that the legacy subsystem AR-09 named now has **zero** Redux representation again: `parse_kfm` still parses the sequence catalog and is called by nothing outside its own crate's tests.
- **Evidence**: `docs/engine/animation.md:237-240`: *"Redux has **no** `NiControllerManager` / KFM equivalent. `AnimationStack` supplies the blend mechanism and `byroredux_nif::kfm` supplies the catalog, but nothing joins them: no spawn path builds a per-actor sequence catalog, and no system drives sequence transitions."* The doc and the deletion commit are self-aware and honest about this — not a silent gap — but no open issue currently tracks "AR-09 needs re-solving," only the (closed) removal ticket `#3886`.
- **Impact**: Any actor behavior depending on legacy multi-sequence blending driven by KFM triggers (attack interrupting idle, alerted→combat transitions with authored sync groups) has no engine-side mechanism beyond a manual single-layer `AnimationStack` crossfade. Blast radius is bounded today — nothing currently builds per-actor KFM catalogs at spawn time either, so no regression in *observed* behavior — the gap is architectural, for whenever actor AI needs sequence-driven blending.
- **Related**: `#338` (original AR-09), `#3886` (the deletion, itself correct), `TD8-2026-09-05-03` (tech-debt finding that prompted the deletion).
- **Suggested Fix**: Not urgent to rebuild speculatively (the deletion commit's own rationale — "wiring is milestone-sized, rebuild alongside a real consumer" — is sound). File a tracking issue for AR-09's reopening so it isn't lost, pointing at `git show 07dc6b16` for the recoverable transition-model logic (sync groups, blend-duration precedence chain, `KfmTransitionType` mapping) when an actor bring-up milestone needs sequence blending.

**Verified clean / stale-premise corrections (no new finding filed):**

- **Property → pipeline mapping**: all 12 legacy `NiProperty` types trace to a landing site. `NiAlphaProperty`/`NiMaterialProperty`/`NiTexturingProperty`/`NiVertexColorProperty`/`NiStencilProperty`/`NiZBufferProperty` have dedicated parsers; `NiSpecularProperty`/`NiWireframeProperty`/`NiDitherProperty`/`NiShadeProperty` dispatch through the shared `NiFlagProperty` parser and are consumed in `crates/nif/src/import/material/legacy_properties.rs` (authored-disable semantics preserved, including the Skyrim+ `NiShadeProperty` no-on-disk-flags version gate). `NiFogProperty` is the one deliberate skip (Dimension 3, not re-filed here). `NiRendererSpecificProperty` has zero hits anywhere in the block dispatcher or `nif.xml` — an abstract runtime-only base never serialized to `.nif`, nothing to map.
- **Transform model**: `Transform` is `{Vec3, Quat, f32}`; Gamebryo's `NiTransform` also carries a single *uniform* `m_fScale` — there is no non-uniform-scale-to-uniform-f32 collapse happening at the `Transform`-component boundary itself, contrary to the skill bullet's framing. `crates/nif/src/import/transform.rs::compose_transforms` does straightforward parent×child composition and assumes rotation matrices are already sanitized upstream. The one real, already-triaged fidelity loss is scale genuinely baked into a non-orthogonal authored `NiMatrix3` — that is `#3532`/`#2456`'s SVD classifier territory, already filed, not re-filed here.
- **Animation model — the skill's "parked" pair is now stale for morph-weight**: `FloatTarget::MorphWeight(idx)` is fully wired end-to-end (NIF import → `anim_convert.rs` → `AnimatedMorphWeights` → per-frame sampling in `systems/animation.rs::apply_float_channels` → GPU morph-blend consumption in `render/skinned.rs`). Only `ColorTarget::LightAmbient` remains genuinely parked; the float light targets (`LightDimmer`/`LightIntensity`/`LightRadius`) DO write into `LightSource` — only the ambient *color* slot is parked, tracked under `#983`, captured-but-unconsumed by design (the renderer doesn't consume the per-light ambient slot; cell ambient drives the unlit fallback instead). **Recommend updating the skill text** to drop "morph-weight" from the parked pair.
- **String interning / bone-name resolution**: `StringPool::intern`/`get` lowercase, matching Gamebryo's case-insensitive `GlobalStringTable` semantics for ordinary `Name` comparisons. The one genuine interning-regime split (skin-bone and ragdoll bone-name maps bypass `StringPool`, keying on raw case-preserved NIF names) was filed and closed as `#2458`. Verified: the fix in place (`byroredux/src/name_lookup.rs::get_case_insensitive`, exact match first + rate-limited-warn case-insensitive fallback scan) is the documented *interim* mitigation, not the "fuller fix" (re-key both maps through `StringPool`/`FixedString`) the same file's own `#2458` comment still names as outstanding. Functionally this no longer breaks bone resolution, so closing `#2458` was reasonable — but the dual-normalization-regime gap it described is still literally present in the code and comments. Not re-filed (Existing: `#2458`, verified as closed via interim mitigation).
- `NiAVObject`'s full field decomposition (Parent/Children/GlobalTransform/`WorldBound`/Name/flags) is present and unchanged; `crates/core/src/ecs/components/world_bound.rs` explicitly documents itself as "Equivalent to Gamebryo's `NiAVObject::m_kWorldBound`."

---

## Skill-Text Corrections Recommended (not compat bugs, but worth syncing)

1. Drop "morph-weight" from Dimension 7's "parked" animation-channel pair — only per-light ambient color remains parked (`#983`); float light targets and morph weights are both fully wired.
2. `docs/engine/coordinate-system.md`'s XCLL call-path description should point at `cell_loader::load::xcll_direction_yup`, not `euler_zup_to_quat_yup` (COORD-02 above).
3. `docs/engine/per-game-translation-survey.md`'s SCOL era characterization ("FO4+ only by definition") should be updated to note FNV inclusion via `is_scol_era`.

## Files Reviewed

`crates/core/src/math/coord.rs`, `crates/nif/src/import/coord.rs`,
`byroredux/src/cell_loader/euler.rs`, `byroredux/src/boot/mod.rs`,
`byroredux/src/cell_loader/load.rs`, `crates/nif/src/blocks/strip.rs`,
`byroredux/src/material_translate.rs`,
`crates/core/src/ecs/components/material.rs`,
`crates/nif/src/import/material/legacy_properties.rs`,
`crates/nif/src/import/collision/{ragdoll,shape}.rs`,
`crates/nif/src/blocks/collision/constraints.rs`,
`crates/nif/src/blocks/collision/bhk_constraint_tests.rs`,
`byroredux/src/ragdoll.rs`, `crates/physics/src/ragdoll.rs`,
`byroredux/src/env_translate.rs`, `byroredux/src/scene/world_setup.rs`,
`byroredux/src/cell_loader/{object_lod,lod_bands}.rs`,
`byroredux/src/streaming_helpers.rs`, `crates/nif/src/version.rs`,
`crates/nif/src/shader_flags.rs`, `crates/nif/src/import/mesh/sse_recon.rs`,
`crates/plugin/src/esm/records/mod.rs`, `crates/nif/src/blocks/properties.rs`,
`crates/nif/src/import/transform.rs`, `crates/core/src/animation/types.rs`,
`byroredux/src/systems/animation.rs`, `crates/core/src/string/`,
`byroredux/src/name_lookup.rs`, `byroredux/src/scene/nif_loader.rs`, plus
`docs/engine/{coordinate-system,nifal,exal,physal,per-game-translation-survey,animation}.md`
and `docs/audits/{AUDIT_LEGACY_COMPAT_2026-09-06,AUDIT_NIFAL_2026-09-11}.md`.

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-09-11.md
```
