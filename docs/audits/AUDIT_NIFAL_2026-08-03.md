# NIFAL Audit — 2026-08-03

Deep audit of **NIFAL** (the NIF Abstraction Layer; spec: [nifal.md](../engine/nifal.md)),
run as a 9-dimension orchestrated sweep (3 concurrent Task agents per batch).
Repo HEAD: `1ae86f62`. Delta base: `db625997` (the prior sweep,
[AUDIT_NIFAL_2026-07-27.md](AUDIT_NIFAL_2026-07-27.md), 76 commits).

**Scope**: all 9 dimensions per `.claude/commands/audit-nifal/SKILL.md`.

**Method**: each dimension re-derived its verdict from the live tree — tracing
every carried-forward finding end-to-end through parser → import → translate →
render/consumer rather than trusting the prior report's prose or commit-message
titles, plus targeted `cargo test` runs (`byroredux-nif collision::`,
`byroredux-physics convert::`, the `#2206`/`#1440` regression filters) and git-log
archaeology across the 76-commit delta. No corpus-measurement probes were re-run
this cycle (the last sweep's numbers stand; nothing this sweep found required a
fresh corpus count). Working tree left unchanged.

---

## Executive Summary

**This sweep found 0 CRITICAL, 0 HIGH, 7 MEDIUM, 13 LOW** — a marked improvement
over 2026-07-27's 3 HIGH / 9 MEDIUM / 12 LOW. All three HIGH findings from that
sweep, and all four Collision regressions (2 HIGH + 2 MEDIUM), are **verified
fixed** by direct code trace (not commit-title trust):

- **NIFAL-D3-01** (Lights: `kind`/`direction`/`outer_angle` discarded, all lights
  spawned as point lights) — FIXED, `1a6296e2` / **#2205 CLOSED**, traced
  end-to-end from `walk_node_lights` through `LightSource` to `GpuLight.color_type.w`.
