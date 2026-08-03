# Starfield Compatibility Audit — 2026-08-03

## Executive Summary

Starfield is a first-class `GameKind` in ByroRedux: NIF parsing (BSVER 155+,
BSGeometry geometry path) at 99.99% aggregate clean-parse rate, BA2 v2/v3
archives (zlib + LZ4 block, GNRL + DX10) at 100% extract, CDB
(`materialsbeta.cdb`) + external BGSM/BGEM materials, ESM parsing at ~99.9%
record-parity per the `starfield-esm-roadmap.md` revision, and a walkable
Cydonia interior (cell resolve rate 91.2% of REFRs). This is a
depth/correctness audit of that bring-up surface, not a from-scratch gap
inventory — the goal was to find regressions and latent bugs in
already-shipped functionality, not to catalog known-incomplete work.

**Overall verdict: mostly solid, with one HIGH regression-adjacent bug in
the material boundary.** Most named regression guards checked across all 9
dimensions (BA2 header/compression dispatch, BSGeometry sentinel-slot skip,
CDB chunk indexing, ESM LIGH `DAT2` decode, XCLL canonical sizing, BSVER
155+ shader-tail capture, BGSM/BGEM classification) are intact and
correctly wired, verified live against real vanilla Starfield game data —
not just by reading code. Dimension 8 (NIFAL material translation) was
initially reported clean, but a deeper source trace across every
`MaterialInfo`-writing walker arm found the `BSEffectShaderProperty`/
`BSSkyShaderProperty`/`BSWaterShaderProperty` arms fabricate PBR metalness/
roughness and falsely claim authored emissive/specular/glossiness data,
extending the #1873 chrome-flyer bug class beyond the arm it fixed — this
is cross-game (Skyrim/FO4/FO76/Starfield), not Starfield-specific, and that
re-verification supersedes the earlier "no new findings" read. **16 new
findings total: 1 HIGH, 7 MEDIUM, 8 LOW.** Beyond the material-boundary
finding: one genuine resource-exhaustion vector (BA2 DX10 chunk-sum
amplification), one real diagnosability gap (silent BSGeometry resolve
failures — the exact shape of the historical #1292 collapse, now
undetectable by log alone), a materialsbeta-adjacent tracking gap (the CDB
Phase-2 per-field extraction has no open issue despite being load-bearing
for all Starfield rendering), the NIFAL particle and collision slices being
structurally unreachable on Starfield (particles are authored outside the
NIF container; all colliders route to the undecoded `BhkSystemBinary`
blob), a BGEM version-window classifier bug, and assorted doc-staleness /
low-value hardening items.

**Cross-cutting check (per today's coordination with sibling per-game
audits):** FO3's ungated `env_map_scale` forwarding, FO3's non-T
`bhkRigidBody` displacement bug, and FNV's ragdoll transform-composed-twice
bug were all checked against Starfield's actual code paths. **None are
exercised by Starfield content** — Starfield's only collision object is
`BhkNPCollisionObject → BhkSystemBinary`, an explicit undecoded stub;
architecture collision falls straight to the `synthesize_static_trimesh`
fallback, and NPC ragdolls are the FO4+-style NP-blob format, equally
undecoded. Neither `rigid_body.rs` nor `ragdoll.rs` is ever reached by
Starfield content today. No scope widening recommended for those findings.

## Dimension Findings

### Dimension 1 — BA2 v2/v3 LZ4 Block Decompression

Scope: `crates/bsa/src/ba2.rs` (1448 lines, read in full). Verified live
against 129 real Starfield archives (vanilla + Shattered Space + installed
mods): all extract cleanly, including all 13 v3 LZ4 DX10 texture archives.

**Confirmed correct, no regression:** v2 (8-byte)/v3 (12-byte) header offset
math; `Ba2Compression` dispatch (0=zlib/3=LZ4/other=hard `InvalidData`
error, not silent fallback); per-chunk raw-vs-LZ4 selection inside a single
DX10 texture (decided per chunk via `chunk.packed_size == 0`, not per
archive); GNRL and DX10 sharing one `decompress_chunk` path; DX10 chunk
struct layout is version-invariant back to FO4 v1 (the v3 fix was header-
offset-only, confirmed by tracing `read_dx10_records`).

#### SF-BA2-01: DX10 per-chunk size caps aren't summed across a record — up to 255× allocation amplification per texture
- **Severity**: MEDIUM
- **Location**: `crates/bsa/src/ba2.rs:601-626` (chunk read/cap), `:760-793` (`extract_dx10` loop), `crates/bsa/src/safety.rs:33-39,66-76`
- **Status**: NEW
- **Description**: `checked_chunk_size` caps each DX10 chunk's `packed_size`/`unpacked_size` individually at `MAX_CHUNK_BYTES` (1 GiB), but `num_chunks` is a `u8` (up to 255 chunks per record) and nothing caps the *sum* across a record's chunk list. A hostile/corrupted `.ba2` can declare 255 chunks each near the 1 GiB cap while the real backing `packed_size` bytes stay tiny.
- **Evidence**: `read_dx10_records` checks each chunk's size independently; `extract_dx10`'s loop allocates (zlib: `Vec::with_capacity`; LZ4 safe-decode: an eager `vec![0; unpacked_size]` zero-fill) per chunk with no running total.
- **Impact**: Resource-exhaustion / DoS vector on any path opening untrusted or mod-repacked `.ba2` files — up to 255 sequential ~1 GiB allocation attempts from a small on-disk file. Not memory corruption, but a crash-adjacent failure mode. Not Starfield-specific (shared DX10 code, all BTDX versions), directly surfaced by this dimension's investigation.
- **Related**: Same theme as #2097 (LZ4-01) — untrusted declared size drives allocation — different mechanism (aggregate vs. single-chunk).
- **Suggested Fix**: Track a running total of `unpacked_size`/`packed_size` while reading a DX10 record's chunk list; reject if the sum exceeds a generous per-texture ceiling (e.g. 256 MiB), mirroring the existing `checked_entry_count` pattern.

