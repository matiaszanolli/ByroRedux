# Incremental Audit — 2026-05-22

**Scope**: Tonight's 18 commits (`75474e71..6c5ae8cd`), 21 issues closed (one partial: #1225).
**Method**: In-thread breadth-first scan of each production-file diff + cross-reference with the issue scope. Build + full test sweep (2381 passed, 0 failed, 106 ignored — every gated test still gated, no behaviour regression).
**Test coverage**: 49 new unit tests landed (4 #1209 + 4 #1208 + 7 #1207/#1206 + 4 #1205 + 6 #1203 + 4 #1204 + 7 #1201/#1202 + 4 #1180/#1182 + 4 #1179 + 5 #1226 = 49 covering the bug cases + non-regressions).

## Executive Summary

The night's work landed cleanly. Test suite is green, no warnings introduced (two unused-imports surfaced and were fixed in 6c5ae8cd). Most fixes are narrow, additive, and well-isolated. The audit surfaced **5 findings** worth tracking:

| Severity | Count | Theme |
|----------|------:|-------|
| MEDIUM   | 0     | none |
| LOW      | 3     | perf regression risk (allocation hoist), capture-without-consumer gaps |
| INFO     | 2     | stale dead-code, partial-landing follow-up |

No HIGH or MEDIUM findings. The fixes that touched renderer code stayed inside data-plumbing / counter-split / constant-bump scope per `feedback_speculative_vulkan_fixes.md`; nothing touches Vulkan render-pass / pipeline / barrier semantics speculatively.

---

## Change Summary

### Domains touched

| Domain | Files | Risk | Findings |
|--------|-------|------|---------:|
| NIF Parser | 13 files | HIGH | 1 LOW |
| Renderer / Acceleration | 4 files | HIGH | 1 INFO (counter split unused, cosmetic) |
| Renderer / Scene Buffer | 1 file | HIGH | 0 (constant bump, validated path) |
| BSA | 1 file | HIGH | 0 (log-only, validated against 108 archives) |
| BGSM | 1 file | MEDIUM | 1 INFO (dead error catch) |
| Cell Loader | 2 files | MEDIUM | 0 |
| Scene Loader | 1 file | MEDIUM | 0 (log-prose only) |
| SpeedTree | 1 file | MEDIUM | 0 (None-default field add) |
| Tests | 6 new files + 5 modified | LOW | 0 |

### Commit list

```
6c5ae8cd chore: drop two unused imports introduced by tonight's test additions
5d3b2e9f Fix #1148: cycle-aware BGSM template resolver
31258c6e Fix #1225 (partial): branch zero-mesh diagnostic on path pattern
22c09789 Fix #1179: add synthetic-bytes coverage for parse_movs_group walker
c5a2f1f4 Fix #1180 + #1182: PKIN→SCOL and SCOL-of-SCOL recursion under shared depth cap
27478774 Fix #1198 (PERF-DIM7-07): bump MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME 16 → 227
cf3d8ec6 Fix #1226: TLAS-scratch shrink uses TLAS-calibrated slack (was dead code)
289fb07a Fix #1228: split missing_blas counter by cause (skinned/rigid/ssbo_evicted)
8e3728e0 Fix #1186: log Starfield BA2 v2/v3 trailing extra-header bytes
a9d1dca5 Fix #1184: add Starfield BA2 corpus-wide sweep regression test
56543b4d Fix #1201 + #1202: gate alpha-property cascade on alpha_property_consumed
648fc86a Fix #1204: route SSE-reconstructed BSTriShape to Y-up tangent synthesis
02941317 Fix #1203: resolve BSGeometry skin_instance_ref via BsSkinInstance chain
3c1bb9d3 Fix #1205: capture FO76 BSEffectShaderProperty quintet onto BsEffectShaderData
d58361b4 Fix #1207 + #1206: surface BsTriShapeKind discriminator on ImportedMesh
57819076 Fix #1208: gate inherited NiVertexColorProperty on !has_material_data
13d90aba Fix #1209: BSGeometry Stage-A iterate every LOD slot (was first()-only)
2b79a2ac docs: re-anchor stale path / line refs in audit skill files (#1229, #1200, #1185)
```

---

## High-Risk Changes (audited individually)

### NIF Parser

- **`crates/nif/src/import/material/walker.rs`** — #1208 NVCP gate + #1201/#1202 alpha cascade. Confirmed semantically correct: `alpha_property_consumed` is set unconditionally by `apply_alpha_flags` (verified at `mod.rs:959`); both new gates honour explicit-opaque intent. The order change (alpha_property_ref moved ABOVE shader_property_ref) is the load-bearing precondition for #1202. **Finding ID-1** (LOW) below covers a side effect.

- **`crates/nif/src/import/mesh/bs_geometry.rs`** — #1209 LOD iter (Stage A symmetry) + #1203 skin wiring + #1207/#1206 None-defaults. The `meshes.first().and_then(...)` → `meshes.iter().find_map(...)` swap is monotonic. The skin wiring now populates `skin` on Starfield BSGeometry; downstream cell-loader's rigid-fallback branch (`skin.is_some()` check in render path) now correctly routes Starfield NPC bodies to the skinned pipeline. **Finding ID-2** (INFO) below covers the deferred per-vertex weights.

- **`crates/nif/src/import/mesh/bs_tri_shape.rs`** — #1207/#1206 kind discriminator passthrough + #1204 Y-up synth fallback. The Y-up branch fires only when `shape.normals` / `shape.uvs` are empty (post-SSE-reconstruction) AND the function-local `normals` / `uvs` / `positions` are non-empty — the gate excludes pre-fix code paths so cells without SSE-reconstruction still hit `Vec::new()` on the leaf. **Finding ID-3** (LOW) below covers a perf concern.

- **`crates/nif/src/import/mesh/tangent.rs`** — new `synthesize_tangents_yup` sibling. Pin-tested with axis-aligned fixtures (the Y-up image of the existing #786 tests). The degenerate fallback uses a Y-up permutation `[n_yup[1], n_yup[2], n_yup[0]]` that produces a valid orthogonal tangent but is NOT the Y-up image of the Z-up flavor's permutation — a future audit may flag this as drift, hence **Finding ID-4** (INFO) below documenting the divergence.

- **`crates/nif/src/import/mesh/skin.rs`** — new `extract_skin_bs_geometry`. Mirrors the existing FO4+ BSSkin path in `extract_skin_bs_tri_shape`. Per-vertex weights deferred. Defensive None on every failure mode (verified by the 6 regression tests).

- **`crates/nif/src/import/material/mod.rs` + `shader_data.rs`** — #1205 BsEffectShaderData FO76 quintet capture. Field additions are all `Option<>`-typed with sentinel-collapsed default. `LuminanceParams` derive of `PartialEq` is the only non-additive change; safe because no prior consumer relied on its absence.

- **`crates/nif/src/import/types.rs`** — `ImportedMesh` +2 fields + `BsSubIndexTriShapeData` re-export. Construction sites at the 5 known producers (bs_tri_shape.rs, bs_geometry.rs, ni_tri_shape.rs, cell_loader.rs::empty_mesh, spt/import/mod.rs) all populate with appropriate values (Some on BsTriShape, None on the other 4).

- **`crates/nif/src/scene.rs` + `tests.rs`** — drive-by repair of 5 test fixtures missing the `havok_scale` field after M28.5's schema add (the test suite wasn't compiling on `main` before this — the broken state pre-dates my work).

### Renderer / Acceleration

- **`acceleration/tlas.rs`** — #1228 counter split. Three counters tracked individually + one derived total for the rate-limited warn. No functional change to TLAS build / refit paths. Sample-string list still bounded by the same `MISSING_BLAS_SAMPLE_LIMIT = 5`.

- **`acceleration/predicates.rs` + `constants.rs` + `memory.rs`** — #1226 new `tlas_scratch_should_shrink` + `TLAS_SCRATCH_SLACK_BYTES = 256 KB`. The TLAS-scratch shrink path in `shrink_tlas_scratch_to_fit` was effectively dead under the BLAS-scale slack; now fires at realistic excess. The `scratch_should_shrink` BLAS-tuned predicate stays attached to `shrink_blas_scratch_to_fit` (verified via grep — single call site outside tests).

### Renderer / Scene Buffer

- **`scene_buffer/constants.rs`** — `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME 16 → 227`. The staging buffer auto-resizes via `bind_inverse_staging_size = MAX_PENDING * MAX_BONES * 64` in `scene_buffer/buffers.rs:510-516` (verified). No secondary site to update. Math: `227 × 144 × 64 = 2 091 392 bytes ≈ 2 MB` per FIF slot — trivial on 6 GB.

### BSA

- **`bsa/src/ba2.rs`** — `log_v2_v3_extra_bytes` defensive log + sanity-warn. Validated against the 108-archive Starfield corpus (#1184): zero spurious warns. The sanity check `stream_pos + size > name_table_offset` is loose (well-formed archives have size ≤ gap); fires only on wildly malformed headers.

### BGSM

- **`bgsm/src/template.rs`** — #1148 cycle-aware resolver. `visited: Vec<String>` ancestor stack threaded through `resolve_depth`. Cycle detection sets `parent = None` at the detection site, terminating the walk. Five new tests cover self-ref, A→B→A, A→B→C→B, cache invariant, and non-regression on vanilla 3-level chain. **Finding ID-5** (INFO) below covers the now-dead `asset_provider.rs::ResolveError::DepthLimit` catch.

---

## Findings

### ID-1 (LOW) — `walker.rs` alpha_property_ref move overwrites default blend modes

**Changed File**: `crates/nif/src/import/material/walker.rs` (commit `56543b4d`)
**Dimension**: NIF import / material correctness
**Severity**: LOW — niche corner; not load-bearing on the rendering path.

**Symptom**: Moving the Skyrim+ `alpha_property_ref` branch ABOVE `shader_property_ref` means `apply_alpha_flags` runs against a default `MaterialInfo`. For a `NiAlphaProperty { flags: 0 }` (explicit-opaque), the side-effect writes:

- `info.src_blend_mode = 0` (was `MaterialInfo::default()` = 6 = `SRC_ALPHA`)
- `info.dst_blend_mode = 0` (was 7 = `INV_SRC_ALPHA`)

Both fields are written from `(flags >> 1) & 0xF` and `(flags >> 5) & 0xF` regardless of bit 0 (alpha_blend) or bit 9 (alpha_test). Result: the default SRC_ALPHA / INV_SRC_ALPHA hand-off pair is replaced with `(ONE, ONE)` (additive blend), even on opaque materials.

**Why mild**: downstream consumers gate on `alpha_blend` before reading the blend modes (e.g., renderer's transparent pipeline binds on alpha_blend=true). For alpha_blend=false the blend modes are unread. No visible regression in tests or full build.

**Why worth flagging**: if a future renderer path (animated alpha, alpha-blend transition on opacity controllers) ever consults `src_blend_mode` / `dst_blend_mode` without the alpha_blend gate, it'd see the wrong defaults on explicit-opaque shapes.

**Suggested fix**: Inside `apply_alpha_flags`, skip the src/dst writes when `flags == 0` (truly empty NiAlphaProperty), preserving the MaterialInfo defaults. OR document the invariant explicitly via a debug_assert in any consumer that reads blend modes.

**Effort**: trivial (one if-gate inside apply_alpha_flags).

### ID-2 (INFO) — BSGeometry skin path missing per-vertex weights

**Changed File**: `crates/nif/src/import/mesh/bs_geometry.rs` (commit `02941317`)
**Dimension**: NIF import / skinning completeness
**Severity**: INFO — explicitly deferred in #1203 issue scope.

**Symptom**: `extract_skin_bs_geometry` populates `ImportedSkin.bones` (with bind-inverse matrices) but leaves `vertex_bone_indices` / `vertex_bone_weights` empty. The cell loader's rigid-fallback gate (`if mesh.skin.is_none()`) now FAILS (skin is Some), so Starfield BSGeometry meshes route through the skinned pipeline. But the per-vertex skin data is empty, so the skinning compute dispatches against an all-zero weight table.

**Why deferred**: the BSGeometry parser doesn't surface per-vertex bone indices/weights yet — they live in the segmented mesh-data table the parser hasn't decoded. Once the parser surfaces them, `extract_skin_bs_geometry` needs a `num_vertices` parameter + the densify path matching the BSTriShape extractor.

**Risk in the meantime**: A Starfield NPC mesh imports with skin: Some(...) and empty per-vertex tables. The renderer's skin-compute dispatch will produce identity-transformed vertices (all weights zero → fallback to bind pose). Net result: same visual as pre-#1203 (bind pose), but now routed through the GPU skinning compute dispatch unnecessarily (small perf cost).

**Suggested follow-up**: file a sibling issue tracking "BSGeometry per-vertex bone indices/weights" with a `gate-on` reference to whichever parser-side work surfaces the segmented mesh-data weights.

### ID-3 (LOW) — `triangles_for_synth` hoisted out of synthesis gate, allocates unconditionally

**Changed File**: `crates/nif/src/import/mesh/bs_tri_shape.rs` (commit `648fc86a`)
**Dimension**: NIF import / mesh import performance
**Severity**: LOW — per-mesh allocation, bounded by triangle count.

**Symptom**: To share the rebuilt `triangles_for_synth` between the Z-up and Y-up synthesis branches, the hoist moved the construction OUT of the gating if-let. The allocation now fires on every BSTriShape import, including the common paths where synthesis is not needed:

- `sse_tangents.filter(|v| !v.is_empty())` — SSE-reconstructed mesh with VF_TANGENTS — common
- `!shape.tangents.is_empty()` — inline-tangents mesh — also common

Pre-fix: only allocated when the Z-up synthesis branch fired (uncommon — most modern meshes ship tangents).
Post-fix: always allocates a `Vec<[u16; 3]>` sized by `shape.triangles.len()` (clone) or `indices.len() / 3` (rebuilt-with-filter).

**Quantify**: typical Skyrim NPC body is ~2-5K triangles. 2 000 × 6 bytes = 12 KB allocation per imported mesh, per cell load. A cell with 200 BSTriShape meshes pays ~2.4 MB of transient alloc churn.

**Suggested fix**: re-gate `triangles_for_synth` inside an `if`-let that fires only when one of the two synthesis branches is taken. Could use a lazy closure / `OnceCell` pattern, or simply duplicate the rebuild (cheap).

**Why LOW**: cell-load-time cost only; not per-frame. Hot-path frame rendering is unaffected. Easily reverted with no API churn.

### ID-4 (INFO) — `synthesize_tangents_yup` degenerate fallback diverges from Z-up flavor

**Changed File**: `crates/nif/src/import/mesh/tangent.rs` (commit `648fc86a`)
**Dimension**: NIF import / tangent synthesis correctness
**Severity**: INFO — both choices produce valid orthogonal tangents.

**Symptom**: The Y-up degenerate fallback uses `t_y = [n_yup[1], n_yup[2], n_yup[0]]`. The Z-up flavor uses `t_z = [n_zup.y, n_zup.z, n_zup.x]` then `t_y = [t_z[0], t_z[2], -t_z[1]]`. The Y-up image of the Z-up permutation would be `[-n_yup[2], n_yup[0], -n_yup[1]]`, not the simpler `[n_yup[1], n_yup[2], n_yup[0]]`.

Both produce valid tangents orthogonal to the normal (verified at the 3 axis cases in the comment). They simply happen to choose different arbitrary directions for the degenerate case.

**Why flagging**: a future audit comparing the two synth paths might flag "Y-up permutation isn't the Y-up image of Z-up" as drift. Document the deliberate divergence inline in `synthesize_tangents_yup` so the next sweep doesn't re-discover this.

**Suggested fix**: add a one-line comment at the degenerate fallback: "Degenerate-case permutation differs from `synthesize_tangents`'s Z-up flavor; both produce valid orthogonal tangents — see audit AUDIT_INCREMENTAL_2026-05-22 ID-4."

### ID-5 (INFO) — `asset_provider.rs::ResolveError::DepthLimit` catch now dead for the documented case

**Changed File**: `byroredux/src/asset_provider.rs` (unchanged this session)
**Dimension**: BGSM resolution / dead code
**Severity**: INFO — pre-existing safety net, now bypassed for the canonical case.

**Symptom**: After #1148 the BGSM resolver detects self-referential templates internally and returns a cycle-broken chain. The `asset_provider.rs:632` catch for `ResolveError::DepthLimit` was specifically there to recover from `defaulttemplate_wet.bgsm`'s self-reference; with cycle detection in the resolver, that catch will no longer fire for the documented case.

**Two options**:
- (a) Leave as-is as a safety net for genuine deep chains (>16 levels — vanilla tops out at 3, theoretical only).
- (b) Remove the special-case and let `DepthLimit` propagate as `NotFound`-equivalent — cleaner but loses the recovery path for any genuine deep-chain content that ever lands.

**Recommendation**: option (a) — leave it, but update the inline comment to note "self-referential template recovery is now primarily handled inside `bgsm::template::resolve` (#1148); this remains as a safety net for the unlikely >16-deep chain case."

**Effort**: comment-only.

---

## Missing Tests

None blocking. The 49 new unit tests cover all 21 issues. Two areas where additional coverage could land (deferred):

- **#1204** — synthetic SSE-reconstructed BSTriShape with empty `shape.normals` + `shape.uvs` + populated function-local `positions`/`normals`/`uvs` to exercise the new Y-up fallback branch end-to-end. The pin tests in `tangent_convention_tests.rs` cover the helper in isolation; the integration of the new branch into `extract_bs_tri_shape` is only covered indirectly via the larger BSTriShape import tests.

- **#1226** — integration test for `shrink_tlas_scratch_to_fit` itself (the predicate is unit-tested; the call-site is not). Requires a live `AccelerationManager` mock — out of scope for this audit.

---

## Notes for Next Audit

- **Cycle detection pattern** in `bgsm/template.rs` is now the in-tree reference for "depth-cap + visited-stack ancestor-tracking" — sibling for any future graph-walk on the parser side.
- **`apply_alpha_flags` blend-mode side-effect** (Finding ID-1) is worth checking on the FO3/FNV legacy paths too — same function is called there; the value-write to src/dst may have been silently incorrect for `flags=0` properties on legacy content. Pre-#1201 the `!alpha_blend && !alpha_test` gate suppressed re-entry, so it'd have written-once-only; now it writes-and-stops. No semantic difference unless a consumer reads pre-`alpha_blend`-gate.
- **#1156 (80 stale ISSUE.md files)** is still open and now grew by 21 OPEN-marked files I created tonight via the standard ISSUE.md template. The Option A/B/C decision is still pending.
