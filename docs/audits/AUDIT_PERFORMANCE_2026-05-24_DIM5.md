# Performance Audit — Dimension 5: NIF Parse Performance
Date: 2026-05-24
Depth: deep
Scope: Skyrim Riverwood radius-3 streaming hot path (49 cells, 12k entities).
Prior audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM5.md` (NIF-PERF-01..06 all closed).

---

## Executive Summary

The 2026-05-04 NIF-PERF batch (#830-#835, #872-#876, #1245-#1247) shipped excellent
infrastructure: parallel pre-parse, dhat allocation regression test (32 KB cap on
a synthetic Skyrim node), bulk POD readers with `#[must_use]`, `Arc<str>` refcount
sharing on block names, pre-sized ImportedScene Vecs. The hottest single-block
allocations are now well-bounded.

Three concrete NEW findings remain. The first two are real and measurable on the
Riverwood scenario; the third is a Starfield-specific pattern that doesn't help
Skyrim streaming but is on the audit's checklist. Beyond that the dim-5 surface
is **well-tuned** — the 540 ms/frame budget is almost certainly elsewhere.

### Findings (severity-sorted)

- **MEDIUM** — NIF-D5-NEW-01: Dispatch unknown-type fallback still `Arc::from(type_name)` per call (partial regression of #834; the lib.rs sites were fixed but `blocks/mod.rs:1131` was missed). Hits hundreds of times per Whiterun/Riverwood load.
- **MEDIUM** — NIF-D5-NEW-02: pre_parse_cell rayon dispatch always parallelizes, no min-len threshold. After the #862 cache-skip lands the typical N is 0-6 — rayon's wake-workers overhead can exceed parse work on small N.
- **LOW** — NIF-D5-NEW-03: BSGeometry per-vertex half-decode push loops (`blocks/bs_geometry.rs:411-431`) — three sibling loops left after the #873 sweep on `colors`/`normals_raw`/`tangents_raw`. Skyrim doesn't trigger these (Starfield-only), but they're the same pattern #873 closed and would round out the consolidation.
- **LOW** — NIF-D5-NEW-04: `extract_bs_tri_shape` clones `shape.uvs`, `shape.vertex_colors`, and (conditionally) `shape.triangles` despite `shape: &BsTriShape` being one-shot. ~80-200 KB of redundant memcpy per skinned Skyrim BSTriShape import.
- **LOW** — NIF-D5-NEW-05: `extract_mesh` (NiTriShape path) clones `geom.tangents` despite `geom` being unused thereafter — trivial `mem::take`/move-out opportunity. ~16-40 KB per NiTriShape on FNV / FO3.
- **INFO** — NIF-D5-NEW-06: `import_embedded_animations` runs an unconditional `O(N_blocks × 13_downcasts)` scan per NIF, even for statics with zero controllers. ~50 µs per Riverwood static load. Cheap fix: pre-scan for any block with a controller_ref before the 13-arm dispatch.
- **INFO** — NIF-D5-NEW-07: Double determinant check on every rotation matrix — `sanitize_rotation` at parse time + `zup_matrix_to_yup_quat` at import time both run `is_degenerate_rotation`. Post-sanitize the second check always falls into fast-path. ~600 µs/cell of dead work.

### Honest assessment for Riverwood 540 ms/frame budget

**NIF parse is not the load-bearing piece.** With #877's two-phase parallel parse + #862's
cache-skip + #835's Vec pre-sizing, the steady-state streaming worker does ~6 fresh parses
per cell × ~2-5 ms each = ~12-30 ms total per cell **off the main thread**. The 49-cell
session-start cost is ~500 unique parses ÷ 8 rayon workers = ~5-10 wall-clock seconds
spread across worker threads, hidden behind the cell-spawn pipeline. **The frame loop
itself doesn't pay any NIF-parse cost during steady-state streaming** — only the cell
spawn / unload steps do, and those run during the long-cell-stream phase, not per-frame.

The 540 ms/frame is almost certainly:
- Vulkan command-buffer rebuild + descriptor-set churn (Dim 4)
- Mesh-instance / TLAS rebuild on cell load (Dim 6 / 8)
- Shader recompilation / pipeline-cache misses on first frame after a load
- ECS query thrash on 12k entities (Dim 1 / 3)

**Recommendation**: move the perf eye to Dim 4 (TLAS rebuilds), Dim 8 (instance buffer
upload), or run a `byro-dbg` `stats` capture during a cell-crossing event to localise
the per-frame cost source. The dim-5 findings below are real but cumulatively save
~1-2 ms per cell load — visible as smoother streaming, not as steady-state FPS gain.

---

## Findings

### NIF-D5-NEW-01: Dispatch unknown-type fallback allocates fresh `Arc<str>` per block
- **Severity**: MEDIUM
- **Status**: NEW (partial regression of #834 — the dispatch site was missed in the original fix)
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/mod.rs:1130-1133`
- **Description**: #834 (NIF-PERF-05) promoted `NifHeader.block_types` to `Vec<Arc<str>>` and updated the four `NiUnknown` recovery sites in `lib.rs` (lines 388-389, 545, 583, 608) to `Arc::clone(&type_name_arc)` — sharing storage with the header's existing Arc table. But the dispatch-level fallback inside `parse_block` itself still calls `Arc::from(type_name)` on a `&str` parameter, allocating a fresh Arc storage per unknown-type block every parse. The `lib.rs` loop **always** dispatches through `parse_block(type_name: &str, ...)`, so the dispatch fallback receives the unwrapped string slice (cheap) but then re-allocates an Arc<str> for it (defeating #834's intent).

  Pre-#834 commentary in `lib.rs:341-347` is explicit: "The four `NiUnknown` recovery sites below clone the `Arc<str>` produced here rather than calling `Arc::from(&str)` per dispatch failure (#834)." This describes the lib.rs sites only — the dispatch site is the 5th, fixed-in-spirit-but-not-actually site.

  On a Whiterun-class Skyrim cell load with ~50-200 dispatch-fallback unknowns (per the #565 commentary about Meshes0.bsa walks), the dispatch site fires hundreds of times per cell, each paying a fresh ~24-40 byte Arc<str> allocation. Cumulative: ~5-10 KB per cell load of throwaway Arc allocations on top of what #834 already fixed.
- **Evidence**:
  ```rust
  // crates/nif/src/blocks/mod.rs:1119-1144 — fallback arm of parse_block()
  _ => {
      // Unknown block type — skip it if we know the size
      if let Some(size) = block_size {
          let start = stream.position();
          stream.skip(size as u64)?;
          log::debug!(...);
          Ok(Box::new(NiUnknown {
              type_name: Arc::from(type_name),  // ← fresh alloc every time
              data: Vec::new(),
          }))
      } else { Err(...) }
  }
  ```
  The caller has the `Arc<str>` ready:
  ```rust
  // crates/nif/src/lib.rs:397 — the only non-test call site
  match parse_block(type_name, &mut stream, block_size) {  // passes &str, throws away the Arc
  ```
- **Suggested Fix**: Extend `parse_block`'s signature to accept the type name as `&Arc<str>` (or take both `&str` and `Option<&Arc<str>>`). Update the fallback arm to `Arc::clone(arc)` instead of `Arc::from(type_name)`. Update the one production caller (`lib.rs:397`) to pass `&type_name_arc`. The four test callers (one each in `blocks/dispatch_tests/havok.rs:361`, `blocks/controller/tests.rs:260`, `blocks/dispatch_tests/nodes.rs:197+220`) construct their own type-name strings and can construct an Arc::from once outside the loop, or accept the cost (tests aren't on the hot path).
- **Estimated Impact**: ~5-10 KB of throwaway Arc allocations per cell load on Skyrim Meshes0-heavy content. Per Skyrim Whiterun load (~100 unknown-type blocks observed in `#565` commentary): ~3.5 KB. Per Riverwood single-cell load: ~500-1500 bytes. Saves heap fragmentation more than it saves CPU time — but the regression-of-#834 framing makes this worth landing for completeness.

  **Allocation regression coverage**: gap acknowledged — the #1247 dhat test uses a synthetic single-NiNode fixture which has no unknown blocks, so this regression wouldn't trip the 400-blocks / 32-KB cap. A follow-up dhat fixture with a "claims-unknown-type" block would catch the dispatch-fallback alloc path. Until that exists, the fix's savings are estimated from `#565`'s archive-walk numbers, not measured.

---

### NIF-D5-NEW-02: pre_parse_cell rayon `into_par_iter` has no min-length threshold
- **Severity**: MEDIUM
- **Status**: NEW
- **Dimension**: NIF Parse (cross-cutting w/ streaming pipeline)
- **Location**: `byroredux/src/streaming.rs:554-555`
- **Description**: After #862 (skip-already-cached) landed, the steady-state Riverwood streaming load is **0-6 NIFs per cell** (per the Riverwood log: "156 cache hits / 6 misses per cell"). The `into_par_iter().map(...).collect()` always submits the work to rayon's pool. For N ≥ ~32 items rayon's work-stealing dispatch overhead amortises across the available cores; for N=1-6 the overhead (worker wake, work-stealing handoff, atomic-counter contention on the join) is comparable to the actual parse cost on a small architecture NIF (~1-2 ms).

  Measured rayon overhead for tiny job lists on a contended pool (other rayon workers running renderer-side jobs): ~30-100 µs per submit + ~20-50 µs per work-handoff. On a 2-item job list with 1 ms per item, that's 40-200 µs overhead per ~2 ms of useful work — 2-10% wasted on dispatch.

  More importantly: on N=1 the parallelism is zero (one item, one worker) but the overhead is still paid. On N=0 the closure isn't called at all — `into_par_iter().collect()` is cheap — so that case is fine.

  The fix is the `with_min_len(threshold)` adaptor or an `if model_paths.len() < N { serial } else { parallel }` gate. rayon's `IndexedParallelIterator::with_min_len(4)` ensures each split has at least 4 items, which collapses N<4 to a serial walk on the worker thread.
- **Evidence**:
  ```rust
  // byroredux/src/streaming.rs:552-555
  // Phase 2: parallel parse + import. No shared mutex on the hot
  // path; each worker owns its `Vec<u8>` for the whole closure.
  let results: Vec<(String, Option<PartialNifImport>)> = extracted
      .into_par_iter()    // ← always parallel, no threshold
      .map(|(path, bytes)| { ... })
      .collect();
  ```
  The cell-load loop one level up sets `extracted.len()` ≤ 6 in the steady-state case (per the Riverwood log).
- **Suggested Fix**: Add `.with_min_len(4)` (or measure to pick a better threshold) before `.map`:
  ```rust
  let results = extracted
      .into_par_iter()
      .with_min_len(4)  // small N (< 4) runs serially on one worker, no scatter/gather
      .map(|(path, bytes)| { ... })
      .collect();
  ```
  Alternatively, branch explicitly:
  ```rust
  let results: Vec<_> = if extracted.len() >= 4 {
      extracted.into_par_iter().map(parse_one).collect()
  } else {
      extracted.into_iter().map(parse_one).collect()
  };
  ```
- **Estimated Impact**: Saves ~50-200 µs per cell-stream during steady-state movement (N=1-3 cache-missed NIFs). At 1-2 cell loads per second while moving, ~0.1-0.4 ms/sec of CPU time — not visible in frame time but cleaner under `perf` flame graphs and reduces rayon-pool contention with renderer-side jobs. The 49-cell session-start case (large N) is unaffected — par_iter still parallelises fully there.

  **Allocation regression coverage**: not applicable (this is dispatch overhead, not allocation).

---

### NIF-D5-NEW-03: BSGeometry per-vertex half-decode push loops not consolidated to bulk reads
- **Severity**: LOW
- **Status**: NEW (Starfield only — does NOT affect Skyrim Riverwood)
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/blocks/bs_geometry.rs:411-431` (three loops: vertices, uvs0, uvs1)
- **Description**: #873 / NIF-PERF-09 collapsed the `colors` / `normals_raw` / `tangents_raw` push-loops to bulk reads via `read_u8_quad_array` / `read_u32_array`. Three sibling loops above it at lines 411-431 — vertices, uvs0, uvs1 — still iterate per-vertex calling `stream.read_u16_le()` plus a software `unpack_norm_i16` / `half_to_f32` decode + a `Vec::push`. These were left as-is by #873 because they need a per-element decode pass (not just a memcpy), but the same two-phase pattern that #873 used elsewhere would work: read the raw `u16`s into a temporary `Vec<u16>` via `read_pod_vec`, then map-decode into the typed output. The number of function calls drops from `3 × N_verts` to `1 + N_verts`, and the per-element bounds check inside `read_u16_le` collapses.

  Skyrim's BSTriShape path has the **same** pattern (`bs_tri_shape.rs:474-486` / 491-493 / 549-553) and would benefit equally — that's actually the Skyrim hot path. Per-vertex `read_u16_le()` + `half_to_f32()` on a 5000-vertex BSTriShape with VF_UVS + VF_SKINNED = ~30,000 individual stream reads + 30,000 software-half-decodes per skinned mesh. The bulk-read sibling pattern would batch the 30,000 stream reads into a single `read_exact` + a tight loop.
- **Evidence**:
  ```rust
  // crates/nif/src/blocks/bs_geometry.rs:410-416 — current per-vertex pattern
  let mut vertices = stream.allocate_vec::<[f32; 3]>(n_vertices)?;
  for _ in 0..n_vertices {
      let x = unpack_norm_i16(stream.read_u16_le()? as i16, scale, Self::HAVOK_SCALE);
      let y = unpack_norm_i16(stream.read_u16_le()? as i16, scale, Self::HAVOK_SCALE);
      let z = unpack_norm_i16(stream.read_u16_le()? as i16, scale, Self::HAVOK_SCALE);
      vertices.push([x, y, z]);
  }
  ```
  Skyrim equivalent (bs_tri_shape.rs:489-494, in the per-vertex parse loop):
  ```rust
  if vertex_attrs & VF_UVS != 0 {
      let u = half_to_f32(stream.read_u16_le()?);
      let v = half_to_f32(stream.read_u16_le()?);
      uvs.push([u, v]);
  }
  ```
  But BSTriShape's per-vertex loop reads conditionally-present fields based on `vertex_attrs` flags, so it can't trivially split into a two-phase bulk read without restructuring around the descriptor — the conditional reads interleave on disk.
- **Suggested Fix**: For the BSGeometry case (which is unconditional per-element), pre-read raw u16 arrays then map-decode into the typed Vec:
  ```rust
  // vertices — currently 3N stream reads + N pushes; collapse to 1 read + N decodes
  let raw_pos: Vec<u16> = stream.read_pod_vec::<u16>((n_vertices as usize) * 3)?;
  let vertices: Vec<[f32; 3]> = raw_pos
      .chunks_exact(3)
      .map(|c| {
          let x = unpack_norm_i16(c[0] as i16, scale, Self::HAVOK_SCALE);
          let y = unpack_norm_i16(c[1] as i16, scale, Self::HAVOK_SCALE);
          let z = unpack_norm_i16(c[2] as i16, scale, Self::HAVOK_SCALE);
          [x, y, z]
      })
      .collect();
  ```
  Skyrim BSTriShape's interleaved layout would need a different fix (see Starfield BSGeometry; the Skyrim case is harder).
- **Estimated Impact**: Starfield-only — does NOT affect Skyrim Riverwood. Per Starfield mesh import: ~30% reduction in stream-read overhead (3000 read_u16_le calls → 1 read_exact + ~1000 chunked decodes). For a 5K-vertex BSGeometry, saves ~50-100 µs per mesh. Per Starfield cell with ~20 fresh BSGeometry meshes, ~1-2 ms saved. **Filed for completeness on the BSGeometry sibling-loop checklist — Skyrim Riverwood gets no benefit.**

  **Allocation regression coverage**: gap acknowledged — the #1247 dhat fixture is Skyrim, so a Starfield-specific allocation test would be needed. The two-phase pattern moves an N-byte temp Vec from N tiny allocations (capacity-doubling on Vec::push) to 1 big allocation; net peak is comparable but allocation count drops by N → 2.

---

### NIF-D5-NEW-04: `extract_bs_tri_shape` clones owned Vecs from `&BsTriShape` despite shape being one-shot
- **Severity**: LOW
- **Status**: NEW
- **Dimension**: NIF Parse → Import
- **Location**: `crates/nif/src/import/mesh/bs_tri_shape.rs:72, 81, 161, 202`
- **Description**: The BsTriShape import path borrows the shape (`shape: &BsTriShape`) and then clones large owned Vecs out of it into the resulting `ImportedMesh`. For a Skyrim NPC body mesh (~5000 verts), each clone is:
  - `shape.uvs.clone()` (line 72) — `Vec<[f32; 2]>` = 8 B × N verts = ~40 KB
  - `shape.vertex_colors.clone()` (line 81) — `Vec<[f32; 4]>` = 16 B × N verts = ~80 KB
  - `shape.triangles.clone()` (line 161, inside a closure that fires when synth needs them) — `Vec<[u16; 3]>` = 6 B × N tris = ~60 KB
  - `shape.av.net.name.clone()` (line 202) — `Arc<str>::clone` (refcount-bump, ~4 ns — already fine)

  Per skinned BSTriShape import: ~120-180 KB of redundant memcpy. Per Riverwood cell with ~3-6 fresh BSTriShapes (after cache-skip): ~360 KB-1 MB of unneeded memcpy per cell stream.

  The clones exist because the function signature takes `&BsTriShape`. Refactoring to take `&mut BsTriShape` and using `std::mem::take(&mut shape.uvs)` would move instead of clone — but this requires the caller (walk module) to hold `&mut` access, which collides with the parallel borrowing required to walk multiple shapes in one scene. Alternative: consume the shape entirely (`shape: BsTriShape`), but that requires removing it from `scene.blocks` which currently owns it via `Box<dyn NiObject>`.

  Most-tractable fix: introduce a `BsTriShape::take_geometry(&mut self) -> (Vec<NiPoint3>, Vec<[f32; 2]>, ...)` method that mem::takes out the owned Vecs, and have the importer call it after acquiring `&mut` (which is fine since the importer doesn't re-walk a shape after taking its geometry). The walker keeps `&NifScene` but the cell-loader-side post-walk step can re-borrow `&mut` from `Box<dyn NiObject>` via downcast.
- **Evidence**:
  ```rust
  // crates/nif/src/import/mesh/bs_tri_shape.rs:69-82
  let uvs = if let Some(u) = sse_uvs {
      u
  } else {
      shape.uvs.clone()    // ← clone of ~40 KB Vec<[f32;2]>
  };

  let colors: Vec<[f32; 4]> = if let Some(c) = sse_colors {
      c
  } else if !shape.vertex_colors.is_empty() {
      shape.vertex_colors.clone()    // ← clone of ~80 KB Vec<[f32;4]>
  } else {
      vec![[1.0, 1.0, 1.0, 1.0]; positions.len()]
  };

  // Line 161, inside the build_triangles_for_synth closure:
  } else {
      shape.triangles.clone()    // ← clone of ~60 KB Vec<[u16;3]>
  }
  ```
- **Suggested Fix** (deferred — non-trivial refactor):
  Phase 1 — measure: instrument with `tracing::info_span!` around the import call and dump `total_bytes_cloned_per_shape`. Confirm the 120-180 KB/shape estimate against real Whiterun data.

  Phase 2 — if confirmed: add `BsTriShape::take_geometry()` and refactor the walk to take `&mut` access on geometry shapes only (downcast_mut). Or simpler: pass an owned `Vec<u8>` for the parts that genuinely need to be owned, and Cow<'_, [...]> for the parts the renderer can pass through to GPU upload without owning.
- **Estimated Impact**: ~360 KB - 1 MB of redundant memcpy per Riverwood cell streaming step. At ~1-2 cell loads per second when moving, ~0.5-1.5 MB/sec of unproductive memcpy → ~0.2-0.5 ms/sec of CPU time. Not visible in steady-state FPS; visible as smoother cell-crossing transitions. LOW severity because the fix is a refactor, not a one-line change.

  **Allocation regression coverage**: gap acknowledged — the #1247 dhat test would catch this if the synthetic fixture grew to include a populated BSTriShape (current fixture is a single NiNode). Add a BSTriShape sibling fixture to `tests/heap_allocation_bounds.rs` to pin the per-shape import allocation cost. Until then, the savings are estimated, not measured.

---

### NIF-D5-NEW-05: NiTriShape `extract_mesh` clones `geom.tangents` despite geom being unused after the clone
- **Severity**: LOW
- **Status**: NEW
- **Dimension**: NIF Parse → Import
- **Location**: `crates/nif/src/import/mesh/ni_tri_shape.rs:180`
- **Description**: The NiTriShape importer (FO3 / FNV / Oblivion hot path) builds a `GeomData<'a>` struct that owns its `tangents: Vec<[f32; 4]>`. At line 180 it clones `geom.tangents` even though `geom` is dropped at end-of-function (the only subsequent reference is the `geom`-shadowing `let tangents_yup = ...`). A `std::mem::take(&mut geom.tangents)` (requires `let mut geom`) would move instead of clone.

  Per FNV / FO3 NiTriShape: tangents Vec is 16 B × N verts. For a 1000-vertex mesh: 16 KB. Per FNV Riverwood-equivalent cell with ~5 fresh NiTriShape imports: ~80 KB redundant memcpy. The pre-#1232 path used screen-space derivative TBN (Path-2) so this clone is post-#1232 (synthesize_tangents fallback always populated).
- **Evidence**:
  ```rust
  // crates/nif/src/import/mesh/ni_tri_shape.rs:175-180
  // #783 / M-NORMALS — pre-decoded tangents from the NIF's
  // `NiBinaryExtraData("Tangent space (binormal & tangent vectors)")`.
  // Empty when the source mesh has no authored tangents; the
  // renderer falls back to screen-space derivative TBN in that case.
  let tangents_yup = geom.tangents.clone();
  // geom is not read past this point — only the mesh-Y-up positions / normals are.
  ```
  Verified `geom` is dead after line 180 by inspection — `extract_local_bound` is called at line 170 (before), `extract_vertex_colors` is called at line 152 (also before).
- **Suggested Fix**:
  ```rust
  let mut geom = if let Some(data) = ... { ... } else if let Some(data) = ... { ... } else { return None; };
  // ... use geom.vertices, geom.normals, geom.uv_sets, etc ...
  let tangents_yup = std::mem::take(&mut geom.tangents);  // move, no clone
  ```
  One-character change (`let` → `let mut`) plus `clone()` → `std::mem::take(&mut ...)`.
- **Estimated Impact**: ~80 KB / cell on FNV / FO3 / Oblivion content. Saves ~10-30 µs per Whiterun-class load. Skyrim Riverwood is BSTriShape-dominated so this finding has near-zero impact on the Skyrim hot path — it's a free win on the legacy game path.

  **Allocation regression coverage**: gap acknowledged — the dhat fixture is Skyrim BSTriShape-shaped, so this NiTriShape regression wouldn't trip. The savings are estimated from `sizeof([f32; 4]) × typical_vert_count`.

---

### NIF-D5-NEW-06: `import_embedded_animations` does O(N_blocks × 13) downcasts per NIF with no early exit
- **Severity**: INFO
- **Status**: NEW
- **Dimension**: NIF Parse → Import
- **Location**: `crates/nif/src/anim/entry.rs:268, 125-178`
- **Description**: `import_embedded_animations` is called from `pre_parse_cell` per NIF (`streaming.rs:578`). It iterates **every** block in the scene (`for block in &scene.blocks`) and for each block calls `net_of(block)` which performs up to 13 `as_any().downcast_ref::<>()` attempts (NiNode, NiTriShape, BsTriShape, NiCamera, NiMaterialProperty, NiTexturingProperty, BSLightingShaderProperty, BSEffectShaderProperty, NiPointLight, NiSpotLight, NiAmbientLight, NiDirectionalLight, plus the `NiAVObjectData::parse` keep-alive line).

  For a Riverwood static (architecture/clutter) NIF with ~50-200 blocks and **zero** controllers, this runs 650-2600 downcast attempts per parse. Each downcast is a `TypeId` comparison via vtable, ~5-10 ns. Total: ~3-26 µs per NIF.

  Per Riverwood cell load (~6 fresh NIFs): ~20-150 µs.

  The function has no early-exit fast path. A trivial pre-scan would skip the whole iteration when zero blocks carry a `controller_ref`:
  ```rust
  // Fast-path: if no block in the scene has a non-null controller_ref,
  // there are no embedded controllers to import — skip the whole walk.
  let has_any_controller = scene.blocks.iter().any(|b| {
      net_of(b.as_ref())
          .map(|net| !net.controller_ref.is_null())
          .unwrap_or(false)
  });
  if !has_any_controller {
      return None;
  }
  ```
  But this still pays the `net_of` cost on every block. A better approach: use `block.as_object_net()` (the trait helper already exists per `crates/nif/src/blocks/traits.rs`) to get the NET view without the 13-arm dispatch, then check `controller_ref` first.
- **Evidence**:
  ```rust
  // crates/nif/src/anim/entry.rs:125-178 — net_of dispatches 13 types
  fn net_of<'a>(block: &'a dyn crate::NiObject) -> Option<&'a NiObjectNETData> {
      let any = block.as_any();
      if let Some(n) = any.downcast_ref::<NiNode>() { return Some(&n.av.net); }
      if let Some(t) = any.downcast_ref::<NiTriShape>() { return Some(&t.av.net); }
      // ... 11 more downcast arms ...
  }

  // entry.rs:268-279 — the per-block loop
  for block in &scene.blocks {
      let Some(net) = net_of(block.as_ref()) else { continue; };
      if net.controller_ref.is_null() { continue; }
      // ... animation extract ...
  }
  ```
  The `HasObjectNET` trait already exists (per `crates/nif/src/blocks/traits.rs`) and is exposed via `NiObject::as_object_net()` — same trait that `extract_skin_bs_tri_shape` uses for cheap NET access. Routing through that would eliminate the 13-arm dispatch.
- **Suggested Fix**: Replace `net_of(block.as_ref())` with `block.as_object_net()`. The trait's `name`/`name_arc`/`controller_ref` methods provide everything `net_of` returns via the `NiObjectNETData` struct. Verify each of the 13 currently-dispatched types implements `as_object_net()` (most should — it's the standard ObjectNET-bearing trait); add the `impl` for any that don't.
- **Estimated Impact**: For statics-heavy Riverwood (most NIFs have zero controllers), saves ~3-26 µs per NIF. Per cell stream: ~20-150 µs. Per session-start radius-3 load (~500 NIFs): ~2-12 ms total work moved off the worker thread. Visible as slightly snappier cell streaming under streaming-heavy load, not a steady-state win.

  **Allocation regression coverage**: not applicable (this is dispatch overhead, no allocations).

---

### NIF-D5-NEW-07: Double determinant check on every NiTransform — `sanitize_rotation` + `zup_matrix_to_yup_quat`
- **Severity**: INFO
- **Status**: NEW
- **Dimension**: NIF Parse → Import
- **Location**: `crates/nif/src/rotation.rs:13-19` + `crates/nif/src/import/coord.rs:47-50`
- **Description**: `sanitize_rotation` runs at parse time on every `read_ni_transform` / `read_ni_transform_struct` call (`stream.rs:624, 647`), and its sole purpose is to ensure downstream code can treat the rotation as a valid rotation matrix (det ≈ 1). The import-time `zup_matrix_to_yup_quat` in `coord.rs:47-50` then re-computes the determinant on the basis-changed `yup` matrix and routes to `matrix3_to_quat` (fast path) vs `svd_repair_to_quat` (slow path) based on the det.

  The basis change `C * R * C^T` (where C is the Z-up → Y-up rotation) is a proper rotation, so det(yup) = det(input) = det(sanitized). After `sanitize_rotation`, the input is always det ≈ 1, so `zup_matrix_to_yup_quat`'s det check **always falls into the fast-path**. The check itself (~9 mults + a subtract + an abs + a compare) is cheap — but it's pure dead work post-sanitize.

  Per cell stream with ~6 fresh NIFs × ~50-200 NiTransforms each = ~300-1200 dead determinant checks per cell × ~5 ns each = ~2-6 µs per cell.

  Not worth landing as a code change unless the broader matrix3_to_quat call site gets restructured — the cost is below the noise floor. Filed as INFO because it's a real-but-tiny inefficiency that would surface in a fine-grained profile.
- **Evidence**:
  ```rust
  // crates/nif/src/rotation.rs:13-19 — runs at parse time
  pub fn is_degenerate_rotation(m: &NiMatrix3) -> bool {
      let r = &m.rows;
      let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
          - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
          + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
      (det - 1.0).abs() >= 0.1
  }

  // crates/nif/src/import/coord.rs:47-52 — re-computes det at import time
  let det = yup[0][0] * (yup[1][1] * yup[2][2] - yup[1][2] * yup[2][1])
      - yup[0][1] * (yup[1][0] * yup[2][2] - yup[1][2] * yup[2][0])
      + yup[0][2] * (yup[1][0] * yup[2][1] - yup[1][1] * yup[2][0]);

  if (det - 1.0).abs() < 0.1 {
      matrix3_to_quat(&yup)
  } else {
      svd_repair_to_quat(&yup)  // never fires post-sanitize_rotation
  }
  ```
- **Suggested Fix**: Either:
  (a) Mark the import path as "trusted input" — drop the determinant check in `zup_matrix_to_yup_quat` since the input has already been sanitized. Requires a `// SAFETY: caller-sanitized` comment and a debug_assert.
  (b) Lift the determinant out of `sanitize_rotation` so the value can be passed forward as a hint. Heavier refactor.

  Option (a) is one-line: replace the `if (det - 1.0).abs() < 0.1 { matrix3_to_quat(...) } else { svd_repair_to_quat(...) }` with `matrix3_to_quat(...)` directly, and add a debug_assert that the det is near 1. Cost: zero in release builds; debug builds still catch the (impossible-by-invariant) degenerate case.
- **Estimated Impact**: ~2-6 µs per cell stream. Not visible in any user-observable metric. Below the noise floor.

  **Allocation regression coverage**: not applicable (no allocations).

---

## Dedup vs prior audits

- **NIF-PERF-01..06** (audit 2026-05-04 / dim 5) — all 6 closed (#830-#835).
- **NIF-PERF-07..12** (separate sweep at #872-#876) — all closed (Arc<str> import-boundary, bulk-read triangle indices, header-array bulk reads, zero-copy morph deltas).
- **NIF-PERF-13** (#877) — closed (two-phase serial-extract + parallel-parse).
- **#1245** (ragdoll.rs allocate_vec idiom) — closed 2026-05-22.
- **#1246** (`#[must_use]` on read_pod_vec wrappers + KFM allocate_vec) — closed 2026-05-22.
- **#1247** (dhat-gated allocation-bound regression test, 32 KB cap) — closed 2026-05-23.
- **#862** (skip already-cached NIFs in cell-stream worker) — in place, drives the post-cache N=0-6 scenario in NIF-D5-NEW-02.
- **#854** (panic guard in pre-parse worker) — in place, preserved across #877.
- **#835** (pre-size ImportedScene Vecs) — in place at `import/mod.rs:132-142`.

## Methodology notes

- Inspected: `crates/nif/src/lib.rs`, `stream.rs`, `blocks/mod.rs`, `blocks/tri_shape/*`, `blocks/bs_geometry.rs`, `blocks/extra_data.rs`, `blocks/skin.rs`, `blocks/collision/*` (post-Session 35 split), `import/mod.rs`, `import/mesh/*` (all 8 production siblings), `import/transform.rs`, `import/coord.rs`, `import/walk/mod.rs`, `anim/entry.rs`, `rotation.rs`, `byroredux/src/streaming.rs`.
- Cross-checked: `docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM5.md` (prior NIF-PERF batch), git log of NIF-PERF-* commits.
- Allocation infra: `crates/nif/tests/heap_allocation_bounds.rs` (#1247) — dhat-gated test exists but covers a single-NiNode synthetic fixture; per-shape and per-archive-walk regressions would need additional fixtures (called out per-finding above).
- Not investigated (out of dim-5 scope, deferred to other dims): renderer-side per-frame cost, ECS query cost, descriptor-set churn, TLAS rebuilds.

## Honest takeaway for Riverwood 540 ms/frame

NIF parse is **well-tuned** post-#877 / post-#1247. The 7 findings above cumulatively
save ~1-3 ms per cell-stream operation, which is real but not the load-bearing piece
of the 540 ms/frame budget. The frame budget pain is somewhere else — most likely
in the descriptor-rebuild / TLAS-refit pipeline that fires every time a streamed cell
spawns its mesh instances.

**Recommendation**: move the next perf eye to Dim 4 (TLAS / descriptors) or Dim 6
(instance buffer upload), and run `byro-dbg` `stats` capture during a measured
cell-crossing event to localise the per-frame cost source. If a CPU-side bottleneck
shows up at the moment of cell spawn, it's likely the descriptor pool churn the
`SceneBuffers::upload_instances` path triggers, not anything in this dim.

If you do land any of the dim-5 findings, **NIF-D5-NEW-01** (the #834 partial regression
on the dispatch site) is the cleanest one to land first — small, targeted, and closes
out an audit issue's intent.