#### SF-BA2-02: v3 header-boundary diagnostic log reads the stream position 4 bytes early
- **Severity**: LOW
- **Location**: `crates/bsa/src/ba2.rs:233-236`, `:447-472` (`log_v2_v3_extra_bytes`)
- **Status**: NEW
- **Description**: For v3, the header-boundary sanity log captures `stream_position()` before the 4-byte `compression_method` field is read (32 bytes in, not the true 36-byte post-header offset). The v2 branch captures it correctly (nothing left to read).
- **Impact**: Log-only — a `log::trace!`/`log::debug!` diagnostic, never affects control flow or parsing correctness.
- **Suggested Fix**: Move the log call to after `method_buf` is read, or pass `stream_pos + 4` with a comment.

**Existing: #2097 (LZ4-01, OPEN, LOW)** — corroborated with source-level evidence: traced the pinned `lz4_flex 0.11.6` (`safe-decode`/`safe-encode` enabled) and confirmed a declared `unpacked_size` that undershoots reality returns `Err(OutputTooSmall)` gracefully, not a panic — a structural guarantee of the safe-decode implementation, not luck. #2097's own "no panics found" empirical claim is confirmed correct; its "unpinned assumption" framing (nothing asserts the safe-decode features stay enabled) still stands. No severity change.

---

### Dimension 2 — BSGeometry Mesh Extraction

Scope: `crates/nif/src/import/mesh/bs_geometry.rs`, `crates/nif/src/blocks/bs_geometry.rs`, `byroredux/src/asset_provider/archive.rs`. Cross-checked against 4,000 randomly-sampled real `.mesh` companions from `Starfield - Meshes01.ba2` (320,483 entries).

**Confirmed correct, no regression:** Stage A (inline)/Stage B (external `.mesh`) dispatch is unambiguous by construction (the internal-geometry flag is computed once per block); #1292 `geometries\` path-prefix preservation (verified against the real BA2 name table); #1209 full-LOD-slot iteration (no `.first()` short-circuit); #1828/#1829 sentinel-slot skip (the emptiness test is in the match guard, so a sentinel-first slot correctly falls through); #1203 `BSSkin::Instance`/`BoneData` chain; #1232 tangent synthesis producing genuinely unit-length tangents; `metalness_override`/`roughness_override` forwarding from `classify_legacy_pbr`. No vertex-count/attribute-channel overflow risk found (max observed 36,194 verts, comfortably inside `u16` index space; all downstream vertex assembly is bounds-checked regardless).

