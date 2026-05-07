---
date: 2026-05-06
audit: performance
focus: dimensions 5 (NIF Parse), 8 (Material Table & SSBO Upload)
depth: deep
---

# Performance Audit — 2026-05-06

Focused two-dimension sweep:

- **Dim 5** — NIF parse hot path (parse → block → import → streaming pre-parse)
- **Dim 8** — R1 architecture: GpuMaterial dedup, MaterialBuffer SSBO upload

Both dimensions read their respective 2026-05-04 baselines (NIF: #823–#833 alloc sweep; R1: 2026-05-03 R1 audit) and dedup against open issues (#779, #781).

## Executive Summary

| Severity | Count | Files Touched |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 4 | NIF parse path |
| LOW | 4 | NIF (3) + R1 SSBO (1) |
| INFO | 2 | NIF (1) + R1 SSBO (1) |
| **Total** | **10** | |

**Headline**: All 2026-05-04 NIF alloc-sweep baselines (#830/#831/#832/#833) hold — no regressions. R1 architecture (the GpuInstance 112 B / GpuMaterial 260 B split) is structurally sound; the 2026-05-03 audit's open R1-N4/N6/N7 items have all closed since via #804/#806/#807. No CRITICAL or HIGH findings.

**Estimated FPS impact**: Negligible at current scenes. Top three quick wins (NIF-PERF-09/10/11) shave 300–600 KB of throwaway memcpy per cell load and reduce ~150 K stream-fn calls on FO4-class actor meshes — wall-clock impact in the cell-load critical path, not steady-state framerate. DIM8-01 saves ~3 MB/s sustained PCIe in steady-state interiors but is below the signal floor today.

**Infrastructure gap (recurring)**: dhat / alloc-counter regression coverage remains unwired. Every alloc-reduction finding below carries an explicit "expected savings are estimates; warrants a follow-up 'wire dhat for this site' issue" caveat per the audit-performance command spec.

## Hot Path Analysis

| Operation | When | Cost (estimate) | Status |
|---|---|---|---|
| `pre_parse_cell` rayon-parallel NIF parse | cell load | ~6–7× speedup vs serial (#830) | ✅ baseline holds |
| `read_pod_vec<T>` single-alloc bulk read | every block with POD arrays | 1 alloc + 1 `read_exact` (#833) | ✅ 7 readers route through it |
| `allocate_vec` `#[must_use]` gate | every alloc-with-bound | 9 prior misuse sites cleaned (#831) | ✅ |
| `bump_counter` per-block tally | every block parse | no per-block `to_string` (#832) | ✅ |
| `Arc::from(type_name)` per `NiUnknown` | recovery / unknown-type fallback | ~5–10 KB / cell (re-finding) | ⚠ NIF-PERF-07 |
| `Arc::from(name)` in walk resolvers | per-light, per-tree-bone | per-cell, unmeasured | ⚠ NIF-PERF-08 |
| BSGeometry per-element push-loops | every FO4+ BSGeometry block | N×bounds-check overhead | ⚠ NIF-PERF-09 |
| `chunks_exact(3).map().collect()` triangles | per renderable shape | 1 extra alloc + memcpy/shape | ⚠ NIF-PERF-10 |
| `MaterialTable::intern` byte-hash dedup | per DrawCommand | amortized O(1), 97% dedup-hit | ✅ |
| `upload_materials` SSBO write | every frame (unconditional) | ~3 MB/s steady-state PCIe | ⚠ DIM8-01 |
| Material table cell-transition behavior | per cell unload | per-frame ephemeral, no leak | ✅ |
| GpuInstance / GpuMaterial size pins | compile-time | 112 B / 260 B both pinned | ✅ |

## Findings

### MEDIUM

#### NIF-PERF-07: `Arc::from(type_name)` per `NiUnknown` placeholder still allocates a fresh `Arc<str>` (5 sites)
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/lib.rs:292](crates/nif/src/lib.rs#L292), [:424](crates/nif/src/lib.rs#L424), [:462](crates/nif/src/lib.rs#L462), [:487](crates/nif/src/lib.rs#L487); [crates/nif/src/blocks/mod.rs:953](crates/nif/src/blocks/mod.rs#L953)
- **Cost**: per-recovery alloc; ~5–10 KB / cell on Skyrim Meshes0-class archive walks. Re-finding of NIF-PERF-05 (2026-05-04) — proposed fix has not landed.
- **Evidence**:
  ```rust
  // lib.rs:291-294 — animation skip path (one of 5 identical sites)
  blocks.push(Box::new(blocks::NiUnknown {
      type_name: Arc::from(type_name),  // fresh alloc every time
      data: Vec::new(),
  }));
  ```
  `header.block_types` is `Vec<String>` ([crates/nif/src/header.rs:24](crates/nif/src/header.rs#L24)); `block_type_name` returns `&str`. The header storage that should be the single owner of these strings doesn't exist as `Arc<str>`, so every NiUnknown synthesis allocates a private copy.
- **Why it matters**: Recovery paths fire often on Oblivion (no block_sizes table) and on Skyrim Meshes0 archive walks. Each allocation accumulates during a parse already on the cell-load critical path.
- **Proposed fix**: Promote `NifHeader.block_types` to `Vec<Arc<str>>`. The 5 NiUnknown sites become `Arc::clone(&type_name_arc)` (atomic increment, no alloc). Unblocks NIF-PERF-08.
- **dhat gap**: Expected savings are estimates; no quantitative regression guard exists today — warrants a follow-up "wire dhat for the NIF parse loop" issue.

#### NIF-PERF-08: `Arc::from(name)` in `resolve_affected_node_names` / `resolve_block_ref_names` allocates instead of cloning
- **Dimension**: NIF Parse → Import
- **Location**: [crates/nif/src/import/walk.rs:877](crates/nif/src/import/walk.rs#L877), [:909](crates/nif/src/import/walk.rs#L909)
- **Cost**: per-light affected-node + per-FO4/FO76 SpeedTree bone resolution; cell-load tier.
- **Evidence**:
  ```rust
  // walk.rs:870-879 — resolve_affected_node_names
  let Some(name) = net.name() else { continue; };
  if name.is_empty() { continue; }
  out.push(std::sync::Arc::from(name));  // ← fresh Arc<str> alloc per name
  ```
  `HasObjectNET::name()` returns `Option<&str>` deref'd from existing `Arc<str>` storage on `NiObjectNET.name` — re-wrapping it via `Arc::from(&str)` always allocates.
- **Proposed fix**: Add `fn name_arc(&self) -> Option<&Arc<str>>` to `HasObjectNET`. Resolver writes `Arc::clone(arc)` — atomic increment, no alloc. Same refcount-promotion pattern as #248. Naturally pairs with NIF-PERF-07's header promotion.
- **dhat gap**: Estimates only; warrants follow-up dhat-wiring issue.

#### NIF-PERF-09: `BSGeometry` per-element `read_u32_le` push-loops where bulk readers exist
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/blocks/bs_geometry.rs:425-435](crates/nif/src/blocks/bs_geometry.rs#L425-L435), [:467-482](crates/nif/src/blocks/bs_geometry.rs#L467-L482), [:484-493](crates/nif/src/blocks/bs_geometry.rs#L484-L493), [:495-508](crates/nif/src/blocks/bs_geometry.rs#L495-L508)
- **Cost**: per-FO4+/Starfield BSGeometry block; N×bounds-check overhead instead of 1.
- **Evidence**:
  ```rust
  // bs_geometry.rs:425-435 — push-loop where read_u32_array exists
  let n_normals = stream.read_u32_le()?;
  let mut normals_raw = stream.allocate_vec::<u32>(n_normals)?;
  for _ in 0..n_normals {
      normals_raw.push(stream.read_u32_le()?);   // ← N function calls
  }
  ```
  All four sites (normals_raw, tangents_raw, LOD tris, meshlets, cull_data) are POD arrays whose on-disk layout matches in-memory representation; the #831/#833 sweep that fixed skin.rs and tri_shape.rs missed bs_geometry.rs.
- **Why it matters**: BSGeometry is the dominant geometry block on FO4/FO76/Starfield. A 50K-vertex actor mesh = 200K function calls where 1 `read_u32_array(n)` would suffice.
- **Proposed fix**: Mirror the skin.rs cleanup — replace push-loops with `read_u32_array` / `read_u16_array` / `read_f32_array`. For LOD `tris` extend `read_pod_vec` to `[u16; 3]` (paired with NIF-PERF-10).
- **dhat gap**: Estimates only; warrants follow-up dhat-wiring issue.

#### NIF-PERF-10: `chunks_exact(3).map().collect()` triangle pattern survives in 3 sites
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/blocks/skin.rs:315-319](crates/nif/src/blocks/skin.rs#L315-L319), [crates/nif/src/blocks/tri_shape.rs:765-768](crates/nif/src/blocks/tri_shape.rs#L765-L768), [:1454-1457](crates/nif/src/blocks/tri_shape.rs#L1454-L1457)
- **Cost**: per-shape; 1 extra alloc + 1 memcpy per call. Payload `num_triangles × 6 bytes`.
- **Evidence**:
  ```rust
  // tri_shape.rs:1453-1460 (NiTriShapeData)
  let triangles = if has_triangles {
      let flat = stream.read_u16_array(num_triangles * 3)?;     // alloc 1
      flat.chunks_exact(3)
          .map(|tri| [tri[0], tri[1], tri[2]])
          .collect()                                             // alloc 2
  } else { Vec::new() };
  ```
  `[u16; 3]` is bitwise identical to three contiguous u16s — single bulk read into `Vec<[u16; 3]>` is correct.
- **Why it matters**: Triangles present on every renderable shape. Skyrim/FO4/Starfield NPC bodies have 5–15 NiSkinPartitions × ~1000 triangles. Per cell with ~50 NPCs: ~300–600 KB redundant memcpy.
- **Proposed fix**: Add `read_u16_triple_array(count) -> Vec<[u16; 3]>` on NifStream delegating to `read_pod_vec::<[u16; 3]>(count)`. Same pattern as existing `[f32; 2]`/`[f32; 4]`/`NiPoint3` cases.
- **dhat gap**: Estimates only; warrants follow-up dhat-wiring issue.

### LOW

#### NIF-PERF-11: `morph.rs` re-allocates `Vec<NiPoint3>` to `Vec<[f32; 3]>` despite identical layout
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/blocks/controller/morph.rs:221-222](crates/nif/src/blocks/controller/morph.rs#L221-L222)
- **Cost**: per-morph-target × num_vertices; only triggers on FaceGen-bearing NIFs. ~40 KB / FaceGen head × 8 morph targets × 5K verts.
- **Evidence**:
  ```rust
  let points = stream.read_ni_point3_array(num_vertices as usize)?;
  let vectors: Vec<[f32; 3]> = points.into_iter().map(|p| [p.x, p.y, p.z]).collect();
  ```
  `NiPoint3` is `#[repr(C)]` with three `f32`s, no padding — bitwise identical to `[f32; 3]`.
- **Proposed fix**: Change `MorphTarget.vectors` from `Vec<[f32; 3]>` to `Vec<NiPoint3>` so the read result is consumed in place.
- **dhat gap**: Estimates only.

#### NIF-PERF-12: Header `block_type_indices` / `block_sizes` use per-element push-loops
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/header.rs:174-177](crates/nif/src/header.rs#L174-L177), [:200-203](crates/nif/src/header.rs#L200-L203)
- **Cost**: ~1–2 ms shaved off cell-streaming critical path (100–300 NIF headers/cell × 1000 blocks × ~5 µs).
- **Proposed fix**: Lift `read_pod_vec`'s body into a standalone cursor-side helper that the header parser can call before the NifStream wrapper exists.
- **dhat gap**: Estimates only.

#### NIF-PERF-13: `pre_parse_cell` runs `extract_mesh` inside the rayon closure — BSA mutex serializes I/O across workers
- **Dimension**: NIF Parse → Streaming
- **Location**: [byroredux/src/streaming.rs:364-368](byroredux/src/streaming.rs#L364-L368)
- **Cost**: workers contend on `Mutex<File>`; ~10–20% additional speedup over current 6–7× (#830).
- **Evidence**:
  ```rust
  // inside par_iter().map() closure
  let Some(bytes) = tex_provider.extract_mesh(&path) else { ... };  // ← BSA Mutex<File>
  let scene = byroredux_nif::parse_nif(&bytes)?;
  ```
  All N rayon workers calling `extract_mesh` block on the file mutex during `read_at`.
- **Proposed fix**: Two-phase pre-parse — serial extract (one worker) → parallel parse+import on `(path, bytes)` pairs. Removes BSA mutex contention from rayon worker critical path.
- **dhat gap**: Allocation impact neutral (same Vec<u8> bytes flow). Win is wall-clock; needs streaming-cell wall-clock benchmark, not dhat.

#### DIM8-01: Per-frame full re-upload of material SSBO even when byte-identical to last frame
- **Dimension**: Material Table & SSBO Upload
- **Location**: [crates/renderer/src/vulkan/scene_buffer.rs:999-1031](crates/renderer/src/vulkan/scene_buffer.rs#L999-L1031), [crates/renderer/src/vulkan/context/draw.rs:1029-1033](crates/renderer/src/vulkan/context/draw.rs#L1029-L1033)
- **Cost**: ~3 MB/s sustained PCIe traffic in steady-state interior cells (200 unique mats × 260 B × 60 fps).
- **Evidence**:
  ```rust
  // draw.rs:1029-1033 — unconditional upload every frame, no dirty gate
  if !materials.is_empty() {
      self.scene_buffers.upload_materials(&self.device, frame, materials)
          .unwrap_or_else(|e| log::warn!("Failed to upload materials: {e}"));
  }
  ```
  `build_render_data` walks the same ECS queries in the same order frame-to-frame, so for a static cell `materials` IS byte-identical between frames.
- **Why it matters**: Below signal floor today (mat upload dwarfed by 134 KB/frame instance upload that legitimately changes), but ratchets up if MAX_MATERIALS empirical use grows.
- **Proposed fix**: 64-bit content hash (xxh3 over raw bytes) compared against last frame's hash; skip `copy_nonoverlapping + flush_if_needed` on hit. Mirrors terrain dirty pattern at scene_buffer.rs:1085. ~10 lines. Pairs naturally with **#781** (DrawCommand hash-cache) — same xxh3 primitive.

### INFO

#### NIF-PERF-14: `BsTriShape` per-vertex stride loop overhead — needs descriptor-major rewrite, not alloc tweak
- **Dimension**: NIF Parse
- **Location**: [crates/nif/src/blocks/tri_shape.rs:626-761](crates/nif/src/blocks/tri_shape.rs#L626-L761)
- **Cost**: per-vertex × ~5–10 stream calls; ~150K stream-fn calls per FO4 actor.
- **Status**: Observation only — fix would require a structural rewrite (pre-read entire vertex stride into Vec<u8>, unpack via flag-dispatched index arithmetic). Out-of-scope for an alloc-counting audit. Filed so the next BsTriShape redesign window includes it.

#### DIM8-04: Material upload path lacks dirty-flag stat for steady-state vs cell-transition discrimination
- **Dimension**: Telemetry / Observability
- **Location**: [crates/core/src/ecs/resources.rs:198-216](crates/core/src/ecs/resources.rs#L198-L216), [byroredux/src/main.rs:888-891](byroredux/src/main.rs#L888-L891)
- **Cost**: zero CPU; observability gap.
- **Status**: Defer to DIM8-01 closeout — without dirty tracking, the stat is "always 1." After DIM8-01 lands, add `materials_uploaded_this_frame: bool` + `materials_uploads_skipped: usize` to `ScratchTelemetry` so `ctx.scratch` shows skip ratio.

## Prioritized Fix Order

### Quick wins (each ≤30 LOC, ≤30 min)

1. **NIF-PERF-09** — port BSGeometry parser to existing `read_u32_array` / `read_u16_array` / `read_f32_array`. Pure deletion of push-loop pattern. **~30 LOC**.
2. **NIF-PERF-10** — extend `read_pod_vec::<[u16; 3]>`; replace 3 `chunks_exact` callers. **~25 LOC**.
3. **NIF-PERF-11** — change `MorphTarget.vectors: Vec<NiPoint3>`. **~5 LOC** + 1 consumer update.
4. **DIM8-01** — xxh3 dirty gate on `upload_materials`. **~10 LOC**. Naturally co-implements with #781.

### Architectural

5. **NIF-PERF-07 + NIF-PERF-08 (combined)** — promote `NifHeader.block_types` to `Vec<Arc<str>>` and add `name_arc()` to `HasObjectNET`. Both findings collapse to "`Arc::clone` replaces `Arc::from(&str)`." **~50 LOC** + call-site updates.
6. **NIF-PERF-13** — split `pre_parse_cell` into serial-extract + parallel-parse phases. Removes BSA mutex contention from worker critical path. **~30 LOC** with telemetry hook for measuring.

### Defer / observe

7. **NIF-PERF-12** — header bulk reads. Wait until NIF-PERF-07 touches the header anyway.
8. **DIM8-02** — `MaterialTable::new()` capacity hint. Wait until #779 right-sizing telemetry collects per-game peaks.
9. **DIM8-04** — material upload telemetry. Wait until DIM8-01 lands.
10. **NIF-PERF-14** — BsTriShape stride-loop rewrite. Wait until parser gets touched for a structural reason.

### Infrastructure (orthogonal, blocks regression coverage on items 1–6)

11. **Wire dhat for the NIF parse loop** — feature-gated integration test running `parse_nif` on a fixture cell's NIF set asserting `<= N` total allocations. Single piece of infrastructure that gives all 6 NIF-PERF-* findings a regression guard.

## Closed since last audit (verified this run)

- **NIF-PERF-01** (counter `to_string`) — closed via `bump_counter` helper at lib.rs:217-223
- **NIF-PERF-02** (`chunks_exact` in bulk readers) — closed via `read_pod_vec` at stream.rs:282-317
- **NIF-PERF-03** (`allocate_vec` misuse) — closed via `#[must_use]` at stream.rs:207
- **NIF-PERF-04** (ImportedScene Vec preallocation) — closed at import/mod.rs:739-743
- **NIF-PERF-06** (single-threaded streaming worker) — closed via #830 + #854 panic guard
- **R1-N4** (avg_albedo dead bytes) — closed via #804 (GpuMaterial 272 → 260 B)
- **R1-N6** (missing per-field offset test) — closed via #806 (`gpu_material_field_offsets_match_shader_contract` at material.rs:492)
- **R1-N7** (material_id == 0 overload) — closed via #807 (`MaterialTable::new()`/`clear()` seed slot 0 with neutral default)
- **#780 / PERF-N1** (no dedup telemetry) — closed (`ScratchTelemetry.materials_unique` + `materials_interned` plumbed end-to-end)

## Notes

- Telemetry gap remains: there is no `NifParseTelemetry` resource analogous to `ScratchTelemetry`. With #830 + the proposed NIF-PERF-13, a per-cell `parse_ms / extract_ms / models_parsed` resource would let the streaming pipeline observe its own efficiency.
- `repair_rotation_svd_or_identity` (rotation.rs:23) and `svd_repair_to_quat` (coord.rs:134) are confirmed non-regressing — both gated behind det-threshold fast-paths; SVD only runs on degenerate matrices (~1% of NIF rotations).
- R1 architecture is solid: 112 B GpuInstance + 260 B GpuMaterial, both with size-pin AND per-field-offset-pin tests. R1-reversal sentinel (`gpu_instance_does_not_re_expand_with_per_material_fields` at scene_buffer.rs:1477) defends the slim-instance contract.
- Open R1-adjacent items: **#779** (MAX_MATERIALS = 4096 over-allocates BAR by 1.5–1.8 MB; awaits empirical right-sizing data), **#781** (DrawCommand hash-cache; pairs with DIM8-01).
