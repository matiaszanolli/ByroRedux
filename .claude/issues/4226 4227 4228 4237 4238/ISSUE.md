# #4226 — TD8-002: Three stale `#[allow(dead_code)]` in `groundcover_translate.rs` — #4054 already gave them a production consumer

**Description**: `SUPPRESSION_KEYWORDS`, `AFFINITY_KEYWORDS`, and `layer_affinity` all carry a dead-code annotation claiming no consumer exists; #4054 wired `layer_affinity` into `cell_loader/terrain.rs`'s real (non-test) `CellSplatLayer` builder, which flows into `render/groundcover.rs`'s GPU upload. The sibling `layer_affinities` (plural) is still genuinely uncalled and should NOT be touched.

**Suggested Fix**: Remove the three attributes; tighten the doc comment to say only the GPU `groundcover_scatter.comp` `affinity(splat)` dispatch is still pending. Do NOT touch `layer_affinities`.

## Completeness Checks
- [x] **SIBLING**: Confirmed `layer_affinities` (plural) is still genuinely uncalled outside its own test — kept its `#[allow(dead_code)]`.
- [x] **TESTS**: N/A — removing a stale suppression attribute; `cargo check -p byroredux` with no dead-code warning is the verification.

**Disposition**: Removed the three stale attributes (`SUPPRESSION_KEYWORDS`, `AFFINITY_KEYWORDS`, `layer_affinity`), rewrote the doc block to describe the actual consumer chain, and verified `cargo check -p byroredux` produces zero dead-code warnings for these three symbols while `layer_affinities` correctly still keeps its attribute (confirmed live via grep: `layer_affinity` has a call site in `cell_loader/terrain.rs:178`; `layer_affinities` has none outside its own test).

---

# #4227 — TD9-001: Golden-frame pixel regression guard has zero effective coverage pending a human GPU regen

**Description**: The project's only pixel-level render regression guard cannot currently pass — the baseline PNG predates recent renderer changes. Closed #3849 converted the false-positive failure mode into an explicit staleness assertion, but does not restore actual pixel-regression coverage. **Suggested Fix**: No source change needed — a one-time manual regeneration (`BYROREDUX_REGEN_GOLDEN=1 cargo test --release -p byroredux -- --ignored cube_demo_golden_frame`) on a Vulkan-capable machine.

**Disposition**: **SKIPPED — not addressed in this batch.** This issue's own suggested fix requires a one-time manual step run on a Vulkan-capable machine by a human, committing a freshly-rendered baseline PNG + capture file. This sandbox has no interactive GPU session to produce and visually validate a new golden frame, and fabricating or guessing a baseline image would be actively harmful (it would silently corrupt the one regression guard the project has). This mirrors the established project precedent for #3922 (a prior HIGH-severity renderer fix also requiring real game data + RenderDoc/screenshot verification unavailable in this sandbox) — left open for the user to run manually. No code or doc change made; left as-is.

---

# #4228 — FNV-D2-01: material_translate.rs module doc undercounts the Phase-2 resolvers (missing resolve_unresolved_gloss_neutral_roughness)

**Severity**: LOW
**Dimension**: NIFAL Canonical Translation (FNV Slice) — `/audit-fnv` Dimension 2
**Location**: `byroredux/src/material_translate.rs:28-45` vs `:1074-1093` (`resolve_unresolved_gloss_neutral_roughness`)
**Status**: NEW

**Description**: The module doc's Phase-2 resolver table lists only two resolvers, undercounting a third (`resolve_unresolved_gloss_neutral_roughness`, #3905), called from both of the same two production call sites.

**Suggested Fix**: Add a third row to the Phase-2 resolver table and update "Both Phase-2 resolvers…" to "All three Phase-2 resolvers…".

## Completeness Checks
- [x] **SIBLING**: Checked `docs/engine/nifal.md` — found the identical undercount.
- [x] **TESTS**: N/A — documentation-only fix.

**Disposition**: Added the third row to both the `material_translate.rs` module doc table and `docs/engine/nifal.md`'s mirrored table (the SIBLING check found the same undercount there), updating "Both"/"both resolvers" to "All three"/"all three resolvers" in each.

---

# #4237 — FO3-D1-2026-09-11-02: Window_Environment_Mapping/Eye_Environment_Mapping decoded but never reach the glass classifier