#### SF2D2-03: External `.mesh` resolve failure is completely silent — the exact #1292 failure mode has no log signal
- **Severity**: MEDIUM
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:66-103`
- **Status**: NEW
- **Description**: Stage B has three distinct "no geometry found" exits (no resolver supplied; archive-resolve miss; every slot exhausted) and **none of them logs anything**. Only the rarer sub-failure cases inside a *successful* resolve (parse error, sentinel body) log, at `debug!`.
- **Impact**: A future archive-set misconfiguration, missing archive, or path-convention drift reproduces the #1292 symptom (near-total mesh-spawn collapse across all vanilla Starfield content — 288,231 of 320,483 `Meshes01.ba2` entries are `.mesh` companions) with an empty log. Recovering the diagnosis last time required a dedicated investigation session.
- **Suggested Fix**: Add `log::debug!` on the resolve miss (naming the canonical path) and `log::warn!` when every slot is exhausted (naming the shape). Consider a dropped-`BSGeometry` counter surfaced via `byro-dbg`.

#### SF2D2-04: `.mesh` suffix/`geometries\` head composed unconditionally, contradicting the field's documented "path or stem" semantics
- **Severity**: LOW
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:70`
- **Status**: NEW
- **Description**: The importer always composes `geometries\{mesh_name}.mesh` with no inspection of `mesh_name`, but nifly (the cited wire-format authority) and this codebase's own block-level doc both document the field as holding *either* a bare stem *or* a full path. A `mesh_name` already carrying the prefix/suffix double-composes into a guaranteed miss.
- **Impact**: Zero on vanilla (every real `.mesh` name is a bare 20-hex stem); affects authoring-tool output / mods using readable paths, where the mesh silently vanishes (compounded by SF2D2-03's silence).
- **Suggested Fix**: Skip the prepend/append when the name already carries them (reuse the case/separator-insensitive head test already written in `normalize_mesh_path`).

#### SF2D2-05: Four `import_nif_scene` call sites pass no `MeshResolver` — external-geometry `BSGeometry` there imports to zero meshes, silently
- **Severity**: LOW (currently unreachable for Starfield; becomes MEDIUM once Starfield distant-object LOD is wired)
- **Location**: `byroredux/src/cell_loader/object_lod.rs:262`, `placement_lod.rs:469`, `terrain_lod_btr.rs:137`, `byroredux/examples/dump_nif.rs:151`
- **Status**: NEW
- **Description**: These call the no-resolver `import_nif_scene` overload even though `object_lod.rs`/`placement_lod.rs` already hold a `tex_provider: &TextureProvider` (which *is* a `MeshResolver`) in scope.
- **Impact**: Not Starfield-reachable today (`object_lod` is `.bto`-keyed and Starfield's `LODMeshes.ba2` has zero `.bto`; `placement_lod` is Oblivion-gated) — but Starfield ships 19,535 `meshes\lod\generated\..._lod_N.nif` files that a future distant-object-LOD arc would hit, inheriting a silent 100% drop if it reuses either helper.
- **Suggested Fix**: Thread the already-in-scope `tex_provider` through as `Some(tex_provider)`.

#### SF2D2-06: `#1232` tangent-synthesis guard is vacuously true — synthesis can run against a fabricated up-normal
- **Severity**: LOW
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:147-158`, `:192`
- **Status**: NEW
- **Description**: The synthesis-branch guard `!normals.is_empty() && !uvs.is_empty() && !positions.is_empty()` is meant to require "otherwise-populated" geometry, but `normals` is already filled with a `[0,1,0]` placeholder upstream whenever authored normals are absent — so the guard reduces to `!uvs.is_empty()`, and Gram-Schmidt tangent synthesis can silently run against a constant fabricated normal.
- **Impact**: Empirically unreachable on vanilla (0 of 4,000 sampled `.mesh` files lacked normals/tangents/UV0). Correctness-of-intent hardening only; latent trap for modded/tool-exported content.
- **Suggested Fix**: Track `normals_authored` explicitly and gate on that instead of `!normals.is_empty()`.

#### SF2D2-07: Vertex-count / attribute-channel overflow — assessed, no defect (informational)
- **Severity**: LOW (informational)
- **Location**: `crates/nif/src/blocks/bs_geometry.rs:382-460`
- **Status**: NEW (informational baseline, no fix required)
- **Description/Evidence**: 4,000-file real-data sample: max `n_vertices` 36,194 (well under `u16` ceiling); attribute channels (`n_uv1`/`n_normals`/`n_colors`) are always either 0 or exactly `n_vertices` — no ragged-channel case exists in vanilla content, and downstream assembly is bounds-checked regardless.

**Cross-reference**: `BSDynamicTriShape` (the sibling Skyrim SE FaceGen zero-mesh finding) shares **no** code path with `bs_geometry.rs` — Starfield FaceGen ships as `BSGeometry`, not `BSDynamicTriShape`. Does not propagate here.

---

### Dimension 3 — CDB Material Database Correctness

Scope: `crates/sfmaterial/src/{reader,chunk,string_table,types,value}.rs`, `byroredux/src/asset_provider/material.rs`. Verified live against the real vanilla `materialsbeta.cdb` (extracted from `Starfield - Materials.ba2`): **parses cleanly end-to-end — 97 classes / 1,438,780 instances**, matching the ~1.44M figure cited in code comments.

**No new findings.** All checklist items confirmed intact:
- **#762 / chunk-overflow guard**: `index_chunks` rejects a chunk whose declared size exceeds remaining bytes (`Error::ChunkOverflow`); pinned by `probe_header_skips_instance_walk` and the header/chunk-table tests in `reader.rs`.
- **#1571 DLC/Creation CDB discovery**: `discover_starfield_cdbs` genuinely scans each archive for every `materials\materialsbeta.cdb` **and** `materials\creations\<plugin>\materialsbeta.cdb` via `is_materialsbeta_cdb_path` — confirmed NOT a hardcoded single-path extract.
- **Unknown ChunkType/BuiltinType/ClassFlags handling**: all three are pinned, named, hard-error paths (`Error::UnknownChunkType`/`UnsupportedBuiltin`/`UnknownClassFlags`, the last one naming the offending class index + editor name per #1569) — a deliberate all-or-nothing design (no per-instance recovery in a 1.44M-instance CDB), not a panic. #1569 (closed) already covers this design decision; no regression.
- **`peek_magic`**: correctly distinguishes a CDB (`BETH` signature) from a loose BGSM via the cheapest possible 4-byte reject, wired into the discovery path.
- **Per-field CDB material extraction**: confirmed **not yet implemented** — the `.mat` arm in `merge_external_material` (`asset_provider/material.rs:710-723`) sets `material.is_pbr = true` and returns immediately; metalness/roughness/textures stay at NIF-derived defaults. This is a real, load-bearing gap for Starfield rendering quality, but it is a known, in-code-documented Phase 2 deferral (#1289), not a new bug — **see Dimension 9's SF-D9-2026-08-03-03, which independently surfaced that this deferral has no open GitHub issue tracking it**, and the Remaining-Work Chain section below.

---

### Dimension 4 — Starfield ESM Resolve-Rate Baseline

Scope: `byroredux/src/sf_smoke.rs`, `crates/plugin/examples/sf_smoke.rs`, `crates/plugin/src/esm/cell/support.rs`. All measurements run live against real `Starfield.esm`.

**No findings — every regression-guard check reproduces its documented baseline exactly:**

| Check | Baseline | Live result |
|---|---|---|
| Cydonia REFR resolve rate | 91.2% (25,437/27,898) | 91.2% (25,437/27,898) — MATCH |
| LIGH resolve count (#1567 `DAT2` guard) | 656/656 | 656/656 — MATCH |
| GBFM/PNDT/STDT/BIOM byte weight | 36/26/12/5.3 MB | 36.1/25.8/12.0/5.3 MB — MATCH |
| Top-level GRUP byte coverage | 86.1% | 86.1% — MATCH |

`starfield_ligh_dat2_decodes_to_light_data` and the full `esm::cell` module (111 tests) pass clean. No CELL-handler silent drop, no base-type indexing regression. The 2,461 unresolved REFRs (slot 0x00) are the pre-existing, already-tracked #1576 BFCB-component-block gap, not re-filed. Minor non-finding: `DISPATCH_HANDLED_FOURCCS` has grown from the doc's stated 110 entries to 115 live, with byte-coverage unchanged — cosmetic doc drift only, not filed.

---

### Dimension 5 — ESM + Cell Bring-up Regression Surface

Scope: `crates/plugin/src/esm/reader.rs`, `records/mod.rs`, `cell/walkers.rs`, `byroredux/src/cell_loader/spawn.rs`.

**Confirmed correct, no regression:** HEDR→`GameKind::Starfield` classification uses tolerance bands (not exact float equality); PDCL conscious-skip (#1568) still named, not folded into the anonymous catch-all; XCLL_SIZES_STARFIELD `[28, 108]` decode is correct and the corrected "shares only bytes 0-39, diverges into a distinct height-fog model" framing (#1293) is reflected in the doc comment; per-cell NAVM collection (#1272); all six spawn-path guards (#1294 `base_layer` gate, #1235 `SceneFlags::from_nif`, #1295 `DoorTeleport`, #1212/#1213/#1214 component attachment, #1284 `SkinSlotPool` ceiling raise — traced end-to-end to `MAX_TOTAL_BONES = 196608`).

**Checklist correction (not a finding, a stale-checklist note):** the `IsCollisionOnly` marker component referenced in the audit checklist has been fully removed from the codebase (#1570, closed via #1632/#1633). BLAS exclusion for synthesized colliders is now structural: `spawn_trimesh_collider_ghost` spawns a physics-only entity with no `MeshHandle` at all, so it can never enter BLAS/TLAS regardless of any marker. Confirmed correct and working; the audit skill's own checklist should be updated to reference the ghost-entity pattern.

#### SF-D5-2026-08-03-01: Stale "Skyrim + 16-byte tail" framing survives in a test assertion message
- **Severity**: LOW
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:172-174`
- **Status**: NEW
- **Description**: #1293 corrected the module doc comment and the test's own docstring to say Starfield's 108-byte XCLL "shares only bytes 0-39 with Skyrim, then diverges into a distinct volumetric height-fog model" — but the `assert_eq!` failure-message string in `starfield_xcll_sizes_pinned` (added by the earlier #1291 commit, untouched by #1293) still reads "Skyrim+ 92-byte body + 16-byte SF tail," exactly the disproven framing, three lines below the corrected docstring.
- **Impact**: No functional/parsing impact — decode logic and canonical-size table are correct and byte-verified. Impact is confined to future maintainers if this pinned assertion ever fires.
- **Suggested Fix**: Update the message to match the corrected docstring.

**Cross-cutting scope check** (see Executive Summary) — FO3's `rigid_body.rs` bug and FNV's `ragdoll.rs` bug are both confirmed **not exercised** by Starfield content (Starfield's only collision path is the undecoded `BhkSystemBinary` stub); FO3's `legacy_properties.rs` env_map_scale bug is architecturally off the Starfield material path (CDB/BSGeometry, not legacy `NiTexturingProperty`).

---

### Dimension 6 — NIF Shader Blocks, BSVER 155+ (regression guard)

Scope: `crates/nif/src/blocks/shader.rs`, `crates/nif/src/blocks/shader_tests/starfield.rs`, `crates/nif/src/shader_flags.rs`.

**No new findings — clean regression-guard dimension.** Ran the full Starfield-tagged test set (18 tests across shader/dispatch/material modules) — all pass:

- **#1510 regression guard (`BSShaderType155` / 4-byte over-read → NiUnknown truncation)**: confirmed intact — `parse_fo76_plus` dispatch is unchanged, no NiUnknown truncation of `BSLightingShaderProperty` blocks anywhere in the live Starfield mesh corpus (per Dimension 7's block histograms).
- **#1606 undocumented BSLightingShaderProperty tail**: confirmed correctly implemented. `read_starfield_tail` (`shader.rs:760-778`) captures exactly `block_size - consumed` bytes — never a hardcoded 38, never over-reads — gated on `bsver >= STARFIELD` and `block_size.is_some()`; returns empty otherwise. The dispatcher (`blocks/mod.rs:577-582`) correctly threads `block_size` through to `BSLightingShaderProperty::parse_with_size` and `BSEffectShaderProperty::parse_with_size`. The legacy `parse(stream)` (`None` size) path yields an empty tail as designed. Tests `parse_bs_lighting_starfield_captures_trailing_tail` and `..._tail_empty_without_size_or_drift` both pass.
- **CRC32 hash → flag-name table**: checklist item 1 asked whether this is opaque. **It is not** — `crates/nif/src/shader_flags.rs::bs_shader_crc32` is a well-populated, named table of 30 CRC32 constants (see the dedicated table below), consumed by `is_decal_from_modern_shader_flags` / `is_two_sided_from_modern_shader_flags` / the `modern_effect_shader_bit` family in `crates/nif/src/import/material/mod.rs`, each with a doc comment citing its nif.xml line and the cross-game bit-semantic caveats.
- Sibling BSEffectShaderProperty +32B under-read: confirmed absent from the open-issue list (not independently tracked) — noted per instruction, not re-filed.

---

### Dimension 7 — Real-Data Validation

Scope: `crates/nif/tests/parse_real_nifs.rs`, `crates/bsa/tests/ba2_real.rs`, ROADMAP.md + `docs/engine/game-compatibility.md`.

**Confirmed accurate against a live run, no regression:** aggregate clean-parse rate 99.9933% ≈ 99.99% (89,270/89,276), exactly matching ROADMAP.md; #746/#747 residual truncation tail unchanged at 6/29,849 (all `BSWeakReferenceNode`, `meshes\terrain\cydoniacity\...` + 2 others); 129/129 real BA2 archives (vanilla + DLC + mods) extract cleanly; zero `NiUnknown` blocks across 5 hand-traced representative meshes (clutter, ship hull, character skeleton, weapon, terrain) and full block-type histograms of all 5 mesh archives; `cross_game_translation_completeness` reports Starfield at 100% structural consistency.

#### SF-D7-01: `docs/engine/game-compatibility.md` still states stale "99.64%/#746/#747" figures contradicting its own updated matrix row
- **Severity**: LOW
- **Location**: `docs/engine/game-compatibility.md:13-19`, `:194-196`, `:399`
- **Status**: NEW
- **Description**: The doc's per-game matrix row (line 38) was correctly updated to 99.99%/#2105/"mis-attributed #746-#747", but three other spots in the same file (summary prose, Tier-2 section, long-tail drift section) were missed in that reconciliation pass and still assert the pre-fix 99.64%/#746-#747 numbers as current fact. `#746`/`#747` are themselves closed issues.
- **Impact**: Doc-only. Risk is a future contributor citing the stale figures or re-opening closed issues. Same failure mode as the already-tracked #2264 (TD6-001) ROADMAP doc-rot finding, different file.
- **Suggested Fix**: Sync the three stale spots to line 38's corrected text.

---

### Dimension 8 — NIFAL Canonical Material Translation for Starfield

Scope: `byroredux/src/material_translate.rs` (`translate_material`), `crates/core/src/ecs/components/material.rs` (`Material::resolve_pbr`), `crates/nif/src/import/material/dedicated_shader.rs`, `crates/nif/src/import/walk/mod.rs`.

**Four new findings (1 HIGH, 3 MEDIUM) surfaced on a deeper re-pass; a first pass reported this dimension clean.** The single-boundary *contract* (one `translate_material`, plain `f32` scalars, no per-draw fallback) does hold — but a full trace of every `MaterialInfo`-writing walker arm found the boundary's *inputs* are fabricated for three shader-property classes, and that Starfield's particle and collision NIFAL slices are structurally unreachable rather than merely "not exercised by this content." This supersedes the earlier "no new findings" read.

**SF-D8-01** (HIGH, NEW): `PbrClassifierInputs::specular_authored` is wired as `self.has_material_data` (`crates/nif/src/import/material/mod.rs:1159-1164`), on the documented invariant that `has_material_data` is set **only** by the `NiMaterialProperty`/`BSLightingShaderProperty` arms — the only two that populate `specular_color`. That invariant is false post-#2059: `apply_bs_effect_shader`, `apply_bs_sky_shader`, and `apply_bs_water_shader` (`dedicated_shader.rs:421,555,575`) all set `has_material_data = true` without ever touching `specular_color`, which stays at its `[1.0, 1.0, 1.0]` struct default. For any `BSEffectShaderProperty` with `env_map_scale > 0.3` — which includes essentially every Starfield effect-shader block, since Starfield's material-reference stub ships `env_map_scale: 1.0` and stubs out nearly all 748 such blocks in the Meshes01 corpus — `classify_pbr_keyword`'s env-map arm fires against the unauthored default and fabricates `metalness = 0.4`, `roughness = 0.55`. This extends the #1873 chrome-flyer bug (fixed only for the PPLighting arm) to three more arms, cross-game (Skyrim/FO4/FO76/Starfield), and also breaks keyword-only (non-BGEM) effect-shader glass promotion, since `helpers.rs`'s `metalness >= 0.3` gate now blocks it using a value no artist authored. Suggested fix: replace the proxy with a dedicated `MaterialInfo::specular_authored` bool set only at the two sites that actually assign `specular_color`, gated also on `!shader.material_reference` so the Starfield stub can't claim authorship either.

**SF-D8-02** (MEDIUM, NEW): Starfield's `BSLightingShaderProperty` material-reference stub (`crates/nif/src/blocks/shader.rs:784-827`, returned whenever the block name is non-empty — effectively all 189,801 `BSLightingShaderProperty` blocks in the Meshes01 corpus) ships fabricated placeholder scalars (`emissive_multiple: 1.0`, `glossiness: 1.0`, `specular_color: [1,1,1]`, `specular_strength: 1.0`). `apply_bs_lighting_shader` copies these onto `MaterialInfo` unconditionally — never checking `shader.material_reference`, the flag the stub sets for exactly this purpose — and sets `emissive_source = EmissiveSource::Lighting` plus `has_material_data = true`, both falsely claiming authorship. This is a NIFAL no-fabrication violation and a trap for the CDB Phase-2 work (see below): a future implementer reading `emissive_source == Lighting` would reasonably conclude the NIF authored these values and write merge logic that defers to them, silently suppressing CDB-authored data for all ~189,801 materials. Suggested fix: gate the rich-material capture block on `!shader.material_reference`, leaving `emissive_source = None` and `has_material_data = false` when the body was never parsed.

**SF-D8-03** (MEDIUM, NEW): the NIFAL particle slice (`walk/mod.rs:536-563`, `extract_emitter_params`/`extract_emitter_rate`) is structurally unreachable on Starfield — the full Meshes01 per-block histogram (31,058 files, 22 distinct block types) contains zero `NiPSys*`/`NiParticleSystem` blocks. Starfield authors particle systems entirely outside the NIF container, so this isn't a silent drop of translatable data, but it means the NIFAL particle regression suite (#1411/#1434/#1445/#1771/#1775, all Oblivion/FO3/FNV/Skyrim-driven) says nothing about Starfield, and nothing in `docs/engine/nifal.md` states the slice is inapplicable there. Suggested fix: record "Starfield: particle slice N/A" in `docs/engine/nifal.md` and the compat matrix, and add a corpus-baseline assertion so a future format discovery flips a test red instead of passing silently.

**SF-D8-04** (MEDIUM, NEW): the NIFAL collision slice never fires on Starfield content at all — confirmed 100% of Starfield colliders route to the undecoded `BhkSystemBinary` blob (33,867 `bhkNPCollisionObject` + 22,895 `bhkPhysicsSystem` + 316 `bhkRagdollSystem` in Meshes01, zero `bhk*Shape` blocks of any kind), so `BhkMultiSphereShape`/`BhkConvexListShape` translation, while correctly implemented for Oblivion→FO4, is dead code with respect to Starfield — sharper and broader than the ROADMAP's existing "ragdolls blocked on `BhkSystemBinary`" note, since it's *all* Starfield collision, not just ragdolls. The synthesized-trimesh fallback (`cell_loader/spawn.rs:1477-1478`) is also narrower than the shape arms it stands in for: it only fires for `RenderLayer::Architecture`, so Starfield Clutter/Actor/container content currently spawns with **no collider at all**, not even an approximate one. Suggested fix (short term): widen the synthesized-trimesh fallback beyond Architecture, and log a once-per-cell count of dropped `BhkSystemBinary` colliders so the gap is measurable.

What still holds, re-confirmed on this pass:
- **Single-boundary contract**: `translate_material` has exactly two call sites in the entire codebase (`byroredux/src/cell_loader/spawn.rs:1303` and `byroredux/src/scene/nif_loader.rs:879`). `Material.metalness`/`.roughness` are plain `f32` fields (not `Option<f32>`); `resolve_pbr`'s classifier arm only fires on a NaN sentinel (an unreachable backstop for NIF/BGSM-imported content, which always arrives pre-classified as `Some`) — mechanically correct, though SF-D8-01/02 show what it's asked to clamp is sometimes fabricated.
- **`classify_glass_into_material`** (a second metalness/roughness write site) is called *from within* `translate_material` itself (line 183, after `resolve_pbr`) — not an independent boundary, no violation.
- **Cross-check with Dimension 3 (CDB)**: combining SF-D8-02 with the `.mat` arm's Phase-1-only merge (Dimension 9/SF-D9-2026-08-03-03), roughly 189,801 of 190,549 Starfield surfaces reach the Disney BSDF lobe as untextured, matte, fully-dielectric white (`roughness 0.85`, `metalness 0.0`, no albedo/normal/emissive) — the concrete boundary-side shape of the CDB Phase-2 gap, quantified against the real corpus.

---

### Dimension 9 — BGSM/BGEM External Material Flow

Scope: `crates/bgsm/src/{base,bgsm,bgem}.rs`, `byroredux/src/asset_provider/material.rs`, `byroredux/src/cell_loader.rs`, `byroredux/src/helpers.rs`.

**Confirmed correct, no regression:** BGEM dispatched distinctly from BGSM (magic-based, extension fallback, one-shot warn on mismatch); `merge_external_material` signature is `&mut ImportedMaterial` (no NIFAL boundary violation); `BGSM_AUTHORED`/`TRANSLUCENCY`/`MODEL_SPACE_NORMALS` flags all trace correctly to their `ImportedMaterial` source fields; the `bgem_glass_without_alpha_is_not_classified` regression test (guarding against a stuck `glass_enabled` flag misclassifying opaque architecture) exists and is green; Disney BSDF classification confirmed to happen only at the `translate_material` boundary, never per-draw in shader code.

#### SF-D9-2026-08-03-01: BGEM legacy glass-bundle detection reads a shadowed field that is structurally always `false` for version >= 10
- **Severity**: MEDIUM
- **Location**: `byroredux/src/asset_provider/material.rs:126-142`; shadowed field pair at `crates/bgsm/src/base.rs:209-213` vs `crates/bgsm/src/bgem.rs:131-134`
- **Status**: NEW
- **Description**: `BaseMaterial::parse_after_magic` only populates `environment_mapping`/`environment_mapping_mask_scale` for `version < 10`; for `version >= 10` it hardcodes `(false, 1.0, depth_bias)`. BGEM separately re-reads the same two values into its own `BgemFile::environment_mapping` field — a shadowing pair. `bgem_uses_glass_behavior` reads the *base* copy, which is structurally `false` for v >= 10. Combined with the classifier's own `version < 21` gate, the legacy glass-bundle arm can only ever fire for BGEM v < 10, leaving versions 10-20 (Skyrim SE = v20) a dead window where neither the legacy bundle nor `glass_enabled` (v >= 21 only) can trigger.
- **Evidence**: `BgemFile::environment_mapping` (the field the parser actually populates for v >= 10) has zero consumers anywhere in the workspace; the only test coverage hand-builds a struct literal bypassing the parser, so it cannot observe the shadowing.
- **Impact**: A BGEM v10-v20 authoring a transparent environment-mapped shell falls through to opaque-plastic classification unless a `glass` keyword happens to match. Not Starfield-facing (Starfield uses `.mat`/CDB, not BGEM), but cross-cutting for Skyrim SE/FO76-era BGEM content.
- **Suggested Fix**: Add a version-aware accessor (`BgemFile::env_mapping_enabled()`) that reads the correct field per version, plus a parser-driven (not struct-literal) regression fixture at v20.

#### SF-D9-2026-08-03-02: `BgemFile::effect_pbr_specular` still has zero consumers after #1358 closed
- **Severity**: LOW
- **Location**: `crates/bgsm/src/bgem.rs:76,169-171`; merge arm at `byroredux/src/asset_provider/material.rs:1097-1241`
- **Status**: Residual of #1358 (CLOSED)
- **Description**: #1358 named three BGEM scalars; two landed (`base_color`, `soft_depth`). The third, `effect_pbr_specular` (BGEM v >= 20), is parsed and never read — the BGEM merge arm never sets `material.is_pbr`, so a v >= 20 BGEM opting into PBR specular still shades on whatever NIF-derived `is_pbr` was.
- **Impact**: Small and bounded — a missed opt-in, not a wrong-write. #1358 is not fully closed as titled.
- **Suggested Fix**: Forward `effect_pbr_specular` into `material.is_pbr` in the BGEM arm, mirroring #1352's BGSM policy.

#### SF-D9-2026-08-03-03: Starfield `.mat` merge forwards zero authored material data, and the CDB Phase-2 deferral has no open tracker
- **Severity**: MEDIUM
- **Location**: `byroredux/src/asset_provider/material.rs:710-723` (the `.mat` arm), `:331-376` (`has_starfield_cdb`/`register_starfield_cdb`)
- **Status**: NEW (as a *tracking* finding — the deferral itself is documented in-code; #1289/#1290 are both CLOSED and ROADMAP.md has no Phase-2 row)
- **Description**: The `.mat` arm is a two-statement stub: flips `material.is_pbr = true` and returns. No texture role, metalness/roughness, alpha/blend, or two-sided/decal state is extracted from the CDB. `register_starfield_cdb` deliberately does a header-only `probe_header` — the class/instance tree is never walked, so there is currently no code path from CDB contents to `ImportedMaterial` at all. This independently corroborates Dimension 3's finding of the same gap from the reader side.
- **Impact**: Every Starfield surface renders with NIF-derived, keyword-classified metalness/roughness under the Disney lobe and whatever textures the NIF happened to carry — the classic "chrome/posterized" symptom for any surface whose real maps live in the CDB. Blast radius is all Starfield rendering. The severity here is for the **untracked deferral** (no open issue, no ROADMAP row), not for re-litigating the closed Phase-1 work.
- **Suggested Fix**: File a tracking issue for "CDB Phase 2 — per-field extraction into `ImportedMaterial`", and pin the checklist invariant ("`.mat` paths land in named `MaterialTextureSet` roles, never a CDB slot index") now with a test, so it's enforced before the extraction code exists.

**Existing, reconfirmed live (not re-filed):**
- **#2109 (SF-D9-02, OPEN, LOW)** — BGEM v21/v22 glass-overlay params + envmap-mask-scale + v11 emittance still dropped in merge; premise verified against current code.
- **#2108 (SF-D9-01, OPEN, MEDIUM)** — `EFFECT_PALETTE_COLOR`/`ALPHA` still derived from LUT-texture presence, not the authored palette-enable flag; premise verified, two existing tests currently pin the wrong behavior.

---

## CRC32 Flag Table

`crates/nif/src/shader_flags.rs::bs_shader_crc32` — the CRC32 hash-to-flag-name table for the FO76+/Starfield shader-flag arrays (`sf1_crcs`/`sf2_crcs`, gated BSVER >= 132 / >= 152). Contrary to the audit checklist's open question, this table is **not opaque** — all 30 known CRC32 values are named, documented with their nif.xml source name, and consumed by the modern shader-flag classifier family in `crates/nif/src/import/material/mod.rs`:

| Flag name | CRC32 (decimal) | Consumer |
|---|---|---|
| `DECAL` | 3849131744 | `is_decal_from_modern_shader_flags` |
| `DYNAMIC_DECAL` | 1576614759 | `is_decal_from_modern_shader_flags` |
| `TWO_SIDED` | 759557230 | `is_two_sided_from_modern_shader_flags` |
| `CAST_SHADOWS` | 1563274220 | — |
| `ZBUFFER_TEST` | 1740048692 | — |
| `ZBUFFER_WRITE` | 3166356979 | — |
| `VERTEX_COLORS` | 348504749 | — |
| `PBR` | 731263983 | — |
| `SKINNED` | 3744563888 | — |
| `ENVMAP` | 2893749418 | — |
| `VERTEX_ALPHA` | 2333069810 | — |
| `FACE` | 314919375 | — |
| `GRAYSCALE_TO_PALETTE_COLOR` | 442246519 | `modern_effect_shader_bit` family |
| `HAIRTINT` | 1264105798 | — |
| `SKIN_TINT` | 1483897208 | — |
| `EMIT_ENABLED` | 2262553490 | — |
| `GLOWMAP` | 2399422528 | — |
| `REFRACTION` | 1957349758 | — |
| `REFRACTION_FALLOFF` | 902349195 | — |
| `NOFADE` | 2994043788 | — |
| `INVERTED_FADE_PATTERN` | 3030867718 | — |
| `RGB_FALLOFF` | 3448946507 | — |
| `EXTERNAL_EMITTANCE` | 2150459555 | — |
| `MODELSPACENORMALS` | 2548465567 | — |
| `TRANSFORM_CHANGED` | 3196772338 | — |
| `EFFECT_LIGHTING` | 3473438218 | — |
| `FALLOFF` | 3980660124 | — |
| `SOFT_EFFECT` | 3503164976 | `modern_effect_shader_bit` family |
| `GRAYSCALE_TO_PALETTE_ALPHA` | 2901038324 | `modern_effect_shader_bit` family |
| `WEAPON_BLOOD` | 2078326675 | — |
| `LOD_OBJECTS` | 2896726515 | — |
| `NO_EXPOSURE` | 3707406987 | — |

Not every named constant currently has a dedicated call site (several are pinned for future use / documentation completeness), but none of the 30 are opaque raw hashes — each carries a nif.xml-sourced name and a doc comment.

## Remaining-Work Chain

Per `starfield-esm-roadmap.md`, ESM Phases 0+1 are done and Phases 2-4 are invalidated by the 99.9%-parity measurement. The genuine remaining-work chain, in order:

1. **Per-field CDB material extraction (#1289 Phase 2 follow-up)** — `.mat`-resolved materials currently reach the Disney BSDF lobe with NIF-derived defaults, not CDB-authored values. This is the single highest-value remaining item for Starfield visual fidelity; per this audit's Dimensions 3 and 9, **it currently has no open GitHub issue or ROADMAP row tracking it** — recommend filing one (see SF-D9-2026-08-03-03).
2. **Exterior worldspace tiles** — not yet in scope for Starfield (Cydonia is an interior).
3. **Space-cell / planet / GBFM records** — the GBFM/PNDT/STDT/BIOM Starfield-only base types are conscious stubs (36+26+12+5.3 MB of Starfield.esm), a deliberate Phase-3 deferral, not a defect.
4. **#746/#747 NIF truncation tail** — closed/superseded by #2105; a residual 6/29,849-file `BSWeakReferenceNode` tail with an unexplained-but-unchanged root cause remains in MeshesPatch.

This is **not** "BGSM parser first, ESM very far" — both have shipped; the CDB per-field extraction is a rendering-fidelity gap on top of an already-working load/spawn/parse pipeline.

## Deduplication Summary

Checked all 12 new findings against `/tmp/audit/issues.json` (200 most recent GitHub issues) before filing. Confirmed still-live and unchanged: #2097 (LZ4-01), #2108 (SF-D9-01), #2109 (SF-D9-02), #1576 (SF-D4-03), #1358 (CLOSED, residual named separately). No duplicates filed.

## Finding Count Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 7 |
| LOW | 8 |
| **Total** | **16** |

MEDIUM: SF-BA2-01, SF2D2-03, SF-D9-2026-08-03-01, SF-D9-2026-08-03-03
LOW: SF-BA2-02, SF2D2-04, SF2D2-05, SF2D2-06, SF2D2-07, SF-D5-2026-08-03-01, SF-D7-01, SF-D9-2026-08-03-02

---

Suggested next step: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-08-03.md`