- **NIFAL-D6-01/02/03/04** (Collision: `/3` chunk-index divisor destroying 77% of
  Skyrim collision geometry; `BhkBoxShape` sign-flipped half-extents; void
  `TriMesh` suppressing the synthesized fallback; dropped strip-chunk residual) —
  all FIXED, **#2203/#2204/#2208 CLOSED**, `cargo test -p byroredux-nif
  collision::` (87 tests) and `-p byroredux-physics convert::` (14 tests) green.
- **NIFAL-D4-02** (Nodes: `billboard_mode` dropped on the cell-loader path) —
  FIXED, `4fd214aa` / **#2206 CLOSED**, all 3 new regression tests pass.
- **NIFAL-D2-02** (strip-walk panic on chunk overrun) — FIXED, clamped +
  `break`s, matches the sibling degrade-to-`None` discipline.

**This sweep's own method held up**: two genuine new MEDIUM findings surfaced
by re-deriving logic rather than assuming a prior fix was complete —
**MAT-D1-NEW-04** (six authored Skyrim+/FO4 `BSLightingShaderProperty` shading
scalars captured at import, never reach the canonical `Material`, missed by the
2026-07-27 sweep despite predating it) and a **new Collision MEDIUM**
(`finish_trimesh`'s index-bounds guard validates against the *merged* vertex
total, not each source sub-buffer's own range, so a corrupt multi-chunk/multi-ref
NIF can splice unrelated geometry into one triangle — no vanilla-content
trigger, corrupt/adversarial-input class only).

**Five of the seven MEDIUM findings are carryovers already tracked by open
GitHub issues** (#2210, #2211, #2212, #2213, #2214) — re-confirmed live and
unchanged, not re-derived from scratch. **The completeness harness
(`crates/nif/tests/translation_completeness.rs`) is untouched across the entire
76-commit delta** — it still measures the raw tier only and samples via
alphabetical truncation, so it still could not have caught any of this sweep's
translate-boundary findings, exactly as diagnosed last time.

### Systemic read: this sweep breaks, not repeats, the 2026-07-27 failure pattern

The prior sweep's core diagnosis was *"convergence audits were call-graph reads,
not output measurements."* This cycle's dimension agents traced fixes
concretely (ran the actual test filters, read the exact clamp/guard code, diffed
struct fields against iteration helpers field-by-field) rather than trusting
`nifal.md` prose or commit titles — and it paid off twice, catching
MAT-D1-NEW-04 and the `finish_trimesh` gap that a shallower pass would have
missed. What has **not** improved is the completeness harness itself, which
remains the wrong instrument for this discipline (see Dimension 9 findings
below) — every translate-boundary bug in both this sweep and the last was found
by manual code tracing, at the cost the harness exists to eliminate.

---

## Per-Category Tier Matrix

| Category | Boundary fn | single-boundary | no-fabrication | no-leak | no-render-time-fallback |
|---|---|---|---|---|---|
| Material | `material_translate.rs::translate_material` | PASS — exactly 2 callers (`nif_loader.rs:879`, `spawn.rs:1303`) | LOW open (`#2232` `ior` overload undocumented) | **1 new MEDIUM** (MAT-D1-NEW-04: 6 shading scalars dropped) + 2 LOW (no cross-crate `material_kind` pin; TLAS-exclusion asymmetry) | PASS (`static_meshes.rs` reads resolved fields directly) |
| Geometry / Transform | `coord.rs` + `rotation.rs::sanitize_rotation` (parse-time) + `transform.rs::compose_transforms` | LOW open (D2-01: 3 hand-copied de-strip implementations, orientation-equivalent today) | PASS | PASS | PASS |
| Skinning | `mesh/skin.rs` (#613 global remap) | N/A | PASS | PASS (spot-checked, unchanged) | PASS |
| Lights | `import/walk/mod.rs` → `LightKind` | N/A | MEDIUM open (D3-02: uncited `2048.0` fallback, `#2210`) | **FIXED** — D3-01 closed, `#2205` | PASS |
| Nodes | (by design, no single boundary — spec §2) | N/A (documented) | PASS | **FIXED** — D4-02 closed, `#2206`; 7 parked fields re-verified zero consumers | N/A |
| Particles | `systems/particle.rs::apply_emitter_overlays` | LOW open (D5-01: texture_path/src_blend/dst_blend still copy-pasted outside the boundary, byte-identical, latent) | PASS | PASS | PASS |
| Collision | `import/collision/shape.rs::resolve_shape_inner` | PASS (all 16 documented `bhk*Shape` arms translate; `dispatch_coverage_tests` green) | **1 new MEDIUM** (`finish_trimesh` cross-buffer index-bounds gap) | **FIXED** — D6-01/02/03/04 all closed (`#2203`/`#2204`/`#2208`) + D2-02 strip-panic fixed | N/A |
| Animation | `anim_convert.rs::convert_nif_clip` (+ undeclared 2nd: `asset_provider/animation.rs::convert_hkx_clip`) | **1 new LOW** (D7-NEW-01: `hkx`'s `convert_hkx_clip` undeclared in spec) + LOW (D7-03: duplicated discriminator tables) | MEDIUM open (D7-01: embedded-clip duration ignores transform channels, `#2211`) | PASS | PASS |
| Shader flags / texture sets | `shader_flags.rs` + `import/material/dedicated_shader.rs` | PASS (texture-role unification clean, `values()`/struct fields diffed field-by-field, no drift) | MEDIUM open (D8-01: BGSM alpha-test priority inversion, `#2212`) | PASS | **PASS** (0 `if game ==` across the entire shader tree, re-confirmed by direct grep) |
| *Completeness harness* | `crates/nif/tests/translation_completeness.rs` | — | — | — | FAIL as a verification instrument — D9-01 (`#2213`), D9-02 (`#2214`), D9-03 (unfiled) |

Doc-drift carried forward: `docs/engine/nifal.md` still cites the deleted
`ShaderFlags<'a>` type at line 253 (D8-02, half-fixed — both SKILL.md files were
corrected, the spec itself was not), still cites `import/collision.rs` instead
of the post-#1876 `import/collision/shape.rs` / `import/collision/mod.rs` split
(D6-06), and still lumps morph-weight channels in with genuinely-parked ambient
channels despite morph-weight now reaching a live `AnimatedMorphWeights` ECS
sink (D7-02). The Passthroughs table's furniture-marker row is half-stale in the
opposite direction — `BSFurnitureMarker` has been consumed since M41.5 but the
table still calls it unwalked (D4-03).

---

## Findings

### MEDIUM

#### MAT-D1-NEW-04 — Six authored Skyrim+/FO4 `BSLightingShaderProperty` shading scalars captured at import, then silently dropped at the canonical `Material` boundary

- **Severity**: MEDIUM
- **Dimension**: Material · **Tier Violated**: no-leak
- **Game Affected**: Skyrim LE/SE (BSVER 83–129: `lighting_effect_1/2`), FO4/FO76/Starfield (BSVER 130+: `subsurface_rolloff`, `rimlight_power`, `backlight_power`, `fresnel_power`)
- **Location**: captured at [`crates/nif/src/import/types.rs:484-491`](../../crates/nif/src/import/types.rs#L484-L491), copied at [`crates/nif/src/import/material/mod.rs:1239-1246`](../../crates/nif/src/import/material/mod.rs#L1239-L1246), also independently sourced from BGSM at [`byroredux/src/asset_provider/material.rs:1029`](../../byroredux/src/asset_provider/material.rs#L1029); never read by [`byroredux/src/material_translate.rs`](../../byroredux/src/material_translate.rs) (no field exists on `crates/core/src/ecs/components/material.rs`'s canonical `Material`, nor on `GpuMaterial`)
- **Status**: NEW (the gap itself predates the 2026-07-27 sweep — landed with `#1241`, 2026-05-23 — but no prior audit report flagged it)
- **Description**: `#1241`'s own regression-test doc comment states the goal
  explicitly: 8 `BSLightingShaderProperty` PBR scalars must land on
  `MaterialInfo` and propagate through every mesh extractor into
  `ImportedMesh`. That half (raw tier) is done and tested. Of the 8,
  `refraction_strength` and `grayscale_to_palette_scale` (explicitly documented
  as deferred at `triangle.frag:984`) complete the translate step. The other 6
  — `lighting_effect_1`, `lighting_effect_2`, `subsurface_rolloff`,
  `rimlight_power`, `backlight_power`, `fresnel_power` — dead-end on
  `ImportedMaterial` with zero consumers anywhere in `byroredux/src/`,
  `crates/debug-server/`, or `crates/debug-protocol/` (verified by repo-wide grep).
- **Evidence**: `grep -rn "lighting_effect_1\|subsurface_rolloff\|rimlight_power\|backlight_power" byroredux/src crates/debug-server crates/debug-protocol` returns nothing; `translate_material`'s `Material { ... }` literal has no corresponding fields.
- **Impact**: Skin/hair/cloth materials on Skyrim LE/SE and FO4/FO76/Starfield
  that author non-default rim-lighting, backlight, subsurface-rolloff, or
  Fresnel-exponent values (a routine Bethesda skin-shader authoring pattern)
  render with the engine's fixed Disney BSDF response instead of the author's
  tuned curve. Shading-fidelity gap only (nothing crashes or renders as the
  wrong *kind*), so it stays below HIGH — but it is genuine authored-data loss,
  and `nifal.md`'s "Materials — converged" verdict slightly overstates completeness.
- **Suggested Fix**: Add the missing fields to the canonical `Material`
  (mirroring how `translucency_*` was added in `#1147`) and to `GpuMaterial`,
  copy them in `translate_material`, and wire a `triangle.frag` consumer — or
  at minimum land them on `Material` and note in `nifal.md` as "captured, not
  yet shaded," matching the existing `grayscale_to_palette_scale` precedent.

#### NIFAL-D6-07 — `finish_trimesh`'s index-bounds guard validates against the merged total, not each source sub-buffer's own range, so a corrupt NIF can splice unrelated geometry into one triangle

- **Severity**: MEDIUM
- **Dimension**: Collision · **Tier Violated**: no-fabrication (defense-in-depth gap in the boundary the D6-03 fix itself introduced)
- **Game Affected**: Skyrim LE/SE/DLC (`resolve_compressed_mesh`, multi-chunk merge) and any game with a multi-`data_ref` `bhkNiTriStripsShape`/`BhkMeshShape` (`resolve_tri_strips_data_refs`) — theoretical on corrupt/truncated NIFs only, not observed on vanilla content
- **Location**: [`crates/nif/src/import/collision/shape.rs:591-605`](../../crates/nif/src/import/collision/shape.rs#L591-L605) (`finish_trimesh`), consumed by `resolve_compressed_mesh` (`shape.rs:484-582`) and `resolve_tri_strips_data_refs` (`shape.rs:361-407`)
- **Status**: NEW
- **Description**: `resolve_compressed_mesh` merges `big_verts` then each
  chunk's quantized vertices into one flat `all_verts`/`all_indices` pair
  before calling `finish_trimesh`; `resolve_tri_strips_data_refs` does the same
  across a shape's `data_refs`. `finish_trimesh`'s only bounds check validates
  each index against the *final merged* vertex count, not the count of the
  specific sub-buffer it was decoded from. A corrupt/truncated NIF whose
  `CmsBigTri.v1/v2/v3` exceeds `data.big_verts.len()` but is still less than
  the eventual `all_verts.len()` (because later chunks pushed enough vertices
  to cover the gap) passes the guard unchanged — it silently indexes into a
  *different* chunk's vertex data, connecting two unrelated pieces of geometry
  instead of being dropped as corrupt. This is exactly the failure mode
  `finish_trimesh`'s own doc comment claims to prevent ("a corrupt tail cannot
  poison otherwise usable authored geometry") — it prevents
  degenerate/globally-out-of-range poisoning, but not cross-buffer splicing.
- **Evidence**:
  ```rust
  let vertex_count = vertices.len() as u32;   // the FINAL merged total
  indices.retain(|[a, b, c]| {
      a != b && b != c && a != c && *a < vertex_count && *b < vertex_count && *c < vertex_count
  });
  ```
- **Impact**: No known vanilla-content trigger — every real archive's
  per-buffer indices are correctly local by construction. Corrupt/adversarial-NIF
  robustness gap in the same class the surrounding code already defends
  against (`#1409`, `#1779`, `#1385`): the next malformed or hand-edited NIF
  that trips it gets a garbage triangle silently merged into a real static's
  collider instead of the intended graceful `None` → synthesized-fallback path.
- **Suggested Fix**: Track each sub-buffer's own vertex-count offset alongside
  `base`, and validate `*a < base + local_count` (or, simpler, validate/retain
  each source's own index slice before pushing it into `all_indices`), so
  `finish_trimesh`'s existing global check becomes a pure belt-and-suspenders
  pass rather than the only one.

#### NIFAL-D3-02 — the `2048.0` no-attenuation fallback is still an uncited constant that *is* the shipped behaviour

- **Severity**: MEDIUM
- **Dimension**: Skinning/Lights · **Tier Violated**: no-fabrication
- **Game Affected**: FNV (82/82 spawnable point lights measured), FO3, FO4, Starfield
- **Location**: [`crates/nif/src/import/walk/mod.rs:1661-1685`](../../crates/nif/src/import/walk/mod.rs#L1661-L1685) (function shifted ~24 lines since the 2026-07-27 report; byte-identical constant)
- **Status**: Existing: **#2210** (OPEN) — re-confirmed unchanged; `git log -p` on this range traces the constant and its comment back unmodified to `#156` (2026-04-07). The `#2205` lights fix touched only `LightSource`/`spawn_nif_lights`/`render/lights.rs`, never this function.
- **Description**: `2048.0` has no citation (no Gamebryo 2.3 `NiLight`
  default, no measured derivation), unlike its sibling `LIGHT_RANGE_EXTENSION`
  (`byroredux/src/render/lights.rs:55`), which cites OpenMW's
  `lighting_util.glsl` directly. Per the no-guessing policy, an invented magic
  number that *is* the shipped behavior for 82/82 measured FNV point lights is
  exactly the fabrication the policy exists to catch.
- **Impact**: Every affected light's cull radius is fabricated rather than
  sourced; `spawn.rs:559-564` partially mitigates for cell-placed lights by
  preferring the ESM LIGH authored radius when present, but the NIF-direct /
  no-ESM-radius case remains a genuine no-fabrication violation.
- **Suggested Fix**: unchanged from the original report — cite a source for
  `2048.0` (a legacy `NiLight` default, or a measured derivation) or replace it.

#### NIFAL-D7-01 — embedded-clip duration still ignores transform-channel key times

- **Severity**: MEDIUM
- **Dimension**: Animation · **Tier Violated**: no-fabrication
- **Game Affected**: All (Oblivion → Starfield) — any loose NIF with an inline transform/keyframe controller and no float/color/bool/texture-flip sibling channel
- **Location**: [`crates/nif/src/anim/entry.rs:553-579`](../../crates/nif/src/anim/entry.rs#L553-L579) (`import_embedded_animations`, the `max_time` scan)
- **Status**: Existing: **#2211** (OPEN) — re-confirmed unfixed at HEAD
- **Description**: `import_embedded_animations`'s duration computation scans
  float/color/bool/texture-flip channels for the maximum key time but never
  scans `clip.channels` (the `TransformChannel` map populated by the `#1440`
  inline-transform-controller arm). A mesh whose only embedded controller is a
  transform/keyframe controller — the documented use case for `#1440`
  (animated scenery: fans, doors, lifts, swinging signs) — leaves `max_time` at
  `0.0` and falls through to the fabricated `1.0` fallback, silently truncating
  any authored animation longer than 1 second. The `#1440` regression test
  (`crates/nif/src/anim/tests/channel.rs:659-766`) passes for the wrong reason:
  its last transform key happens to sit at `t=1.0`, so it gives no signal that
  the scan is missing.
- **Impact**: Any loose-NIF animated-scenery mesh whose only controller is an
  inline transform/keyframe controller longer than 1s gets its authored
  animation truncated to a 1-second loop.
- **Suggested Fix**: Add a loop over `clip.channels` in the `max_time` scan;
  strengthen the `#1440` fixture's last key time (e.g. `4.0`) so the test can
  no longer pass by accident.

#### NIFAL-D8-01 — synthesized FO4 alpha-test threshold (128/255) still blocks the authored BGSM value

- **Severity**: MEDIUM
- **Dimension**: Shader-flags/Effects · **Tier Violated**: no-fabrication
- **Game Affected**: Fallout 4 (any BSVER ≥ 130 content pairing a NIF F4SF2 bit-25 with a BGSM)
- **Location**: [`byroredux/src/asset_provider/material.rs:1063-1066`](../../byroredux/src/asset_provider/material.rs#L1063-L1066) (shifted from `:1038-1042` by unrelated refactor churn; same bug)
- **Status**: Existing: **#2212** (OPEN) — re-confirmed unfixed; `grep -n "set_alpha_test"` returns zero hits
- **Description**: The BGSM merge still gates on `!material.alpha_test`, which
  is not chain-local — it arrives pre-set `true` from the NIF F4SF2 bit-25 path
  whenever `alpha_threshold` was still `0.0`. When both the NIF flag and an
  authored BGSM `alpha_test_ref != 128` are present, the guard is `false` and
  the authored value never lands — inverting the priority `#1592`'s own
  comment states (NIF flag should be strictly lower-priority than the BGSM
  merge, which should OR-upgrade). The BGEM sibling overwrites unconditionally,
  confirming BGSM is the sole outlier.
- **Impact**: FO4 foliage/grate/fence materials authoring a non-128 alpha-test
  cutoff render with the wrong threshold when the NIF also carries the F4SF2
  bit. Reachability on vanilla FO4 remains unmeasured.
- **Suggested Fix**: add a `set_alpha_test` chain-local sentinel next to
  `set_alpha` so the authored BGSM value always wins.

#### NIFAL-D9-01 — completeness-harness alphabetical truncation still confounds cross-game comparison by content class

- **Severity**: MEDIUM
- **Dimension**: Completeness · **Tier Violated**: (harness gap — no production tier)
- **Game Affected**: all seven
- **Location**: `crates/nif/tests/translation_completeness.rs` (`files.sort(); truncate(200)`)
- **Status**: Existing: **#2213** (OPEN) — file is byte-for-byte unchanged since before the 76-commit delta (`git log` shows no commits touching it since `05d68926`, which predates `db625997`)
- **Description**: unchanged from 2026-07-27 — the sample confound (Skyrim
  100% `meshes\actors\`, Oblivion 100% `meshes\architecture\`) means large
  fill-rate divergences reflect content class, not game, requiring manual
  disambiguation the harness should make unnecessary.
- **Suggested Fix**: stratified sampling (round-robin across top-level directories) before truncation.

#### NIFAL-D9-02 — completeness harness still measures the raw tier only; `translate_material` is never called

- **Severity**: MEDIUM
- **Dimension**: Completeness · **Tier Violated**: (harness gap)
- **Game Affected**: all seven
- **Location**: `crates/nif/tests/translation_completeness.rs` (`MaterialStats::record` takes `&ImportedMesh`)
- **Status**: Existing: **#2214** (OPEN) — unchanged
- **Description**: unchanged from 2026-07-27. The harness covers 2 of 9 NIFAL
  categories, import-half only — mechanically unable to have caught
  MAT-D1-NEW-04 (this cycle) or any of the six translate-boundary bugs the
  prior sweep found. This sweep's own findings (MAT-D1-NEW-04, NIFAL-D6-07)
  were both caught by manual code tracing, not the harness — the gap the
  harness is supposed to close is still being closed by hand.
- **Suggested Fix**: add a canonical-tier sibling harness in `byroredux/tests/`
  that drives `translate_material` (and, as they gain boundaries, the other
  categories) and asserts on canonical-tier output.

---

### LOW

| ID | Dimension | Tier | Summary |
|---|---|---|---|
| **MAT-D1-NEW-01** | Material | single-boundary | No cross-crate assert pins the NIF importer's `material_kind` 101/102/103 literals to `byroredux_renderer::MATERIAL_KIND_*`; `crates/nif` has no dependency on `byroredux-renderer` so the only asserts are literal-to-literal inside the producing crate. A future renumber would keep `cargo test -p byroredux-nif` green while silently dropping every effect/no-lighting/fire-haze surface to the default-lit arm. No GitHub issue found. Fix: two-line cross-crate assert in `byroredux/src/material_translate.rs`'s test module. |
| **MAT-D1-NEW-02** | Material | no-render-time-fallback (defense-in-depth) | `draw_command_eligible_for_tlas` (`crates/renderer/src/vulkan/acceleration/predicates.rs:437-441`) excludes `MATERIAL_KIND_EFFECT_SHADER` from the TLAS but not `MATERIAL_KIND_FIRE_REFRACTION`, despite the latter's own constant doc requiring the same exclusion. No live defect (no producer currently sets `in_tlas` for fire-refraction draws) — defense-in-depth gap only. No GitHub issue found. |
| **MAT-D1-NEW-03** | Material | no-leak (doc-only) | Canonical `Material::ior` carries a discriminated overload (0-1 distortion strength when `kind == 103`, vs Fresnel IOR otherwise) documented only at the producer/shader, not on the canonical field's own doc comment. **Existing: #2232** (filed from a different audit dimension, covers the identical gap). |
| **NIFAL-D2-01** | Geometry | single-boundary | The `#2193` de-strip dedup is incomplete: `resolve_tri_strips_data_refs` was unified to call `NiTriStripsData::to_triangles()` directly, but `resolve_compressed_mesh`'s chunk-strip walk and `NiSkinPartition`'s inline destrip (`crates/nif/src/blocks/skin.rs:300-318`) remain separate hand-copies. All three verified orientation-equivalent today — latent, not live. Notably, `resolve_compressed_mesh`'s copy *did* diverge to the wrong convention until an unrelated bug-fix pass (`3b9227341`) silently corrected it — small evidence the drift risk is real. No GitHub issue found. |
| **NIFAL-D4-03** | Nodes | (doc) | `docs/engine/nifal.md:309`'s passthrough table still calls `BSFurnitureMarker` "parsed, not walked into `Imported*`" — stale since `#2010`/M41.5 Phase B; furniture markers are consumed by `systems/sandbox.rs` via `furniture_component()`. `BSInvMarker` (the row's other half) genuinely is still passthrough-only. Fix: split the row in two. No GitHub issue found. |
| **NIFAL-D5-01** | Particles | single-boundary | `texture_path`/`src_blend`/`dst_blend` are authored `NiPSysEmitter` overrides folded by an identical 9-line block copy-pasted at `byroredux/src/scene/nif_loader.rs:520-528` and `byroredux/src/cell_loader/spawn.rs:649-657`, outside `apply_emitter_overlays`. The `8a15b064` "streamline particle emitter selection" refactor touched only the preset-selection heuristic, not this block — still byte-identical, still latent. No GitHub issue found. |
| **NIFAL-D6-06** | Collision | (doc) | `docs/engine/nifal.md:202,216`, `nif-parser.md`, and `architecture.md` still cite `import/collision.rs::resolve_shape` — the function lives in `import/collision/shape.rs` post-#1876 split, and the limitations table is at the top of `import/collision/mod.rs`. Doc-only, no behavior impact. |
| **NIFAL-D6-08** | Collision | parked-not-leak | `NiTriStripsData.normals` (per-vertex, parsed) is never cross-checked by the collision path (`resolve_tri_strips_data_refs`), unlike the sibling `packed_triangle_winding` check `c4481c78` added for `BhkPackedNiTriStripsShape`. Explicitly **not** a fix for open issue #2193 — that issue's own investigation already hand-checked this for the actual repro entity and found zero winding/normal disagreements across all 913 triangles. Documented asymmetry only. |
| **NIFAL-D7-02** | Animation | (doc) | `docs/engine/nifal.md:244-245` still lumps morph-weight channels in with genuinely-parked per-light ambient channels ("intentionally parked... no renderer consumer yet"). Since `a8b0cf64`, morph-weight channels reach a live `AnimatedMorphWeights` ECS sink every frame (confirmed via `sink_lifecycle_end_to_end_tests`) — they only lack a GPU/mesh-vertex-blend consumer (tracked separately by `#2221`). Ambient genuinely is still dropped. The doc conflates two different states. |
| **NIFAL-D7-03** | Animation | single-boundary (secondary) | The `operation`→`FloatTarget` and `target_color`→`ColorTarget` discriminator tables are duplicated between the KF arm (`crates/nif/src/anim/channel.rs:296-301,383-388`) and the embedded arm (`crates/nif/src/anim/entry.rs:358-365,378-383`). Byte-identical today. Not a duplicate of `#2067` (a different duplication — `NiSingleInterpController` prologue). |
| **NIFAL-D7-NEW-01** | Animation | single-boundary (spec gap) | The new `hkx` crate's `convert_hkx_clip` (`byroredux/src/asset_provider/animation.rs:165-276`) is a second, legitimate production boundary constructing the canonical `AnimationClip` (from Havok 2010 packfile data, not NIF — cannot route through `convert_nif_clip`). It reuses the canonical type correctly (no parallel struct) but is undeclared in `nifal.md`'s Animation section. Also synthesizes two text-key events (`ExitCartEnd`/`IdleFurnitureExit`) not present in the source clip — deliberate, well-commented, but an uncited fabrication in the spec's framing. Fix: add a paragraph naming this as the second declared boundary. |
| **NIFAL-D8-02** | Shader-flags | (doc, half-fixed) | `docs/engine/nifal.md:253` still cites the deleted `ShaderFlags<'a>` typed view (removed by `#1897`) and calls the bit-collision guards "compile-time equivalence asserts" when they are `#[test]` runtime asserts. `.claude/commands/audit-nifal/SKILL.md` and `audit-nif/SKILL.md` were already corrected — only the authoritative spec doc itself lags. |
| **NIFAL-D9-03** | Completeness | (harness gap) | Fill-rate floors in `translation_completeness.rs` carry ~33pp median slack; `metO`/`rghO` pinned at 100% with no assertion; `normal_map` asserted for no game. No GitHub issue found — recommend filing if this report is published. |

---

## Documented-limitation ledger (parked-not-leak / no-action — do NOT re-report next sweep)

Re-verified against HEAD `1ae86f62`:

- **Regressions confirmed fixed, do not reopen**: NIFAL-D3-01 (lights kind/direction/cone,
  `#2205`), NIFAL-D4-02 (billboard mode on cell-loader path, `#2206`),
  NIFAL-D6-01/02/03/04 (compressed-mesh `/3` divisor, box-shape sign flip, void-TriMesh
  fallback suppression, dropped strip residual — `#2203`/`#2204`/`#2208`), NIFAL-D2-02
  (strip-walk overrun panic). All traced through current code, not just commit titles;
  relevant test suites green.
- **NIFAL-D6-05**: `CmsChunk.transform_index`/`chunk_transforms` parsed but never read in
  `resolve_compressed_mesh`; re-confirmed still a genuine no-op (no code changed since the
  prior 12,069-entry all-identity measurement). Not re-measured fresh this cycle since
  nothing in the relevant code changed.
- **Node/mesh parked fields** — all 7 (`bs_value_node`, `bs_ordered_node`, `tree_bones`,
  `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`) re-measured at **zero**
  canonical consumers this cycle; every non-test hit is producer-side.
  `ImportedTextureEffect` (dead extractor, content-absent), `NiSwitchNode` identity
  (walked via active-index only), `bs_bound` (loose-path-only) re-verified as documented
  and unchanged.
- **Collision documented limitations**: `BhkPlaneShape` → `None` (`#1334`, documented at
  its arm); `BhkNPCollisionObject` (FO4/FO76/Starfield `BhkSystemBinary` blob, falls back
  to `synthesize_static_trimesh`); `BhkPCollisionObject`/`BhkSimpleShapePhantom`/
  `BhkAabbPhantom` (trigger volumes, need a `TriggerVolume` ECS path). `hkMotionType` byte
  → canonical `MotionType` collapse re-verified correct, zero-mass-Dynamic→Static
  reclassification correctly gated on `mass <= 0.0`.
- **Particles**: `initial_color` intentionally unapplied; size-over-life curve documented
  future work; multi-emitter scene-first attribution is `#1402`, closed as a documented
  deferral.
- **Animation**: per-light ambient colour channels genuinely parked (dropped, zero
  consumer); `AnimationTextKeyEvents` produced/registered/drained but no system reads the
  labels (footsteps are distance-driven); morph-weight channels are **no longer parked** —
  see NIFAL-D7-02 above (doc still needs updating, code is fine).
- **Material / shader flags**: emissive scale is a measured no-op (`nifal.md` §4); glass
  classified once, alpha-aware, correctly ordered after PBR resolve; `material_kind: u32`
  deliberately kept as the GPU dispatch contract; fire-refraction (`kind 103`) unreachable
  on FO76/Starfield by format absence, not a leak.
- **Skinning**: `body_part_flags` parked, zero consumers (unchanged, spot-checked only).
- **Shader-flags texture-role unification** (2026-07-27 `1d94eb24`/`05d68926` refactor):
  `MaterialTextureSet<T>`'s 18 roles are exactly mirrored by `values()`/`map_ref`, field by
  field, with two exhaustive-struct-literal tests now guarding future drift — verified
  clean this cycle, the primary risk this refactor introduced did not materialize.
- **`crates/scripting/src/` cinematic/quest additions** (SCEN/PACK, `PlayIdle`/
  `SetVehicle`/`SetMotionType`/`ExitCart`, cart-exit root motion): traced as pure
  *consumers* of the existing `AnimationClip`/text-key-event pipeline, not a second NIFAL
  producer — no new category warranted here.
- **Pre-existing open issues confirmed still accurate, not regressed, not duplicated**:
  `#2193` (Oblivion `is_grounded`/inverted contact normal — the `c4481c78` winding fix
  legitimately helps other `BhkPackedNiTriStripsShape` content but does not touch the
  `bhkNiTriStripsShape`-derived resolver arm implicated in `#2193`'s own repro entity;
  correctly remains OPEN), `#2221` (morph-weight GPU/mesh consumer), `#2232` (`Material::ior`
  triple-meaning doc gap).

## Method note

No corpus-measurement probes were re-run this cycle. All findings and fix
verifications came from direct reading of current source against the prior
sweep's citations, targeted `cargo test` filters (`byroredux-nif collision::`,
`byroredux-nif billboard`, `byroredux-physics convert::`), and `git log`/`git show`
archaeology across the 76-commit delta (`db625997..1ae86f62`). Where the prior
sweep's corpus numbers are still the operative evidence (e.g. NIFAL-D3-02's
82/82 FNV measurement, NIFAL-D6-01's Skyrim chunk counts), this report cites
them as unchanged rather than re-deriving them, since no code in those paths
changed.
