# SKY-2026-08-27-D1-01: SSE `SkinPartition.Triangles` already hold global indices — remapping them through `vertex_map` drops 17.6% of skinned triangles and mis-indexes 35.6% more

Labels: critical,nif-parser,nif,bug,game:skyrim,legacy-compat

- **Severity**: CRITICAL
- **Confidence**: CONFIRMED (read the code + nifly + measured against Skyrim SE archives)
- **Location**: `crates/nif/src/import/mesh/sse_recon.rs:111-136` (the remap loop; the lookup
  itself at `sse_recon.rs:120`)
- **Description**:
  `try_reconstruct_sse_geometry` treats each `NiSkinPartition.partitions[i].triangles` entry as
  a **partition-local** index and translates it through `part.vertex_map` to reach the global
  packed-buffer vertex space:

  ```rust
  for (i, &local) in tri.iter().enumerate() {
      match part.vertex_map.get(local as usize) {
          Some(g) => globals[i] = g,
          None => { ok = false; break; }
      }
  }
  ```

  On Skyrim SE that translation is a no-op at best and a corruption at worst, because the
  `Triangles` field of an SSE `SkinPartition` is **already** expressed in the shape's global
  vertex space. nifly states this explicitly and unconditionally for `Stream() == 100`
  (BSVER 100 = Skyrim SE), in `NiSkinPartition::Sync`
  (`/mnt/data/src/reference/nifly/src/Skin.cpp:82-85`):

  ```cpp
  if (stream.GetVersion().User() >= 12 && stream.GetVersion().Stream() == 100) {
      if (stream.GetMode() == NiStreamReversible::Mode::Reading)
          bMappedIndices = false;
  ```

  and documents the meaning of that flag in `include/Skin.hpp:105-109`:

  ```cpp
  // bMappedIndices is not in the file; it is calculated from
  // the file version.  If true, the vertex indices in triangles
  // and strips are indices into vertexMap, not the shape's vertices.
  // trueTriangles always uses indices into the shape's vertex list.
  bool bMappedIndices = true;
  ```

  With `bMappedIndices == false`, nifly's `PrepareTrueTriangles` takes the branch
  `p.trueTriangles = p.triangles;` (`Skin.cpp:422-434`) — i.e. the `Triangles` field *is* the
  true/global list on SSE, and the trailing `Triangles Copy` (`nif.xml:2168`) is a duplicate of
  it, not a differently-indexed sibling.

  Two consequences in HEAD:
  1. A raw index `>= vertex_map.len()` is treated as malformed content and the whole triangle
     is **dropped** (the `#725 / NIF-D4-04` policy). It is not malformed; it is a perfectly
     valid global index that simply exceeds this partition's *vertex count*.
  2. Every raw index `< vertex_map.len()` is silently **replaced** by `vertex_map[index]`,
     which points at a different, unrelated vertex.

  The parser side is not at fault — `NiSkinPartition::parse`
  (`crates/nif/src/blocks/skin.rs:299-352`) reads `Triangles` at the correct wire position and
  skips the trailing `Triangles Copy`; only the *interpretation* downstream is inverted.

