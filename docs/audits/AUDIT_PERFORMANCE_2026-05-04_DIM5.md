# Performance Audit — Dimension 5: NIF Parse Performance
Date: 2026-05-04
Depth: deep

## Hot Block Inventory
| Block Type | Frequency (per cell) | Per-block cost | Notes |
|---|---|---|---|
| `NiNode` (+ aliases BSFadeNode/BSLeafAnimNode/RootCollisionNode/...) | very high (~1000s on FNV exterior, ~100s interior) | scene-graph node, light — properties + children Vecs | dispatch table treats 8 type names as alias to `NiNode::parse` (`blocks/mod.rs:165-172`) |
| `NiTriShape` / `NiTriStrips` / `BSSegmentedTriShape` | high (~100s) | vertex/index/UV/normal/color/tangent + skin Vecs (8 separate `allocate_vec`s) | `tri_shape.rs:520-548` — preallocated via `allocate_vec`, but bulk reads use `chunks_exact + map + collect` (2x alloc) |
| `BsTriShape` | high on Skyrim+/FO4+ | per-vertex loop with 6+ `read_u16_le` half-float calls + 8-Vec push pattern | `tri_shape.rs:550-685` — vertex stride loop, 1 read per element |
| `BSGeometry` (FO4+ packed) | high on FO4 / FO76 / SF | half-float UV + meshlet + cull data | `bs_geometry.rs:347-450` |
| `NiTransformController` / `NiTransformInterpolator` / `NiTransformData` | very high on character NIFs (animated) | string-table refs + key arrays | skip-able in geometry-only mode via `is_animation_block` (lib.rs:59) |
| `bhk*` (Havok) | medium on collision-bearing meshes | per-block stub or full layout | most are `block_size`-skip stubs (`is_havok_constraint_stub` at lib.rs:118) |
| `NiUnknown` (fallback) | varies — Skyrim Meshes0 hits 1000s | single `stream.skip(block_size)` + Arc<str> | fast-path at `blocks/mod.rs:927-953` |

## Findings

### NIF-PERF-05: `Arc::from(type_name)` per NiUnknown placeholder allocates instead of refcount-bump
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Status**: Partial regression of #248
- **Location**: `crates/nif/src/lib.rs:279, 406, 444, 469`
- **Description**: Each block-recovery / animation-skip path constructs an `NiUnknown { type_name: Arc::from(type_name), data: Vec::new() }`. Per #248 (cited at `blocks/mod.rs:103-105`) the entire reason `NiUnknown.type_name` is `Arc<str>` instead of `String` was to avoid per-block-name allocation when many blocks share the same name. But these 4 construction sites all `Arc::from(&str)`, which **always allocates** a fresh `Arc<str>` storage with the bytes copied in.

  The fix #248 made NiUnknown's *field* an `Arc<str>` so callers *could* share storage — but the only caller (lib.rs) doesn't share. The header already stores type names in `header.block_types: Vec<String>` (one entry per **distinct** type in the file, then `block_type_indices` maps block→type_index). Promoting that storage to `Vec<Arc<str>>` would let lib.rs `Arc::clone` the existing storage (atomic increment, no allocation) per recovery.

  Interacts with NIF-PERF-01: the same promotion would let recovery counters use `HashMap<Arc<str>, _>` keyed off the same Arc — eliminating the `to_string()` allocations on the hot loop.
- **Evidence**:
  ```rust
  // lib.rs:279 — animation skip path
  blocks.push(Box::new(blocks::NiUnknown {
      type_name: Arc::from(type_name),  // fresh alloc every time
      data: Vec::new(),
  }));
  // Repeated identically at lines 406, 444, 469.
  ```
  ```rust
  // header.rs:24
  pub block_types: Vec<String>,                // one entry per distinct type name
  pub block_type_indices: Vec<u32>,            // one entry per block, indexes block_types
  ```
