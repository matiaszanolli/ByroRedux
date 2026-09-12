# NIFAL Audit — 2026-09-11

Full nine-dimension `/audit-nifal` sweep (all dimensions executed, no preset
narrowing). Each dimension ran as its own agent against the live tree, with
independent re-verification of prior claims rather than trusting doc prose,
plus a fresh review of every commit touching its entry-point files since the
last time that dimension was swept. Baselines used: `docs/audits/
AUDIT_NIFAL_2026-08-30.md` (last full 9-dimension sweep) and `docs/audits/
AUDIT_NIFAL_2026-09-05.md` (narrowed `texture-roles-deep` preset, Dimensions
1 + 8 only).

## Executive Summary

**4 open findings: 0 CRITICAL, 1 HIGH, 2 MEDIUM, 1 LOW.** Two are NEW this
sweep, two are carried forward as still-open from prior reports. Two further
items from the 2026-09-05 report (D8-02, D8-03) were verified **fixed** since
and are recorded in the documented-limitation ledger below rather than
counted as open findings.

Per-category convergence against `docs/engine/nifal.md`'s §2 leak inventory:

| Category | Status this sweep |
|---|---|
| Material | converged — re-verified clean, 0 findings |
| Mesh Water (NIFAL/WATAL seam) | converged — re-verified clean, 0 findings |
| Geometry / Transform | converged (reference template) — re-verified clean, 0 findings |
| Skinning | half-stale, unchanged (documented gaps #2440/#2441 still open, not re-filed) |
| Lights | converged — re-verified clean, 0 findings; completeness guard confirmed passing |
| Nodes | triaged by design — re-verified clean, 0 findings |
| Particles | emitter-base converged, 1 existing LOW carried forward (#4044) |
| Collision | audited/converged — re-verified clean, 0 findings |
| Animation / controllers | **1 NEW HIGH finding** (B-spline rotation channel finiteness gap) |
| Shader flags / texture sets | converged, 1 existing MEDIUM carried forward (#3901); 2 prior LOWs (D8-02, D8-03) confirmed fixed |
| Cross-cutting completeness (Dim 9) | **1 NEW MEDIUM finding** (Animation + Particles lack a completeness/dispatch-coverage guard); #2532 confirmed CLOSED, stale "OPEN" ledger note corrected |

Tier-invariant violation counts across all NEW+existing-open findings: **0
single-boundary**, **0 no-fabrication** (the HIGH finding is a *missing
finiteness guard*, i.e. a no-fabrication *hardening gap*, not a fabricated
value — see its Tier-Violated field), **2 no-leak** (D8-01, and the
harness-coverage gap is filed under a "coverage gap" classification per
precedent, not a strict no-leak violation), **1 no-render-time-fallback: N/A**.

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | PASS | PASS | PASS | PASS | `byroredux/src/material_translate.rs::translate_material` — 3 production callers + Cornell harness; signature still narrowed to `&ImportedMaterial` |
| Mesh Water (NIFAL/WATAL seam) | PASS | PASS (parked `damage_per_second=0.0`, documented) | PASS | PASS | `byroredux/src/material_translate.rs::attach_mesh_water` + `water_material_from_mesh`/`water_kind_from_mesh_geometry`/`water_volume_from_mesh` |
| Geometry / Transform | PASS | PASS | PASS | PASS | Per-game extractors → shared `Vec<[f32;3]>`/`Vec<u32>`; `crates/nif/src/import/coord.rs`, `rotation.rs`, `transform.rs::compose_transforms` |
| Skinning | N/A by design (loose-NIF vs cell-loader structural split) | — | **PARTIAL** (documented #2440/#2441, not re-filed) | PASS | `SkinnedMesh::new_with_global` (`byroredux/src/scene/nif_loader.rs`) — sole production constructor, but the cell-loader path never reaches it |
| Lights | PASS | — | PASS | PASS | `byroredux/src/systems/light_anim.rs::translate_light` — sole constructor; `light_dispatch_coverage_tests` closes the completeness gap `#2532` named |
| Nodes | N/A by design (two structurally different load paths, documented) | — | PASS (7 raw-tier-parked fields, zero canonical consumers, re-verified) | — | No single `translate_node` boundary exists or is expected |
| Particles | PASS | PASS (`initial_color`/size-curve deferrals documented no-op) | **PARTIAL** — NIFAL-2026-09-11-N5-01 (Existing: #4044) | PASS | `byroredux/src/systems/particle.rs::apply_emitter_overlays` — sole overlay boundary except the greyscale-LUT field |
| Collision | PASS | PASS | PASS | — | `crates/nif/src/import/collision/shape.rs::resolve_shape_inner` — 16/16 shape arms, `dispatch_coverage_tests` passing |
| Animation / controllers | PASS (2 declared boundaries, same canonical target) | **FAIL — NIFAL-D7-2026-09-11-01 (HIGH)** | PASS | PASS | `byroredux/src/anim_convert.rs::convert_nif_clip` + `byroredux/src/asset_provider/animation.rs::convert_hkx_clip` |
| Shader flags / texture sets | PASS | PASS | **PARTIAL** — NIFAL-2026-09-05-D8-01 (Existing: #3901) | PASS (zero `if game ==` in `triangle.frag`/`include/*.glsl`, re-confirmed) | Dispatch by block type; `MaterialTextureSet<T>`, 26 roles / 5 walks, 4 of 5 now compiler- or test-guarded |
| Cross-cutting completeness | — | — | — | — | **NEW gap**: NIFAL-D9-NEW-01 (MEDIUM) — Animation + Particles have no completeness/dispatch-coverage guard, unlike Material/Collision/Lights |

## Findings

### HIGH

#### NIFAL-D7-2026-09-11-01: B-spline rotation channel is missing the belt-and-braces finiteness guard its float/translation/scale siblings got in #3765
- **Severity**: HIGH
- **Dimension**: Animation
- **Tier Violated**: no-fabrication (a poisoned/NaN transform value reaching the canonical `AnimationClip` is exactly the class of defect the tier's finite-guard doctrine exists to prevent)
- **Game Affected**: FO3/FNV and Skyrim+ (any title whose NIF content authors `NiBSplineCompTransformInterpolator` — the compressed-keyframe format is not Skyrim-only, per the project's own `feedback_bspline_not_skyrim_only` note)
- **Location**: `crates/nif/src/anim/bspline.rs:367-401` (`extract_transform_channel_bspline`'s rotation branch), contrast with the sibling guards at lines 248 (float), 341 (translation), 408 (scale)
- **Status**: NEW
- **Description**: Commit `9ce6b7a5` (Fix #3765) added an `is_key_value_sane` check on the *sampled* `deboor_cubic` output at three of the four B-spline sub-channel call sites — float (`extract_float_channel_bspline`), and translation + scale inside `extract_transform_channel_bspline` — as a "belt-and-braces" layer beyond the already-added `channel_slice` guard on `offset`/`half_range`. The commit message and the fixture comment (`tests/bspline.rs:84-91`) both explicitly enumerate this as covering "all four" sub-channels, but the rotation branch (`bspline.rs:371-391`) was left unguarded: `deboor_cubic`'s raw `[w,x,y,z]` output is fed directly into the quaternion-normalize arithmetic (`len_sq = w*w+x*x+y*y+z*z; inv = 1.0/len_sq.sqrt(); w *= inv; ...`) with no finiteness check on either the pre- or post-normalize values, and the result is unconditionally pushed to `rotation_keys`.

  This is not simply "the same NaN-input case the other three guard against" — the normalize arithmetic introduces its *own*, distinct overflow path that a direct `is_key_value_sane(p[i])` check (mirroring the translation/scale siblings) would not even fully catch on the *pre*-normalize values: a per-component value that individually passes `is_key_value_sane` (finite, `< 3.0e38`) can still overflow to `Infinity` when squared in `w*w` (anything with `|w| > ~1.84e19` squares past `f32::MAX`), making `len_sq` infinite; `1.0 / sqrt(Infinity) = 0.0`; and `Infinity * 0.0 = NaN`. So a rotation control point that is individually "sane" by the exact predicate used elsewhere can still poison the pushed `RotationKey` through the normalize step alone. Separately, if `deboor_cubic` itself overflows one component to `Infinity` (the literal "pathologically large but individually finite `half_range`" scenario the commit describes for the other channels), the same `Infinity * 0.0 → NaN` path fires. Either way, an unguarded NaN quaternion reaches `RotationKey` → `convert_nif_clip`'s direct `Quat::from_xyzw` (`byroredux/src/anim_convert.rs:394`, no sanitization) → the canonical `AnimationClip` → `AnimationPlayer`/`sample_blended_transform` (`crates/core/src/animation/interpolation.rs` has no NaN guard on rotation) → bone `GlobalTransform` → skinned-BLAS refit / TLAS instance transform — the same "NaN AABB in an acceleration-structure build is undefined behavior" failure chain #3765's own commit message names as the reason the other three channels needed the fix.

  Note the coincidental partial protection: if *all four* raw components happen to be NaN (not Infinity) at once, `len_sq` is NaN, `NaN > f32::EPSILON` is false, and the `else` branch substitutes an identity quaternion — so the purely-NaN-input case is accidentally caught by the comparison's false-on-NaN semantics. The Infinity-input case (and the "individually sane but squares-to-Infinity" case) is not caught by that same accident, which is why this reads as a real gap rather than an already-covered case.
- **Evidence**: Confirmed by direct code reading (`bspline.rs:367-401`); confirmed `is_key_value_sane`'s definition (`keys.rs:21-23`, finite + below `FLT_MAX_SENTINEL = 3.0e38`) does not itself protect against squaring overflow. Confirmed no downstream consumer (`interpolation.rs`, `anim_convert.rs`) re-checks rotation key finiteness. Confirmed via `git show 9ce6b7a5` that the fix's own diff and test-file comments list only float/translation/scale as guarded call sites, and the commit message's summary sentence says "the three previously ungated push sites" (not four).
- **Impact**: A crafted or corrupted B-spline-compressed rotation channel (large-magnitude quantized control points, or a `half_range` large enough to make squared components overflow during normalize) can still poison a bone's rotation with NaN despite #3765 having shipped as a fix for "the" B-spline finiteness gap. Same downstream blast radius #3765 itself described: `GlobalTransform` → skinned mesh vertex positions → BLAS refit / TLAS build with a NaN-containing AABB, which is undefined behavior per Vulkan's acceleration-structure build contract (potential VkDevice loss or GPU crash), not merely a visual glitch.
- **Related**: Sibling/parent of #3765 (this is the one channel that commit's own fix left out); same failure family as #1443 (`is_key_value_sane`'s origin) and the "NIF Corpus Baseline / stride drift" finite-guard lineage.
- **Suggested Fix**: Add the same `is_key_value_sane` check the translation/scale branches use, but on the *post-normalize* `[w,x,y,z]` (not just the pre-normalize `p[]`, since the overflow can be introduced by the normalize arithmetic itself) — e.g. `if [w,x,y,z].iter().all(|v| is_key_value_sane(*v)) { rotation_keys.push(...) }` else skip the sample (mirroring the "skip just that sample" behavior the other three branches already have). A test analogous to `bspline_dequant_and_deboor_can_overflow_and_the_guard_catches_it` but driving the quaternion-normalize path specifically would pin this the same way #3765 pinned its three siblings.

### MEDIUM

#### NIFAL-D9-NEW-01: Animation and Particles remain the only two declared NIFAL boundaries with no completeness/dispatch-coverage guard
- **Severity**: MEDIUM
- **Dimension**: Completeness
- **Tier Violated**: harness-coverage gap — no production tier violated (same classification precedent as the now-closed #2532)
- **Game Affected**: all seven (harness-coverage gap, not a per-game data bug)
- **Location**: `byroredux/src/material_translate.rs:2243-2244` (the scoping comment names this itself); no equivalent structural guard exists for `byroredux/src/anim_convert.rs::convert_nif_clip`, `byroredux/src/asset_provider/animation.rs::convert_hkx_clip`, or `byroredux/src/systems/particle.rs::apply_emitter_overlays`
- **Status**: NEW (successor to closed #2532, which explicitly named Animation as "the next extension" when it fixed Lights and confirmed Collision)
- **Description**: #2532 closed with Collision (pre-existing) and Lights (new) both gaining a completeness-style guard, and its own closing comment correctly re-scoped the remaining gap to Animation (`convert_nif_clip`/`convert_hkx_clip`) only. That leaves **Particles** (`apply_emitter_overlays`) unmentioned and still unguarded — the original #2532 body listed it as one of the ~5 boundaries in scope, but neither the issue's fix nor its closing comment addressed it, and no separate issue was found tracking it (closest hits are #4044/#4023, unrelated shader-lane gaps).
- **Evidence**: `grep -n "canonical_completeness_harness\|dispatch_coverage" -r crates/nif/src byroredux/src` finds exactly 3 guard modules: `material_translate.rs::canonical_completeness_harness` (Material), `import/collision/mod.rs::dispatch_coverage_tests` (Collision), `import/walk/lights.rs::light_dispatch_coverage_tests` (Lights). None reference `convert_nif_clip`, `convert_hkx_clip`, or `apply_emitter_overlays`. All three existing guards pass (`cargo test -p byroredux-nif --lib dispatch_coverage`, `--lib light_dispatch`, `cargo test --bin byroredux canonical_completeness_harness` — 2 + 8 tests, all green).
- **Impact**: A silent field-drop or a whole-arm omission in `convert_nif_clip`/`convert_hkx_clip` (e.g. the historical #3345 phase-drop, #3432 NaN-latch, #2305 undeclared second boundary — all now closed but all caught by manual audit, not by a harness) or in `apply_emitter_overlays` would not be caught by any automated signal today; it depends entirely on a human audit sweep noticing, exactly the gap #2532 itself was filed to close for Collision/Lights.
- **Related**: Successor/residual scope of closed #2532 (itself the successor of closed #2214). Not a re-derivation of #2532 — the correction here is that #2532's fix closed 2 of the 3 gaps it implied (Collision was already fine, Lights got fixed), leaving Animation (as the fix commit itself states) and Particles (which the fix commit didn't mention) still open.
- **Suggested Fix**: For Animation, add a kitchen-sink-value harness analogous to Material's (both `convert_nif_clip` and `convert_hkx_clip` produce the same canonical `AnimationClip` target and both live in `byroredux`, so — unlike Collision/Lights — no crate-boundary obstacle applies). For Particles, `apply_emitter_overlays`'s failure mode is closer to Lights/Collision's ("a whole overlay field stops being unioned in") than Material's per-field copy, so a structural/revert-and-fail guard is the better fit. Update the Material module's scoping comment once either lands, the same way #2532's fix updated it for Lights.

#### NIFAL-2026-09-05-D8-01: `TextureFlipEntry.texture_slot` carries a raw `TexType` onto a canonical ECS component (carried forward, still open)
- **Severity**: MEDIUM
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: no-leak
- **Game Affected**: Oblivion (measured: 1 of 54 vanilla `NiFlipController` blocks); structurally all games
- **Location**: `crates/core/src/ecs/components/animated.rs:188-193` (leaked field), `byroredux/src/render/static_meshes.rs:403` (`.and_then(|f| f.handle_for_slot(0))`), `byroredux/src/render/static_meshes.rs:275-280` (the mis-specified remediation comment), `byroredux/src/anim_convert.rs:241-242` (producer)
- **Status**: Existing: #3901, still OPEN — re-verified unchanged this sweep
- **Description**: `TextureFlipEntry.texture_slot: u32` still carries the doc comment "Raw `TexType` slot from the source NIF"; `handle_for_slot(0)` is still hardcoded at the sole call site, silently dropping every `NiFlipController` targeting a non-base slot. The deferral comment at `static_meshes.rs:275-280` still points an implementer at `slot_to_role` (`BSShaderTextureSet` numbering), which is incompatible with `TexType`'s numbering and would bind the wrong role if followed. `git log --since=2026-09-05` on the affected files shows no touching commit; only unrelated refactors landed in the window.
- **Evidence**: `handle_for_slot(0)` verified present at `static_meshes.rs:403` today. No commit since 2026-09-05 touches `animated.rs`/`anim_convert.rs`'s relevant code.
- **Impact**: Unchanged from 2026-09-05 — one vanilla Oblivion mesh's glow flipbook does not animate; the wrong-table remediation plan is still live in the file an implementer would read first, risking a future "fix" that binds the wrong texture instead of no texture.
- **Related**: #3901 (filed, OPEN, body matches this finding).
- **Suggested Fix**: Resolve `TexType` to a `MaterialTextureSet` role at `anim_convert.rs`'s import-side hop via the existing `NiTexturingProperty` mapping in `crates/nif/src/import/material/legacy_properties.rs` (NOT `slot_to_role`), store the canonical role on `TextureFlipEntry`, and correct the `static_meshes.rs` comment regardless of whether the rest is deferred.

### LOW

#### NIFAL-2026-09-11-N5-01: Greyscale-palette LUT resolution still hand-copied at both particle spawn sites (carried forward, still open)
- **Severity**: LOW
- **Dimension**: Particles
- **Tier Violated**: single-boundary
- **Game Affected**: FO4 (BGEM greyscale/palette remap population), structurally reachable wherever a `BSEffectShaderProperty` authors a greyscale texture
- **Location**: `byroredux/src/systems/particle.rs` (`apply_emitter_overlays`), vs. the two `preset.greyscale_lut_index = …resolve_texture(ctx, tex_provider, …)` blocks in `byroredux/src/scene/nif_loader.rs:1531-1535` and `byroredux/src/cell_loader/spawn.rs:1130-1134`
- **Status**: Existing: #4044 (OPEN, filed from `AUDIT_RENDERER_2026-09-06.md` REN-2026-09-06-D6-04); re-verified live and unchanged this sweep
- **Description**: `apply_emitter_overlays` is the single overlay boundary through which every authored `NiPSysEmitter` override reaches the preset — most recently proven out by `84c4a1df` (#3589), which moved the sibling `effect_shader_flags` packing (the palette *enable* bits) inside the boundary specifically to avoid this exact divergence class. The **LUT texture those bits index** (`greyscale_lut_index`, landed by `67eacf8f`/#3590 in the same delta) was left outside the boundary, hand-copied identically at both spawn call sites immediately after each `apply_emitter_overlays` call.
- **Evidence**: Both copies confirmed still byte-identical (differing only in local variable names). Each carries a "Mirrored in the sibling site" comment acknowledging the duplication in prose. No commit since #4044 was filed touches this block.
- **Impact**: Latent, not live — the two copies remain byte-identical, so nothing currently renders wrong. The boundary's own rustdoc claim ("the single overlay boundary that folds every authored emitter override") is technically false for this one field; a future edit to LUT-resolution semantics has two sites to land in instead of one, reintroducing exactly the divergence risk #1513/#3589 closed for every other field.
- **Related**: #1513 (the boundary), #2610/#3589 (the sibling `effect_shader` field, fixed), #3590 (this field, landed hand-copied), #3897/#3898 (the FO4 population it would affect if it ever diverges).
- **Suggested Fix**: Add an eleventh parameter `greyscale_lut: Option<u32>` to `apply_emitter_overlays`, move `preset.greyscale_lut_index = greyscale_lut.unwrap_or(0)` inside it, and leave only the `resolve_texture(ctx, tex_provider, …)` call (which needs `&mut VulkanContext`, unavailable inside the pure boundary fn) at each call site — the same shape `84c4a1df` used for `effect_shader`.

## Documented-limitation ledger

Restated so the next sweep does not re-derive them.

- **#2440** (Skinning) — the cell-loader spawn path never builds a `SkinnedMesh`
  (no per-placement node-entity map exists there); cell-placed skinned geometry
  renders frozen in bind pose. NPCs unaffected — they always route through the
  loose-NIF path. Unchanged, re-verified this sweep, not re-filed.
- **#2441** (Skinning) — `SkinnedMesh.bones`/`skeleton_root` carry `Option`s
  past the translation boundary as a *terminal, logged sentinel* ("bone-name
  lookup against the placement's node map failed"), not a resolve-later leak.
  Unchanged, re-verified this sweep, not re-filed.
- **7 raw-tier-parked Node fields** (`bs_value_node`, `bs_ordered_node`,
  `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`) —
  zero canonical ECS consumers, each blocked on a not-yet-built consumer
  feature. Re-grepped this sweep; the only non-`types.rs`/parser/test hits are
  a new `#[cfg(test)]` fixture constructor in `cell_loader/terrain_lod_btr.rs`
  that hardcodes every field to `None` — not a consumer.
- **Passthroughs** (`NiTextureEffect` content-absent; `BSInvMarker` parsed-not-
  walked; `NiSwitchNode` identity not surfaced; `bs_bound` loose-NIF-only) —
  unchanged, re-verified this sweep. `BSFurnitureMarker` is **consumed**
  (since #2010) — do not re-flag it as parked.
- **Emissive scale is a deliberate no-op** (`docs/engine/nifal.md` §4) — no
  normalization constant exists or is wanted; do not propose one.
- **Starfield particle slice is N/A**, not a leak — zero `NiPSys*` blocks in
  the full Starfield mesh corpus, pinned by
  `starfield_corpus_has_no_particle_blocks`.
- **NIFAL-2026-09-05-D8-02** (`record_external_texture_sources` unguarded role
  walk) — **FIXED** by `012dfa93` (2026-09-11): rewritten via `zip_map_ref`,
  making a forgotten role a compile error rather than a silent mislabel.
  Verified with passing tests. Do not re-file.
- **NIFAL-2026-09-05-D8-03** (`.claude/commands/_audit-common.md` said "18
  named roles") — **FIXED**: now reads 22, matching `docs/engine/nifal.md` and
  `SKILL.md`. Not verified this sweep whether `documented_texture_role_list_
  matches_the_struct` was extended to scan this third file specifically (the
  count itself is confirmed correct) — worth a one-line check next sweep, not
  a re-file.
- **#3515** (`texture_clamp_mode` default mismatch across tiers) — **FIXED**
  by `bb8ced68` this window (default corrected 0→3, pinned by a dedicated test
  module). Do not carry forward as open.
- **#3567** (Oblivion `APPLY_HILIGHT2` normal-alpha double-claimed as both
  parallax height and specular mask) — **FIXED** by `1247d6fa` this window
  (`normal_alpha_spec_binding_applies` now excludes the parallax-height-in-
  alpha route). Do not carry forward as open.
- **#2532** (canonical-tier completeness harness coverage) — **CLOSED**
  (`5b32f7e2`, 2026-09-09). The 2026-08-30 ledger's "OPEN, 1 of ~5 boundaries
  covered" note is stale; Collision and Lights are now both covered, leaving
  Animation and Particles as the residual gap, captured fresh this sweep as
  NIFAL-D9-NEW-01. Do not re-derive #2532 itself as open.
- **D1-01** (`AUDIT_NIFAL_2026-09-05.md`, near-mirror fallback gated on
  authored path instead of resolved gloss handle) — **FIXED** by `7e2abb73`
  (#3905), 2026-09-07. Do not carry forward as open.

## Stale Candidates Dropped

| Candidate | Why dropped |
|---|---|
| D1-01 (near-mirror fallback gate) | Fixed by `7e2abb73`/#3905, re-verified in place |
| D8-02 (`record_external_texture_sources` unguarded) | Fixed by `012dfa93`/#3903, re-verified with passing tests |
| D8-03 (`_audit-common.md` "18 named roles") | Fixed, re-verified by grep across all three prose copies |
| #3515 (`texture_clamp_mode` default mismatch) | Fixed by `bb8ced68`, dedicated test module added |
| #3567 (Oblivion parallax/spec-mask double-claim) | Fixed by `1247d6fa`, measured population 0/35,322 vanilla Oblivion meshes |
| #2532 ledger note "OPEN, 1 of ~5 boundaries" | Stale — closed `5b32f7e2` 2026-09-09; Collision + Lights now covered, residual scope captured as NIFAL-D9-NEW-01 |
| #3072 / #3074 (Nodes, `finish_partial_import` hardcoded-`None` sites) | Both fixed — `flame_attach_offset`/`furniture` now populated in `cell_loader/partial.rs` |
| #3176 / #3177 (tangent-synthesis: zero-vector fallback seed, unnormalized Z-up normal) | Both fixed by `19742e80`, symmetric across Z-up/Y-up producers |
| #3432 (Animation: `duration`/`weight` unsanitized) | Fixed by `19742e80`, both scalars now sanitized at the `convert_nif_clip` boundary |

## Method notes

- Nine dimension agents ran independently against the live tree (no dimension
  relied on another's output for its own findings), each performing its own
  `gh issue list` dedup pass, its own read of the relevant `docs/audits/
  AUDIT_NIFAL_*.md` baselines, and its own `git log` diff review of every
  commit touching its entry-point files since that dimension's last sweep.
- Dimension 9 (Completeness) additionally **executed** the ignored
  cross-game completeness harness (`cargo test -p byroredux-nif --test
  translation_completeness -- --ignored`) against installed vanilla game data
  for all seven titles, plus the three existing dispatch/kitchen-sink
  completeness guards (Material, Collision, Lights) — all green, no
  fill-rate divergence from documented baselines.
- Dimension 3 (Skinning/Lights) and Dimension 5 (Particles) ran the relevant
  `cargo test -p byroredux-nif --lib` suites directly rather than relying on
  static reading alone, confirming the very-recent #3856 (2026-09-09,
  satellite-walker split) and #3930 (2026-09-10, Starfield `SkinAttach`
  resolution) changes hold up under test as well as under code review.
- Total findings in this report: **4 open** (1 HIGH, 2 MEDIUM, 1 LOW), plus
  **9 items confirmed fixed/closed** since the prior baselines (recorded in
  the ledger and stale-candidates table above so they are not re-derived next
  sweep).

## Next Step

```
/audit-publish docs/audits/AUDIT_NIFAL_2026-09-11.md
```

Domain label `nifal` for all four open findings. Add `animation` +
`game:fo3` + `game:fnv` + `game:skyrim` to the HIGH B-spline finding;
`test-gap` to NIFAL-D9-NEW-01; `import-pipeline` to D8-01 (existing #3901,
no new issue needed) and `game:oblivion`; `import-pipeline` + `game:fo4` to
N5-01 (existing #4044, no new issue needed).
