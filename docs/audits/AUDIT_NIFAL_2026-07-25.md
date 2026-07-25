# NIFAL Audit — 2026-07-25

Deep audit of **NIFAL** (the NIF Abstraction Layer; spec: `docs/engine/nifal.md`),
run as one leg of a `comprehensive` audit-suite sweep. Repo HEAD: `ca7a4e0e`.

**Scope**: all 9 dimensions per `.claude/commands/audit-nifal/SKILL.md`. Dimension 1
(Material) was independently deep-audited this same sweep by the `/audit-renderer`
leg — its 5 checklist items (single `translate_material` boundary, plain-f32
metalness/roughness with idempotent `resolve_pbr`, resolved `EmissiveSource`, no
per-game branch in `MaterialTable::intern`, particle emitter overrides
kinematics/size but not color) were reported PASS with no findings there. This
report re-verifies Dimension 1's regression pins lightly (callers-count check,
clamp/backstop check) rather than re-deriving from scratch, and gives full
attention to Dimensions 2–9.

**Method**: direct code inspection (grep for call sites / construction sites /
downcast arms, `Read` on entry-point files), `git log`/`git show` against the prior
`AUDIT_NIFAL_2026-07-16.md` sweep to confirm its two findings were fixed and the
fix holds, and targeted `cargo test` runs (`byroredux-nif`, `byroredux-core`, the
`--ignored` `translation_completeness` harness). No sub-agent delegation was used
for the final synthesis — every claim below was checked against the live tree in
this session.

---

## Executive Summary

NIFAL remains **converged** across all 9 dimensions. This sweep found **zero new
findings** (CRITICAL/HIGH/MEDIUM/LOW). Highlights:

- Both findings from the prior sweep (`AUDIT_NIFAL_2026-07-16.md`) — **MAT-D1-01**
  (glass/fabric keyword classifier unbounded substring match, HIGH) and
  **NIFAL-D4-01** (furniture-marker heading `Option` re-resolved by a gameplay
  heuristic, MEDIUM) — were fixed in commit `3e0129d8` (2026-07-18, `#2009`/`#2010`)
  and remain fixed at HEAD: `contains_any_ci_word` now boundary-checks the
  `"ice"`/`"gem"` keywords, and the canonical `FurnitureMarkerKind` enum
  (`Sit`/`Sleep`/`Lean`) is resolved once at the `furniture_component` translate
  boundary, with `is_sit_marker` reading the resolved `kind` field directly. Both
  verified live in the tree with their regression tests present and passing.
- Every category-boundary invariant (single-boundary / no-fabrication / no-leak /
  no-render-time-fallback) checked this sweep holds. See the tier matrix below.
- The `#[ignore]`-gated `cross_game_translation_completeness` harness passes with
  all per-game fill-rate floors green (Oblivion/FO3/FNV/SkyrimSE/FO4/FO76/Starfield).
- One previously-documented LOW/no-action item (`D2-NEW-02` from
  `AUDIT_NIFAL_2026-05-30.md` — a dead, unreachable-in-production second SVD-repair
  path inside `coord.rs::zup_matrix_to_yup_quat`) was independently re-derived by
  this sweep before being matched against the prior report; it still holds exactly
  as previously classified (LOW, no tier violated, no action). Restated in the
  ledger below, not re-filed.
- A handful of NIFAL-adjacent gaps remain open as pre-existing tracked issues
  (`#1856` FO3-D1-01 water-shader-flags dead-end, `#2109`/`#2108` Starfield
  BGEM/EFFECT_PALETTE gaps, `#2099`/`#2098` Starfield UV/bound gaps, `#1827` FO4-D4-02
  Starfield BSGeometry bone data) — all confirmed still open, still accurate,
  not regressed, not duplicated by this report.

**Bottom line**: this is a verification-clean sweep. The NIFAL layer's own
regression-pin discipline (every fix ships a test at both the boundary and the
consumer) is doing its job — nothing drifted in the 9-day delta since the last
dedicated NIFAL sweep (`c3e09bb5..ca7a4e0e`, ~30 commits, mostly renderer/CI work
per `git log`, none touching a translate boundary in a way that regressed).

---