**Severity**: LOW
**Dimension**: FO3 Rendering Path (Inline Shaders) — `/audit-fo3` Dimension 1
**Location**: `crates/nif/src/import/material/legacy_properties.rs:19-30` (`legacy_env_map_scale`), `crates/nif/src/shader_flags.rs:36-40`, `byroredux/src/material_translate.rs:649-666` (`classify_glass_into_material` call)
**Status**: NEW

**Description**: `Window_Environment_Mapping`/`Eye_Environment_Mapping` are parsed and only feed `legacy_env_map_scale`'s on/off decision for `env_map_scale`; `classify_glass_into_material` never receives them, so window glass with a non-keyword-matching filename is misclassified as opaque dielectric instead of glass.

**Suggested Fix**: Carry `window_env_mapping: bool` through `MaterialInfo`/`ImportedMaterial` as an additional positive signal into `classify_glass_into_material`.

## Completeness Checks
- [x] **CANONICAL-BOUNDARY**: Signal lands at the NIF import → `Material` boundary (`ImportedMaterial.window_env_mapping` → `classify_glass_into_material`), not a render-time keyword rescan.
- [x] **TESTS**: 3 new regression tests added and verified against a temporary revert.

**Disposition**: Added `window_env_mapping`/`window_env_mapping_consumed` to `MaterialInfo`, a new `legacy_window_env_mapping()` helper (narrower than `legacy_env_map_scale`'s gate — deliberately excludes the plain `Environment_Mapping` bit, since that fires on non-glass reflective surfaces like polished metal/power armor), wired at all 7 production `env_map_scale_consumed` call sites in `legacy_properties.rs`, threaded through `ImportedMaterial.window_env_mapping`, and passed as a new final parameter into `classify_glass_into_material`, which now treats it as an independent positive glass signal alongside `bgem_glass` (same standing, but still subject to the existing alpha-coverage/decal/conductor gates).

Added 3 new tests: a non-keyword window classifies glass when the bit is set; the same fixture does NOT classify glass when the bit is absent; a conductor (`metalness >= 0.3`) is NOT reclassified by the bit alone. All updated call sites (22 existing test call sites gained the new trailing `false` argument) verified passing. Verified the fix is load-bearing by temporarily reverting the final gate to ignore `window_env_mapping` — confirmed the new positive-case test failed — then restored and re-confirmed all 24 `glass_classification_tests` pass.

---

# #4238 — FO4-2026-09-11-D4-03: psg_vertex_stride reads vertex-attribute field without the 12-bit mask its sibling applies

**Severity**: LOW
**Dimension**: 4 — NIF BSVER 130 + Half-Float + FO4 Collision
**Location**: `crates/nif/src/import/precombine.rs:93-101` (vs `:122`)
**Status**: NEW

**Description**: `psg_vertex_stride` computes `attrs = (vertex_desc >> 44) as u16` (unmasked), while `decode_shared_geom_object` twenty lines later correctly masks with `& 0xFFF`. `BSVertexDesc`'s Vertex Attributes field is 12 bits at position 44; the unmasked form leaks 4 bits of the adjacent "Unused 2" field.

**Impact**: None today — every `VF_*` constant is below `0x800`, so the leaked bits can't change the current `attrs & VF_FULL_PRECISION` result. Latent trap for the first consumer testing a bit ≥ `0x1000`.

**Suggested Fix**: Apply `& 0xFFF` at the stride function too, matching `decode_shared_geom_object`.

## Completeness Checks
- [x] **SIBLING**: Confirmed no other `vertex_desc >> 44` site in the crate is similarly unmasked (checked `bs_tri_shape.rs`, `sse_recon.rs`, both correctly masked).
- [x] **TESTS**: A regression test pins a `vertex_desc` with bits set above `0xFFF` at position 44 producing the same extracted attrs as the masked form.

**Disposition**: Extracted a shared `vertex_attrs_field(vertex_desc) -> u16` helper (masked) used by both `psg_vertex_stride` and `decode_shared_geom_object`, replacing the two independent extractions — this also closes the SIBLING duplication risk, not just this one instance. Discovered mid-fix that the issue's own suggested test (asserting `psg_vertex_stride`'s *output* differs) cannot actually distinguish masked from unmasked behavior, exactly as the issue's "Impact" note says: no `VF_*` constant is above `0xFFF`, so a leaked bit never changes `attrs & VF_FULL_PRECISION`. Wrote the regression test against the extracted `vertex_attrs_field` helper directly instead, which does correctly fail pre-fix (confirmed by temporarily reverting the helper to unmasked and observing the test fail) and pass post-fix.
