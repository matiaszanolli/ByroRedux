# Memory Budget

Where VRAM and RAM go, what the ceilings are, and how each subsystem
handles overflow. The dev GPU is an RTX 4070 Ti (12 GB); the RT-minimum
target is 6 GB. Constants are verified against source; byte math is shown.

---

## Starfield Component Database (CPU-side)

The Starfield `materialsbeta.cdb` is 105,037,616 B on disk. The production
presence path uses `probe_header` and retains only a bounded, 128-entry cache
of header results; it does not retain inflated CDB blobs. A full generic
`ComponentDatabaseFile::parse` materialises 1,438,780 dynamic instances and
measured **9,188,720 KB peak RSS** on the vanilla database (2026-08-18). Tools
that need the full tree must call `parse_with_limits` with an explicit
instance budget; the default `parse` remains the compatibility/unbounded path
for offline tooling while Phase 2 develops an indexed material lookup.

---

## ESM Index (CPU-side)

`EsmIndex` (`crates/plugin/src/esm/records/index.rs`) is 93 session-lifetime
`HashMap`s, one per record type, populated by `parse_esm_with_load_order` and
accumulated across a load order via `merge_from` (vanilla FO4 is base + 7 DLC
masters). Nothing evicts these maps; they are held for the whole session.
This is the largest single CPU-side allocation in a normal run and, unlike
every other subsystem on this page, was previously undocumented here.

Measured with `crates/plugin/examples/esm_dim8_bench` under
`/usr/bin/time -f %M`, release build (2026-08-30):

| Master | File size | Parse time | Peak RSS | Index ≈ RSS − file |
|---|---|---|---|---|
| `Oblivion.esm`   | 265 MB  | 1.41 s | 1 441 MB | ~1.18 GB |
| `Fallout3.esm`   | 275 MB  | 1.23 s | 1 059 MB | ~0.78 GB |
| `FalloutNV.esm`  | 234 MB  | 1.17 s |   861 MB | ~0.63 GB |
| `Skyrim.esm`     | 238 MB  | 1.27 s |   980 MB | ~0.74 GB |
| `Fallout4.esm`   | 315 MB  | 1.69 s | 1 440 MB | ~1.13 GB |
| `SeventySix.esm` | 880 MB  | 3.41 s | 3 509 MB | ~2.63 GB |
| `Starfield.esm`  | 1.39 GB | not run — no safe headroom on this host; extrapolating the FO76 ratio puts it near 4 GB | | |

Survivable on a 12 GB+ dev box; not necessarily on a 16 GB machine with a
modded FO76/Starfield load order, especially once other subsystems' RAM
residency (streaming caches, asset-provider archive index) is added on top.

Most of the 93 maps are lean, but a meaningful fraction — `camera_shots`,
`menu_icons`, `voice_types`, and the ~30 `MinimalEsmRecord` stub maps — are
`EDID`-only stubs with no consumer, each retaining a `String` per record.
Trimming or lazily-populating those is a separate, unscoped follow-up.

---

## Scene Buffers (per-frame SSBOs / UBOs)

Resident for the lifetime of `VulkanContext`. Double-buffered
(`MAX_FRAMES_IN_FLIGHT` = 2) — two live copies, two in-flight frames.
Constants in [`scene_buffer/constants.rs`](../../crates/renderer/src/vulkan/scene_buffer/constants.rs).

