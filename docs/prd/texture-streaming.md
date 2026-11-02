# Texture Streaming — Product Requirements

**Status**: PROPOSED (2026-11-01). Revision 2, same day, folds in review
round 1 (the BC2 overrun premise corrected to #4512's fix, settings-registry
and image-replacement HEAD claims corrected, the "wanted mip" attribution
withdrawn and the Unreal citations quoted verbatim, testable wording for
TXS-RES-003 and TXS-WANT-004, launcher and debug-CLI owners added, prose
numbers moved to links). Revision 1 was the first draft: the M39 promise
(low mips first, high mips on demand, a texture VRAM budget with LRU
eviction), the residency vocabulary, requirements by area, phases and the
five decisions it needs from the owner. Nothing below is built as a result
of this document yet. Statements about how an original engine, a file format,
a tool or an external specification behaves cite an external source; a claim
that could not be verified is marked **Phase 0: verify** instead of being
stated as fact.

**Scope**: M39 in the [ROADMAP Tier 5 table](../../ROADMAP.md#tier-5--renderer-polish-quality-not-capability).
Every 2D DDS texture the engine loads from a BSA, a BA2 or the loose tier
becomes sampleable at a coarse resolution (its *floor*) as soon as it is
first used, gains its finer mips only while something on screen or in the
ray-traced scene wants them, and loses them again, least recently wanted
first, when the texture pool goes over its budget. In scope: the upload
path's mip-range support (and confirming closed the BC2 staging-overrun
Known Issue that #4512 fixed), mip-range reads in the archive layer, the
residency state per texture, the wanted-mip signal, the budget, eviction,
and the telemetry that proves each one. Out of scope: bindless slot reuse
(the generational free-list [Memory Budget § Texture Registry](../engine/memory-budget.md#texture-registry)
tracks as tech debt), offline
re-baking of archives into mip-ordered bundles (M82), geometry streaming
(M53), page-granular virtual texturing, and texture recompression or
upscaling (`tools/texture-upscale`).

**Related**: this document states the *promise*; the designs below own the
*how*. The streamer itself has no design yet; Phase 0 writes it
(`docs/engine/texture-streaming.md`, to be registered in
[docs/engine/index.md](../engine/index.md)).

| Mechanism | Owning design |
|---|---|
| VRAM ceilings, texture registry slots, staging pool, deferred destroy, BLAS LRU precedent | [Memory Budget](../engine/memory-budget.md) |
| DDS parse, staging copy, layout transitions, bindless registry | [Vulkan Renderer § Texture upload](../engine/renderer.md#texture-upload) |
| Texture resolve, cache keys, batched upload at cell load | [Asset Pipeline § 12](../engine/asset-pipeline.md#12-texture-lookup-and-upload) |
| BSA compression, BA2 DX10 mip chunks, DDS header reconstruction, texture prefetch | [Archives](../engine/archives.md) |
| Per-game engine tiers | [Game Compatibility](../engine/game-compatibility.md) |
| `SettingsRegistry`, settings persistence, graphics presets, GPU pre-flight | [Launcher § 4 Settings](../engine/launcher.md#4-settings-one-model-two-skins) |
| Dotted console commands (`tex.*`) served to `byro-dbg` | [Debug CLI § CommandRegistry dispatch](../engine/debug-cli.md#commandregistry-dispatch-dotted-names) |
| Wanted-mip signal, residency state machine, budget arithmetic, eviction order | `docs/engine/texture-streaming.md` (Phase 0 writes it) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `TXS-` prefix.

---

## 1. Product thesis

Texture memory is the one large VRAM consumer with no ceiling. The
[Memory Budget](../engine/memory-budget.md) page states the owner's frame
(the dev card, the ray-tracing minimum card and the whole-renderer target,
in its opening paragraph and under its
[VRAM Rough Budget](../engine/memory-budget.md#vram-rough-budget-rtx-4070-ti-typical-fnv-interior)
table); the minimum card is the one this feature exists for. Static
BLAS residency is held to a fixed budget with LRU eviction
([Memory Budget § LRU eviction](../engine/memory-budget.md#lru-eviction));
textures are uploaded as full mip chains and stay resident until the last
cell that references them unloads
([Memory Budget § Texture Registry](../engine/memory-budget.md#texture-registry)).
The rough budget's texture row is the content row with the widest
typical-to-peak spread
([Memory Budget § VRAM Rough Budget](../engine/memory-budget.md#vram-rough-budget-rtx-4070-ti-typical-fnv-interior)),
and nothing bounds it.

The feature is for two readers. The player on the minimum card needs exteriors
and dense interiors (FO4 MedTek, Commonwealth grid-crosses) to fit; the
player on any card needs a newly streamed cell to show its surfaces without
waiting for the finest mip of every DDS to inflate and upload.

The original engines already trade resolution for memory, but globally:
Gamebryo's renderer interface carries one renderer-wide mipmap skip level
(`NiRenderer::SetMipmapSkipLevel`), which `NiDX9SourceTextureData` documents
as "the number of mipmap levels to skip with loading the DX representation
from the texture" (Gamebryo 3.2 reference, `Include/NiRenderer.h` and
`Documentation/Reference/NiDX9Renderer/NiDX9SourceTextureData.htm` under
`/mnt/data/src/reference/gamebryo-v32/`), and Oblivion's
video presets set `iTexMipMapSkip` from 2 (Very Low, Low) through 1
(Medium) to 0 (High, Ultra High) (UESP, *Oblivion Mod:Oblivion.ini*,
§ Video Settings Presets; local dump `uesp-wiki/Oblivion Mod/Oblivion.ini.wiki`).
Redux replaces the global skip with a per-texture decision.

**Corollary that shapes every requirement below: resolution is spent per
texture, where it is seen, under one byte budget; the floor of every
texture is always resident, and no texture ever samples a mip level that
is not resident.** The engine will not reduce every texture uniformly, will
not page sub-mip tiles, will not rebake the archives, and will not let a
budget overrun turn a surface into the missing-texture checker.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Mip chain** | A DDS's levels from the main image (level 0) down to the declared last level; level 0 is stored first, each following level after it (Microsoft, *DDS Texture Example*, <https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-file-layout-for-textures>) |
| **Streamable texture** | A 2D, single-layer DDS whose mip chain extends above its floor. Every other texture is **non-streamable** (§3.2) |
| **Floor** | The coarsest levels of a streamable texture, uploaded on first use and never evicted while the texture has a holder. Its size rule is decision Q3 |
| **High mips** | The levels finer than the floor |
| **Resident range** | The levels `[first_resident, mip_count)` present in GPU memory for one texture. `first_resident = 0` means the full chain |
| **Wanted mip** | The finest level the streamer wants resident for a texture this update. The term is this document's; Unreal Engine's overview calls the comparable quantities the texture's "ideal" resolution and, after the budget pass, its "planned" one (Epic Games, *Texture Streaming Overview*, <https://dev.epicgames.com/documentation/en-us/unreal-engine/texture-streaming-overview-for-unreal-engine>) |
| **Mip deficit** | A texture whose resident range is coarser than its wanted mip |
| **Promotion / demotion** | Moving `first_resident` toward level 0 / away from it |
| **Texture pool budget** | The byte ceiling on the GPU memory that resident texture images may hold (§5.5, decision Q4) |
| **Holder** | One acquisition of a texture's handle, counted by `TextureEntry::ref_count` (`crates/renderer/src/texture_registry/mod.rs`); the registry's existing term is "reference" (#524) |
| **Chunk** | One independently compressed piece of a BA2 DX10 texture record, covering a contiguous mip range (`Dx10Chunk` in `crates/bsa/src/ba2.rs`; [Archives § DX10 records](../engine/archives.md#dx10-records-24-bytes-base--24-bytes-per-chunk)) |
| **Settle** | The point after a load or camera move where no promotion is queued and none is in flight |

---

## 3. Core model

### 3.1 Residency states

A texture is in exactly one state. `tex.residency` (TXS-TEL-001) prints the
state count, and the state per texture on request.

| State | Meaning | Descriptor points at |
|---|---|---|
| **reserved** | Handle allocated, upload queued, nothing on the GPU (the #881 deferred-upload behaviour that exists today) | the fallback texture of the matching view kind |
| **floor** | Only the floor is resident | the floor image |
| **partial** | The floor plus some, not all, high mips | an image whose levels are exactly the resident range |
| **full** | The whole chain is resident | the full image |
| **whole** | A non-streamable texture, uploaded whole (§3.2) | its image |

Transitions: reserved → floor or whole at the batched flush; floor ⇄ partial
⇄ full by promotion and demotion; any state → released when the last holder
drops. A failed promotion leaves the texture in its prior state.

### 3.2 Texture classes

| Class | Streamable | Why |
|---|---|---|
| 2D DDS with a mip chain above the floor (diffuse, normal, gloss, glow, detail, …) | yes | the case M39 exists for |
| 2D DDS whose whole chain fits inside the floor | no, **whole** | nothing to stream |
| Single-mip DDS | no, **whole** | no coarser level exists. The CS wiki, writing about Oblivion menu textures, says some exporters "write a mipmap count of 1 when it should be 0", which "causes the textures to display corrupted pixels if the player's texture quality isn't at the maximum setting" (*Oblivion XML Reference*, local dump `cs-uesp-wiki/(main)/Oblivion XML Reference.wiki`); `parse_dds` already treats a count of 0 as 1. Redux uploads them whole and counts them against the budget |
| Cubemap DDS (six layers, `TextureViewKind::Cube`) | no, **whole** | environment maps are sampled by direction, not by on-screen texel density |
| RGBA uploads (`register_rgba`, `update_rgba`, `DynamicRgbaUploads`: Scaleform, MenuXml HUD, egui) | no, **whole** | engine-generated, single-mip |
| Fallback and neutral-fallback handles | no, **whole** | the targets of every redirect |
| Terrain per-layer height maps (#6195) | no, **whole** | engine-generated and already capped ([Memory Budget § Terrain height maps](../engine/memory-budget.md#terrain-per-layer-height-maps-6195-6314)) |

### 3.3 Inventory at HEAD

The inventory below is the state at HEAD on 2026-11-01 (`66550cf8a`). It is
hand-written; Phase 1 replaces the residency rows with the output of
`tex.residency`, and the [Memory Budget](../engine/memory-budget.md) ledger
remains the home of the constants.

| # | Piece | Today |
|---|---|---|
| 1 | DDS parse | `parse_dds` (`crates/renderer/src/vulkan/dds.rs`) clamps the header mip count to the full chain and rejects edges above `MAX_TEXTURE_DIMENSION` |
| 2 | Upload | `Texture::record_dds_upload` (`crates/renderer/src/vulkan/texture.rs:212`) creates the image with `meta.mip_count` levels and copies every level from one staging buffer; `build_dds_copy_regions` (`texture.rs:590`) walks `dds::mip_size` for every level and layer and returns the byte total from that same walk (#4512). There is no mip-range entry point |
| 3 | Staging-size check | a short payload is a release error (`ensure!`, #4511, `texture.rs:228`) and the staging buffer is acquired at `dds::total_data_size`; the region total is compared with that size and with the buffer through `debug_assert!` only (`texture.rs:381`–`389`). The live +8 B overrun was traced by #4512 (closed 2026-09-20, `80813026a`) to pooled staging released at the driver-rounded `allocation.size()`, not to region pricing; pool entries now record the requested size |
| 4 | Batched flush | `TextureRegistry::enqueue_dds_with_clamp*` reserves the slot with a fallback descriptor and `flush_pending_uploads` drains the queue in sub-batches of at most `MAX_UPLOAD_BATCH_BYTES` (`crates/renderer/src/texture_registry/upload.rs`, `mod.rs`) |
| 5 | Holders and release | `TextureEntry::ref_count`; the last `drop_texture` / `drop_textures` moves the image to the entry's `pending_destroy` ring, redirects the slot to the fallback and purges the path cache (`texture_registry/release.rs`); cell unload calls `drop_textures` (`byroredux/src/cell_loader/unload.rs`) |
| 6 | Slot space | grow-only: a dropped slot's index is never reused (`release.rs` module doc, #372). #2030 closed (`cae95112`) with the slot telemetry only (`live_slot_count` / `dead_slot_count`); the free-list is tracked as tech debt with no open issue ([Memory Budget § Texture Registry](../engine/memory-budget.md#texture-registry)) |
| 7 | Texture budget | none. The allocator warns once at 80% of the smallest DEVICE_LOCAL heap (`allocator::approaching_oom_line`); `GpuMemoryBudget` caches the live `VK_EXT_memory_budget` reading (`crates/renderer/src/vulkan/allocator.rs`). Static BLAS has `MAX_BLAS_BUDGET_BYTES` (`acceleration/constants.rs`) |
| 8 | Eviction | none for textures; BLAS has `AccelerationManager::evict_unused_blas` (`acceleration/blas_static.rs`) |
| 9 | Sampling clamp | the material samplers are shared per clamp mode with `min_lod(0.0)` / `max_lod(16.0)` (`material_sampler_info`, `texture_registry/mod.rs`), so no per-texture clamp exists; RT hit shaders sample with explicit `textureLod` (`shaders/include/ray_hit.glsl`), several at level 0 |
| 10 | BA2 DX10 chunks | `Dx10Chunk` keeps `start_mip` (read only by the order validation) and `end_mip` (`#[allow(dead_code)]`); `Ba2Archive::extract` assembles every chunk behind a reconstructed header (`crates/bsa/src/ba2.rs`) |
| 11 | BSA and loose | `Archive::extract` (`byroredux/src/asset_provider/archive.rs`) returns the whole file |
| 12 | CPU prefetch | `PrefetchStore` stages extracted DDS bytes on the stream pool under `STAGED_BYTE_CAP` (`byroredux/src/asset_provider/texture_prefetch.rs`) |
| 13 | Per-texture derived data | `average_rgb` reads the smallest mip (`dds.rs`); `has_alpha` comes from the format (`format_has_alpha`) |
| 14 | Telemetry | `tex.loaded` / `tex.missing` count entities and paths (`byroredux/src/commands/assets.rs`); the stats line's `textures=` is `TextureRegistry::len()`, dead slots included (`byroredux/src/app_events.rs`); `ownership_sample.rs` samples `texture_live_slots`; no resident-bytes figure exists |
| 15 | Image replacement under a held handle | `TextureRegistry::update_rgba` (`texture_registry/mod.rs:862`) replaces the image of a live handle when extent or format changes and retires the old one through `pending_destroy`; it refuses a released handle (#4879). No DDS path replaces an image under a held handle |

---

## 4. Goals and non-goals

### 4.1 Goals

- **TXS-G-001 — Floor first.** A cell's streamable textures are sampleable at
  their floor no later than the same cell's full chains are today, measured
  by the floor-ready metric (§8).
- **TXS-G-002 — Bounded texture memory.** Resident texture bytes stay at or
  under the texture pool budget, floors excepted (TXS-BUD-002), in every
  in-scope game's acceptance scenes.
- **TXS-G-003 — No loss at settle.** With the budget not exceeded, a pinned
  camera bench scene settles with zero mip deficits.
- **TXS-G-004 — No new slots.** Streaming adds no bindless slot: a scene's
  `TextureRegistry::len()` is the same with streaming on and off.
- **TXS-G-005 — Clean uploads.** The BC2 staging-copy overrun Known Issue is
  shown closed by a validation-clean long run after #4512's fix, and the
  region-versus-staging check is a release-mode error.

### 4.2 Non-goals

- **TXS-NG-001.** Bindless slot reuse. The grow-only slot space and the
  generational free-list that would fix it are tracked as tech debt in
  [Memory Budget § Texture Registry](../engine/memory-budget.md#texture-registry)
  (#2030 closed with telemetry only, §3.3 row 6). Streaming must not make it
  worse (TXS-RES-002); it does not fix it.
- **TXS-NG-002.** Rebaking BSA/BA2 into a mip-ordered native bundle. That is
  M82, which ROADMAP lists as depending on M39
  ([Tier 10](../../ROADMAP.md#tier-10--ecosystem-unlock-post-tier-4-horizon));
  M39 reads the archives as shipped.
- **TXS-NG-003.** Geometry and BLAS streaming. Mesh residency is M53's
  cluster format ([Tier 11](../../ROADMAP.md#tier-11--performance-ceiling-post-tier-4-horizon));
  static BLAS already has its own budget and LRU.
- **TXS-NG-004.** Page-granular virtual texturing (sub-mip tiles, feedback
  pages). The granularity is one whole mip level; a sparse-residency
  *mechanism* for whole levels is decision Q1, not a tile scheme.
- **TXS-NG-005.** Texture recompression, mip regeneration or upscaling.
  Those belong to `tools/texture-upscale` (ROADMAP Known Issue "Offline
  texture-set upscale finalization").
- **TXS-NG-006.** Reading the original games' INI texture-quality settings.
  Redux does not honour Oblivion's `iTexMipMapSkip` (§1) or any other
  title's INI texture-quality key; the budget setting replaces them
  (TXS-BUD-005, §6 row 1).

---

## 5. Requirements by area

### 5.1 Upload path (`TXS-UP`)

- **TXS-UP-001.** A recorded long-run command (game, cell or grid, run
  length) MUST produce zero `vkCmdCopyBufferToImage` "exceeds VkBuffer total
  size" validation messages under `BYRO_VALIDATION=1`, both before streaming
  lands and with it on. Today: the Known Issue
  ([ROADMAP § Correctness and content](../../ROADMAP.md#correctness-and-content),
  dated 2026-09-18) predates #4512's fix (2026-09-20, §3.3 row 3), which
  traced the same +8 B overrun class to pooled staging release, and has not
  been re-run since. **Phase 0: verify** that a multi-minute `--bench-hold`
  streaming run, the shape the Known Issue describes, is clean at HEAD; if
  it is, the Known Issue is struck citing #4512, and if it is not, the
  command and its first failing texture are recorded there.
- **TXS-UP-002.** `record_dds_upload` MUST return an error, in release
  builds, when the copy regions address more bytes than the staging buffer
  holds. Today: `debug_assert!` only (§3.3 row 3).
- **TXS-UP-003.** The upload path MUST accept a mip range
  `[first, mip_count)` of a parsed DDS and record copy regions for exactly
  those levels, with the staging size taken from the same walk that builds
  the regions. Today: every upload covers every level, and
  `build_dds_copy_regions` already returns its walk's byte total (§3.3
  row 2).
- **TXS-UP-004.** A promotion's upload MUST go through the same staging pool
  and the same `MAX_UPLOAD_BATCH_BYTES` sub-batch bound as a cell-load
  flush; it MUST NOT add a second staging retention budget.

### 5.2 Source reads (`TXS-SRC`)

- **TXS-SRC-001.** The BA2 DX10 reader MUST expose an extraction of the
  chunks covering a requested mip range, returning their bytes and the
  covered `start_mip`/`end_mip`, without decompressing any other chunk of
  the record. Today: `end_mip` is dead code and `extract` assembles every
  chunk (§3.3 row 10). Archive2, the FO4 Creation Kit archiver, splits a DDS
  into at most `-maxChunkCount` chunks (default 4) and gives a mip its own
  chunk when it is at least `-singleMipChunkX` × `-singleMipChunkY`
  (default 512 × 512) (Fallout 4 CK wiki, *Archive2*, local dump
  `falloutck-uesp-wiki/(main)/Archive2.wiki`). **Phase 0: verify** with a
  census of the vanilla FO4 and Starfield texture BA2s which mip ranges the
  last chunk covers per record, and how many records ship one chunk only.
- **TXS-SRC-002.** For a BSA texture the provider MUST serve the floor's
  bytes from one extraction of the file. A BSA file is stored either raw or
  as one zlib (v103/v104) or LZ4 (v105) block, with a per-file inversion of
  the archive default (UESP, *Oblivion Mod:BSA File Format* and *Skyrim
  Mod:Archive File Format*, local dumps under `uesp-wiki/`). A raw file or a
  loose file MAY be read from the floor's byte offset alone. Decision Q2
  fixes what a later promotion of a BSA texture reads.
- **TXS-SRC-003.** Per-game and per-archive differences MUST stay in
  `crates/bsa` and `Archive` (`byroredux/src/asset_provider/archive.rs`).
  The renderer's upload API receives DDS metadata, a mip range and bytes,
  and `crates/renderer/Cargo.toml` MUST NOT gain a `byroredux-bsa`
  dependency. Today: it has none.
- **TXS-SRC-004.** A DX10 record whose chunk layout does not allow a floor
  read (a single chunk, or a last chunk finer than the floor rule) MUST
  fall back to a whole-file read and be counted under its own
  `tex.residency` line, not refused.
- **TXS-SRC-005.** `PrefetchStore` SHOULD stage floor reads for streamable
  textures instead of whole files once TXS-SRC-001 lands, under the same
  `STAGED_BYTE_CAP` ([Archives § Texture prefetch](../engine/archives.md#texture-prefetch)).

### 5.3 Residency (`TXS-RES`)

- **TXS-RES-001.** On first use a streamable texture MUST reach the
  **floor** state in the batched flush that installs textures today
  (`flush_pending_uploads`); no cell-load path MAY wait for its high mips.
- **TXS-RES-002.** Promotion and demotion MUST keep the texture's
  `TextureHandle`: `TextureRegistry::len()` MUST NOT change across either.
  A Vulkan-free registry test (new, `promotion_keeps_handle_and_slot_count`)
  checks it.
- **TXS-RES-003.** The image view written into a texture's bindless slot
  MUST address no level outside that texture's resident range, so no
  sampling path, raster or RT hit, can read a non-resident level. A
  Vulkan-free registry test (new, `slot_view_covers_only_resident_range`)
  checks the view's level range against the resident range after a
  promotion and after a demotion. Today every image exposes all of its
  levels because every level is resident. The mechanism is decision Q1.
- **TXS-RES-004.** A replaced image MUST retire through the entry's
  `pending_destroy` ring with the existing `MAX_FRAMES_IN_FLIGHT` aging
  ([Memory Budget § Deferred-Destroy Queue](../engine/memory-budget.md#deferred-destroy-queue));
  no promotion or demotion path MAY call `device_wait_idle`.
  `texture_pending_destroy_count` on the `rt-integrity` line MUST return to
  zero after settle.
- **TXS-RES-005.** The classes in §3.2 marked **whole** MUST be uploaded
  whole, and `tex.residency` MUST print the count and bytes of each class.
- **TXS-RES-006.** `avg_rgb` and `has_alpha` MUST be available from the
  floor upload and MUST NOT change on promotion. Today `average_rgb` already
  reads the smallest mip (§3.3 row 13).
- **TXS-RES-007.** When the last holder drops, every resident level MUST be
  released in that drop, whatever the residency state, and a promotion still
  queued or in flight for that handle MUST be cancelled, as #4879 does for a
  queued first upload today (`release_ref`).
- **TXS-RES-008.** Disabling streaming with one switch (a setting, decision
  Q5) MUST restore today's full-chain upload, so every A/B in §7.2 has a
  same-binary control.

### 5.4 Wanted mip (`TXS-WANT`)

- **TXS-WANT-001.** The streamer MUST compute a wanted mip for each
  streamable texture with a holder, at a cadence the design states, from the
  instances that reference it. The signal is decision Q2.
  `tex.residency <path>` MUST print the wanted mip and the resident range.
- **TXS-WANT-002.** A texture referenced by an instance in the TLAS but not
  in the raster draw list (seen only by reflections, GI or shadows) MUST be
  wanted at no coarser than the floor, and SHOULD be wanted at a level the
  design derives for secondary rays; the design names the rule.
- **TXS-WANT-003.** A texture whose instances are neither drawn nor in the
  TLAS MUST be wanted at the floor.
- **TXS-WANT-004.** The promotion queue MUST place every texture referenced
  by an instance in the raster draw list ahead of every texture referenced
  only by TLAS instances or by neither. A queue unit test (new,
  `promotion_queue_orders_drawn_first`) checks it. This follows the first
  load-priority criterion Unreal documents, "Load visible mips first" (Epic
  Games, *Texture Streaming Overview*).
- **TXS-WANT-005.** A texture MUST NOT be demoted within a hysteresis window
  of its last promotion; the window is design-owned and printed by
  `tex.residency`. The churn counter (TXS-TEL-002) MUST read zero on a
  pinned-camera bench.

### 5.5 Budget and eviction (`TXS-BUD`)

- **TXS-BUD-001.** The texture pool budget MUST be one number of bytes,
  logged once at renderer start with its derivation (decision Q4) and
  printed by `tex.residency`.
- **TXS-BUD-002.** Resident texture bytes MUST stay at or under the budget,
  except that floors and **whole** textures are never evicted; when they
  alone exceed the budget a single warning line MUST name both totals.
- **TXS-BUD-003.** Over budget, the streamer MUST demote the textures with
  the oldest last-wanted frame first, one level per demotion, never below
  the floor. Unreal's overview describes its budget pass as "dropping one
  mip at a time, until the planned solution gets under budget", with the
  rule "Drop mips that are not visible, dropping the last recently seen
  first" (Epic Games, *Texture Streaming Overview*).
- **TXS-BUD-004.** A promotion that would exceed the budget MUST first
  evict per TXS-BUD-003 and MUST be declined when eviction cannot make room,
  leaving the texture in its state and counting the decline.
- **TXS-BUD-005.** The budget MUST be overridable through one registered
  setting in `SettingsRegistry` (`crates/core/src/settings/`), so the
  minimum card can be emulated on the dev card and presets in
  `assets/graphics_presets.toml` can move it
  ([Launcher § 4.3](../engine/launcher.md#43-presets-and-hardware-pre-flight)).
  Today `core::settings::builtin` registers five `render.*` settings
  (`render.upscaler` and four terrain knobs, `builtin.rs:31`–`57`); none is a
  texture setting, and `render.upscaler` is the only one the shipped
  presets move.
- **TXS-BUD-006.** Promotion upload bytes per frame MUST be capped by a
  design-owned constant, and the cap MUST appear in the
  [Memory Budget](../engine/memory-budget.md) ledger with a ledger test of
  the existing `memory_budget_ledger_tests` shape.
- **TXS-BUD-007.** A declined allocation for a promotion MUST leave the
  texture at its prior state and MUST NOT fall back to the missing-texture
  checker. Today a failed first enqueue returns the fallback handle
  (`resolve_texture_view_with_clamp`, `byroredux/src/asset_provider/texture.rs`)
  and a failed flush leaves the slot on the fallback (#1922); those
  first-upload paths are unchanged.

### 5.6 Telemetry (`TXS-TEL`)

- **TXS-TEL-001.** A new console command `tex.residency` MUST print the
  count and bytes per residency state (§3.1) and per class (§3.2), the
  budget, the floor bytes, the deficit count and the decline count; with a
  path argument it MUST print that texture's state, resident range, wanted
  mip and last-wanted frame.
- **TXS-TEL-002.** A per-session bench line (new, `tex-stream:`) MUST carry
  resident bytes (current and peak), promotions, demotions, evictions,
  declines, churn and the floor-ready time of the last cell load.
- **TXS-TEL-003.** `ownership_sample.rs` MUST sample resident texture bytes
  beside `texture_live_slots`, so `m-exteriors.sh soak` gates their return
  to baseline with the other owners.
- **TXS-TEL-004.** The [Memory Budget](../engine/memory-budget.md) Texture
  Registry section MUST describe the pool, its budget and its eviction, and
  its "no explicit texture-count eviction policy" paragraph MUST be
  rewritten in the same change that lands TXS-BUD-003.

---

## 6. Deliberate deviations and risks

| # | Original behaviour | Redux behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | One renderer-wide skip count of mip levels applies to every texture as it loads (Gamebryo 3.2 reference, `NiRenderer::SetMipmapSkipLevel`; Oblivion `iTexMipMapSkip` presets, UESP *Oblivion.ini*) | Per-texture residency under one byte budget (TXS-BUD-001) | The `iTexMipMapSkip` "texture quality" tweak as a VRAM lever | A user who lowers texture quality in the original INI sees no effect → the budget setting is the lever (TXS-BUD-005, TXS-NG-006) |
| 2 | A DDS's declared levels are all sampleable once the file is loaded (DDS layout, Microsoft) | High mips arrive after first use | — | Visible pop-in on a fresh cell or a fast turn → floor first (TXS-RES-001), on-screen first (TXS-WANT-004), hysteresis (TXS-WANT-005), deficit metric (§8) |
| 3 | An Oblivion menu texture exported with a mip count of 1 instead of 0 shows corrupted pixels below maximum texture quality (CS wiki, *Oblivion XML Reference*) | Single-mip files are **whole** and always full resolution | The header hex-edit workaround the CS wiki describes | Large single-mip files bypass streaming → counted against the budget and reported by class (TXS-RES-005, TXS-BUD-002) |
| 4 | FO4 BA2 textures are split into mip chunks by Archive2 (CK wiki) | The last chunk is read first as the floor (TXS-SRC-001) | — | A mod archive built with `-maxChunkCount 1` or unusual chunk settings has no separate tail → whole-file fallback, counted (TXS-SRC-004) |
| 5 | Engine-internal: RT hits sample explicit LODs, some at level 0 (`ray_hit.glsl`) | Off-screen surfaces seen by rays may hold coarser mips than raster would | — | Blurrier reflections or alpha-tested shadow cutouts of off-screen objects → TXS-WANT-002 and the Phase 2 reflection A/B |
| 6 | Engine-internal: a held DDS texture's image is never replaced; only `update_rgba` replaces a held image, for engine-generated RGBA (§3.3 row 15) | DDS images are replaced on promotion and demotion, on the same `pending_destroy` path `update_rgba` uses | — | Use-after-free of a retired image → deferred destroy ring and validation-clean soak (TXS-RES-004, A9) |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

The renderer side is the same code for every game. Per-game differences are
archive formats, handled in the archive layer (TXS-SRC-003). Waves follow
what the archive allows: BA2 games can read a floor without inflating the
high mips; BSA games cannot when the file is compressed (TXS-SRC-002).

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 2. BSA v103, zlib ([Archives § Compression by version](../engine/archives.md#compression-by-version)); cells load (ROADMAP compatibility matrix) |
| Fallout 3 | yes | Wave 2. BSA v104, zlib |
| Fallout: New Vegas | yes | Wave 2. BSA v104, zlib; the reference title for the soak and HUD gates |
| Skyrim (LE / SE / AE) | SE / AE yes; LE no | Wave 2 for SE (BSA v105, LZ4 frame). LE is parse-only in the compatibility matrix, so there is no rendered scene to accept against |
| Fallout 4 | yes | Wave 1. BA2 DX10 chunks (Archive2); MedTek, Dugout Inn and the Commonwealth grid-cross are existing bench and smoke scenes |
| Fallout 76 | no | Parse-only, no cell loader ([Game Compatibility](../engine/game-compatibility.md#per-game-support-detail)). Its BA2 DX10 reads share FO4's code, so TXS-SRC-001 serves it with no extra work once cells render |
| Starfield | yes | Wave 1. BA2 v2/v3 DX10 chunks; the reader at HEAD decodes each chunk as raw or LZ4 independently (`ba2.rs` module doc), so a mip-range read follows the same per-chunk rule; the walkable Cydonia interior is the acceptance scene |

### 7.2 Acceptance

Game data stays on the machine; nothing below implies redistribution. Every
live scenario follows the [bench methodology](../../ROADMAP.md#how-to-run-a-bench)
(pinned `--camera-pos` / `--camera-forward`, same-machine control) and uses
the TXS-RES-008 switch as its control. Release builds need
`BYRO_DEBUG_SERVER=1` for `byro-dbg` to attach.

| # | Scenario | Driven by | Data | Passes when |
|---|---|---|---|---|
| A1 | Mip-range regions and pricing | new `cargo test -p byroredux-renderer` tests `copy_regions_for_mip_range_match_staging` and `staging_overrun_is_a_release_error` | none (hand-built `DdsMetadata`) | regions cover exactly the range; overrun returns `Err` |
| A2 | BA2 chunk-range extract | new `cargo test -p byroredux-bsa` test `dx10_extract_mip_range_reads_only_tail_chunks` | synthetic fixture | only the tail chunks are decompressed; bytes equal the tail of a full extract |
| A3 | Handle stability | new Vulkan-free registry test `promotion_keeps_handle_and_slot_count` | none | TXS-RES-002 holds |
| A4 | Chunk-layout census | new example `crates/bsa/examples/ba2_mip_chunk_census.rs` | vanilla FO4 and Starfield texture BA2s | every DX10 record classified (floor-readable / whole fallback) |
| A5 | Staging overrun stays closed | the long-run command Phase 0 records (TXS-UP-001), `BYRO_VALIDATION=1`, streaming off and on | the game and cell Phase 0 records | zero "exceeds VkBuffer total size" messages in both runs |
| A6 | Floor first | `--bench-hold` on FO4 MedTekResearch01 and Starfield Cydonia, then `byro-dbg` `tex.residency` | FO4, Starfield | floor-ready time ≤ control's full-chain flush time; zero **reserved** after the flush |
| A7 | Settle | same scenes, pinned camera, `tex.residency` after settle | FO4, Starfield; Wave 2 adds FNV GSProspectorSaloonInterior, Skyrim WhiterunBanneredMare | zero deficits; zero churn; `textures=` equals control |
| A8 | Budget clamp | same scenes with the TXS-BUD-005 setting lowered below the control's resident bytes (`BYROREDUX_SETTINGS_PATH` isolated) | FO4, Starfield | resident ≤ budget + floors + whole; `tex.missing` unique count equals control |
| A9 | Streaming soak | `docs/smoke-tests/m-exteriors.sh fo4 soak` (existing script; Phase 3 adds the TXS-TEL-003 owner) | FO4 Commonwealth | resident texture bytes return to baseline each cycle; zero validation errors under `BYRO_VALIDATION=1` |
| A10 | Long session | `docs/smoke-tests/p5-soak.sh fnv` (existing) | FNV | passes as today, with the `tex-stream:` peak at or under the budget |
| A11 | Reflection A/B | pinned mirror-like pose chosen in Phase 2, `screenshot` with streaming on and off | FO4 | pixel delta within the control's run-to-run noise |

---

## 8. Metrics

Definitions and granularity only. The numbers go in a new texture-streaming
row set in ROADMAP's bench-of-record section, measured per
[ROADMAP § How to run a bench](../../ROADMAP.md#how-to-run-a-bench), with
their repro command in ROADMAP's repro table, and the constants in
[Memory Budget](../engine/memory-budget.md).

| Metric | Definition | Granularity |
|---|---|---|
| Floor-ready time | cell-load start → last texture of that load leaves **reserved** | per scene, streaming on vs the TXS-RES-008 control |
| Settle time | camera stop → no promotion queued or in flight | per scene, pinned pose |
| Deficit rate | textures with a deficit ÷ streamable textures with a holder, at settle | per scene |
| Resident texture bytes | sum of resident levels' allocation sizes, current and session peak | per scene, per soak cycle |
| Budget adherence | peak (resident − floors − whole) ÷ budget | per scene, per budget setting |
| Churn | demotions within the hysteresis window of a promotion | per bench run |
| Promotion bandwidth | promotion upload bytes per frame, p99 | per bench run |
| Fallback whole-file reads | DX10 records read whole ÷ DX10 records resolved | per game |

---

## 9. Delivery sequence

M39 depends on no open milestone; M40 cell streaming (shipped) provides
the residency events it builds on. M82 depends on M39 and is not absorbed.

**Phase 0 — Survey, census and design (no engine code).** Write
`docs/engine/texture-streaming.md` (state machine, wanted-mip rule, budget
arithmetic, eviction order) and register it. Run the TXS-UP-001 long run
at HEAD and record its command. Run the A4 census over the vanilla FO4 and Starfield texture
BA2s (an `examples/` binary, not linked into the engine) and the existing
`crates/bsa/examples/dds_mip_scan.rs` over each BSA game's texture
archives, to size the floor rule and count single-mip files. Measure the
inflate cost of a whole-file read against a floor-only read on one BSA and
one BA2 game. Resolve Q1–Q5. Close every **Phase 0: verify** item.
Exit: the design and both census tables are checked in, the BC2 Known
Issue is either struck citing #4512 with the clean command or carries the
failing command, and every §10 item carries **Decided**.

**Phase 1 — Upload correctness and residency plumbing.** TXS-UP-001…004,
TXS-RES-002…008, TXS-TEL-001. Streaming stays off by default; promotion is
exercised by forcing a floor-then-full sequence on every texture.
Exit: A1, A3 and A5 pass; A9 runs validation-clean with the forced sequence.

**Phase 2 — Wave 1: BA2 floor first and on-demand promotion.** TXS-SRC-001,
TXS-SRC-003, TXS-SRC-004, TXS-SRC-005, TXS-RES-001, TXS-WANT-001…005,
TXS-TEL-002. Exit: A2, A6, A7 and A11 pass on FO4 and Starfield, and the
floor-ready and settle rows are published in ROADMAP.

**Phase 3 — Budget and eviction.** TXS-BUD-001…007, TXS-TEL-003, TXS-TEL-004.
Exit: A8 passes at two budget settings on FO4 and Starfield, A9 passes with
the new owner, and the budget-adherence row is published.

**Phase 4 — Wave 2: BSA games.** TXS-SRC-002 for Oblivion, FO3, FNV and
Skyrim SE, per the Q2 decision. Exit: A6–A8 and A10 pass on FNV and Skyrim
SE, A6 on Oblivion and FO3, and streaming is on by default for every
in-scope game.

---

## 10. Decisions required

1. **Q1 — Residency mechanism.** Recommended: **reallocate on change** — on
   promotion create an image with the new level count, copy the resident
   levels from the old image on the GPU, upload the new levels, rewrite the
   slot's descriptor, retire the old image through `pending_destroy` (the
   path `update_rgba` already uses for held RGBA handles, §3.3 row 15). It
   needs no optional Vulkan feature and keeps every image fully bound, at
   the cost of a transient second image per promotion (bounded by
   TXS-BUD-006). Alternative: **sparse residency** — images created with
   sparse residency, levels bound and unbound in place. No copy and no
   descriptor rewrite, but it needs `sparseResidencyImage2D`; the levels
   below the sparse block size form one mip tail that must be bound as an
   opaque region; and without `residencyNonResidentStrict` reads of unbound
   regions return undefined values (Khronos, *Vulkan Specification*,
   chapter "Sparse Resources", <https://docs.vulkan.org/spec/latest/chapters/sparsemem.html>);
   the queue-side bind and driver support across the minimum-card class
   become **Phase 0: verify** work. Alternative: **full allocation with a
   view clamp** (`VK_EXT_image_view_min_lod`, `VkImageViewMinLodCreateInfoEXT`,
   <https://docs.vulkan.org/refpages/latest/refpages/source/VK_EXT_image_view_min_lod.html>)
   — trivial and copy-free, but it saves no VRAM, so it fails TXS-G-002.
2. **Q2 — Wanted-mip signal and BSA promotion source.** Recommended: a CPU
   estimate from each referencing instance's world bounds, distance and a
   per-mesh UV texel density captured at NIF import, in the manner Unreal
   documents ("texel world sizes" for "a good texel per pixel ratio", Epic
   Games, *Texture Streaming Overview*), and for BSA games a promotion
   re-extracts the file through the stream pool. It needs no shader change
   and no readback, at the cost of being an estimate and of inflating a
   compressed BSA texture twice. Alternative: a GPU feedback buffer the
   fragment and hit shaders write their sampled level into, read back a
   frame later. Exact for raster and rays, at the cost of shader work in
   `triangle.frag` and `ray_hit.glsl` and a readback each frame.
   Alternative for BSA only: keep the high-mip bytes in host RAM after the
   first extraction, trading RAM for a second inflate.
3. **Q3 — Floor rule.** Recommended: the floor is the last chunk of a BA2
   DX10 record, and for every other source the levels at or below the edge
   the A4 census finds that chunk tops out at, so all games share one
   effective floor. Alternative: one fixed edge for every source,
   independent of the census. Simpler to state, but on BA2 a floor that
   does not match a chunk boundary forces a read of the next chunk too.
4. **Q4 — Budget derivation.** Recommended: a fraction of the live
   `VK_EXT_memory_budget` heap budget (already cached in `GpuMemoryBudget`)
   minus the non-texture rows the ledger already bounds, recomputed at the
   allocator's existing sample points, so the minimum card and the dev card
   get different budgets without a setting. Alternative: a fixed constant
   per preset, like `MAX_BLAS_BUDGET_BYTES` for BLAS, with the launcher's
   pre-flight heap reading picking the preset
   ([Launcher § 4.3](../engine/launcher.md#43-presets-and-hardware-pre-flight)).
   Predictable and testable, but a larger card is held to the minimum
   card's figure unless the preset moves it.
5. **Q5 — User-facing settings.** Recommended: two registered settings —
   the budget override (TXS-BUD-005) and the streaming on/off switch
   (TXS-RES-008) — with the presets in `assets/graphics_presets.toml`
   moving only the budget. Alternative: an `iTexMipMapSkip`-style global
   skip as a third setting for low-end cards. Familiar to Oblivion players,
   but it reintroduces the uniform reduction this PRD replaces (§6 row 1).

---

## 11. Document map

| Need | Go to |
|---|---|
| VRAM targets, texture registry slots, staging pool, BLAS LRU precedent | [memory-budget.md](../engine/memory-budget.md) |
| Prefetch store and its cap | [memory-budget.md § Texture Prefetch Store](../engine/memory-budget.md#texture-prefetch-store-cpu-side), [archives.md § Texture prefetch](../engine/archives.md#texture-prefetch) |
| DX10 chunk record layout, DDS reconstruction | [archives.md](../engine/archives.md#dds-header-reconstruction) |
| Upload flow and bindless registry | [renderer.md § Texture upload](../engine/renderer.md#texture-upload) |
| Resolve and batched flush at cell load | [asset-pipeline.md § 12](../engine/asset-pipeline.md#12-texture-lookup-and-upload) |
| Per-game engine tiers | [game-compatibility.md](../engine/game-compatibility.md) |
| Settings registry, presets and GPU pre-flight | [launcher.md § 4](../engine/launcher.md#4-settings-one-model-two-skins), [§ 4.3](../engine/launcher.md#43-presets-and-hardware-pre-flight) |
| Console commands reached through `byro-dbg` | [debug-cli.md](../engine/debug-cli.md#commandregistry-dispatch-dotted-names) |
| Smoke scripts and how to run them | [docs/smoke-tests/README.md](../smoke-tests/README.md) |
| Architecture invariants | [CLAUDE.md](../../CLAUDE.md#architecture-invariants) |
| Live milestone state, Known Issues and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: BC2 overrun premise (#4512), settings-registry and image-replacement HEAD claims, #2030 status, wanted-mip attribution and verbatim Unreal quotes, testable TXS-RES-003 / TXS-WANT-004, launcher and debug-CLI owners, prose numbers to links | claude: revise |