## Per-Category Tier Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Material | `material_translate.rs::translate_material` | PASS (exactly 2 callers: `nif_loader.rs:899`, `spawn.rs:1165`) | PASS (emissive no-op stands; MAT-D1-01 word-boundary fix holds) | PASS (`metalness`/`roughness` plain `f32`, NaN-sentinel backstop only) | PASS (renderer reads `m.metalness`/`m.roughness` directly) |
| Geometry / Transform | `coord.rs` + `rotation.rs::sanitize_rotation` (parse-time) + `transform.rs::compose_transforms` | N/A (no single fn needed; converges structurally) | PASS | PASS (one exception — see Documented-limitation ledger, D2-NEW-02, LOW/no-action) | PASS (`MeshRegistry::upload` is format-agnostic) |
| Skinning | `mesh/skin.rs` (global bone-index remap, #613) | N/A | PASS | PASS (global indices only; u16-range guard intact; `body_part_flags` parked, zero consumers) | PASS |
| Lights | `import/walk/mod.rs` → `LightKind` | N/A | PASS | PASS (zero renderer matches on `NiAmbientLight`/`NiDirectionalLight`/`NiPointLight`/`NiSpotLight`) | PASS |
| Nodes | (by design, no single boundary — spec §2) | N/A (documented) | PASS | PASS (7 parked fields — `bs_value_node`, `bs_ordered_node`, `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index` — confirmed zero consumers outside `types.rs`/parser/tests) | N/A |
| Furniture (Nodes sub-category) | `cell_loader/references/attach.rs::furniture_component` | PASS (1 construction site) | PASS | PASS (D4-01 fixed — `FurnitureMarkerKind` resolved once, `is_sit_marker` reads `m.kind`) | N/A |
| Particles | `systems/particle.rs::apply_emitter_overlays` | PASS (2 real callers: `nif_loader.rs:544`, `spawn.rs:612`) | PASS (`initial_color` still intentionally unapplied; size-over-life curve still documented future work) | PASS | PASS (force fields converted once at overlay time, not per-frame) |
| Collision | `import/collision/shape.rs::resolve_shape_inner` | PASS | PASS (`hkMotionType` full-enum collapse intact) | PASS (16 shape arms present, matches spec exactly; `BhkPlaneShape` documented `None` exception) | N/A |
| Animation | `anim_convert.rs::convert_nif_clip` | PASS (6 callers of one fn — correct per spec) | PASS | PASS (morph-weight channels reach a real canonical `AnimatedMorphWeights` component every frame; text-key events wired end-to-end to `AnimationTextKeyEvents`) | PASS |
| Shader flags / texture sets | `shader_flags.rs` + `import/material/dedicated_shader.rs` | PASS (block-type dispatch) | PASS | PASS (all 9 `BSLightingShaderProperty` variants forward trailing data; FO4 `Model_Space_Normals`/`Alpha_Test` bits reach `MaterialInfo`, #1592/#1985/#2091 chain intact) | PASS (`triangle.frag` + `include/*.glsl`: zero `if game ==`) |

Note on Shader flags: the spec's entry-point reference to `import/material/walker.rs`
for the #1592 FO4-flag merge is stale — that logic now lives in
`crates/nif/src/import/material/dedicated_shader.rs` (a file split since the spec
was last updated). The logic itself is correct and unchanged; this is a doc-rot
note, not a finding (see Suggested doc fix below).

---

## Findings

**None new this sweep.** Every checklist item across Dimensions 2–9 was traced to
the live tree and found intact; Dimension 1's regression pins (independently
re-verified: exactly 2 `translate_material` callers, `resolve_pbr` clamp +
NaN-backstop unchanged) also hold, consistent with `/audit-renderer`'s parallel-leg
finding of 0 issues.

### Doc-rot note (not filed as an issue — trivial, bundle into next docs pass)
- `.claude/commands/audit-nifal/SKILL.md` Dimension 8 "Entry points" cites
  `import/material/walker.rs` for the FO4 `Model_Space_Normals`/`Alpha_Test` merge
  (#1592). That logic now lives in `crates/nif/src/import/material/dedicated_shader.rs`
  (confirmed via `grep`; `walker.rs` still exists but no longer contains this code).
  Low value as a standalone issue — recommend folding into the next
  `_audit-validate.sh`-driven doc-rot pass alongside any other skill-file path drift.

---

## Documented-limitation ledger (parked-not-leak / no-action — do NOT re-report next sweep)

Re-verified against HEAD `ca7a4e0e`:

- **`D2-NEW-02`** (`AUDIT_NIFAL_2026-05-30.md`, LOW, tier violated: none): the
  defensive second SVD-repair path inside `coord.rs::zup_matrix_to_yup_quat`
  (`svd_repair_to_quat`) is unreachable in production. Re-derived independently
  this sweep before being matched against the prior report: `sanitize_rotation`
  (`crates/nif/src/rotation.rs`) runs once at parse inside both
  `NifStream::read_ni_transform` and `read_ni_transform_struct`
  (`crates/nif/src/stream.rs:664,687`), so every `NiMatrix3` reaching
  `zup_matrix_to_yup_quat` (14 call sites across `import/walk/mod.rs`,
  `import/mesh/{ni_tri_shape,bs_tri_shape,bs_geometry}.rs`, `import/precombine.rs`)
  already has determinant ≈ 1.0; the axis-swap similarity transform preserves
  determinant, so the function's own degenerate-check branch never fires from a
  production call site — only from direct unit-test callers that hand it a raw
  degenerate matrix. Confirmed still true, still LOW, still no action needed
  (leave as harmless defense, per the prior report's own suggested fix).
- **Node passthroughs** (Dim 4, 7 fields): `bs_value_node`, `bs_ordered_node`,
  `tree_bones`, `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index` — all
  confirmed zero consumers outside `crates/nif/src/import/types.rs`, the parser,
  and `_tests.rs` files this sweep.
- **`ImportedSkin::body_part_flags`** (Skinning, #1659): raw-tier-parked,
  zero consumers confirmed — blocked on a dismemberment/armor-slot-hiding system.
- **Mesh/scene passthroughs**: `ImportedTextureEffect` /
  `import_nif_texture_effects` confirmed called only from
  `crates/nif/src/import/walk/tests.rs` — dead in production because
  content-absent, matching `nifal.md` §2. `NiSwitchNode` identity (active-index
  walked, discriminator unsurfaced), `BSInvMarker`, `BSBound` (loose-NIF path
  only, `cell_loader/nif_import_registry.rs:119-132` documents the asymmetry
  in-line) all confirmed unchanged.
- **Collision**: `BhkNPCollisionObject` (FO4+ Havok blob, separate decoder
  project; falls back to `synthesize_static_trimesh`) and `BhkPCollisionObject`
  phantoms (need a `TriggerVolume` ECS path; `is::<BhkNPCollisionObject>()` /
  `is::<BhkPCollisionObject>()` discriminators confirmed intact at
  `import/collision/mod.rs:80,117,120`) remain documented limitations, not leaks.
  `BhkPlaneShape` still the sole shape arm that returns `None` by design.
- **Particles**: size-over-life curve and multi-emitter attribution remain
  documented future work; `initial_color` still intentionally unapplied
  (confirmed via the 4 `initial_color: [1.0,1.0,1.0,1.0]` test-fixture comments
  in `systems/particle.rs` all still marked "must NOT win").
- **Animation**: per-light ambient colour channels remain matched-but-discarded
  (no per-light ambient slot); morph-weight channels write into
  `AnimatedMorphWeights` every frame (confirmed live at
  `systems/animation.rs:274,968`) with still no GPU morph-blend renderer
  consumer — captured, not a leak, per the spec's own framing.
- **Emissive scale**: resolved no-op (spec §4) — no new measurement invalidates
  this; not re-investigated in depth this sweep since no code in the emissive
  path changed in the delta.
- **Pre-existing open GitHub issues confirmed still accurate, not regressed, not
  duplicated by this report**: `#1856` (FO3 `WaterShaderProperty.water_shader_flags`
  captured on `MaterialInfo` but zero consumers past it — re-confirmed via grep,
  still true), `#2109`/`#2108` (Starfield BGEM/EFFECT_PALETTE merge gaps, renderer/
  Starfield-material scoped), `#2099`/`#2098` (Starfield secondary UV / bounding-
  sphere scale, `nif-parser`-scoped — parse-side per the audit-nif/audit-nifal
  scope split, not re-investigated here), `#1827` (Starfield `BSGeometry` empty
  bone data, self-documented "informational, out of FO4 scope").

---

## Test evidence

```
cargo test -p byroredux-nif --test translation_completeness -- --ignored
  → test cross_game_translation_completeness ... ok (all per-game fill-rate floors passed)

cargo test -p byroredux-nif -p byroredux-core
  → all passing (0 failed)
```

---

## Method notes

- Deduplication baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200`
  → 29 OPEN issues (saved to `/tmp/audit/issues.json`), cross-checked against every
  finding candidate before write-up.
- Delta since last dedicated NIFAL sweep (`AUDIT_NIFAL_2026-07-16.md`, HEAD then
  `c3e09bb5`): ~30 commits to `ca7a4e0e`, reviewed via `git log --oneline`; the
  fix commit for that sweep's two findings (`3e0129d8`) was inspected in full via
  `git show` rather than trusting the commit message, confirming both the
  word-boundary matcher and the `FurnitureMarkerKind` resolution landed as
  described and remain in place at HEAD.
- All claims in this report were verified directly against the live tree in this
  session (grep for call/construction sites, `Read` on entry-point files,
  `cargo test` runs) — no sub-agent delegation was used for the dimensions
  reported here, per this sweep's explicit instruction to avoid the stalling
  pattern seen in a prior attempt.
- Game data present for Oblivion / FO3 / FNV / Skyrim SE / FO4 / FO76 / Starfield
  (per `_audit-common.md` locations) — the `translation_completeness` harness
  exercises all seven.

Suggest: `/audit-publish docs/audits/AUDIT_NIFAL_2026-07-25.md` (note: with zero
new findings, there is nothing for `/audit-publish` to file as a GitHub issue this
sweep — it should report a clean pass).
