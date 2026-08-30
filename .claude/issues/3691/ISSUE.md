# #3691 — PERF-D8-2026-08-30-03: the skinning parse path — the parser's largest per-block allocation family — has zero gate coverage, and carries two unreserved growth sites

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D8-2026-08-30-03`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: low,performance,nif-parser,nif,test-gap,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3691

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: LOW
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/skin.rs:299` (`NiSkinPartition` strip branch) · `crates/nif/src/import/mesh/sse_recon.rs:134` (`reconstruct_sse_geometry`) · gate gap: `crates/nif/tests/heap_allocation_bounds*.rs` (no fixture declares `NiSkinData` / `NiSkinPartition` / `BSDismemberSkinInstance`)
- **Status**: NEW
- **Description**: Two sites grow a file-driven bulk vector from `Vec::new()` even though the element count is already in hand, which is the exact `#833` / `#831` pattern the `allocate_vec` / `read_pod_vec` family exists to remove:
  1. `blocks/skin.rs:299` — `let mut triangles = Vec::new();` then, in the strip branch, `triangles.extend(destrip(&strip))` once per strip (`:311-314`). `num_triangles` was read 47 lines earlier at `:252`. The sibling non-strip branch at `:318` correctly bulk-reads via `read_u16_triple_array`.
  2. `import/mesh/sse_recon.rs:134` — `let mut indices = Vec::new();` then three `push`es per triangle across every partition (`:141-153`). The total is `partition.partitions.iter().map(|p| p.triangles.len()).sum() * 3`, computable up front.

  Neither is reachable from any dhat fixture, so neither has a CI floor, and neither would be caught by a regression that made them worse.
- **Evidence**: `skin.rs:249-360` — `num_vertices` / `num_triangles` / `num_bones` / `num_strips` read at `:251-255`; `triangles` initialised `Vec::new()` at `:299`; strip `extend` loop `:311-314`. `sse_recon.rs:133-158` — `vertex_count` known at `:133`, `indices` `Vec::new()` at `:134`, `push` ×3 at `:150-152`. The module's own comment (`sse_recon.rs:113-127`) measures the corpus at 18,753,141 triangles over 26,940 skinned shapes, i.e. ~696 tri/shape ≈ 2,088 `push`es and ~11 reallocations per shape on a Skyrim actor/facegen load.
- **Impact**: Small in absolute terms (~11 doubling reallocs + ~8 KB of memcpy per skinned shape), but it is the guarded pattern re-appearing in the *one* block family that no gate watches, on the game with the largest skinned-content footprint. The real risk is the gate gap: `NiSkinPartition` is the deepest nested file-driven allocator in the parser (`allocate_vec(num_partitions)` → per partition `read_u16_array(num_vertices)` + `read_f32_array(num_vertices × num_weights_per_vertex)` + `read_bytes(num_vertices × num_weights_per_vertex)`), and nothing bounds it.
- **Related**: #833, #831, #1549 (the de-strip landing), #3355 (the SSE triangle bound retarget); PERF-D8-2026-08-30-02 (same gate).
- **Suggested Fix**: `skin.rs:299` → `let mut triangles = stream.allocate_vec_sized::<[u16; 3]>(num_triangles as u32)?;` (the sized variant, since `[u16;3]` has an honest 6-byte wire size). `sse_recon.rs:134` → `Vec::with_capacity(partition.partitions.iter().map(|p| p.triangles.len()).sum::<usize>() * 3)` (in-memory count, no file-driven bound needed). Separately, extend the import bound file proposed in Finding 02 with a fixture carrying one `NiSkinData` + one `NiSkinPartition` (2 bones, 8 verts, 4 tris) so the family gets a CI floor at all.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
