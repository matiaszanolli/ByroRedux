# #4216 — TD7-001: `BLOOM_BYTES_PER_PIXEL_X1024`'s geometric-series literal is not derived from or tested against `BLOOM_MIP_COUNT`

**Description**: `BLOOM_BYTES_PER_PIXEL_X1024` bakes in `341`/`340` as bare literals, correct only for the current `BLOOM_MIP_COUNT = 5`. Unlike its sibling `CAUSTIC_BYTES_PER_PIXEL` (#2679), this constant has no test recomputing it from live values.

**Suggested Fix**: Either replace the literal with a `const fn` deriving the geometric sum from `BLOOM_MIP_COUNT`, or keep the literal but add a test recomputing the series and asserting equality.

## Completeness Checks
- [x] **SIBLING**: `CAUSTIC_BYTES_PER_PIXEL`'s derive-from-live-values pattern (#2679) mirrored.
- [x] **TESTS**: 3 new tests added and verified.

**Disposition**: Took the `const fn` derivation approach. Added `bloom_geometric_sum_x1024(levels: u32) -> u32`, a closed-form geometric series (`(1024 - 1024/4^levels) / 3`), exact for `levels` in `1..=5`. `BLOOM_BYTES_PER_PIXEL_X1024` now derives from `BLOOM_MIP_COUNT` via this function instead of hardcoded literals. Added 3 tests: an independent (different-algorithm) recomputation cross-check, a pin of the exact pre-fix literal values at the current mip count, and a monotonicity check across the exact-coverage range. All verified passing; `cargo test -p byroredux-renderer` (985 tests, +3) passes.

---

# #4229 — FNV-D2-02: translate_material keys glass classification and pre-computed PBR scalars off different (pre/post overlay) texture paths

**Severity**: LOW
**Dimension**: NIFAL Canonical Translation (FNV Slice) — `/audit-fnv` Dimension 2
**Location**: `byroredux/src/material_translate.rs:475-666`, `crates/nif/src/import/material/mod.rs:1387-1411,1518-1519`
**Status**: NEW

**Description**: `translate_material` classifies glass from the caller's overlay-resolved texture path, but PBR scalars (`metalness_override`/`roughness_override`) arrive pre-computed at NIF-import time from the mesh's own un-overlaid path. An overlay swap can carry the original texture's PBR classification onto a materially different surface.

**Suggested Fix**: Key both the glass classification and the PBR-scalar pre-computation off the same resolved texture path — either defer PBR-scalar classification to `translate_material`'s call site, or pass the overlay-resolved path back into `classify_legacy_pbr` before the NIF-import-time PBR computation runs.

## Completeness Checks
- [x] **CANONICAL-BOUNDARY**: Recomputation stays entirely inside `translate_material` (the NIFAL boundary), using the same public `classify_pbr_keyword` free function the NIF-import-time classifier already uses — no per-game logic pushed into shaders/renderer.
- [x] **SIBLING**: No other `MaterialInfo`/`ImportedMaterial` field was found with the same pre/post-overlay key mismatch during this investigation.
- [x] **TESTS**: 3 new regression tests, verified against a temporary revert.

**Disposition**: Investigated two implementation strategies (pool-threading into `translate_material` vs. moving classification entirely into `translate_material`); chose the narrower, additive approach: added `ResolvedPaths::source_base_color` (the mesh's own un-overlaid base-color path, resolved once by each of the 4 call sites, which already hold the `StringPool` lock needed). `translate_material` compares it against the overlay-resolved `textures.base_color`; on divergence — and only when `!source.bgsm_pbr_scalars_authored` (the legacy keyword-classified path, never an authored BGSM override) and `source.metalness_override.is_some()` (mirrors `classify_legacy_pbr`'s own gate without re-deriving it) — it recomputes via `classify_pbr_keyword` using the overlay's effective texture/normal/gloss-map presence. `nif_loader.rs` (loose-NIF, no overlay) and `placement_lod.rs` (exempt from overlays, same #3465 reasoning) pass `None`, preserving identical behavior. `mesh_instance.rs` (the one REFR-overlay-bearing site) now captures the mesh's own base-color path in `ResolvedMeshPaths` before the overlay resolve overwrites it.

3 new tests: an overlay swap to a keyword-distinct texture recomputes correctly; no divergence leaves the precomputed classification untouched; a BGSM-authored material is never reclassified by an overlay swap. Verified the fix is load-bearing by temporarily forcing `overlay_changed_base_color = false` and confirming the positive-case test fails, then restored. Full `byroredux` suite (2072 tests, +3) passes unchanged.

---

# #4267 — SF-DIM1-02: no byte-literal fixture pins the documented mixed-raw+LZ4-chunk DX10 BA2 record

**Severity**: LOW
**Dimension**: Dimension 1 — BA2 v2/v3 LZ4 Block Decompression
**Location**: `crates/bsa/src/ba2.rs`
**Status**: NEW

**Description**: The BA2 reader documents and handles a mixed-raw/LZ4-chunk DX10 texture-record population (3.66% of the real corpus), but no byte-literal unit fixture exercises this shape — only the opt-in real-data sweep would catch a regression.

**Suggested Fix**: Add a synthetic byte-literal fixture (constructed record header + one raw chunk + one LZ4-compressed chunk) to the default BA2 test suite.

## Completeness Checks
- [x] **SIBLING**: N/A — this is a new fixture, no sibling pattern to duplicate.
- [x] **TESTS**: Added, and verified against a temporarily-broken implementation.

**Disposition**: Added `extract_dx10_concatenates_a_raw_chunk_and_an_lz4_chunk_in_one_record`, calling `extract_dx10` directly with an in-memory `Cursor` holding a synthetic body: chunk 0 stored raw (`packed_size == 0`), chunk 1 LZ4-block compressed via `lz4_flex::block::compress` (the same encode helper the existing `decompress_chunk_lz4_roundtrip` test uses). Asserts the synthesized DDS header still leads the output and the concatenated pixel data matches raw-passthrough + LZ4-decompressed bytes in chunk order. Verified load-bearing by temporarily forcing the raw/LZ4 branch selector to always take the raw path — confirmed the test fails — then restored. `cargo test -p byroredux-bsa --lib` (93 tests, +1) passes.

---

# #4287 — SF-2026-09-11-D9-02: RefrTextureOverlay::fill_from_bgsm claims exact BGEM parity with merge_external_material but drops base_texture and envmap_mask_texture, and fills env unconditionally with no env_mapping_enabled gate

**Severity**: MEDIUM
**Dimension**: Dimension 9 — BGSM/BGEM External Material Flow
**Location**: `byroredux/src/cell_loader/refr.rs:316-340` (`fill_from_bgsm`, `.bgem` arm)
**Status**: NEW

**Description**: `fill_from_bgsm`'s `.bgem` arm never forwards `bgem.base_texture` into `self.diffuse`, never forwards an envmap-mask texture, and fills `self.env` unconditionally with no gate on `env_mapping_enabled()` — reintroducing, on this second parallel resolver, exactly the two defects #2643 already fixed on the primary `merge_external_material` path.

**Suggested Fix**: Add the missing `base_texture`→`self.diffuse` and envmap-mask→env-mask forwarding calls, and gate the `env` fill on the BGEM's environment-mapping-enabled flag, mirroring `merge_external_material` exactly.

## Completeness Checks
- [x] **CANONICAL-BOUNDARY**: N/A — cell-loader REFR overlay resolution, not the `translate_material` boundary.
- [x] **SIBLING**: Verified against `merge_external_material`'s BGEM arm (`byroredux/src/asset_provider/material/merge.rs`) as the reference implementation and mirrored it exactly.
- [x] **TESTS**: Paired with #4288's guard-test fix; verified against a temporary revert.

**Disposition**: Added `Self::fill(&mut self.diffuse, Some(bgem.base_texture.as_str()), pool)` and wrapped the `env`/`env_mask` fills in `if bgem.env_mapping_enabled() { ... }`, matching `merge_external_material`'s BGEM arm field-for-field. Verified load-bearing via `fill_from_bgsm_forwards_every_bgem_texture_role` (see #4288).

---

# #4288 — SF-2026-09-11-D9-03: the guard test for D9-02's regression leaves both dropped fields empty in its fixture, so the every-BGEM-texture-role test covers 6 of 8 and cannot fail

**Severity**: LOW
**Dimension**: Dimension 9 — BGSM/BGEM External Material Flow
**Location**: `byroredux/src/cell_loader/refr_texture_overlay_tests.rs:714`
**Status**: NEW

**Description**: `fill_from_bgsm_forwards_every_bgem_texture_role`'s fixture leaves `base_texture`/envmap-mask empty — the exact two fields #4287 found dropped — so the test asserted nothing about those roles and would not fail even with the bug present.

**Suggested Fix**: Populate `base_texture` and the envmap-mask field in the fixture, paired with #4287's production fix.

## Completeness Checks
- [x] **SIBLING**: N/A.
- [x] **TESTS**: This IS the test fix.

**Disposition**: Populated `base_texture` and `envmap_mask_texture` in the fixture, plus `base.environment_mapping = true` (required for #4287's new `env_mapping_enabled()` gate to let the envmap fills through at all — the fixture's `BgemFile` otherwise defaults to `version < 10` reading `base.environment_mapping`, which defaults false). Added assertions for `ov.diffuse` and `ov.env_mask`. Verified this test correctly fails against a temporarily-reverted (pre-#4287) production implementation — confirmed the `diffuse` assertion fails with `left: None` — then restored, confirming the pairing actually catches the regression it's meant to guard.
