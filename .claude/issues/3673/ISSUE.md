# #3673 — PERF-D8-2026-08-30-02: the dhat allocation gate stops at `parse_nif` — the import tier it should also cover is ~2× the peak live heap and 3–5× the CPU

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D8-2026-08-30-02`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,nif-parser,test-gap,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3673

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: `crates/nif/tests/heap_allocation_bounds.rs:150,352,454` · `crates/nif/tests/heap_allocation_bounds_geometry.rs:227` · guarded-but-unguarded code: `crates/nif/src/import/mod.rs:119` (`import_nif_scene`), `:536` (`import_nif_with_collision_and_resolver`)
- **Status**: NEW
- **Description**: All four bound tests call exactly one function — `byroredux_nif::parse_nif`. The `#831` / `#832` / `#833` / `#408` allocation discipline they exist to enforce at CI cadence applies just as much to the import tier (`import/mesh/`, `import/material/`, `import/walk/`, `import/collision/`), which is where the per-vertex / per-bone / per-target **output** vectors are actually built. A regression that reverted `import_nif_scene_impl`'s `#835` pre-sizing, or introduced a per-vertex `push` growth in `decode_sse_shape_buffer` / `extract_morph_targets`, would pass every current gate. This is a defense-in-depth gap in the gate itself, not merely absent test coverage: the gate's stated purpose (`heap_allocation_bounds.rs:1-31`) is to promote the allocation pins from audit cadence to CI cadence, and it currently promotes the smaller half.
- **Evidence**: dhat-instrumented run over a 400-file `Skyrim - Meshes0.bsa` sample, taking `HeapStats::max_bytes` immediately after `parse_nif` and again after `import_nif_scene` + `import_nif_lights` + `import_nif_particle_emitters` + `import_embedded_animations`:

  ```
  peak_after_parse_max = 5_325_133 B     peak_after_import_max = 12_360_375 B   (2.32x)
     peak_all=12_360_375  peak_parse= 5_325_133  meshes\effects\dragondeathtestexport.nif
     peak_all=10_098_464  peak_parse= 3_975_580  meshes\furniture\blacksmithingskyforgemarker.nif
     peak_all= 5_176_305  peak_parse= 2_599_361  meshes\architecture\whiterun\wrbuildings\wrtempleofk01.nif
     peak_all= 2_636_212  peak_parse= 1_367_976  meshes\dlc02\architecture\telvannitower\dlc2telvannigourdhouseext01.nif
  ```
  The top-8 worst NIFs all land in the 2.0–2.3× band. CPU split is in Finding 01's table.
- **Impact**: The one quantitative, CI-enforced allocation contract on the NIF load path bounds the cheaper half of it. Every allocation-hygiene finding in the import tier will keep being re-derived by hand at audit cadence — which is exactly the failure mode #1247 was filed to end.
- **Related**: #1247, #1381, #1763, #2114 (the four gate-landing issues); PERF-D8-2026-08-30-01; the 2026-08-24 report's Dim 8 note on `import/mesh/morph.rs` being "not dhat-bound yet".
- **Suggested Fix** (concrete, measured): add a third bound file `crates/nif/tests/heap_allocation_bounds_import.rs` (its own binary — `dhat::Profiler` is a process singleton), reusing `build_fo4_packed_vertex_nif(16)` from `heap_allocation_bounds.rs` but widened to ~256 vertices, and wrapping `parse_nif` **plus** `import_nif_scene(&scene, &mut StringPool::new())` in one profiler scope. Register it in the existing `nif-heap-allocation-bounds` CI job (`.github/workflows/ci.yml:182-185`). Pin the initial bound at the same ~5× headroom the siblings use, measured on first landing; the 2.0–2.3× parse→import ratio above is the sanity check the number must sit above. Bumping the packed-vertex fixture to 256 verts also gives the bound a slope, so a per-vertex `push`-growth revert shows up as a super-linear jump rather than a constant.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
