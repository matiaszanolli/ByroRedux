# Texture Streaming — Design

**Status**: PROPOSED (2026-11-03), written in M39 Phase 0 (#6627). This is
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

At NIF import, each mesh records `world_per_uv`: the square root of the sum
of its triangles' object-space areas over the sum of their UV-space areas
(UV channel 0). It is the object-space length one UV unit covers, the
quantity Unreal's overview calls a "texel world size". It is captured in
the `Imported*` tier and carried through `translate()` with the mesh
([NIFAL](nifal.md)), then stored per `MeshHandle` in `MeshRegistry`.
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
instance's `WorldBound`; `DrawCommand` carries no bound today, so Phase 2
adds its centre and radius where `build_render_data` already reads it for
the frustum cull. A texture's width is its level-0 width (the larger edge
for non-square).

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

Each frame at most `MAX_PROMOTION_UPLOAD_BYTES_PER_FRAME` of promotion
uploads are submitted; the remainder waits for the next frame. Phase 1 sets
the value from the measured upload cost per MiB under the forced
floor-then-full sequence on the A9 soak, as the largest cap that keeps the
frame-time p99 within run-to-run noise of the streaming-off control, and
records it in [Memory Budget](memory-budget.md) with a ledger test.

### 6.3 Hysteresis (TXS-WANT-005)

A texture is demoted only when its `wanted` has been coarser than
`first_resident` for `DEMOTE_AFTER_FRAMES` consecutive frames **and** at
least that many frames have passed since `last_promoted_frame`. Phase 2
sets the value as the smallest window that gives zero churn on the A7
pinned-camera benches and the A6 fresh-cell loads, and `tex.residency`
prints it.

---

## 7. Budget and eviction (Q4, TXS-BUD)

### 7.1 Budget

```
texture_budget = F × device_local_budget − non_texture_reserve
```

- `device_local_budget` is `GpuMemoryBudget::live_heap_budget_bytes`,
  refreshed at the throttled telemetry sample
  (`VulkanContext::sample_live_memory_budget`). Without
  `VK_EXT_memory_budget` it is `GpuMemoryBudget::smallest_heap_bytes`.
- `non_texture_reserve` is the sum of the [Memory Budget](memory-budget.md)
  ledger's device-local rows other than textures at the current resolution
  (scene buffers × frames in flight, the static BLAS budget, reservoirs,
  screen-sized RT and post-process images, terrain height maps). One
  function computes it beside the ledger tests, so the ledger stays the
  only place the figures live.
- `F` is the headroom fraction. Phase 3 sets it from A8: the largest value
  at which the minimum-card emulation (the override set to that card's
  budget) runs validation-clean with no allocation failure.

The override setting (§8), when non-zero, replaces the whole expression.
The budget is logged once at renderer start with its terms (TXS-BUD-001),
then printed by `tex.residency` on each change.

### 7.2 Eviction (TXS-BUD-002…004, -007)

Over budget, the streamer demotes the texture with the oldest
`last_wanted_frame` by one level, repeating until under budget or every
texture is at its floor (TXS-BUD-003). Floors and **whole** textures are
never evicted; if they alone exceed the budget, one warning names both
totals (TXS-BUD-002). A promotion that would exceed the budget first evicts
by the same order and is declined, and counted, when that cannot make room
(TXS-BUD-004). A failed image allocation for a promotion leaves the entry
exactly as it was (TXS-BUD-007): the old image is retired only after the new
one exists and its copies are recorded.

Retiring images (PRD §2) are counted apart and do not count against the
budget; they are bounded by the per-frame cap times `MAX_FRAMES_IN_FLIGHT`.

---

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
- The `tex-stream:` bench line: resident bytes (current, peak), promotions,
  demotions, evictions, declines, churn, floor-ready ms of the last load.
- `ownership_sample.rs` samples resident texture bytes beside
  `texture_live_slots` (TXS-TEL-003).

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

| Item | Settled by |
|---|---|
| `MAX_PROMOTION_UPLOAD_BYTES_PER_FRAME` | Phase 1 measurement (§6.2) |
| `DEMOTE_AFTER_FRAMES` | Phase 2 measurement (§6.3) |
| `F` | Phase 3 measurement (§7.1) |
| TLAS-only one-level bias | A11 in Phase 2 (§5.3) |