| Buffer | Constant | Entries | Entry size | Per-frame | × 2 FIF |
|---|---|---|---|---|---|
| Light SSBO | `MAX_LIGHTS` = 1023 (`RESERVOIR_LIGHT_MASK`, #8e7582ed — not 512, that's `MAX_LIGHTS_PER_CLUSTER`) | 1023 | 64 B | 64 KB | **128 KB** |
| Instance SSBO | `MAX_INSTANCES` = 262 144 | 262 144 | 160 B (#3231) | 41.9 MB | **83.9 MB** |
| Previous-model SSBO (`33d9a468`) | `MAX_INSTANCES` = 262 144 | 262 144 | 64 B (`mat4`) | 16.8 MB | **33.6 MB** |
| Indirect draw SSBO | `MAX_INDIRECT_DRAWS` = 262 144 | 262 144 | 20 B | 5.2 MB | **10.5 MB** |
| Material SSBO | `MAX_MATERIALS` = 16 384 | 16 384 | 432 B | 6.75 MB | **13.5 MB** |
| Terrain tile SSBO | `MAX_TERRAIN_TILES` = 1 024 | 1 024 | 96 B (`GpuTerrainTile`, 3× `[u32; 8]`) | — | **~96 KB** (single shared buffer, NOT FIF-doubled) |
| Bone buffers ¹ | `MAX_TOTAL_BONES` = 196 608 | 196 608 | 64 B | 12.6 MB/buffer | **100.6 MB** |
| Camera UBO | — | 1 | 368 B (#3323) | 368 B | **736 B** |

¹ Eight 12.6 MB bone-sized allocations, not one: palette (`bone_device`),
`bone_world` staging, and `bone_world` device-copy are each FIF-doubled
(3 families × 2 FIF = 6 × 12.6 MB ≈ 75.5 MB), plus two single (non-FIF)
buffers — `bind_inverses_persistent` and the `bind_inverse_upload_staging`
scratch (`1 366 × MAX_BONES_PER_MESH(144) × 64 B ≈ 12.6 MB`, M29.6). Total
≈ 75.5 + 12.6 + 12.6 ≈ **100.6 MB**. See
[`scene_buffer/buffers.rs`](../../crates/renderer/src/vulkan/scene_buffer/buffers.rs)
`allocate_scene_render_buffers`.

**Total resident scene buffers:** ≈ **225 MB** across all copies.

Exceeding `MAX_INSTANCES` logs a one-shot `warn!` and clamps to
`MAX_INSTANCES` (#956/#992) — it is no longer a `debug_assert`. Exceeding
`MAX_MATERIALS` is **not** silent: `MaterialTable::intern_by_hash` bumps
`overflow_count`, fires a one-shot `warn!` (a `Once` latch, so no per-overflow
log spam), and over-cap entries share the neutral-default material slot 0 for
the rest of the session. The per-frame overflow count is exposed through the
`ctx.scratch` console command (#797 / SAFE-22 + #807). It is also no longer a
`debug_assert` — `app_frame.rs` carried one until #2795, which panicked a
debug build on this exact supported degrade (reachable per the code's own
recorded Skyrim radius-3 measurement, 4000+ unique materials).

---

## ReSTIR Reservoirs

[`restir.rs`](../../crates/renderer/src/vulkan/restir.rs) — screen-sized,
double-buffered (`MAX_FRAMES_IN_FLIGHT` = 2) STORAGE buffers for ReSTIR-DI
temporal reservoir reuse (Session 49 denoiser overhaul). Unlike every other
entry on this page, size scales with **swapchain resolution**, not a fixed
constant — recreated on every resize.

Formula: `width × height × RESERVOIR_STRIDE` bytes per FIF slot
(`RESERVOIR_STRIDE` = 32 B, one [`Reservoir`] per pixel).

| Resolution | Per-slot | × 2 FIF |
|---|---|---|
| 1920×1080 | 66.4 MB | **132.7 MB** |
| 2560×1440 | 118.0 MB | **235.9 MB** |
| 3840×2160 | 265.4 MB | **530.8 MB** |

This was the largest single VRAM addition of the denoiser overhaul (PERF-D5-NEW-04
/ #1814) — at 4K it is over 13% of the ~4 GB engine budget target below — but
had no ledger entry here and no attributing telemetry until #1814 added a
`log::info!` at both `ReservoirBuffers::new` and `recreate_on_resize` reporting
the computed size.

No leak: create-once + recreate-on-resize with a fenced destroy
(`recreate_swapchain` waits both frames-in-flight before dropping the old
buffers). Stale reservoir contents across a resize are harmless — the
final visibility ray re-validates every shaded sample.

---

## RT-Denoiser & Post-Process Screen-Sized Resources

Like the ReSTIR reservoirs above, every one of these scales with
resolution, not a fixed constant, and every one of them
had **no ledger entry here** until this sweep (#1872 — sibling finding
from #1814's ReSTIR audit: grep confirmed zero mentions of SVGF, Bloom,
SSAO, TAA, Volumetrics, Water, or Caustic anywhere on this page).

The resolution that matters is `frame_extents.render`, **not** the output /
swapchain extent: `context/mod.rs` and the resize path both pass
`render_extent.width/height` to every constructor on this page. Under the
shipped FSR 3.1 Quality default that is 1/1.5 of output per axis, so the
per-resolution rows below are upper bounds labelled by render resolution —
a 4K *output* frame allocates the 2560×1440 row, not the 3840×2160 one.
`presentation` and the upscaler's own output images are the exception; they
are output-sized.

### SVGF (indirect-lighting denoiser)

[`svgf.rs`](../../crates/renderer/src/vulkan/svgf.rs) — **four** screen-sized
images per frame-in-flight (`MAX_FRAMES_IN_FLIGHT` = 2), all allocated in the
same per-FIF loop in `SvgfPipeline::new_inner`:

| Image | Format | B/px |
|---|---|---|
| `indirect_history` | B10G11R11_UFLOAT_PACK32 | 4 |
| `moments_history` | RGBA16F | 8 |
| `atrous_color` ×2 (à-trous ping-pong, consumed by the `ATROUS_ITERATIONS` = 3 spatial pass) | B10G11R11_UFLOAT_PACK32 | 4 each |

20 B/px/slot × 2 FIF = **40 B/px** total. The à-trous pair was missing from
this ledger until #2679 (PERF-D3-03), which published 24 B/px. `svgf.rs`'s
`SVGF_BYTES_PER_PIXEL` derives the number from the live formats and
`bytes_per_pixel_matches_documented_memory_budget` pins the table below
against it; `SvgfPipeline::new_inner` / `recreate_on_resize` log it.

| Resolution | Total (4 images, 2 FIF) |
|---|---|
| 1920×1080 | ~82.9 MB |
| 2560×1440 | ~147.5 MB |
| 3840×2160 | ~331.8 MB |

### TAA

[`taa.rs`](../../crates/renderer/src/vulkan/taa.rs) — one RGBA16F
(8 B/px) history image per frame-in-flight, ping-ponged the same way
as SVGF (current frame writes one slot, reads the other as history).

| Resolution | Total (2 FIF) |
|---|---|
| 1920×1080 | ~33.2 MB |
| 2560×1440 | ~59.0 MB |
| 3840×2160 | ~132.7 MB |

### Glass + Water Caustics

[`caustic.rs`](../../crates/renderer/src/vulkan/caustic.rs) (glass-side)
and [`water_caustic.rs`](../../crates/renderer/src/vulkan/water_caustic.rs)
(water-side) each own a full-resolution R32_UINT atomic accumulator image,
double-buffered per FIF. They are **not** the same size: the glass side is a
three-layer array (`CAUSTIC_COLOR_LAYERS` = 3) so RGB radiance survives the
scalar image atomics, while the water side stays at `.array_layers(1)`.

| Accumulator | Layers | B/px (2 FIF) |
|---|---|---|
| Glass (`caustic.rs`) | 3 | 24 |
| Water (`water_caustic.rs`) | 1 | 8 |

**32 B/px** combined. This ledger said 16 B/px until #2679 (PERF-D3-03) —
the RGB conversion (`610cb170`, 2026-08-11) tripled the glass side and the
doc did not follow. `caustic.rs`'s `CAUSTIC_BYTES_PER_PIXEL` derives the
glass half from `CAUSTIC_COLOR_LAYERS` and
`caustic_bytes_per_pixel_matches_documented_memory_budget` pins both halves
against the table below; `CausticPipeline::new` / `recreate_on_resize` log it.

| Resolution | Glass | Total (both accumulators, 2 FIF) |
|---|---|---|
| 1920×1080 | ~49.8 MB | ~66.4 MB |
| 2560×1440 | ~88.5 MB | ~118.0 MB |
| 3840×2160 | ~199.1 MB | ~265.4 MB |

### SSAO

[`ssao.rs`](../../crates/renderer/src/vulkan/ssao.rs) — one R8_UNORM
(1 B/px) image per frame-in-flight (no ping-pong; computed after the
main render pass, read the following frame).

| Resolution | Total (2 FIF) |
|---|---|
| 1920×1080 | ~4.1 MB |
| 2560×1440 | ~7.4 MB |
| 3840×2160 | ~16.6 MB |

### Bloom

[`bloom.rs`](../../crates/renderer/src/vulkan/bloom.rs) — a mip pyramid
(5 down-levels + 4 up-levels, B10G11R11_UFLOAT_PACK32, 4 B/px) seeded
from a **half-resolution** base. The pyramid carries no history across
frames, but it **is** FIF-doubled like everything else on this page:
`BloomPipeline` owns one independent `BloomFrame` per frame-in-flight,
each with its own down and up images.

That is a requirement, not an oversight. `dispatch()` rewrites
`down_descriptor_sets[0]` binding 0 every frame and writes every mip with
no pre-barrier — sound only because each slot's images are exclusive to
that slot and gated by the frame fence (the #931 barrier-reduction
rationale). Collapsing `frames` to a single shared pyramid would
reintroduce the cross-frame WAR that argument depends on being absent.

Extents follow the **render** extent (`frame_extents.render`), so every
figure below shrinks under any FSR preset.

| Resolution | Per frame-in-flight | Total (2 FIF) |
|---|---|---|
| 1920×1080 | ~5.5 MB | ~11.0 MB |
| 2560×1440 | ~9.8 MB | ~19.6 MB |
| 3840×2160 | ~22.1 MB | ~44.1 MB |

### FSR 3.1 Upscaler (default, `5c7acfe2`)

[`frame_upscaler.rs`](../../crates/renderer/src/vulkan/frame_upscaler.rs),
`presentation.rs`, `exposure.rs`, `crates/fsr3-sys`. Unlike every other entry
in this section, FSR 3.1 Quality (the shipped default) renders at a **lower
internal resolution** and upscales to the swapchain's **output resolution** —
the two axes are no longer the same, so figures below are split accordingly.
Leak-free and FIF-correct (verified 2026-07-25 sweep); reactive/transparency
masks are G-buffer attachments (see [Shader Pipeline](shader-pipeline.md)'s
G-Buffer table), not counted again here.

| Resource | Resolution axis | Notes |
|---|---|---|
| Upscaler output image | Output | One per FIF, consumed by `presentation.frag` |
| FSR 3.1 SDK working memory | Internal (per-preset scratch, driver-managed) | Allocated by the vendored FidelityFX SDK context, not a `GpuBuffer`/`Attachment` this doc otherwise tracks |
| Native-blit fallback (`--upscaler taa`) | Output | No SDK context; a plain blit, no extra resident memory beyond the existing TAA history above |

Quality-preset internal-resolution scale factor and the four-preset (Quality/
Balanced/Performance/Ultra Performance) breakdown are tracked in ROADMAP.md's
Session 60 closeout, not duplicated here.

### Volumetrics (M55) — resolution-scaled since Session 62

[`volumetrics.rs`](../../crates/renderer/src/vulkan/volumetrics.rs) — like
every other entry in this section, the froxel grid scales with the
**render** resolution (`froxel_extent`, deliberately downstream of the FSR
preset query — using the final output resolution here would silently
overspend whenever FSR Quality/Balanced/Performance is active). One froxel
column per `froxel_xy_divisor` (default **8**, Frostbite's own density) render
pixels in X/Y, `froxel_z_slices` (default 64) depth slices.

**Six volumes per frame-in-flight slot**, not two — `44 B/froxel/slot`
(`FROXEL_VOLUMES_PER_SLOT` / `FROXEL_BYTES_PER_SLOT` in `volumetrics.rs`,
which the boot log and a regression test both read):

| Volume | Format | B/froxel | Carries |
|---|---|---:|---|
| `lighting_volumes` | RGBA16F | 8 | scattering + transmittance (V-buffer) |
| `integrated_volumes` | RGBA16F | 8 | ray-marched integral |
| `combustion_state_volumes` | RGBA16F | 8 | fuel, temperature K, σ_a, radiance calibration |
| `combustion_dynamics_volumes` | RGBA16F | 8 | velocity xyz + σ_s |
| `combustion_optical_volumes` | RGBA16F | 8 | transported optical properties |
| `emission_history_volumes` | R32F | 4 | deterministic-emission fraction (#2809) |
| **Total** | | **44** | × `MAX_FRAMES_IN_FLIGHT` (2) = **88 B/froxel** |

Formula: `ceil(width / 8) × ceil(height / 8) × 64 froxels × 44 B × 2 FIF`

| Render extent | Grid (W×H×64) | Froxels | Total (6 volumes, 2 FIF) |
|---|---|---:|---:|
| 1920×1080 | 240×135×64 | 2 073 600 | **~183 MB** |
| 2560×1440 | 320×180×64 | 3 686 400 | **~324 MB** |
| 3840×2160 | 480×270×64 | 8 294 400 | **~730 MB** |

**This is still the largest resolution-scaled allocation in the engine.** The
grid keys on *render* extent, so an FSR preset shrinks it quadratically — FSR
Quality (1.5×) at 1080p output renders 1280×720 for **~81 MB**, and at 4K
output renders 2560×1440 for **~324 MB**.

Budget per preset rather than against one ceiling — a single number is true for
the default and false for a mode nobody targets:

| Configuration | Volumetrics | Fixed floor (all resolution-scaled rows + scene SSBOs) |
|---|---:|---:|
| 1080p native | ~183 MB | ~1.10 GB |
| 4K output, FSR Quality (renders 1440p) | ~324 MB | ~1.55 GB |
| **4K native** (`--upscaler taa`) | ~730 MB | **~2.32 GB** |

Native 4K leaves ~1.68 GB for vertices, textures, BLAS and TLAS against a
~0.97 GB typical FNV interior, so the `< 4 GB` target now holds there too. It
was **~4.51 GB — over the ceiling before any content loaded** — at the previous
`froxel_xy_divisor` of 4.

Four of the six volumes — the three combustion transport fields and the
emission-history sidecar — are allocated unconditionally with the pipeline
(`context/mod.rs`), so they are resident in every session including scenes that
never light a fire. That is 20 B of the 22 B/froxel/FIF that fog alone does not
need: **~133 MB of the 183 MB at 1080p, ~531 MB of the 730 MB at native 4K.**

The end state is to move them into local high-density volumes attached to the
effect, which is where compact fire belongs anyway — advection is numerically
diffusive, so a coarser global grid roughly doubles smear per step and fire
degrades at `/8` in a way fog does not.

**Measured, not assumed** (2026-08-21, one binary, `--froxel-xy-divisor 4` vs
`8`, fixed camera, `--upscaler taa`, capture at a fixed frame):

| Scene | mean \|Δ\| per channel | where the difference is |
|---|---:|---|
| `--combustion-lab` (fire + fog) | 2.79 / 255 | flame region mean **7.18**, max **188**; rest of frame mean 2.59 |
| FNV exterior `--grid 0,0` (fog, no fire) | **0.03 / 255** | 0.01 % of pixels differ by more than 8 |

At `/4` the flame is a coherent tapered plume; at `/8` it breaks into two or
three blocky columns with a squared top — the exact failure `0ff7b537` raised
the density to avoid. The fog path is essentially insensitive to the divisor.
So the divisor's whole perceptual cost is combustion, which is the argument for
moving combustion local rather than for paying 4x globally. Note that neither a
bilateral composite upsample nor a blue-noise march offset recovers this: both
address sampling artifacts in the fog read, while this is advection diffusion in
the transported field. Once they are local, the global grid is
two volumes at 24 B/froxel (~199 MB at native 4K) and its density stops being a
fire question at all. Runtime work, tracked separately.

Note the inject and integrate dispatches are *already* skipped in a fogless
frame — `requires_dispatch` falls back to `record_neutral_frame`, a single clear
to `(0,0,0,1)` that composite applies as an exact no-op. The unconditional cost
is residency, not bandwidth.

Prior to Session 62 (2026-07-26→2026-08-01) the grid was a **fixed**
160×90×128 volume regardless of resolution (≈59.0 MB total, the flat
`56 MB` figure this section previously documented at every resolution —
that older figure used a binary-MiB basis rather than this doc's
decimal-MB convention elsewhere).

The old claim here — "understating peak 4K VRAM by almost exactly 2× (118.0 MB
vs. 59.0 MB)" — was **circular**: it checked the pre-Session-62 grid against the
bad summary row rather than against the code. That `~29.5 MB` row was the fixed
160×90×128 grid counted at half its multiplicity, with a 4× resolution scale
bolted onto a grid that by definition did not scale — wrong three ways at once.
The real understatement was ~18×.

The grid then grew along **two independent axes** without either reaching
this ledger (#3117): `froxel_xy_divisor` went 8 → 4 in `0ff7b537`
(2026-08-17), quadrupling the froxel count, and the per-slot volume set went
2 → 6 across `0ff7b537`→`4a35819e`. The `/4` quality default is denser so
compact fire and smoke do not collapse to one blocky ray column; **note that
it is a 4× VRAM and 4× inject-dispatch change that has not been through a
bench-of-record refresh.**

---

## Acceleration Structures (BLAS / TLAS)

[`acceleration/constants.rs`](../../crates/renderer/src/vulkan/acceleration/constants.rs)

### Scratch buffers

| Constant | Value | Role |
|---|---|---|
| `BLAS_REBUILD_SLACK_BYTES` | 16 MB | Retained headroom above peak before BLAS-scratch shrink |
| `TLAS_SCRATCH_SLACK_BYTES` | 256 KB | Retained headroom above peak before TLAS-scratch shrink |
| `TLAS_REBUILD_SLACK_BYTES` | 1 MB | Retained headroom above peak before TLAS instance-buffer shrink |

`shrink_blas_scratch_to_fit` runs at cell-unload
([`unload.rs`](../../byroredux/src/cell_loader/unload.rs)) and on swapchain
recreate ([`resize.rs`](../../crates/renderer/src/vulkan/context/resize.rs))
to reclaim VRAM after a peak scene is evicted or the swapchain resizes.

`shrink_tlas_to_fit` and `shrink_tlas_scratch_to_fit` (#1911 / REN-D1-01) are a
**different** call site with a stricter precondition: they run at the end of
every `draw_frame`
([`draw.rs`](../../crates/renderer/src/vulkan/context/draw.rs), post
`current_frame` increment), targeting the FIF slot whose fence was just
waited at this frame's start — not cell-unload. A future teardown path
copying the "runs at cell-unload" placement for these two would hit the
#1782 class of bug: destroying TLAS/instance buffers that an in-flight
command buffer still references.

### Reserve floors

| Constant | Value | Role |
|---|---|---|
| `MIN_TLAS_INSTANCE_RESERVE` | 8 192 instances | Never shrink the TLAS instance buffer below this |
| `WORKING_SET_FLOOR` | 8 192 instances | Post-shrink TLAS capacity floor |
| `MIN_BLAS_BUDGET_BYTES` | 256 MB | Minimum BLAS-budget floor (BLAS allocation heap / 3, capped below) |

### Build flags (split post #1196)

| Constant | Value | Applies to |
|---|---|---|
| `UPDATABLE_AS_FLAGS` | `PREFER_FAST_TRACE \| ALLOW_UPDATE` | TLAS (refit on static-layout frames) |
| `SKINNED_BLAS_FLAGS` | `PREFER_FAST_BUILD \| ALLOW_UPDATE` | Skinned BLAS (refits >> builds at steady state) |
| `STATIC_BLAS_FLAGS` | `PREFER_FAST_TRACE \| ALLOW_COMPACTION` | Static mesh BLAS (compact after build) |

`SKINNED_BLAS_FLAGS` deliberately uses `FAST_BUILD` not `FAST_TRACE`:
empirically on RTX 4070 Ti, small skinned-mesh BVHs (~5K–15K triangles)
produced worse total GPU cost with `FAST_TRACE` (wider tree adds refit
overhead that exceeds the traversal saving). Switching back recovered
+15.8 FPS on Prospector (R6a-prospector-regress, 2026-05-16).

### LRU eviction

`AccelerationManager::evict_unused_blas` runs pre-batch and mid-batch
(triggered at 90% of BLAS budget). Eviction check interval:
`BATCH_EVICTION_CHECK_INTERVAL` = 64 BLAS builds. LRU victim = the BLAS
with the smallest last-used frame tick.

One more call site (#1911 / REN-D1-01), with `pending_bytes = 0` (#1792 —
it has no in-flight batch context to report on top of): a per-frame call at
the end of `draw_frame`'s TLAS-build block
([`draw.rs`](../../crates/renderer/src/vulkan/context/draw.rs)).

#2914 — this paragraph used to name a third site, "a single-shot guard
inside `build_blas` itself … for the ad-hoc / UI-quad / lazy-upload path".
That was wrong twice over: the single-shot `build_blas` /
`build_blas_for_mesh` pair had **no caller anywhere in the workspace**, and
the UI quad is uploaded with `for_rt = false`, so it never had a BLAS to
guard. Both functions were deleted under #2914, following the #1141
precedent that removed the skinned sibling `build_skinned_blas`. Every
static BLAS is now built through `build_blas_batched` (the M40 cell-loader
path), which carries its own pre-batch and mid-batch eviction guards.

BLAS refit count before a forced rebuild: `SKINNED_BLAS_REFIT_THRESHOLD`
= 600 frames (~10 seconds at 60 FPS). After 600 refits the BLAS is
rebuilt from scratch to prevent BVH quality decay.

---

## Texture Registry

[`crates/renderer/src/texture_registry.rs`](../../crates/renderer/src/texture_registry.rs)

| Item | Value |
|---|---|
| Bindless array ceiling | `min(device.maxPerStageDescriptorUpdateAfterBindSampledImages, 65 535)` |
| Descriptor pool | `max_textures × 2 × MAX_FRAMES_IN_FLIGHT` combined image sampler descriptors — each per-frame set carries **two** `max_textures`-sized bindings |
| Staging pool cap | 128 MB (retained after upload flush, #239) |
| Deferred-destroy countdown | `MAX_FRAMES_IN_FLIGHT` = 2 frames |

There is no explicit texture-count eviction policy. When the bindless
array fills, new uploads are rejected with an error and the caller
(`asset_provider::resolve_texture`) falls back to the checkerboard
handle — degrades gracefully, no crash/corruption.

**Slots leak on cell revisit (#2030 / MEM-D3-01).** The registry is
strictly grow-only: every registration takes a fresh `textures.len()`
index, and `drop_texture` deliberately never reuses a dropped slot's
index — handle stability is load-bearing (#372: reuse would produce
silent material corruption on any dangling `GpuInstance.texture_index`
reference). GPU image memory itself *is* correctly reclaimed via the
deferred-destroy ring; what leaks is the finite slot-index space. So
re-entering a previously-unloaded cell re-registers its textures as
**new** slots instead of hitting the dedup cache, and a long session
that revisits cells repeatedly can exhaust the ceiling even on vanilla
content — this is a slow-motion, session-length concern independent of
mod load-order size. `TextureRegistry::live_slot_count()` /
`dead_slot_count()` split the two so `dead` dominating `live` is the
signal this is happening; `check_slot_available` logs a one-time
warning at 90% capacity including both counts. A real fix (generational
free-list gated on a deferred-destroy fence proving no live
`GpuInstance.texture_index` still references the slot) is tracked as
tech debt — not yet implemented.

---

## Mesh Registry

[`crates/renderer/src/mesh.rs`](../../crates/renderer/src/mesh.rs)

| Constant | Value | VRAM |
|---|---|---|
| `MAX_MESH_SLOTS` | 16 777 216 (1 << 24) | handle-table slots only (not VRAM) |
| `VERTEX_POOL_SOFT_CAP` | 4 M vertices | ~416 MB (104 B/vertex) |
| `VERTEX_POOL_HARD_CAP` | 16 M vertices | ~1.66 GB |
| `INDEX_POOL_SOFT_CAP` | 16 M indices | ~64 MB (4 B/index) |
| `INDEX_POOL_HARD_CAP` | 64 M indices | ~256 MB |

The vertex stride is 104 B (20 × f32 + 4 × u32 + 8 × u8 — position,
colour (widened `vec3`→`vec4`, `cd2b5fe4`), normal, UV, bone
indices/weights, splat channels, tangent); test-pinned
(`assert_eq!(size_of::<Vertex>(), 104)`, `crates/renderer/src/vertex.rs`).
Soft caps emit a `warn!`; hard caps return an error.
`check_pool_growth()` is called at every upload.

**Registry overflow guard** (`667d1a28`): `NifImportRegistry` now defaults
to a 2 048-entry LRU cap (configurable via `BYRO_NIF_CACHE_MAX=N`; `=0`
disables the LRU). Before this guard, unbounded cell loads could silently
exhaust the `MAX_MESH_SLOTS` table.

### Global geometry SSBO rebuild (#3298 / #3463)

The pool rows above are **single-generation** figures. Growing the global
geometry SSBO takes one of two paths, and one of them is transiently
double-buffered:

| Path | Gate | Resident generations | Extra peak |
|---|---|---|---|
| Resumable (`GeometryRebuildInProgress`) | projected < `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES` (256 MiB) | 2 | up to ~512 MiB total, i.e. 2× the projected size |
| Atomic idle-reclaim | projected ≥ 256 MiB, existing buffers present | 1 | — (old released before new is allocated) |

The resumable path allocates the full-size replacement **while the old
generation is still bound and serving draws**, and swaps only when both
targets are fully copied — an accepted trade (#3298) that turns a
multi-hundred-ms atomic stall into bounded per-frame chunks. `#3443`
restored the idle gate, so this doubling is *bounded by the 256 MiB
threshold*: it cannot reach the `VERTEX_POOL_HARD_CAP` +
`INDEX_POOL_HARD_CAP` figures, because a rebuild that large takes the
atomic path instead. Any budget arithmetic that doubles the hard caps is
therefore wrong post-#3443.

**Plus a second, distinct retained staging pool — the mesh side.** This is
separate from `TextureRegistry::staging_pool` (the "Staging pool cap" row in
the [Texture Registry](#texture-registry) section above); `MeshRegistry`
builds its own `geometry_staging_pool`, also at `DEFAULT_STAGING_BUDGET_BYTES`
(128 MiB), via the same `StagingPool::new`. A chunked rebuild acquires
`GEOMETRY_REBUILD_CHUNK_BYTES` (64 MiB) once for the vertex chunk and once
for the index chunk — up to 128 MiB total, i.e. the *entire* budget, not
just one chunk. Because both entries sit inside the retained budget,
`release` keeps them rather than evicting them, and there is no production
`trim_to(0)` caller anywhere in the workspace for either staging pool — so
this stays resident for the process lifetime after the first chunked
rebuild. This is new steady-state residency, not pre-existing: the
pre-#3298 atomic path staged the *whole* vertex/index buffer in one
`acquire` (commonly 600+ MiB for a large scene), which immediately blew
the 128 MiB budget and was evicted largest-first, leaving the pool near
empty — #3298 changed the mesh pool's steady state, not its cap.

Source of truth for the doubling is `GeometryRebuildInProgress`'s own doc
comment (`crates/renderer/src/mesh.rs`); this row exists so a budget
decision made from this page does not silently assume one generation.

---

## NIF Import Cache

[`byroredux/src/cell_loader/nif_import_registry.rs`](../../byroredux/src/cell_loader/nif_import_registry.rs)

Caches parsed + imported `NifScene` objects to avoid re-parsing the same
NIF when multiple REFRs reference it.

| Item | Value |
|---|---|
| Default cap | 2 048 entries |
| Override | `BYRO_NIF_CACHE_MAX=N` env var (`=0` disables LRU entirely) |
| Eviction strategy | LRU by last-access tick; smallest tick = victim on overflow |

The cap bounds *scene count*, not VRAM. Each cached entry holds
`ImportedScene` in CPU RAM (vertex data, block tree); the GPU resources
reside in `MeshRegistry` and are keyed separately.

---

## Material / BGSM Cache

[`byroredux/src/asset_provider/material.rs`](../../byroredux/src/asset_provider/material.rs)

| Constant | Value | Eviction |
|---|---|---|
| `MAX_BGEM_CACHE_ENTRIES` | 1 024 | Half-evict (remove oldest 512) on overflow |
| `MAX_FAILED_PATHS` | 1 024 | Half-evict (remove oldest 512) on overflow |
| `TemplateCache` cap | 256 entries | BGSM chain templates; LRU |

**Half-eviction** (`797424e4`, #1430): both maps use a companion
`VecDeque<String>` as an insertion-order tracker. When the map reaches
its ceiling, the oldest `N/2` keys are drained from the deque and
removed from the map. This keeps the recent working-set resident and
eliminates the cold-restart thundering-herd that a full flush caused.

---

## Deferred-Destroy Queue

[`crates/renderer/src/deferred_destroy.rs`](../../crates/renderer/src/deferred_destroy.rs)

GPU resources (textures, buffers, BLAS handles) cannot be freed
immediately after an ECS component drops them — the GPU may still be
reading them from an in-flight frame.

| Item | Value |
|---|---|
| Countdown depth | `DEFAULT_COUNTDOWN` = `MAX_FRAMES_IN_FLIGHT` = 2 frames |
| Implementation | `VecDeque<(frame_id, T)>` per resource type |
| Tick site | `draw_frame()` step 4 — after the in-flight fence wait, before recording |

Resources are not freed until `current_frame - frame_id >= countdown`.
The fence wait in step 1 of `draw_frame` guarantees all GPU work for
the fence slot is complete before the tick runs (#418).

## Morph-target GPU resources — #3661

The immutable morph delta buffer is cached by `MeshHandle` and shared by all
live entity slots for that mesh. Each entity keeps its own host-visible weight
buffer because animation changes those weights independently.

| Row | Count | Bytes |
|---|---:|---:|
| `morph_slots` | active skinned entities with morph targets | unique live mesh deltas + one weight buffer per entity |

The exact live values are exposed by `SkinCoverageStats` and the
`skin.coverage` command as `morph_slots` and `morph_bytes`. The byte total is
the logical Vulkan buffer sizes: the sum of
`vertex_count × target_count × 16` once per live mesh delta, plus
`target_count × 4` once per entity weight buffer. Allocator
page/granularity overhead is not included.

---

## Scaleform UI (Ruffle / wgpu) — #3431

[`crates/ui/src/player.rs`](../../crates/ui/src/player.rs)

The SWF menu layer runs Ruffle on its **own wgpu device**, separate from the
engine's `VulkanContext`. That is a second `VkInstance` / `VkDevice` /
`VkQueue` plus every Ruffle pipeline object, and it is **process-lifetime**:
`shared_descriptors()` parks the `Arc<Descriptors>` in a `static OnceLock`
(#2733), which is never dropped, so it is released by the OS at exit rather
than by wgpu. The singleton's own doc comment says "one idle logical device
is retained after the last menu's player is dropped"; in practice the
`OnceLock` never releases it at all.

| Allocation | Owner | 1920×1080 |
|---|---|---|
| `TextureTarget` render texture (`Rgba8Unorm`) | Ruffle wgpu device | ~8.3 MB |
| `TextureTarget` `MAP_READ` readback buffer (`padded_bytes_per_row × height`) | Ruffle wgpu device | ~8.3 MB |
| `SwfPlayer::pixel_buffer` | host RAM, not VRAM | ~8.3 MB |
| Engine-side UI `VkImage` + view | `TextureRegistry` | ~8.3 MB |
| Deferred-destroy copies of that image (up to `MAX_FRAMES_IN_FLIGHT`) | deferred-destroy ring | ~8.3–16.6 MB |

≈25–42 MB per live menu, **plus one whole extra logical device**. The first
four rows are per `SwfPlayer`; the device is shared across all of them.

The deferred-destroy copies are not a leak — the ring drains — but they are
resident because `TextureRegistry::update_rgba` recreates the image rather
than updating it in place, so an animating HUD cycles a fresh full-viewport
`VkImage` every frame (#3429).

### Not yet ledgered

A grep of this page for the owning subsystem name is the cheapest way to
find a gap in it. One is known and unquantified:

- **`StagingPool` retained capacity** beyond the geometry rebuild's 64 MiB
  above. The pool's budget is a *retention* bound (128 MiB default), not an
  in-flight bound, and texture uploads share it.

Both are listed rather than estimated on purpose: a fabricated number on
this page is worse than an acknowledged hole, because the page is cited as
authoritative rather than re-derived.

---

## VRAM Rough Budget (RTX 4070 Ti, typical FNV interior)

| Subsystem | Typical | Peak |
|---|---|---|
| G-buffer (7 attachments per [`gbuffer.rs`](../../crates/renderer/src/vulkan/gbuffer.rs)'s own table — normal/motion/mesh_id/raw_indirect/albedo at 4 B/px + FSR reactive/transparency masks at 1 B/px = 22 B/px, × 2 FIF; **not** counting the separate HDR colour, depth, or depth-history attachments) | ~91 MB (1080p) | ~365 MB (4K) |
| Scene SSBOs | ~223 MB | ~223 MB |
| ReSTIR reservoirs (2 FIF) | ~133 MB (1080p) | ~531 MB (4K) |
| SVGF history + à-trous pair (2 FIF) | ~83 MB (1080p) | ~332 MB (4K) |
| TAA history (2 FIF) | ~33 MB (1080p) | ~133 MB (4K) |
| Glass + water caustics (2 FIF) | ~66 MB (1080p) | ~265 MB (4K) |
| SSAO (2 FIF) | ~4 MB (1080p) | ~17 MB (4K) |
| Bloom pyramid (2 FIF) | ~11 MB (1080p) | ~44 MB (4K) |
| Volumetrics froxel grid (6 volumes, 44 B/froxel/slot, 2 FIF) | ~183 MB (1080p native) | **~730 MB (4K native)** — ~81 MB at 1080p / ~324 MB at 4K with FSR Quality |
| FSR 3.1 upscaler output (2 FIF, output resolution) | ~33 MB (1080p) | ~133 MB (4K) — SDK working memory not separately tracked |
| Vertex / index pools | ~208 MB | ~1.66 GB cap |
| Global geometry SSBO rebuild (#3298) | — (idle) | +2× projected, ≤ ~512 MB, + up to 128 MiB retained mesh-side staging (one 64 MiB vertex-chunk entry + one 64 MiB index-chunk entry, #3298's chunked path) |
| Scaleform UI (Ruffle wgpu device + target + readback + engine image) | ~25 MB (one menu) | ~42 MB + a second logical device |
| Textures (BC compressed) | ~400 MB | ~2 GB |
| BLAS structures | ~300 MB | ~1 GB (heavy scene) |
| TLAS + scratch | ~50 MB | ~256 MB |
| Pipeline cache blob | < 10 MB | — |
| **Estimated total** | **~1.81 GB** | **~3.72 GB at native 4K**, inside the < 4 GB target but with less margin than previously recorded — see the per-preset table in the Volumetrics section |

The 6 GB RT-minimum and 4 GB budget ceiling are not enforced by code;
they are design targets. The RTX 4070 Ti (12 GB) has headroom for all
known scene sizes. The renderer samples the allocator at startup and after
completed streaming, debug-load, and interior-cell transactions. A warning
fires when total allocated bytes exceed 80% of the smallest DEVICE_LOCAL
heap (`(heap / 5) * 4`, with a 2 GB fallback when no DEVICE_LOCAL heap is
reported); the warning is latched once per renderer context, while each
sample still records the INFO allocation report.

---

## See Also

- [`constants.rs`](../../crates/renderer/src/vulkan/scene_buffer/constants.rs) — all `MAX_*` values
- [`acceleration/constants.rs`](../../crates/renderer/src/vulkan/acceleration/constants.rs) — BLAS/TLAS slack + eviction thresholds
- [Shader Pipeline](shader-pipeline.md) — SSBO sizes in context of descriptor sets
- [Vulkan Renderer](renderer.md) — BLAS/TLAS lifecycle, LRU eviction, compaction