- **Evidence**: measured over both vanilla SSE mesh archives
  (`/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim - Meshes0.bsa`
  and `Meshes1.bsa`) with a throwaway `#[ignore]` probe in `crates/nif/tests/` (since deleted).

  Proof that the indices are global, not local — every single triangle index is a **member of
  its own partition's `vertex_map` values**, which is the definition of a global index
  belonging to that partition:

  ```
  partitions_with_tris=40599 subset_of_map_VALUES_ok=40599 bad=0
  lookups=56259423 indices_that_are_map_VALUES=56259423 indices_within_map_LENGTH=48042230
  ```

  56,259,423 of 56,259,423 indices are vertex_map *values*; only 48,042,230 are within
  vertex_map's *length*. Under the local-index reading, 8,217,193 lookups (14.6%) are
  out-of-range garbage; under the global reading, zero are:

  ```
  tri_index_lookups=56259423 raw_in_range_global=56259423 raw_oob_global=0
  remap_changes_index=21306465 remap_drops=8217193
  ```

  A concrete shipped example (a partition whose `vertex_map` is a scattered global set):

  ```
  EXAMPLE meshes\actors\character\facegendata\facegeom\skyrim.esm\00045c59.nif
      part nverts_buf=996 p.num_vertices=30 map_len=30
      map[0..8]=[244, 235, 243, 245, 252, 253, 254, 258]
      tri0=Some([244, 235, 243])
  ```

  `triangles[0] == [244, 235, 243]` is literally `vertex_map[0..3]`. Under the local reading
  those indices are 8× past the end of a 30-entry map and the triangle is dropped; under the
  global reading they address vertices 244/235/243 of a 996-vertex buffer, which is exactly
  what the map says this partition covers.

  Damage on the shipped corpus (per `NiSkinPartition` block carrying a global vertex buffer):

  ```
  blocks=26913 clean_blocks=16412 damaged_blocks=10501 total_loss_blocks=0
  blk_tris=18753141 blk_dropped=3297664 blk_corrupted=6681098
  ```

  and split by the shape type that consumes it:

  ```
  BSDynamicTriShape: recon_path_shapes=21139 damaged=8637 tris=12199358 dropped=2521012
  BSTriShape:        recon_path_shapes=5801  damaged=1864 tris=6559972  dropped=776652
  ```

  The 61% that come out clean are the single-partition shapes whose `vertex_map` happens to be
  the identity permutation (`identity_maps=16551` of `40599` partitions) — for those the wrong
  remap is accidentally a no-op. The 10,203 multi-partition shapes are where it bites.

  For context, every one of these shapes reaches the reconstructor:
  `extract_bs_tri_shape` (`crates/nif/src/import/mesh/bs_tri_shape.rs:27-31`) calls
  `try_reconstruct_sse_geometry` whenever `shape.triangles.is_empty()`, and a census of all
  81,226 SSE `BSTriShape` blocks found 26,978 with empty inline triangles against 26,913
  partition blocks with a global vertex buffer — i.e. essentially all of them.

  None of this is visible to the existing gates. `cargo test -q -p byroredux-nif` is green, and
  the real-archive gate reports a perfect score:

  ```
  [Skyrim SE] parsed 32709/32709 NIFs: clean 100.00% (32709 clean / 0 truncated / 0 failed)
  ```

  The synthetic unit tests actively lock the bug in:
  `crates/nif/src/import/mesh/sse_skin_geometry_reconstruction_tests.rs:363-444`,
  `partition_vertex_map_remaps_local_indices_to_global`, fabricates `vertex_map = [2, 0, 1]`
  with triangle `[0, 1, 2]` and asserts the output is `[2, 0, 1]`; and
  `partition_triangle_with_out_of_range_vertex_map_index_is_dropped` (line 694) asserts the
  drop policy. Both are hand-built fixtures, not extracted from shipped bytes, so they
  never contradicted the archives.

- **Impact**: 10,501 of 26,940 SSE skinned shapes (39.0%) import with mangled index buffers.
  3,297,664 of 18,753,141 triangles (17.6%) are silently discarded and 6,681,098 more (35.6%)
  reference the wrong vertices — only 46.8% survive intact. The population is exactly Skyrim's
  character content: 8,637 damaged facegen head meshes (`BSDynamicTriShape`, 20.7% of their
  triangles dropped) and 1,864 damaged skinned bodies/creatures (`BSTriShape`, 11.8% dropped).
  Visually this is holed and spike-shot NPC faces and bodies, worst on the multi-partition
  meshes. No shape is lost outright (`total_loss_blocks=0`), which is why it reads as
  corruption rather than as missing content. It also poisons everything derived from the index
  list downstream: `build_triangles_for_synth` → `synthesize_tangents_yup`
  (`import/mesh/bs_tri_shape.rs:175-225`) synthesizes the tangent basis from the corrupted
  triangles, and `extract_local_bound` / BLAS construction inherit the same list.

- **Suggested Fix**: gate the remap on whether the partition is an SSE global-buffer partition.
  Inside `try_reconstruct_sse_geometry` the answer is unconditionally "yes" — the function only
  runs when `partition.global_vertex_data` is `Some`, which `NiSkinPartition::parse` populates
  only for `bsver` in `SKYRIM_SE..FALLOUT4` (`blocks/skin.rs:231-245`). So the loop should
  consume `part.triangles` directly as `u32` global indices, with a bounds check against the
  decoded buffer's vertex count (`decoded.positions.len()`) replacing the `vertex_map` lookup;
  keep the "drop the whole triangle" policy for anything genuinely out of range, which the
  corpus says is zero (`raw_oob_global=0`). Do **not** remove the `vertex_map` reads in
  `remap_bs_tri_shape_bone_indices` (`import/mesh/skin.rs:480-488`) — that one uses `vertex_map`
  correctly, as a global→partition inverse. Replace/retarget the two synthetic tests named
  above with fixtures that model the global-index layout, ideally seeded from a real
  `facegeom` partition. Fix in the same change as SKY-2026-08-27-D1-02, which currently masks
  itself against this bug.

- **Related**: checked the 300-issue dedup baseline (84 open, fetched 2026-08-27) — no open or closed issue covers SSE partition
  triangle index semantics. Not #3221/#3219/#3103/#3071 (open Skyrim), not #3176/#3177
  (tangent guards), not #3187 (slot swap). The code comment at `sse_recon.rs:104-110` cites
  #725 / NIF-D4-04 as the origin of the drop policy; that fix was applied on the inverted
  premise and its claim that "vanilla Bethesda BSAs always supply complete vertex_maps" is
  falsified by the 8.2M out-of-range lookups measured above.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