- **Impact**: On Skyrim Meshes0 archive walks (per #565 commentary), recovery paths fire thousands of times — each one paying a fresh ~24-40 byte allocation (Arc header + str payload). Per cell load with mid-volume recovery (e.g. SE Whiterun, ~50-200 recoveries), ~5-10 KB of throwaway allocations. Combined with NIF-PERF-01 (counter-map keys), a typical Oblivion cell load sees ~150 KB of unneeded short-string allocs from the parse loop. Mid-impact: visible in mass-archive tests, near-invisible per single cell load.
- **Suggested Fix**: Promote `NifHeader.block_types` from `Vec<String>` to `Vec<Arc<str>>` and `block_type_name` to return `Option<&Arc<str>>` (or just the `Arc<str>` cloned; either is cheap). Then in lib.rs the dispatch loop holds an `Arc<str>` and the 4 NiUnknown construction sites become `type_name: Arc::clone(&type_name_arc)`. Same change unblocks NIF-PERF-01's HashMap<Arc<str>, _> migration.

### NIF-PERF-06: Streaming pre-parse worker is single-threaded — serial NIF parse on 16-core CPU
- **Severity**: HIGH
- **Dimension**: NIF Parse (cross-cutting w/ streaming pipeline)
- **Status**: NEW
- **Location**: `byroredux/src/streaming.rs:276-304`
- **Description**: `extract_and_parse_cell` (the streaming worker's per-cell entry point) iterates `model_paths` with a plain `for path in model_paths` loop, calling `byroredux_nif::parse_nif`, `import_nif_lights`, `import_nif_particle_emitters`, and `import_embedded_animations` for each model serially. The worker is one OS thread, so the entire pre-parse phase saturates exactly **1 of 32 hardware threads** on the dev machine (7950X) — the other 31 cores idle while the player waits for an exterior cell to stream in.

  Parsing of distinct NIF files is **embarrassingly parallel**: each call takes a `&[u8]` slice (extracted bytes) and produces an owned `NifScene` with no shared mutable state, no I/O after the byte extraction, and no global state mutation. The workspace already depends on `rayon = "1"`. Replacing the for-loop with a `model_paths.into_par_iter().map(...).collect()` would scale to the worker thread's full parallelism budget.

  The texture provider's `extract_mesh` IS a potential serialization point if the BSA reader holds an internal lock. Need to verify — but even if `extract_mesh` is serial, the parse + 3 import calls dominate the cost (extract is a memcpy from a memory-mapped BSA; parse is the CPU-bound work).
- **Evidence**:
  ```rust
  for path in model_paths {
      let Some(bytes) = tex_provider.extract_mesh(&path) else { ... };
      let scene = match byroredux_nif::parse_nif(&bytes) { ... };           // CPU-bound
      let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
      let lights = byroredux_nif::import::import_nif_lights(&scene);
      let particle_emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
      let embedded_clip = byroredux_nif::anim::import_embedded_animations(&scene);
      parsed.insert(path, Some(PartialNifImport { scene, bsx, lights, particle_emitters, embedded_clip }));
  }
  ```
- **Impact**: For an exterior cell with 100 unique models averaging 3 ms each (`AUDIT_PERFORMANCE_2026-04-20.md` reports ~13-20 ms per 1000-block NIF; smaller per-model average), serial parse takes ~300 ms on one core. Parallelizing across 8 worker threads (rayon's default) drops it to ~40-50 ms — a 6-7× speedup on cell-streaming latency. Especially impactful for FNV/SE exterior radius=3 / =5 grids where 30+ cells stream as the player runs.
- **Suggested Fix**: Replace the for-loop with `rayon`-based parallel parse:
  ```rust
  use rayon::prelude::*;
  let model_paths: Vec<String> = model_paths.into_iter().collect();
  let results: Vec<(String, Option<PartialNifImport>)> = model_paths
      .into_par_iter()
      .map(|path| {
          let entry = (|| -> Option<PartialNifImport> {
              let bytes = tex_provider.extract_mesh(&path)?;  // verify thread-safe
              let scene = byroredux_nif::parse_nif(&bytes).ok()?;
              let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
              let lights = byroredux_nif::import::import_nif_lights(&scene);
              let particle_emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
              let embedded_clip = byroredux_nif::anim::import_embedded_animations(&scene);
              Some(PartialNifImport { scene, bsx, lights, particle_emitters, embedded_clip })
          })();
          (path, entry)
      })
      .collect();
  parsed.extend(results);
  ```
  **Pre-flight verified:** `TextureProvider::extract_mesh` is `&self` (`asset_provider.rs:84`); underlying `BsaArchive` (`crates/bsa/src/archive.rs:119`) and `Ba2Archive` (`crates/bsa/src/ba2.rs:78`) wrap their `File` in `Mutex<File>` so they are already `Send + Sync` — concurrent `extract_mesh` calls will serialize on the file mutex (one File handle = serial I/O, by hardware necessity) but the much-larger parse + import work parallelizes fully across the rayon pool. For peak throughput, lift `extract_mesh` into a **serial pre-pass** that gathers all bytes, then run the par_iter on the (path, bytes) pairs — this hides the I/O latency behind the previous cell's parse work and avoids holding the BSA mutex on parse-bound rayon workers.

### NIF-PERF-04: ImportedScene Vecs grow from default capacity — Vec realloc churn during walk
- **Severity**: LOW
- **Dimension**: NIF Parse → Import
- **Status**: NEW
- **Location**: `crates/nif/src/import/mod.rs:666-673`, `:846-853`, `:753-761`
- **Description**: `import_nif_scene_impl`, `import_nif_with_collision_impl`, and `import_nif_impl` each construct an `ImportedScene` (or local Vec output) with `nodes: Vec::new()`, `meshes: Vec::new()`, etc. The walker pushes one entry per traversed node/mesh, so for a NIF with N traversal hits the Vec doubles ~log2(N) times — every doubling is a fresh allocation + memcpy of the full prefix.

  The total walked-block count is bounded above by `scene.blocks.len()` (every node block produces at most one ImportedNode entry, every shape one ImportedMesh). Pre-sizing `nodes` to `scene.blocks.len()` and `meshes` to `scene.blocks.len() / 4` (heuristic — ~25% of blocks are shapes for typical Bethesda content) would eliminate the realloc churn.
- **Impact**: For a 1000-block NIF (e.g. Megaton interior), ImportedNode growth path is 0→4→8→16→32→64→128→256→512→1024 = 10 reallocations, ~2 KB total memcpy waste per Vec. Across the 8 Vecs in ImportedScene + per-mesh internal Vecs (positions, normals, tangents, uvs, indices, colors, skin), peak realloc cost per cell load is ~50-200 KB of unproductive memcpy. Modest impact (~0.1-0.3 ms per cell), filed for completeness.
- **Suggested Fix**: `let cap = scene.blocks.len(); ImportedScene { nodes: Vec::with_capacity(cap), meshes: Vec::with_capacity(cap / 4), ... }`. Most of these Vecs over-allocate slightly relative to actual yield (some blocks are filtered by `is_editor_marker` / APP_CULLED / unsupported subclasses) but trading some VM commit for zero realloc churn is the right tradeoff at typical NIF sizes.

### NIF-PERF-03: `allocate_vec` misused as bound-check, allocates and drops empty Vec
- **Severity**: HIGH
- **Dimension**: NIF Parse
- **Status**: Regression of #408 (or scope-extension)
- **Location**: 9 sites:
  - `crates/nif/src/blocks/skin.rs:266, 273, 279, 288, 299, 318`
  - `crates/nif/src/blocks/tri_shape.rs:1441, 1455`
  - `crates/nif/src/blocks/controller/morph.rs:222`
- **Description**: These call sites use the pattern:
  ```rust
  stream.allocate_vec::<u16>(num_bones as u32)?;        // result discarded!
  let bones = stream.read_u16_array(num_bones as usize)?;
  ```
  The first call to `allocate_vec` is intended only for its bound-check side effect — but `allocate_vec` is implemented as `Ok(Vec::with_capacity(count as usize))` (`stream.rs:203`). For nonzero counts this reserves heap capacity, then immediately drops the empty Vec on the next semicolon. The downstream `read_u16_array` does its own `check_alloc` (so the bound-check is even redundant) PLUS its own allocation. Net effect: every nonzero call site allocates twice and drops once, when only one allocation is needed.

  This appears to be a misreading of the `allocate_vec` helper purpose — its docstring says it's for replacing `Vec::with_capacity(count as usize)` in the *bind* position. Here the binding is missing, so the helper degrades to a no-op-with-allocation.

  `check_alloc` (also exposed `pub` on `NifStream`) is the right tool when you want bound validation without an allocation — it does a remaining-bytes check and returns `()`.
- **Evidence** (from skin.rs:266-281, all 6 of these in one ~15-line span on the NiSkinPartition partition loop, executed N times where N = num_partitions):
  ```rust
  // #388: `num_bones` is a file-driven u16; bound through allocate_vec.
  stream.allocate_vec::<u16>(num_bones as u32)?;
  let bones = stream.read_u16_array(num_bones as usize)?;

  // Vertex map (conditional on has_vertex_map for v >= 10.1.0.0).
  let vertex_map = if has_conditionals {
      let has = stream.read_byte_bool()?;
      if has {
          stream.allocate_vec::<u16>(num_vertices as u32)?;
          stream.read_u16_array(num_vertices as usize)?
      } else { Vec::new() }
  } else {
      stream.allocate_vec::<u16>(num_vertices as u32)?;
      stream.read_u16_array(num_vertices as usize)?
  };
  ```
- **Impact**: NiSkinPartition is on every NPC body / creature mesh. Skyrim SE NPCs have 6-12 partitions per NiSkinPartition, each running 6 redundant allocations on 100-500 bones / vertices / triangles. That's 36-72 throwaway `Vec::with_capacity` allocations per NPC mesh, times the ~50 NPCs in a typical Whiterun load = ~2000-3500 redundant heap allocations per cell. ~0.2-0.5 ms per cell load on the parser, plus heap fragmentation that compounds across the load. NiTriStripsData (tri_shape.rs:1441/1455) extends the same pattern to legacy Morrowind / Oblivion content.
- **Suggested Fix**: Replace each bare `stream.allocate_vec::<T>(n)?;` with `stream.check_alloc(n as usize * std::mem::size_of::<T>())?` (or just delete it — `read_*_array` already calls `check_alloc` internally before allocating). The bound is enforced at the same point, with no temporary allocation. Add a clippy lint (`unused_must_use` is implicit on `Result` already; add `#[must_use]` to `allocate_vec`'s return, which would have caught this at the original change).

### NIF-PERF-02: `chunks_exact().map().collect()` doubles allocation in bulk array readers
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/stream.rs:251-342` (`read_ni_point3_array`, `read_ni_color4_array`, `read_uv_array`, `read_vec2_array`, `read_u16_array`, `read_u32_array`, `read_f32_array`)
- **Status**: NEW
- **Description**: Each bulk reader allocates a temporary `vec![0u8; byte_count]` buffer for the `read_exact` call, then uses `buf.chunks_exact(N).map(...).collect()` to produce the typed output. This holds **two** allocations live simultaneously: the byte buffer (dropped at function return) and the typed output Vec. For a 10K-vertex BSGeometry the temp buf is 120 KB and the output Vec is another 120 KB — peak 240 KB to read 10K NiPoint3s.

  The bulk readers exist precisely to amortize fixed per-element overhead, so handing back a `Vec` is correct — but the temp byte buffer is unnecessary. Reading directly into a pre-typed `Vec<T>` for any POD `T` (which all of these element types are: `[f32; 3]`, `[f32; 2]`, `u16`, `u32`, `f32`, `[f32; 4]`) costs the same `read_exact` call but avoids the second allocation.
- **Evidence** (representative):
  ```rust
  pub fn read_ni_point3_array(&mut self, count: usize) -> io::Result<Vec<NiPoint3>> {
      let byte_count = count * 12;
      self.check_alloc(byte_count)?;
      let mut buf = vec![0u8; byte_count];           // alloc 1: 12*N bytes
      self.cursor.read_exact(&mut buf)?;
      Ok(buf
          .chunks_exact(12)
          .map(|c| NiPoint3 { x: f32::from_le_bytes([c[0], c[1], c[2], c[3]]), ... })
          .collect())                                  // alloc 2: 12*N bytes (Vec<NiPoint3>)
  }
  ```
- **Impact**: Called ~17 times across the codebase (`tri_shape.rs`, `bs_geometry.rs`, `skin.rs`, `controller/morph.rs`). On a single FNV cell load with ~150 NIFs averaging 2 geometry blocks each, the redundant allocation churns ~2-5 MB through the heap allocator with no useful product. ~0.5-1.5 ms per cell on the parse path. Especially impactful on FO4+ BSGeometry meshes (5-50K verts each).
- **Suggested Fix**: For `Vec<u16>` / `Vec<u32>` / `Vec<f32>` (already POD `T = elem`), allocate `Vec<T>` of capacity `count` and use `bytemuck::cast_slice_mut(&mut vec)` (which the workspace already depends on, see #291) to read directly. For `[f32; 3]` / `[f32; 2]` / `[f32; 4]` (POD bag-of-floats), same trick — these structures are `#[repr(C)]` and bytemuck-castable. Alternative: use `read_exact` into `vec.spare_capacity_mut()` then `set_len(count)`. Either path eliminates the second allocation entirely.

### NIF-PERF-01: `to_string()` on hot block-parse loop for recovery/drift counters
- **Severity**: MEDIUM
- **Dimension**: NIF Parse
- **Location**: `crates/nif/src/lib.rs:332`, `:362`, `:410`, `:448`, `:473`
- **Status**: NEW
- **Description**: The per-block parse loop uses `recovered_by_type.entry(type_name.to_string()).or_insert(0)` and `drifted_by_type.entry(type_name.to_string()).or_insert(0)` to bump per-type counters. Each `to_string()` allocates a fresh `String` even when the entry already exists in the map — that allocation is unconditional because `HashMap::entry(K)` takes `K` by value. The same pattern repeats on the `parsed_size_cache.entry(type_name.to_string())` insertion (line 362) which fires on **every** successful parse on Oblivion-no-block-sizes files (8000+ NIFs in vanilla Oblivion).

  `type_name: &str` here is borrowed from the header's block-type table (`Arc<str>` storage), so an `Arc::clone` would suffice — and `HashMap<Arc<str>, _>::entry` accepts `Arc<str>` cheaply (atomic increment instead of a heap copy). Even better: `raw_entry_mut().from_key(type_name)` lets you check membership before allocating and only pay the alloc on insert.
- **Evidence**:
  ```
  *drifted_by_type.entry(type_name.to_string()).or_insert(0) += 1;          // line 332
  parsed_size_cache.entry(type_name.to_string()).or_default().push(...);    // line 362
  *recovered_by_type.entry(type_name.to_string()).or_insert(0) += 1;        // lines 410, 448, 473
  ```
- **Impact**: On a typical Oblivion cell load (~150 NIFs × ~50 blocks/NIF = 7500 blocks), the success-path cache insertion at line 362 fires once per block — 7500 `String::from(&str)` allocations for type names averaging ~20 chars. ~150 KB of throwaway short-string allocations per cell. On Skyrim Meshes0 archive walks (per the #565 commentary), drifted/recovered paths fire thousands of times per archive. Estimated 0.5-1 ms per cell load, plus heap fragmentation.
- **Suggested Fix**: Use `HashMap<Arc<str>, _>` keyed on the header's already-Arc-allocated type name (`header.block_type_name(i)` already returns the same `Arc<str>` storage). Or change to `raw_entry_mut().from_key(type_name).and_modify(...).or_insert_with(|| (Arc::from(type_name), 0))`. Either way the per-block alloc disappears in the steady-state path where the type name was seen before.

## Summary

**Counts by severity:** 2 HIGH, 3 MEDIUM, 1 LOW. Total 6 findings.

**Findings (severity-sorted):**
- **HIGH** — NIF-PERF-06: streaming worker is single-threaded (~6-7× cell-load speedup available)
- **HIGH** — NIF-PERF-03: `allocate_vec` misused as bound-check at 9 sites, double-allocates per skin partition
- **MEDIUM** — NIF-PERF-01: `to_string()` per block on parse-loop recovery counters (~150 KB/cell on Oblivion)
- **MEDIUM** — NIF-PERF-02: `chunks_exact().map().collect()` doubles allocation in 7 bulk readers
- **MEDIUM** — NIF-PERF-05: `Arc::from(type_name)` per NiUnknown allocates instead of refcount-bump (partial regression of #248)
- **LOW** — NIF-PERF-04: ImportedScene Vecs grow from default capacity, ~10 reallocs per Vec on 1000-block NIF

**Top 3 quick wins (1-50 LOC each):**
1. **NIF-PERF-03** — delete the 9 unbound `stream.allocate_vec::<T>(n)?;` calls (or replace with `stream.check_alloc(n * size_of::<T>())?`). Pure deletion; `read_*_array` already does the bound check internally. Add `#[must_use]` to `allocate_vec` so the compiler catches future regressions. **~15 LOC, ~10 min**.
2. **NIF-PERF-06** — wrap streaming worker's `for path in model_paths` loop in `rayon::par_iter`. With BSA reader pre-confirmed Send+Sync via `Mutex<File>`, this is a one-block change that scales to 8-32× threads. **~30 LOC, ~30 min**.
3. **NIF-PERF-04** — pre-size `ImportedScene` Vecs to `scene.blocks.len()`-derived caps. **~5 LOC, ~5 min**.

**Top 3 architectural changes:**
1. **NIF-PERF-05 + NIF-PERF-01 (combined)** — promote `NifHeader.block_types` from `Vec<String>` to `Vec<Arc<str>>` and propagate through `block_type_name()`. This unblocks both findings: `Arc::clone` replaces `Arc::from` at NiUnknown construction sites, and `HashMap<Arc<str>, _>` replaces `HashMap<String, _>` at recovery-counter sites — eliminating most of the per-block-name allocation churn in lib.rs. **~50 LOC** plus call-site updates in maybe 20 places.
2. **NIF-PERF-02** — add `bytemuck` as a direct dep on `byroredux-nif` (already transitively present), then rewrite the 7 bulk readers to use `bytemuck::cast_slice_mut(&mut Vec::<T>::with_capacity(count))` over the `read_exact` target. Eliminates the temp `Vec<u8>` + transmute on every geometry read. **~80 LOC** for the helper rewrite + dep add.
3. **NIF-PERF-06 (full version)** — split the streaming worker into a serial-I/O lane (BSA extract) and a rayon-parallel CPU lane (parse + 4 import calls). Hides BSA mutex contention behind the previous cell's parse work. Also surfaces a per-cell parallelism telemetry hook that complements `ScratchTelemetry` — propose a `NifParseTelemetry { cells_streamed: u64, models_parsed: u64, parallel_efficiency: f32 }` resource analogous to renderer-side telemetry.

**Dedup notes (vs prior audits):**
- D5-M1 (P5-03 — Arc<str> import-boundary clones) — **closed**: `ImportedNode.name` and `ImportedMesh.name` are already `Option<Arc<str>>` (verified at `import/mod.rs:102, 226`).
- D5-M2 (P5-04 — bulk read methods) — **closed in spirit but suboptimal**: bulk readers were added (`stream.rs:251-342`) per #291 and now exist at all geometry read sites; my NIF-PERF-02 covers the leftover double-allocation in those readers' impl.
- NIF-D3-09 (read_block_ref_list bypassing allocate_vec) — **closed**: verified at `stream.rs:442-449`.
- #408 (allocate_vec blanket sweep) — **partially regressed** in 9 sites where the helper is misused as a bare bound-check. NIF-PERF-03.
- #381 (process-lifetime NifImportRegistry) — **in place and well-designed** (LRU via env var, batched touch). Cache key is lowercased path string — orthogonal to texture archive presence (correct: parse output is independent of textures). No regression.
- #333 (matrix3_to_quat fast-path) — **in place** (`import/coord.rs:50` + `rotation.rs:62`). Determinant is computed twice (parse + import) but each computation is ~9 muls; total ~100 µs/cell — not worth optimizing.
- #341 (stream drift detector) — **release-build free**: gated behind `#[cfg(debug_assertions)]` at `lib.rs:346`.
- #254 (NIF string read optimization) — **in place** (`read_sized_string` and `read_short_string` use try-`from_utf8`-first path).

**Note on telemetry:** NIF parse has no equivalent of `ScratchTelemetry`. NIF-PERF-06's parallelization is the right place to add a `NifParseTelemetry` resource (cells streamed, mean parse ms, parallel-pool peak occupancy) for ongoing visibility into worker-thread saturation. Not a finding, but a recommended follow-up to make NIF-PERF-06's win measurable.
