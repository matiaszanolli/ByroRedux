# Texture Streaming — Design

**Status**: IMPLEMENTED through Phase 3, Phase 4 in progress (2026-11-04); written in M39 Phase 0 (#6627). This is
the *how* behind the [Texture Streaming PRD](../prd/texture-streaming.md)
(ACTIVE, tracking #6626), which owns the promise and every `TXS-` requirement
cited here. It applies the owner's §10 decisions (2026-11-03): Q1 reallocate
on change, Q2 (a) a CPU wanted-mip estimate, Q2 (b) BSA promotions re-extract
through the stream pool, Q3 the last BA2 chunk as the floor, Q4 a budget
derived from the live device-local budget, Q5 two settings with the presets
at their defaults. The census and cost figures in §2 were measured for this
document; the commands that reproduce them are in §2.4.

---

## 1. Shape

```
                     NIF import ──► per-mesh UV texel density (§5.1)
                                            │
 build_render_data ──► DrawCommand list ────┤ (texture indices, model matrix,
                                            │  in_raster / in_tlas)
                                            ▼
                         wanted-mip pass (main thread, per frame, §5)
                                            │
                       promotion queue ◄────┴────► demotion / eviction (§7)
                                │                          │
       stream pool: source read (§4) ──► staged mip range  │
                                │                          │
         flush_pending_uploads: new image, GPU copy of shared levels,
         upload of new levels, descriptor rewrite, old image → pending_destroy (§3)
```

Everything per-game stays in the archive layer (TXS-SRC-003): the renderer
receives a parsed DDS header, a level range and bytes, never an archive.

---

## 2. Phase 0 measurements

### 2.1 BA2 DX10 chunk layout (TXS-SRC-001, A4)

`crates/bsa/examples/ba2_mip_chunk_census.rs` reads each DX10 record's chunk
table through `Ba2Archive::dx10_layout` (nothing extracted). Vanilla FO4 is
the nine `Fallout4 - Textures*.ba2`, `TexturesPatch` and the six DLC texture
archives; the free high-resolution pack is counted apart. Starfield is the
`Textures*`, `LODTextures*`, `GeneratedTextures`, `Constellation`, `OldMars`
and `ShatteredSpace` texture archives.

| Measure | FO4 | FO4 HD pack | Starfield |
|---|---|---|---|
| DX10 records | 41 077 | 36 821 | 129 491 |
| Cubemaps | 72 | 0 | 25 |
| 2D records | 41 005 | 36 821 | 129 466 |
| One chunk only | 2 500 (6.1 %) | 1 745 (4.7 %) | 63 363 (48.9 %) |
| … single-mip (**whole**) | 176 | 189 | 7 891 |
| … whole chain within 256 px (**whole**) | 1 736 | 1 376 | 51 066 |
| … chain above 256 px: whole-file fallback (TXS-SRC-004) | 588 (78.6 MiB) | 180 (23.0 MiB) | 4 406 (450.9 MiB) |
| Multi-chunk records | 38 505 | 35 076 | 66 103 |
| … whose head chunks each hold one level | all | all | all |
| … last chunk tops out at 256 px | 97.5 % | 90.6 % | 89.4 % |
| … at 512 px | 2.5 % | 9.2 % | 10.6 % |
| … at 1024 px | 13 | 67 | 2 |
| 2D bytes, whole chains (inflated) | 31 323 MiB | 96 489 MiB | 84 200 MiB |
| Floor (last chunk) share of multi-chunk bytes | 8.8 % | 3.2 % | 4.6 % |

Every multi-chunk record stores one level per head chunk and the rest of
the chain in the last one, which matches Archive2's documented rule of a
chunk per level of at least 512 × 512 (Fallout 4 CK wiki, *Archive2*). The
large Starfield single-chunk records are single-mip (`textures\actors`,
`textures\interface`), so they are **whole** by class, not fallbacks.

### 2.2 BSA mip chains against a 256-px floor (Q3, TXS-RES-005)

`crates/bsa/examples/dds_mip_scan.rs --floor-edge 256` over each game's
texture BSAs. "Floor share" is the bytes of the levels at or below 256 px in
the streamable files, from the level sizes the header implies.

| Measure | Oblivion (+SI) | FO3 | FNV | Skyrim SE |
|---|---|---|---|---|
| DDS files | 19 841 | 12 261 | 22 054 | 31 833 |
| Cubemaps (**whole**) | 0 | 4 | 5 | 57 (353 MiB) |
| Single-mip (**whole**) | 5 825 (210 MiB) | 1 166 (85 MiB) | 1 879 (161 MiB) | 204 (334 MiB) |
| Whole chain within 256 px (**whole**) | 8 209 (321 MiB) | 8 286 (302 MiB) | 14 918 (494 MiB) | 22 755 (4 063 MiB) |
| Streamable | 5 807 (1 854 MiB) | 2 805 (1 365 MiB) | 5 252 (2 685 MiB) | 8 817 (11 583 MiB) |
| Floor share of streamable bytes | 15.0 % | 12.6 % | 11.4 % | 5.6 % |

Skyrim SE's chains stop one level early (at 2 × 2), which the tool counts
as truncated in 31 726 files; that changes no floor.

### 2.3 Read cost of a floor against a whole file (Q2 (b))

`crates/bsa/examples/texture_read_cost.rs`, a deterministic sample of files
whose level 0 is above 256 px, timed on the second read (warm page cache,
so the figure is the inflate). Ryzen 9 7950X, one thread.

| Archive | Samples | Whole-file read | Floor-only read |
|---|---|---|---|
| FO4 BA2 (zlib) | 2 000 | 2.270 ms per texture (455 MiB/s) | 0.150 ms (last chunk only), 6.6 % of the whole |
| FNV BSA (zlib) | 735 | 1.288 ms (402 MiB/s) | same as whole: one deflate stream, level 0 first |
| Skyrim SE BSA (LZ4) | 1 256 | 0.661 ms (2 081 MiB/s) | same as whole |

So a BA2 floor costs a fifteenth of a whole read, and each BSA promotion
re-pays one whole inflate (~0.7–1.3 ms on a stream-pool thread), which is
the cost Q2 (b) accepted.

### 2.4 Reproduce

```bash
cargo run --release -p byroredux-bsa --example ba2_mip_chunk_census -- "<FO4 Data>/Fallout4 - Textures1.ba2" …
cargo run --release -p byroredux-bsa --example dds_mip_scan -- --floor-edge 256 "" "<FNV Data>/Fallout - Textures.bsa" "<FNV Data>/Fallout - Textures2.bsa"
cargo run --release -p byroredux-bsa --example texture_read_cost -- ba2 "<FO4 Data>/Fallout4 - Textures"[1-9].ba2
cargo run --release -p byroredux-bsa --example texture_read_cost -- bsa "<Skyrim SE Data>/Skyrim - Textures"[0-8].bsa
```

---

## 3. Residency (Q1, TXS-RES)

### 3.1 Entry state

`TextureEntry` (`crates/renderer/src/texture_registry/mod.rs`) gains, for a
streamable texture:

| Field | Meaning |
|---|---|
| `mip_count` | levels of the full chain, from the parsed header |
| `floor_first` | the first level of the floor (§3.3) |
| `first_resident` | the first resident level; `floor_first` in **floor**, 0 in **full** |
| `wanted` | the wanted mip this frame (§5) and the case that set it (drawn / TLAS only / neither) |
| `last_wanted_frame` | the last frame `wanted` was finer than `floor_first` (eviction order, §7.2) |
| `last_promoted_frame` | for the hysteresis rule (§6.3) |
| `source` | how to re-read the high mips: an opaque locator the host owns (archive + path, §4) |
| `class` | the §3.2 class of the PRD, for `tex.residency` |

A **whole** texture keeps today's fields only. The invariant
`texture.is_some() iff ref_count > 0` stands.

### 3.2 States and transitions

The PRD's §3.1 is the vocabulary. The first flush of a streamable texture
creates an image of `mip_count − floor_first` levels from the floor bytes
(TXS-RES-001), so the entry goes **reserved → floor**. A **whole** texture
goes **reserved → whole** with today's code.

A promotion from `first_resident = a` to `b < a`:

1. The stream pool reads levels `[b, a)` (§4); nothing on the GPU changes yet.
2. At the next `flush_pending_uploads`, in one command buffer of the
   batch: create image `I'` with `mip_count − b` levels; copy levels
   `[a, mip_count)` from the live image `I` into `I'` at their new indices
   (`vkCmdCopyImage`, both in `TRANSFER_SRC` / `TRANSFER_DST`, then back to
   `SHADER_READ_ONLY`); upload `[b, a)` from staging through the copy regions
   of §3.4.
3. After the batch's fence (the flush already waits on it), rewrite the
   slot's descriptor to `I'`'s view, push `I` onto `pending_destroy` with the
   current frame id, and set `first_resident = b` (TXS-RES-002, -004).

A demotion from `a` to `c > a` is the same without an upload: copy
`[c, mip_count)` into a new image and retire the old. Both keep the handle,
so `TextureRegistry::len()` never moves (TXS-G-004).

The image view always spans exactly the image's levels, so level 0 of the
view is `first_resident` of the chain and no sampler LOD, raster or RT,
can address a non-resident level (TXS-RES-003). `ray_hit.glsl`'s explicit
`textureLod(…, 0)` then reads the finest *resident* level, which is the
intended behaviour.

### 3.3 Floor rule (Q3)

From §2.1–2.2 the last BA2 chunk tops out at **256 px** for 89–98 % of
multi-chunk records, so the floor edge is 256 px:

| Source | Floor |
|---|---|
| BA2 DX10, more than one chunk | the last chunk: `floor_first` is its `start_mip`, whatever edge that gives (256 px for most, 512 px for 2–11 %) |
| BA2 DX10, one chunk, chain above 256 px | whole-file read (TXS-SRC-004), floor = the levels at or below 256 px of it; counted on its own `tex.residency` line |
| BSA or loose | the levels whose larger edge is at most 256 px |
| Chain entirely at or below 256 px, single-mip, cubemap | **whole** (PRD §3.2) |

`avg_rgb` and `has_alpha` come from the floor upload: `average_rgb` already
reads the smallest level, and the floor always contains it (TXS-RES-006).
Its locator walks from the data offset over every larger level today; it
gains a variant that takes the first level present.

### 3.4 Upload path (TXS-UP)

- `build_dds_copy_regions` takes a first level and builds regions only for
  `[first, mip_count)`, returning the byte total of that same walk; the
  staging buffer is acquired at that total (TXS-UP-003).
- The region total against the buffer becomes an `ensure!` in
  `record_dds_upload`, a release-mode error (TXS-UP-002).
- A promotion's upload is a `PendingDdsUpload` with a level range and a
  target handle, queued like a first upload, so it shares the staging pool
  and the `MAX_UPLOAD_BATCH_BYTES` sub-batches (TXS-UP-004).

### 3.5 Release (TXS-RES-007)

`release_ref` already drops a queued first upload when the last holder goes
(#4879). It also drops a queued promotion for that handle, and a promotion
whose bytes arrive after the release is discarded on arrival. Every image
of the entry, live and retiring, goes through `pending_destroy` as today.

---

## 4. Source reads (Q2 (b), TXS-SRC)

| Source | First use (floor) | Promotion |
|---|---|---|
| BA2 DX10, multi-chunk | inflate the last chunk only | inflate the chunks covering `[b, a)` only |
| BA2 DX10, single chunk | inflate the chunk (the whole file) | inflate it again |
| BSA, compressed | inflate the whole file, keep the floor's bytes | re-extract and inflate the whole file on the stream pool, keep `[b, a)` |
| BSA raw, loose | read the floor's byte range | read `[b, a)`'s byte range |

`Ba2Archive` gains `extract_dx10_mips(path, first_mip) -> (Dx10Layout, Vec<u8>)`,
the bytes of every chunk whose range intersects `[first_mip, num_mips)`
in level order (TXS-SRC-001); `dx10_layout` (added in Phase 0) already
exposes the ranges. The provider (`byroredux/src/asset_provider/`) turns
a source into the renderer's input (parsed header of the full chain, first
level, bytes). Promotion reads run on the same stream pool as the texture
prefetch (`prefetch_textures`), and their staged bytes count against
`STAGED_BYTE_CAP` like any prefetched file (TXS-SRC-005). No high-mip bytes
are kept in host RAM between promotions.

---

## 5. Wanted mip (Q2 (a), TXS-WANT)

### 5.1 Per-mesh texel density

Each mesh records `world_per_uv`: the square root of the sum of its
triangles' object-space areas over the sum of their UV-space areas (UV
channel 0). It is the object-space length one UV unit covers, the
quantity Unreal's overview calls a "texel world size". It is computed
where every scene mesh is registered (`MeshRegistry`'s scene-upload
paths, `mesh::uv_texel_density`), from the same positions and UVs NIF
import produced, so terrain, water, distant LOD and Starfield `.mesh`
geometry carry it without a field per import path; decision Q2 (a)'s
"captured at NIF import" is met at that boundary. The same paths record
the mesh's object-space bounding sphere (`mesh::bounding_sphere`).
Degenerate UVs (zero UV area) give `None`, and such a mesh wants the floor.

### 5.2 Per-instance estimate

For each `DrawCommand` and each texture index it carries:

```
scale     = the largest axis scale of model_matrix
distance  = max(|camera − bound centre| − bound radius, near plane)
px_per_uv = (viewport_height / (2 · tan(fov_y / 2))) · world_per_uv · scale / distance
wanted    = clamp(floor(log2(texture_width / px_per_uv)), 0, floor_first)
```

A texture's wanted mip is the finest over its instances. The bound is the
mesh's object-space bounding sphere transformed by the draw's model
matrix (no `DrawCommand` field needed). A texture's width is its level-0
width (the larger edge for non-square). A drawn terrain tile wants its
layer textures at level 0: their UVs tile by a per-layer scale the
estimate does not model yet, so terrain trades no memory for no blur.
The pass is `texture_streamer::compute_wanted`; `apply_wanted` in the
registry turns its result into requests and demotions.

### 5.3 Cases (TXS-WANT-002, -003)

| Case | Wanted |
|---|---|
| Any instance has `in_raster` | §5.2 over the raster instances |
| Only `in_tlas` instances | §5.2 over them, plus one level (coarser), never past `floor_first` |
| Neither | `floor_first` |

The one-level bias for TLAS-only textures is the secondary-ray rule: a
reflected or bounced ray samples a surface at the footprint of a longer
path than the camera distance, so the camera-distance estimate is the
finest it could need. A11 (the PRD's reflection A/B) is the measurement
that holds or rejects it.

### 5.4 Cadence

The pass runs every frame after `build_render_data`, over the draw list it
already built. It is O(draws × texture slots per draw) with no allocation in
steady state.

---

## 6. Promotion queue

### 6.1 Order (TXS-WANT-004)

The queue is rebuilt each frame from the textures whose `wanted` is finer
than `first_resident`, sorted by:

1. drawn (any `in_raster`) before TLAS-only before neither;
2. larger deficit (`first_resident − wanted`) first;
3. nearer distance first.

### 6.2 Per-frame cap (TXS-BUD-006)

Each frame `flush_pending_promotions` takes queued changes in order until
their staged bytes reach `MAX_PROMOTION_UPLOAD_BYTES_PER_FRAME` (32 MiB) or
their count reaches `MAX_RESIDENCY_CHANGES_PER_FRAME` (64); the remainder
waits for the next frame, and the first change always goes (it cannot be
split). The count cap exists because a demotion stages nothing but still
allocates an image and copies the levels it keeps, so a budget drop would
otherwise land thousands of replacements in one frame. Both are in
[Memory Budget](memory-budget.md) § Texture Registry with ledger tests. The
values are starting points set from the size of a 4096² BC7 chain (~21 MiB)
and a frame's copy-queue stall, not from a p99 sweep of the A9 soak; the
streaming-on frame time on the FO4 soak is the check on them.

### 6.3 Hysteresis (TXS-WANT-005)

A texture is demoted only when its `wanted` has been coarser than
`first_resident` for `DEMOTE_AFTER_FRAMES` consecutive frames **and** at
least that many frames have passed since `last_promoted_frame`. It starts
at 120 frames (2 s at 60 fps); Phase 2 sets the value as the smallest
window that gives zero churn on the A7 pinned-camera benches and the A6
fresh-cell loads, and `tex.residency` prints it.

---

## 7. Budget and eviction (Q4, TXS-BUD)

### 7.1 Budget

```
texture_budget = max(F × capacity − reserve, 128 MiB)
capacity       = GpuMemoryBudget live DEVICE_LOCAL budget (VK_EXT_memory_budget),
                 else the smallest DEVICE_LOCAL heap
reserve        = (device-local bytes this process holds − texture-image bytes)
                 + (static-BLAS budget − static-BLAS bytes in use)
F              = 0.80
```

- `capacity` is what `VulkanContext::live_memory_budget` reports; without the
  extension it is `smallest_device_local_heap_bytes`, and the process's usage
  comes from the allocator's own report instead of the extension.
- The reserve is **measured, not summed from the ledger**: everything the
  process holds on the device that is not a texture image (live or retiring)
  is by definition what the budget must leave, and the one term the ledger
  bounds but the process has not yet taken — the static BLAS growing to its
  own budget — is added as headroom. This replaces the first sketch's
  "sum of the ledger's non-texture rows at the current resolution": it
  follows the resolution, FSR's SDK memory and whatever else is resident
  without a second place that lists them. The ledger stays the home of the
  constants it does state (the cap, the fraction, the minimum).
- `F` = 0.80 is the engine's existing "approaching OOM" line
  (`allocator::approaching_oom_line`), so a texture budget never plans past
  the point the allocator already warns at. It is a policy choice. A8's
  minimum-card emulation (the override at that card's budget) is the check on
  it; the sweep that would pick a larger value was not run.
- The 128 MiB floor stops a transiently large non-texture figure (a cell
  load's staging, a BLAS build's scratch) from collapsing the budget to
  zero. Below it the over-budget warning (TXS-BUD-002) names the shortfall.

`texture_registry::derive_budget` is the pure function; the override
(`render.texture_budget_mib`, §8), when non-zero, replaces the whole
expression unclamped. `VulkanContext::refresh_texture_budget` re-derives it
twice a second while streaming is on, logs the first derivation with its
terms (TXS-BUD-001), and `tex.residency` prints the current one.

### 7.2 Eviction (TXS-BUD-002…004, -007)

`apply_wanted` spends the budget in payload bytes (the DDS header's level
sizes for streamed textures, the allocation size for **whole** ones; the
allocation of a streamed image runs ~2 % above its payload, so the
resident figure `tex.residency` prints can sit that far over the budget).
Queued changes count at the level they will leave resident.

1. **Over budget**, demote one level at a time. First the textures wanted
   coarser than they are resident, down to the level they are wanted at
   (no visible loss), then any texture above its floor, never below it.
   Within a sweep the order is: textures promoted outside the hysteresis
   window before those inside it (TXS-WANT-005 holds under pressure unless
   nothing else is left), then oldest `last_wanted_frame` first
   (TXS-BUD-003).
2. **Each promotion**, in queue order (§6.1): kept if it fits; otherwise made
   to fit by demoting textures wanted coarser than resident, oldest
   last-wanted first; otherwise **declined** (TXS-BUD-004): it leaves the
   queue, the texture keeps its image, and the decline is counted once per
   episode (until the texture's wanted level changes). A texture wanted at
   its resident level is never evicted to fund another's promotion, so two
   wanted textures cannot swap levels every frame.
3. Floors, **whole** textures and queued changes are never evicted; if they
   alone exceed the budget one warning names the totals (TXS-BUD-002).
4. A promotion whose image allocation fails (TXS-BUD-007) is counted as a
   decline, leaves the entry exactly as it was (the old image retires only
   after the new one is installed), and is not requested again for
   `DEMOTE_AFTER_FRAMES` frames.

Retiring images (PRD §2) are counted apart and do not count against the
budget.

A budget below the wanted set binds permanently: the wanted textures that do
not fit are declined each episode and the scene sits at the budget. A scene
that keeps gaining floors while the budget is tight (an interior's actors
arriving after its architecture) can promote a texture and then evict it
when the next floors land; the recent-promotion ordering above narrows this
but does not remove it (Skyrim `WhiterunBanneredMare` at 30 MiB: 12
promotions and 5 evictions before the ordering was added, 5 promotions and 0
evictions after, three runs).

## 8. Settings (Q5, TXS-BUD-005, TXS-RES-008)

| Key | Type | Default | Meaning |
|---|---|---|---|
| `render.texture_streaming` | bool | off until Phase 4, then on | off restores today's full-chain upload (the same-binary control of every A/B) |
| `render.texture_budget_mib` | integer | 0 | 0 uses §7.1; any other value is the budget in MiB |

Both are registered in `core::settings::builtin` beside the existing
`render.*` keys. `assets/graphics_presets.toml` sets neither (Q5).

---

## 9. Telemetry (TXS-TEL)

- `tex.residency` prints one line per state and per class (count, bytes),
  then current and peak resident bytes, retiring bytes, budget, floor bytes,
  deficits, declines, fallback reads, `DEMOTE_AFTER_FRAMES` and the per-frame
  cap. `tex.residency <path>` prints the entry's state, resident range,
  wanted mip with its case, `last_wanted_frame` and `last_promoted_frame`.
- The `tex-stream:` bench line (`texture_streamer::tex_stream_bench_line`),
  printed with streaming off too so one binary yields both arms:
  `streaming resident_bytes peak_bytes floor_bytes promotions demotions
  evictions declines churn hysteresis_frames cell_loads floor_ready_ms
  reserved deficits settled settle_ms settle_restarts`.
  - **Floor-ready** runs from the cell's record parse done (an interior
    apply job's `begin`, an exterior job's `begin` once the cell is found)
    to its last texture flush, recorded only when nothing is left queued
    (`TextureRegistry::record_cell_load_ready`). Each job owns its clock, so
    overlapping or cancelled loads cannot skew another's. The record parse is
    excluded: it reads no texture and costs the same in both arms.
  - **Churn** is a promotion and a demotion of one texture landing within
    `DEMOTE_AFTER_FRAMES` of each other, either order. §6.3 never demotes
    inside the window after a promotion, so in practice it counts a demotion
    promoted back inside the window.
  - **Settle** runs from the last camera stop (or the last cell load's
    textures landing) to the first frame with no promotion requested,
    queued or being read. A stop tolerates 1 unit of travel and 0.1° of turn
    against the clock's start pose: the pinned MedTekResearch01 bench camera
    wobbles 0.0016 units in Y on alternate frames, which restarted an
    exact-match clock every frame. `settle_restarts` near the frame count
    means the camera never stopped.
  - **Evictions** (demotions the budget ordered) and **declines** (promotions
    it refused, or whose allocation failed) read 0 while the budget is not
    exceeded. `budget_bytes` and `peak_above_floor_bytes` (session peak of
    resident less floors and whole textures, the quantity A8 and A10 hold
    to the budget) close the line.
- `ownership_sample.rs` samples every texture image's bytes (live and
  retiring) as the `texture_resident_bytes` class beside `texture_live_slots`
  (TXS-TEL-003). It is an `Occupancy` class: with streaming the bytes follow
  the camera, so the soak holds it to the growth check (no strictly rising
  series beyond 1 %), not to an exact return.

---

## 10. Threading

| Work | Thread |
|---|---|
| Wanted-mip pass, queue build, eviction choice | main, after `build_render_data` |
| Source reads and inflate | stream pool (the prefetch pool) |
| Image creation, copies, uploads, descriptor writes | main, inside `flush_pending_uploads` at frame begin |
| Retiring image destruction | main, the existing `pending_destroy` drain |

No path calls `device_wait_idle` (TXS-RES-004).

---

## 11. Left open

| Item | State |
|---|---|
| `MAX_PROMOTION_UPLOAD_BYTES_PER_FRAME` / `MAX_RESIDENCY_CHANGES_PER_FRAME` | set at 32 MiB / 64 (§6.2); not swept against frame-time p99 |
| `DEMOTE_AFTER_FRAMES` | 120 (2 s at 60 fps): zero churn on the unpinned-budget benches of every game run so far |
| `F` | 0.80, the engine's OOM line (§7.1); not swept |
| TLAS-only one-level bias | A11 (see the PRD's Phase 2 exit) |

---

## 12. Measured results (2026-11-04)

Measured on the 4070 Ti dev box (desktop resident; the driver reported 9–10
GiB of device-local budget), release build, free camera at each cell's
default pose, one run per arm unless stated (the FO4 arms were repeated three
times at `93a0071b6` with the same shape). Control = `render.texture_streaming`
off in the same binary, with an isolated `BYROREDUX_SETTINGS_PATH`. Each row is
`target/release/byroredux --game <g> --cell <cell> --bench-frames 300
--bench-hold`, then `tex.residency` and `tex.missing` through `byro-dbg` and
the `tex-stream:` bench line.

### 12.1 Floor first, settle (A6, A7)

| Scene | Resident MiB, off → on | Floor-ready ms, off → on | Settle ms | Promotions | Deficits / churn / reserved | `textures=` / missing |
|---|---:|---:|---:|---:|---|---|
| FO4 MedTekResearch01 | 766.2 → 135.9 | 3 825 → 1 433 | 1 053 | 90 | 0 / 0 / 0 | 920 = 920 / none |
| FNV GSProspectorSaloonInterior | 76.7 → 38.5 | 595 → 440 | 315 | 64 | 0 / 0 / 0 | 345 = 345 / none |
| Skyrim SE WhiterunBanneredMare | 399.0 → 54.8 | 1 787 → 1 309 | 268 | 50 | 0 / 0 / 0 | 352 = 352 / none |
| Oblivion AnvilHeinrichOakenHallsHouse | 15.9 → 10.5 | 467 → 403 | 57 | 22 | 0 / 0 / 0 | 251 = 251 / none |
| FO3 MegatonMoriartysSaloon | 36.8 → 17.3 | 414 → 341 | 225 | 26 | 0 / 0 / 0 | 195 = 195 / none |
| Starfield citycydoniamainlevel | 1 370.8 → 116.4 | 10 754 → 9 580 | 3 512 | 53 | 0 / 0 / 0 | 898 = 898 / 9 = 9 |

### 12.2 Budget adherence (A8)

`render.texture_budget_mib` pinned below each scene's streaming-on working
set. "Above" is resident less floors and **whole** textures (PRD §8).

| Scene | Budget MiB | Resident MiB | Above floors+whole, MiB | Evictions / declines | `tex.missing` |
|---|---:|---:|---:|---|---|
| FO4 MedTek | 120 / 100 | 121.7 / 101.7 | 46.7 / 26.7 | 0 / 49, 0 / 65 | none (control: none) |
| FNV Prospector | 30 / 24 | 30.4 / 24.4 | 12.1 / 6.1 | 0 / 42, 0 / 54 | none |
| Skyrim SE Bannered Mare | 40 / 30 | 40.5 / 30.5 | 19.8 / 9.9 | 0 / 39, 0 / 45 | none |
| Starfield cydonia main level | 100 / 85 | 101.8 / 86.8 | 31.3 / 16.3 | 0 / 37, 0 / 48 | 9 (control: 9) |

Resident runs about 1.7 % over the budget figure because the budget counts
payload bytes and the allocation rounds up (§7.2). The Skyrim 30 MiB row
first read 12 promotions, 5 evictions and churn 5; see §7.2 for what changed.

### 12.3 Soaks (A9, A10)

- **FO4 Commonwealth 0,0 `grid-soak`** (1 800 frames, five cycles,
  `BYRO_VALIDATION=1`, streaming on at a pinned 400 MiB): zero `VUID-` or
  `SYNC-HAZARD` messages; `texture_resident_bytes` flat across the cycles
  (406 → 417 MiB, against 2 810 MiB with streaming off); the same 151 942
  entities, 14 286 draws and 8 unique missing textures as the control. The
  `m-exteriors.sh fo4 soak` script still hard-fails on `physics_bodies`
  (+17 after five cycles) in both arms, and identically (9 232 → 9 249) on
  the session's starting commit `022d692bd` built separately, so that owner
  is not the texture work's and the A9 criterion ("passes with the new
  owner") is met for the owner and open for the script.
- **FNV `p5-soak.sh`** (A10), 30 minutes, streaming on at a pinned 96 MiB:
  105 cycles, no crash, stuck transition or control loss; session-peak 16.6
  MiB above floors+whole against the 96 MiB budget; 2 150 promotions, 0
  demotions, 0 evictions, 0 declines, 0 churn. Engine RSS drifts upward over
  the run (2 954 → 4 134 MiB at cycle 77, 3 809 MiB at cycle 105, under the
  script's 1.5× gate); the same soak with streaming off drifts the same way
  (2 661 → 3 836 MiB by cycle 50, against 2 954 → 3 718 on), so the drift is
  not streaming's.

### 12.4 Cost and a defect found

- The wanted-mip pass costs 0.52 ms a frame on FO4 MedTek (7 983 draws) and
  0.14 ms on FNV Prospector, measured with a temporary timer around the pass.
  It was 1.5 ms before the per-slot lookups became a dense table and the
  per-frame maps used FxHash.
- The first A10 runs crashed: `SIGSEGV` in the NVIDIA driver under
  `TextureRegistry::begin_frame`, 3 of 3 within three cycles of the
  exterior→interior quickload, with streaming on and not with it off.
  Validation named it: `vkUpdateDescriptorSets` on an invalid `VkImageView`.
  A queued bindless descriptor write kept the view that was current when it
  was queued, and the deferred-destroy ring (aged by frames begun) could free
  that view before the slot replayed its queue. Replay now resolves each
  `(handle, binding)` against the live entry (`23f4aeee3`); the same soak then
  passes 25–27 cycles in three minutes and is validation-clean.

### 12.5 Not met

- **Starfield `citycydoniamainlevel`** (A6–A8) ran on 2026-11-05 once the box had
  the RAM (323 314 entities, 6 837 meshes; the engine peaks at ~16 GiB RSS and
  the host dipped to 1.6–2.0 GiB available, so run it with nothing else open and
  a memory guard). Same shape as the other games: 898 `textures=` in both arms,
  9 unique missing textures in every arm (control and both budgets), 874
  tail-only against 2 whole-file floor reads, one settle restart. The
  "Above floors+whole" figure at budget 100 is 31.3 MiB and at 85 it is 16.3;
  the 37 and 48 declines are the budget refusing promotions, which `tex.residency`
  also counts as deficits while a budget is pinned.
- **A11** (reflection A/B, FO4 MedTek at its default pose, 400-frame stepped
  captures, three per arm): on-versus-off mean absolute difference 0.083–0.099
  of 255 against 0.063–0.073 between equal arms; p99 of the per-pixel channel
  maximum 2 against 1. The difference is not the wanted-mip choice: with every
  texture forced to level 0 under streaming it is 0.087, and biasing the
  wanted level one or two levels finer leaves it at about 0.09. Shifting one
  capture by 4 frames moves the same pose by 1.1–1.8, so the scene is
  timing-sensitive and the arms differ in load time; the residue sits at
  edges of small props and specular glints, half of it on the same pixels as
  the forced-level-0 run. A time-pinned capture is needed before A11 can be
  called met or failed; the pose also is not mirror-like, so it does not test
  the secondary-ray bias this requirement exists for.
  **Repeat, 2026-11-05, time-pinned** (`--bench-mode renderer-static`, so the
  clock is frozen, 400 frames, same MedTek cell, camera at (-1536, 70, -768)
  pitched down 0.2, two captures per arm): equal arms still differ by mean 0.133
  (~200 000 pixels, p99 2), on-versus-off by 0.195–0.207 (p99 3), so freezing
  time did not remove the arm-to-arm noise or the residue above it. The
  Laplacian variance of the frames is the same in both arms (259.1 / 259.3 off,
  258.4 / 258.2 on), so nothing got blurrier. The pose is still not mirror-like:
  `render.debug material_lobe` shows every visible surface dielectric except a
  few papers, so no reflected texture is on screen. A mirror pose has to be found
  in another FO4 cell, with a metal or glass surface facing a texture-rich wall.
- **Open question for A11's real subject.** A reflected ray samples with the
  explicit LOD `mipBias` (`raytrace.glsl`, `sampleRayHitBase`), which is the
  roughness-scaled blur level counted from the *full* chain. On a texture whose
  top `k` levels are not resident the view starts at chain level `k`, so a
  rough reflection (`mipBias > 0`) reads chain level `mipBias + k`, `k` levels
  too coarse, and a smooth one (`mipBias = 0`) reads level `k`. §3.2 only
  argued the `mipBias = 0` case. Not measured; it needs the mirror pose above.
- **Streaming on by default** (the Phase 4 switch flip) is withheld until the
  gates above close.
