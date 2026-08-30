# Safety Audit — ByroRedux — 2026-08-30

**Command**: `/audit-safety` (run as part of the `comprehensive` audit suite)
**Repo state**: `main` @ `64f64480`, working tree clean at audit start
**Severity scale**: `.claude/commands/_audit-severity.md`

## Scope

All eleven dimensions of `.claude/commands/audit-safety/SKILL.md` were run.
Two areas that have **no owner audit skill** were treated as explicitly in scope
for this run and are covered in full rather than incidentally:

- **`crates/mod-runtime`** (Dimension 11 — the sandboxed executable-mod trust
  boundary). Audited as a *contract*, not a live path: it still has no consumer
  in the engine, and "unused" is not reported as a finding.
- **The `crates/fsr3-sys` FFI `# Safety` contracts** (Dimension 1). This is the
  workspace's only live FFI crossing and sits on the engine-default render path.

Crate coverage sanity check against `_audit-common.md`'s 25-crate list: every
crate holding `unsafe`, an FFI boundary, an untrusted-input parser, or a
GPU-struct contract was touched. `crates/audio`, `crates/papyrus`,
`crates/platform`, `crates/spt` and `crates/debug-protocol` were reached only by
the Dimension-4 workspace-wide `unsafe` sweep (which found no `unsafe` in them);
their behavioural surfaces belong to their owner audits.

`crates/save`, `crates/hkx`, `crates/bsa` and `crates/mod-runtime` were verified
to contain **no `unsafe`** rather than skipped — for those crates the absence is
itself the safety property.

## Method notes

- Every finding's premise was re-checked against current `main` before filing.
  Three plausible-looking regressions (#2685, #2475, #2247) and one phantom
  symbol (*REFRACT_PASSTHRU_BUDGET*) were checked and dropped because the
  premise no longer holds. One **OPEN** issue (#3512) was found to have a stale
  premise — see the dedup notes at the end.
- Per the project's no-speculative-Vulkan-fixes rule, no render-pass, barrier or
  pipeline-state restructure is proposed anywhere in this report. The one HIGH
  Vulkan finding (SAFE-D5-01) is a **device-selection** change backed by decoded
  SPIR-V capability words, not a pipeline hypothesis. Dimension 5's remaining
  checks are static; a Vulkan ICD and validation layers are present on this box,
  but no engine was launched (the project forbids spawning a parallel instance
  alongside the user's own).
- Counts were re-derived, not taken from prose: **724** `unsafe { … }` blocks and
  **80** `unsafe fn` declarations across the workspace at audit time.

## Findings summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 2 |
| MEDIUM | 5 |
| LOW | 3 |
| **Total** | **10** |

| ID | Sev | Dim | Title |
|---|---|---|---|
| SAFE-D2-01 | HIGH | 2 | `CsgArchive::read_psg` pre-allocates from an unvalidated FO4 `_oc.nif` length |
| SAFE-D5-01 | HIGH | 5 | Non-RT GPUs are admitted, but `triangle.frag` declares `RayQueryKHR` unconditionally |
| SAFE-D3-01 | MEDIUM | 3 | `SceneImportCache` is uncapped and never evicted |
| SAFE-D4-01 | MEDIUM | 4 | `GpuBuffer::write_mapped` SAFETY comment asserts `T: Copy` implies no padding |
| SAFE-D6-02 | MEDIUM | 6 | `GpuLight` has no Rust↔GLSL lockstep leg and no size pin |
| SAFE-D8-01 | MEDIUM | 8 | Embedded-clip registration bypasses the #790 path memo on the NPC spawn path |
| SAFE-D9-01 | MEDIUM | 9 | B-spline dequantised sample path has no finite guard |
| SAFE-D5-02 | LOW | 5 | `sync.rs` image-fence-aliasing note cites a pre-#2731-split line anchor |
| SAFE-D6-01 | LOW | 6 | `GpuCamera` in-crate doc + pin-test doc still say 352 B (live 368 B) |
| SAFE-D6-03 | LOW | 6 | `GpuMaterial::as_bytes` safety docs cite a nonexistent `Hash` impl and pad fields |

**Dimensions that produced no findings**: **1** (FFI lifetime), **7** (RT
IOR-refraction guards), **10** (debug-ui / egui teardown), **11** (sandboxed mod
runtime). Each was run in full; the per-dimension PASS evidence is below.

---

# Findings

### SAFE-D2-01: `CsgArchive::read_psg` pre-allocates from a caller length that three unvalidated `u32`s off an FO4 `_oc.nif` control
- **Severity**: HIGH
- **Dimension**: 2 — Memory corruption / UB (untrusted-input allocation)
- **Location**: `crates/bsa/src/csg.rs:224-226` (the allocation) ← `byroredux/src/cell_loader/precombined.rs:728-736` (the length)
- **Status**: NEW
- **Description**:
  `CsgArchive::read_psg` opens with `let mut out = Vec::with_capacity(len);` **before**
  any of its careful per-chunk EOF checks run, and `len` is a plain `usize` parameter —
  it is the one size in the CSG reader that never passes through
  `crates/bsa/src/safety.rs`. Its single production caller computes it from three raw
  `u32` fields read straight out of an FO4 `_oc.nif` with no ceiling anywhere on the
  path:

  ```rust
  // byroredux/src/cell_loader/precombined.rs:727-736
  let tri_start = (lod_off_idx / 3) as usize;
  let need = geom.num_verts * stride + (tri_start + lod_count) * 6;
  ...
  let psg = match csg.read_psg(geom.data_offset as u64, need) {
  ```

  `PrecombineGeomRef.num_verts` / `.lod_counts` / `.lod_offsets`
  (`crates/nif/src/import/precombine.rs:212-220`) are copied verbatim from
  `BsPackedSharedGeomData`'s `num_verts: u32` / `tri_count_lod*: u32` /
  `tri_offset_lod*: u32` (`crates/nif/src/blocks/extra_data.rs:840-844`), which
  `collect_precombine_geom_refs` (`precombine.rs:268-274`) forwards without validation.
  Unlike the *non*-shared `BsPackedGeomData` variant — whose payload is in-file and so
  is bounded by `NifStream::check_alloc` — the shared variant's arrays live in the
  external `.csg`, so nothing in the NIF parser ever has to reconcile these counts
  against a byte budget. With `tri_count_lod0` alone at `u32::MAX`, `need` reaches
  ~26 GB; with `num_verts` at `u32::MAX` and a 48-byte stride it reaches ~200 GB.
- **Evidence**:
  ```rust
  // crates/bsa/src/csg.rs:224-231
  pub fn read_psg(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
      let mut out = Vec::with_capacity(len);   // ← unbounded, runs first
      let mut remaining = len;
      let mut pos = offset;
      while remaining > 0 {
          let idx = (pos / CSG_CHUNK_SIZE as u64) as u32;
          ...
          if idx as usize >= self.chunks.len() { return Err(...UnexpectedEof...) }
  ```
  Every other file-controlled size in this same file *is* bounded —
  `checked_entry_count(num_chunks_raw, "CSG chunk")` at `:167`,
  `checked_chunk_size(entry.compressed_size, …)` at `:270`, and `inflate_bounded`
  at `:284`. `read_psg`'s `len` is the sole exception, and it is the one value that
  originates outside this crate.
- **Impact**:
  A corrupt or hostile FO4 `_oc.nif` — ordinary mod-distribution content, and the
  precombine path runs on every FO4 cell load — drives a multi-gigabyte
  `Vec::with_capacity` before a single bounds check executes. On allocation failure
  Rust calls `handle_alloc_error`, which **aborts the process**: not an `Err` any
  caller can handle, and not interceptable by `catch_unwind`. The two `continue`-on-`Err`
  arms at `precombined.rs:737-743` that make every other failure in this loop a skipped
  object are unreachable for this one. On Linux with heuristic overcommit a mid-range
  value may instead succeed as a pure virtual reservation, so the observable symptom
  ranges from a silent multi-GB VA spike to a hard abort depending on the declared
  size and the host's overcommit policy — which is exactly why the ceiling belongs at
  the reader, not at the allocator.
  This is the same class the project has already closed everywhere else it appears:
  #388 (NIF, CRITICAL), #408 (73-site NIF sweep, HIGH), #2614 (Starfield CDB, HIGH),
  #3011 (HKX, HIGH), #3399 (ESM compressed records, HIGH), #3410 (BSA `inflate_bounded`,
  HIGH). Rated HIGH to match those; the amplification here (three independent `u32`s,
  one of them multiplied by a stride) is larger than in the still-open MEDIUM #3512.
- **Related**: #3512 (OPEN, same file, `chunk_bytes` — see the dedup note below);
  #3410 (the `inflate_bounded` helper this should reuse the posture of); #1533 (CLOSED,
  the sibling *index*-bounds check `decode_shared_geom_object` already performs on the
  same fields, at `crates/nif/src/import/precombine.rs:172` — it validates triangle
  indices against `num_verts` but only *after* the buffer has been allocated and read).
- **Suggested Fix**:
  Route `need` through `byroredux_bsa::safety::checked_chunk_size_usize` at the call
  site, and make `read_psg` defensive on its own account by clamping the initial
  `Vec::with_capacity` to `len.min(self.psg_len()? as usize)` (or simply to
  `MAX_CHUNK_BYTES`) so the reader cannot be made to pre-allocate more than the PSG
  space it actually owns. Use `saturating_mul`/`checked_mul` for the `need` arithmetic
  while there, so a wrapped product cannot silently under-read instead.

---


### SAFE-D5-01: `is_device_suitable` admits non-RT GPUs, but the main triangle pipeline unconditionally creates SPIR-V declaring `RayQueryKHR` + `PhysicalStorageBufferAddresses` — the #1561 fix covered only `water.frag`
- **Severity**: HIGH
- **Dimension**: 5 (Vulkan Spec Compliance)
- **Location**: `crates/renderer/src/vulkan/device.rs:251-259`, `:401-425`, `:742-743`, `:786-790`; `crates/renderer/src/vulkan/context/init.rs:676-685`, `:937-948`, `:1114-1134`; `crates/renderer/shaders/triangle.frag.spv`, `triangle.vert.spv`, `caustic_splat.comp.spv`, `volumetrics_inject.comp.spv`, `skin_vertices.comp.spv`
- **Status**: NEW (sibling of CLOSED #1561, which fixed only the `water.frag` half)
- **Description**:
  `VK_KHR_ray_query` / `VK_KHR_acceleration_structure` are **optional** in device selection —
  `REQUIRED_EXTENSIONS` is just `[VK_KHR_swapchain]` (`device.rs:252`), and `is_device_suitable`
  merely records `ray_query_supported = RT_EXTENSIONS.iter().all(has_extension)` (`:425`) without
  rejecting the device. `create_logical_device` then correctly *withholds* the matching features
  when that flag is false: `.buffer_device_address(caps.ray_query_supported)` (`:743`),
  `.acceleration_structure(caps.ray_query_supported)` (`:787`),
  `.ray_query(caps.ray_query_supported)` (`:790`).

  But the **main geometry pipeline is created unconditionally** — `init.rs:676`
  `pipeline::create_triangle_pipeline(…)` has no `ray_query_supported` gate, and
  `pipeline.rs:274-275` / `:682-683` load `TRIANGLE_VERT_SPV` + `TRIANGLE_FRAG_SPV` verbatim.
  Decoding the committed SPIR-V shows those modules declare capabilities the device may not have
  enabled. The caustic pipeline (`init.rs:1114`, and it `return Err`s on failure) and the
  volumetrics pipeline (`init.rs:937`) are likewise ungated.

  Per the Vulkan SPIR-V environment appendix, `RayQueryKHR` requires
  `VkPhysicalDeviceRayQueryFeaturesKHR::rayQuery` and `PhysicalStorageBufferAddresses` requires
  `VkPhysicalDeviceVulkan12Features::bufferDeviceAddress` to be **enabled**; creating a shader
  module / pipeline from such SPIR-V without them violates
  **VUID-VkShaderModuleCreateInfo-pCode-08740**.

  The codebase already knows this argument and applies it elsewhere: `device.rs:433-444` rejects a
  device lacking `shaderInt64` with the comment *"a device without shaderInt64 cannot legally
  create the renderer's shader modules (VUID-VkShaderModuleCreateInfo-pCode-08740)"* —
  the identical reasoning is simply not applied to the RT capabilities. And `init.rs:705-710`
  already asserts *"RT-capable hardware (the only configuration this engine targets — RT is
  mandatory)"* while gating **only** `WaterPipeline` on the flag (the #1561 fix).
- **Evidence**:
  - `OpCapability` decode of the committed `.spv` files (opcode 17 stream):
    ```
    triangle.frag.spv    [Shader, ImageQuery, RayQueryKHR, RuntimeDescriptorArray,
                          InputAttachmentArrayDynamicIndexing, StorageBufferArrayDynamicIndexing,
                          PhysicalStorageBufferAddresses]
    triangle.vert.spv    [Shader, ImageQuery, PhysicalStorageBufferAddresses]
    caustic_splat.comp   [Shader, ImageQuery, RayQueryKHR, PhysicalStorageBufferAddresses]
    volumetrics_inject   [Shader, DerivativeControl, RayQueryKHR]
    skin_vertices.comp   [Shader, ImageQuery, PhysicalStorageBufferAddresses]
    water.frag.spv       [Shader, ImageQuery, RayQueryKHR, …]   ← the only one whose pipeline IS gated
    ```
  - `device.rs:252` `const REQUIRED_EXTENSIONS: &[&CStr] = &[ash::khr::swapchain::NAME];`
  - `device.rs:255-259` `RT_EXTENSIONS` is labelled *"Optional RT extensions (enabled when available)"*.
  - `device.rs:425` `let ray_query_supported = RT_EXTENSIONS.iter().all(|ext| has_extension(ext));`
    — no `return Ok(None)` follows.
  - `device.rs:743` `.buffer_device_address(caps.ray_query_supported)`.
  - `init.rs:676` `let pipelines = pipeline::create_triangle_pipeline(` — no surrounding `if`.
  - `init.rs:710` `let mut water = if device_caps.ray_query_supported {` — the gated sibling.
  - No hard bail anywhere: `grep -rn 'ray_query_supported' byroredux/src crates/renderer/src`
    yields only per-feature `if`/flag reads, never a startup rejection.
- **Impact**:
  On any GPU that passes `is_device_suitable` but lacks `VK_KHR_ray_query` — the suitability
  message itself names *"RDNA1 or newer"*, and RDNA1 (RX 5000) has no ray-query support — startup
  runs `vkCreateShaderModule` / `vkCreateGraphicsPipelines` with capabilities whose features are
  disabled. That is undefined behaviour: the observable outcome ranges from a wall of validation
  errors (*"SPIR-V Capability RayQueryKHR was declared, but one of the following requirements is
  required (VkPhysicalDeviceRayQueryFeaturesKHR::rayQuery)"*) to driver-dependent pipeline-creation
  failure to a hard fault. Because `bufferDeviceAddress` is also withheld, even `triangle.vert`
  (which has no ray query at all, only `PhysicalStorageBufferAddresses`) is illegal on that path —
  so there is no partial-render fallback; the entire main pass is affected. RT-capable hardware
  (the dev 4070 Ti, and everything the project actually targets) is completely unaffected, which
  is why this has never been observed.
- **Related**: #1561 (CLOSED — REN-D2-NEW-01, the `water.frag` half of exactly this problem);
  #1636 / #1478 (the same "feature gated on the wrong flag" shape for `host_query_reset`);
  `device.rs:433-444` (the `shaderInt64` precedent that cites the very same VUID).
- **Suggested Fix**:
  Make device selection match the documented policy instead of restructuring any pipeline.
  Either move `ash::khr::acceleration_structure::NAME` + `ash::khr::ray_query::NAME` +
  `ash::khr::deferred_host_operations::NAME` into `REQUIRED_EXTENSIONS`, or — matching the
  `shaderInt64` shape exactly — add, immediately after `device.rs:425`:

  ```rust
  // The committed shader set declares RayQueryKHR (triangle.frag, caustic_splat.comp,
  // volumetrics_inject.comp, water.frag) and PhysicalStorageBufferAddresses (triangle.vert,
  // triangle.frag, skin_vertices.comp). Both require features this chain only enables when
  // ray_query_supported is true, so an RT-less device cannot legally create these shader
  // modules (VUID-VkShaderModuleCreateInfo-pCode-08740).
  if !ray_query_supported {
      return Ok(None);
  }
  ```

  and let the existing `anyhow::bail!("No suitable GPU found …")` at `device.rs:378-384` carry
  the diagnostic (extend its text with the RT requirement). That single change removes the
  illegal path, makes `init.rs:707`'s "RT is mandatory" comment true, and lets the now-dead
  `ray_query_supported == false` branches (`init.rs:428, 600, 628, 710`, the rt-disabled
  descriptor-layout permutation, `buffer_device_address` gating) be simplified in a follow-up
  rather than in the same commit.
  **Verification note:** the *static* premise above (which capabilities the shipped SPIR-V
  declares, and that the pipeline is created without a gate) is proven from the binaries and the
  source. The *runtime* symptom on an RT-less device would need a validation-layer run on such a
  GPU (or `VK_LAYER_KHRONOS_validation` with a device-simulation layer) to observe directly.

---


### SAFE-D3-01: `SceneImportCache` is an uncapped, never-evicted process-lifetime hold of every NPC skeleton / body / hand / armor `ImportedScene`
- **Severity**: MEDIUM
- **Dimension**: Memory & Resource Leaks (D3 item 5 — CPU-side unbounded growth)
- **Location**: `byroredux/src/scene_import_cache.rs:31-82`, backed by `byroredux/src/parsed_nif_cache.rs:79-102`; inserted at `byroredux/src/scene/nif_loader.rs:452-464`; registered at `byroredux/src/boot.rs:558`
- **Status**: NEW (no open or closed issue mentions `SceneImportCache` / `scene_import_cache`; the two sibling caches were both fixed — #951 for `bgem_cache`/`failed_paths`, #3054 for `sf_cdb_cache`, #635 for `NifImportRegistry` — this is the one wrapper of the shared core that was never given a cap)
- **Description**: `SceneImportCache` wraps `ParsedNifCache<ImportedScene>` and is the
  process-lifetime cache for the *hierarchical* NIF import path
  (`load_nif_bytes_with_skeleton`). Its sibling wrapper `NifImportRegistry` grew an LRU cap
  under #635 (default 2048 entries, `BYRO_NIF_CACHE_MAX` override) because the shared core
  deliberately does no eviction of its own. `SceneImportCache` never got the equivalent:
  it has no `max_entries`, no `access_tick`, no `remove` call site, and no `clear` anywhere
  in the tree. Entries are only ever added, and each entry is a full `Arc<ImportedScene>`
  (positions `[f32;3]`, colors `[f32;4]`, normals `[f32;3]`, tangents `[f32;4]`, uvs
  `[f32;2]`, `indices: Vec<u32>`, plus skin data per mesh — `crates/nif/src/import/types.rs:734-779`),
  i.e. roughly 60+ bytes per vertex retained on the CPU heap *in addition to* the GPU copy.
  This is a substantially heavier per-entry payload than any of the three caches that were
  already capped.
- **Evidence**:
  - The shared core states the contract explicitly —
    `byroredux/src/parsed_nif_cache.rs:79-81`:
    ```rust
    /// Insert (or overwrite) an entry. […]
    /// Does NOT do LRU eviction — that's
    /// the wrapper's responsibility (only `NifImportRegistry`
    /// supports LRU today).
    pub(crate) fn insert(&mut self, key: String, value: Option<Arc<T>>) {
    ```
  - The wrapper adds nothing but a counter — `byroredux/src/scene_import_cache.rs:31-40`:
    ```rust
    pub(crate) struct SceneImportCache {
        core: ParsedNifCache<ImportedScene>,
        bypass_parses: u64,
    }
    ```
    and `:74-82`:
    ```rust
    pub(crate) fn insert(
        &mut self,
        key: String,
        value: Option<Arc<ImportedScene>>,
    ) -> Option<Arc<ImportedScene>> {
        let to_return = value.clone();
        self.core.insert(key, value);
        to_return
    }
    ```
  - Contrast the capped sibling, `byroredux/src/cell_loader/nif_import_registry.rs:291-294`:
    ```rust
    let max_entries = std::env::var("BYRO_NIF_CACHE_MAX")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(2048);
    ```
  - Grep for every `SceneImportCache` use outside its own file returns exactly:
    `boot.rs:558` (`insert_resource`), `scene/nif_loader.rs:426` (`record_bypass_parse`),
    `:444` (`get`), `:458` (`insert`). There is no eviction, no `remove`, no `clear`
    call site anywhere.
  - The population is not just skeletons/bodies: the **armor/outfit** spawn phase routes
    through the same cached call when `hidden_biped_mask == 0`
    (`byroredux/src/npc_spawn/resumable.rs:855-862`), joining skeleton
    (`:653`), body (`:686`), head (`:1040`), and the generic part loader (`:1078`).
    On Skyrim SE / FO4 the distinct-armor-mesh population reachable across a long
    exterior-streaming session is in the hundreds-to-thousands, so the key set is not
    naturally small the way `csg_cache`'s (load-order-keyed) is.
- **Impact**: Monotonic CPU-heap growth across a long session — roughly one full mesh's
  worth of host geometry per distinct NPC-part NIF path ever seen, retained until process
  exit. Not per-frame, so no frame-time cliff, but it is the same unbounded-by-construction
  shape that #951 / #3054 / #635 each closed on the smaller caches, on the largest payload
  of the four. Also defeats the "bounded arena" posture the rest of the streaming path
  maintains: `NifImportRegistry` evicts at 2048 while its structurally identical sibling,
  fed by the same cell-streaming traffic, does not.
- **Related**: #635 (LRU cap for `NifImportRegistry` — the exact fix shape), #951
  (SAFE-26, `bgem_cache` + `failed_paths` unbounded), #3054 (SF-D3-01, `sf_cdb_cache`
  uncapped), #1430 (MEM-04, clear-whole-map vs LRU), #863 / #1854 (clip-handle release must
  be ordered with respect to eviction — the precedent to follow if this cache ever grows
  side-state).
- **Suggested Fix**: Give `SceneImportCache` the same bounded shape as its sibling — either
  reuse `NifImportRegistry`'s `access_tick` + `max_entries` LRU (lifting that machinery into
  `ParsedNifCache` so both wrappers share it, which also removes the duplication the module
  doc already flags), or, minimally, the half-eviction-on-overflow pattern
  `MaterialProvider` uses at `asset_provider/material.rs:759-762`. Default the cap to a
  few hundred entries given the per-entry payload, and honour the existing
  `BYRO_NIF_CACHE_MAX=0` unlimited escape hatch for parity.

---


### SAFE-D4-01: `GpuBuffer::write_mapped`'s SAFETY comment asserts "`T: Copy` guarantees no padding" — false for the generic bound; soundness rests on every current call site coincidentally passing a padding-free struct
- **Severity**: MEDIUM
- **Dimension**: 4 (Unsafe-Block Discipline)
- **Location**: `crates/renderer/src/vulkan/buffer.rs:1160-1166`
- **Status**: NEW
- **Description**:
  ```rust
  pub fn write_mapped<T: Copy>(&mut self, device: &ash::Device, data: &[T]) -> Result<()> {
      // SAFETY: T: Copy guarantees no padding/drop concerns. The pointer is
      // valid and aligned (from a live slice), and size_of_val gives the
      // exact byte length.
      let bytes: &[u8] = unsafe {
          std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data))
      };
  ```
  `T: Copy` guarantees no `Drop` glue, but it says **nothing** about padding. A `Copy` `#[repr(C)]`
  struct whose fields do not tile its size (or one carrying `#[repr(C, align(N))]` with a field sum
  that is not a multiple of `N`) has uninitialised padding bytes, and materialising a `&[u8]` over
  them — then reading them in the `copy_from_slice` at `:1182` — is UB by Rust's uninit-byte rules.
  This is precisely the invariant `bytemuck::Pod` exists to encode, and the crate is already a
  workspace dependency (`crates/nif` uses `AnyBitPattern`).

  This is a *distinct* false claim from the three that #2683 corrected in the same function: that
  issue fixed the `aligned_flush_range` "contained in the allocation" assertions at `:1198-1207`
  and left the `T: Copy` line untouched.

  This is the exact class of finding the brief prioritises — a commented `unsafe` block whose
  stated invariant is false — rather than a missing comment.
- **Evidence**:
  - The bound is `T: Copy`, not `T: Pod` / `T: AnyBitPattern` / `T: NoUninit`
    (`buffer.rs:1160`).
  - All 19 live call sites were enumerated and each `T` inspected; every one is a
    `#[repr(C)]` GPU-contract struct that happens to be padding-free:
    `GpuCamera`, `GpuDalcCube`, `GpuSelectedRayProbe` (`scene_buffer/gpu_types.rs`),
    `SsaoParams` (`ssao.rs:21-23`), `DownsampleParams`/`UpsampleParams` (`bloom.rs:98-107`),
    `CausticParams`, `CompositeParams`, `TaaParams`, `GpuWaterParams`,
    `vk::AccelerationStructureInstanceKHR` (`tlas.rs:179`), and `&[u8]` (`volumetrics.rs:1827`).
    The two `#[repr(C, align(16))]` types — the ones where trailing padding *would* be
    introduced — are `VolumetricsParams` (`volumetrics.rs:66`, all `mat4`/`vec4` fields) and
    `GpuFogVolume` (`volumetrics.rs:161`, 6 × `vec4` = 96 B, pinned by the
    `assert_eq!(size_of::<GpuFogVolume>(), 96)` at `volumetrics.rs:3449`). Both are clean.
  - So this is a **latent** soundness gap, not a live one: nothing in the signature stops the
    next caller from passing a padded type, and no test would catch it.
- **Impact**:
  No current miscompile or corruption. The exposure is that a future `write_mapped` caller with a
  padded `#[repr(C)]` param struct — the natural thing to write when adding a new compute pass —
  silently introduces UB (reading uninit bytes) and simultaneously uploads indeterminate padding
  to the GPU, with the SAFETY comment actively vouching for it. Also nudges anyone reading the
  file toward believing `Copy ⇒ no padding`, which is wrong and reusable in the wrong direction.
- **Related**: #2683 (CLOSED — corrected the other three false SAFETY assertions in this same
  function, left this one); #84 (CLOSED — `write_mapped` silent truncation, the sibling
  robustness issue at `:1174-1181`); `crates/nif/src/stream.rs:790` (the correct pattern:
  `T: AnyBitPattern`).
- **Suggested Fix**:
  Tighten the bound rather than the prose — the invariant then becomes compiler-enforced:
  ```rust
  pub fn write_mapped<T: bytemuck::NoUninit>(&mut self, device: &ash::Device, data: &[T]) -> Result<()> {
      // SAFETY: `T: NoUninit` guarantees every byte of `T` is initialised (no
      // implicit padding), so the byte view contains no uninit bytes. The
      // pointer is valid and aligned (from a live slice) and `size_of_val`
      // gives the exact borrowed length.
      let bytes: &[u8] = bytemuck::cast_slice(data);
  ```
  (`bytemuck::cast_slice` removes the `unsafe` entirely.) Each of the ~13 param structs then needs
  `#[derive(bytemuck::NoUninit)]` (or `Pod`), which is itself the drift guard being asked for. If
  the derive churn is unwanted, the minimum fix is to correct the comment to state the *real*
  invariant — "every current call site passes a `#[repr(C)]` type whose fields tile its size; a
  padded `T` would make this unsound" — so the next caller is warned instead of reassured.

---


### SAFE-D6-02: `GpuLight` has no Rust↔GLSL lockstep leg and no size pin — and a sibling test's doc claims it does
- **Severity**: MEDIUM
- **Dimension**: 6 — R1 material table layout (GpuLight sub-check)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:1681-1723` (the guard), `:1748` (the false claim); `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:268-281` (the struct)
- **Status**: NEW
- **Description**: `GpuMaterial` and `GpuInstance` each have **two** legs of protection: mirror-vs-mirror across the GLSL copies, **and** mirror-vs-Rust field order/type against the `#[repr(C)]` struct (`gpu_material_glsl_field_order_matches_rust_struct:1383`, `gpu_instance_glsl_copies_stay_in_lockstep:1750`). `GpuLight` has only the first leg. `gpu_light_glsl_copies_stay_in_lockstep` (`:1681`) walks the four GLSL declarations (`include/bindings.glsl`, `cluster_cull.comp`, `caustic_splat.comp`, `volumetrics_inject.comp`) and asserts they equal **each other** — it never looks at `gpu_types.rs`. There is additionally **no** `size_of::<GpuLight>()` assertion anywhere in the crate, unlike `GpuInstance` (160 B) and `GpuCamera` (368 B). Consequently: appending or reordering a field on the Rust `GpuLight` while leaving all four GLSL copies untouched passes every test in the workspace, and the CPU would then upload a stride the shaders do not decode.
- **Evidence**:
  - `shader_contract_tests.rs:1701-1722` — the loop body only ever compares `fields` to `reference`, both of which come from `SOURCES` (four GLSL paths). No `include_str!("../gpu_types.rs")`, no `parse_rust_struct_fields`, in contrast to `:1387-1388` (GpuMaterial) and `:1750`ff (GpuInstance).
  - `shader_contract_tests.rs:1748` states the GpuInstance guard delivers "the same two-leg coverage `GpuMaterial` and `GpuLight` already have" — **factually wrong for `GpuLight`**, which is exactly the kind of claim that stops a future author from adding the missing leg.
  - `grep -rn 'GpuLight' crates/renderer/src | grep -iE 'assert|size_of'` returns only *consumers* that derive sizing from `size_of::<GpuLight>()` (`upload.rs:64`, `upload.rs:913`, `buffers.rs:462`, `context/mod.rs:2105`) — self-consistent on the CPU side, so a Rust-only change silently propagates to the buffer stride with no test failure.
  - Current state is correct: Rust `position_radius / color_type / direction_angle / params` (four `[f32; 4]`, 64 B) matches all four GLSL copies verbatim. This is a **missing guard**, not a live drift.
- **Impact**: A `GpuLight` field addition or reorder that touches only the Rust side (or only some subset of a future fifth GLSL copy) ships green. The failure mode is silent per-light data corruption in the clustered-lighting, caustic and volumetric passes — the same class the severity table rates HIGH when it actually happens ("`#[repr(C)]` GPU struct size/layout drifts from shader struct"). Rated MEDIUM here because nothing is currently drifted; this is the defense-in-depth gap that would let it happen unnoticed. `GpuLight` is also the *most* mirrored struct after `GpuInstance` (four copies), and `#1916` already fired once on this exact struct.
- **Related**: #1916 (CLOSED — added the four-way GLSL leg), #2748 (CLOSED — added the Rust leg for `GpuInstance` after finding its guard was presence-only), #1657 (CLOSED — added the Rust leg for `GpuMaterial`), #1810 (CLOSED — stale 48 B `GpuLight` byte-math)
- **Suggested Fix**: Extend `gpu_light_glsl_copies_stay_in_lockstep` with the second leg, reusing the machinery already in the module: `parse_rust_struct_fields(include_str!("../gpu_types.rs"), "pub struct GpuLight")` → `normalize_ident` → assert count + order against the shared GLSL field list, mirroring `gpu_instance_glsl_copies_stay_in_lockstep:1750`ff. Add `assert_eq!(size_of::<GpuLight>(), 64)` alongside `gpu_instance_is_160_bytes_std430_compatible` in `gpu_instance_layout_tests.rs`. Correct the claim at `shader_contract_tests.rs:1748` in the same change (or delete the `GpuLight` mention from it).


### SAFE-D8-01: Embedded-clip registration bypasses the #790 path memo on the per-NPC spawn path
- **Severity**: MEDIUM
- **Dimension**: 8 (NPC / animation spawn safety)
- **Location**: `byroredux/src/scene/nif_loader.rs:1360-1370` (inside
  `load_nif_bytes_with_skeleton`, lines 376-1437); callers
  `byroredux/src/npc_spawn/resumable.rs:653, 686, 859, 1040`
- **Status**: NEW
- **Description**: `AnimationClipRegistry::get_or_insert_by_path` is the #790 dedup
  mechanism, and D8.2 above confirms it is correct and case-insensitive. But
  `load_nif_bytes_with_skeleton`'s embedded-clip branch calls the **un-keyed**
  `registry.add(clip)` instead. That function is invoked once per NPC skeleton, once per NPC
  body part, once per head, and once per equipped item — so any NPC-worn NIF that carries an
  embedded controller stack registers a *fresh full clip copy* on every NPC spawn, and again
  on every cell reload. Nothing releases these handles: the two `release()` call sites
  (`byroredux/src/streaming_helpers.rs:542-544`, `byroredux/src/cell_loader/references/mod.rs:134-136`)
  only retire handles owned by the cell-loader's `NifImportRegistry` LRU. The
  `SceneImportCache` consulted at `nif_loader.rs:426-459` memoises the *parse/import*
  (`ImportedScene`), not the registration — a cache HIT still falls through to line 1370.
- **Evidence**:
  - `byroredux/src/scene/nif_loader.rs:1365-1370` —
    `let mut registry = world.resource_mut::<AnimationClipRegistry>(); registry.add(clip)`.
    Compare `byroredux/src/npc_spawn.rs:520-521`, the sibling KF loader, which correctly uses
    `registry.get_or_insert_by_path(kf_path.to_string(), || clip)`.
  - `crates/core/src/animation/registry.rs:317-322` — the `plain_add_does_not_populate_path_map`
    test documents that `add()` is deliberately outside the dedup map, so this is a genuine
    opt-out, not a latent memo hit.
  - The cell-loader REFR path does NOT have this gap — it memoises the handle on
    `CachedNifImport` (`byroredux/src/cell_loader/nif_import_registry.rs:147, 268`;
    `byroredux/src/cell_loader/partial.rs:86`) and releases it on eviction. The NPC path is
    the one that skipped it.
  - Same call site also `world.spawn()`s an `AnimationPlayer` entity
    (`nif_loader.rs:1403-1412`) with no `Parent` link to `placement_root`, so it is outside
    the subtree a cell unload despawns — a second, entity-level leak on the same path.
- **Impact**: One un-freeable `AnimationClip` (keyframe arrays + text keys + channel HashMap)
  plus one orphan ECS entity per NPC-part NIF carrying an embedded clip, per NPC, per cell
  load. This is exactly the #790 failure shape (steady RAM growth across a walking session)
  reintroduced on a different caller. **Magnitude is content-dependent and I could not
  measure it** — no game archives are present in this checkout and the runtime baselines
  under `.claude/audit-baselines/runtime/` carry no embedded-clip counter — so I am rating
  this MEDIUM rather than the HIGH the "leak that compounds per cell" anchor would give a
  confirmed per-cell leak. If a `tex`-style census shows FNV/Skyrim NPC body/armour NIFs
  commonly carry `NiControllerSequence`/inline transform controllers, escalate to HIGH.
- **Related**: #790 (E-N1, the original grow-only leak, CLOSED), #866 (case folding, CLOSED),
  #863 + #2524 (LRU release wiring, CLOSED), #2689 (slot-header stranding, OPEN),
  #3377 (`PersistentCellApplyJob` leaking pending clip handles, CLOSED)
- **Suggested Fix**: Route the embedded-clip registration through the same memo as the KF
  path — `registry.get_or_insert_by_path(label.to_ascii_lowercase(), || clip)`, keyed on the
  `label` parameter already threaded into `load_nif_bytes_with_skeleton` (it is the archive
  mesh path at every `resumable.rs` call site). Separately, parent the spawned
  `AnimationPlayer` entity to the mesh root so cell unload reclaims it. Add a registry-length
  assertion to a two-NPC spawn test to pin it.


### SAFE-D9-01: B-spline dequantised sample path has no finite guard — a NaN `offset`/`half_range` reaches `Transform` and the GPU
- **Severity**: MEDIUM
- **Dimension**: 9 (NIFAL NaN/Inf boundary)
- **Location**: `crates/nif/src/anim/bspline.rs:44-46` (`dequant`), `:139-160`
  (`dequantize_channel`), `:241-245`, `:332-341`, `:396-403`; source fields
  `crates/nif/src/blocks/interpolator.rs:1372-1377`, `:1440-1441`, `:1499-1500`
- **Status**: NEW
- **Description**: `NiBSplineCompTransformInterpolator`'s six quantisation parameters
  (`translation_offset` / `translation_half_range` / `rotation_*` / `scale_*`) are read as raw
  `f32`s off disk with no validation, then fed to
  `dequant(raw, offset, half_range) = offset + (raw/32767) * half_range`. A NaN or ±Inf in
  either parameter makes every dequantised control point non-finite, and the de Boor
  evaluation propagates it. The **pose-fallback** branches of this same function are correctly
  gated by `is_flt_max` (verified in D8.1 above, `:344-357` / `:376-386` / `:404-412`), and the
  mainline keyframe converters are gated by `is_key_value_sane` (#1443,
  `crates/nif/src/anim/keys.rs:41, 65, 223`) — but the **sampled** branch, which is the whole
  point of the block, passes `deboor_cubic`'s output straight into the key structs with
  neither check. Rotation happens to be safe by accident (the `len_sq > f32::EPSILON` test at
  `:367` is false for NaN, so the quaternion falls back to identity); translation, scale, and
  the whole of `extract_float_channel_bspline` are not.
- **Evidence**:
  - `crates/nif/src/blocks/interpolator.rs:1372-1377` — six consecutive
    `stream.read_f32_le()?` with no `is_finite()` filter before they land on the struct.
  - `crates/nif/src/anim/bspline.rs:334-341` — `translation_keys.push(TranslationKey { value:
    zup_to_yup_pos(zup), … })` where `zup` comes directly from `deboor_cubic`; contrast the
    `else` arm at `:344-357`, which DOES gate on `is_flt_max`.
  - `crates/nif/src/anim/bspline.rs:396-403` — same for `ScaleKey`; contrast `:404-412`.
  - `crates/nif/src/anim/bspline.rs:241-245` — `keys.push(AnimFloatKey { value: p[0] })`,
    ungated, while the static fallback at `:176-182` returns `None` on the sentinel.
  - No downstream gate exists: grepping `is_finite` across the *crates/core/src/animation/* modules hits only
    `player.rs:72`, `player.rs:140`, `stack.rs:72` — all on *time*, none on sampled values.
    `byroredux/src/systems/animation.rs:706-715` and `:953-956` write
    `transform.translation = pos` unconditionally.
  - Reachability: this is not a Skyrim-only path — #1424 (CLOSED) established that
    `NiBSplineCompTransformInterpolator` is reachable on FO3/FNV too.
- **Impact**: A NaN `Transform.translation` propagates to `GlobalTransform`, then into
  `bone_world` (`byroredux/src/render/skinned.rs:180-182`) and the `GpuInstance` model matrix
  — i.e. into the bone-palette SSBO, the skinned-BLAS refit vertices, and the TLAS instance
  transform. A NaN AABB in an acceleration-structure build is undefined behaviour, and a NaN
  bone matrix silently vanishes the whole mesh. `pose_hash`
  (`byroredux/src/render/skinned.rs:231-234`) hashes NaN by bit pattern, so the pose stays
  permanently "dirty" and re-dispatches the skin compute + BLAS refit every frame — the leak
  is per-frame CPU/GPU work as well as UB. Trigger is corrupt or hostile file data (a mod
  archive), not vanilla content: on vanilla NIFs an unused channel carries an INVALID
  (`u32::MAX`) handle, so `channel_slice` returns `None` and the already-gated pose fallback
  runs instead. Rated MEDIUM to match this repo's settled severity for the identical defect
  class from corrupt NIF floats — #3529, #3048, #1534, #3132, #3432 are all MEDIUM.
- **Related**: #772 (the sentinel this path partially honours), #1443 (mainline stream
  sanitiser), #3432 (OPEN — `NiControllerSequence` `duration`/`weight` unsanitised past
  #3258; direct sibling, same layer), #3529 (CLOSED — NaN quad → NaN LocalBound → NaN BLAS
  vertices), #3048 (CLOSED — ±Inf vertex positions into the vertex buffer), #1424
  (B-splines reachable on FO3/FNV)
- **Suggested Fix**: Reject the block at the boundary rather than filtering per key: in
  `extract_transform_channel_bspline` / `extract_float_channel_bspline`, require the relevant
  `offset` + `half_range` pair to satisfy `is_key_value_sane` before building each
  `channel_slice`; a failing pair drops that channel to `None`, which already routes to the
  gated pose fallback. Belt-and-braces, run the pushed values through the existing
  `is_key_value_sane` (`crates/nif/src/anim/keys.rs:21`) at `:334`, `:397`, and `:242` so a
  future arithmetic overflow inside `deboor_cubic` cannot outrun the input check. Pin with a
  synthetic-fixture test alongside `crates/nif/src/anim/tests/sanitize.rs`.

---


### SAFE-D5-02: `sync.rs`'s image-fence-aliasing invariant note cites `draw.rs:180`; the guard now lives at `draw.rs:1746`
- **Severity**: LOW
- **Dimension**: 5 (Vulkan Spec Compliance — documentation of a sync invariant)
- **Location**: `crates/renderer/src/vulkan/sync.rs:102-104`
- **Status**: NEW
- **Description**:
  `FrameSync::images_in_flight`'s docstring is the single place the per-image fence-aliasing
  invariant is written down, and it anchors the guard by line number:

  > *"The aliasing guard `image_fence != in_flight[frame]` at draw.rs:180 then prevents waiting on
  > the just-reset fence belonging to the current frame slot."*

  The guard is real and **intact**, but it is at `crates/renderer/src/vulkan/context/draw.rs:1746`
  — `draw.rs:180` is unrelated code. `draw_frame` was split out of `context/mod.rs` into
  `context/draw.rs` (#2731 / session-34 layout), which invalidated the anchor.
- **Evidence**:
  - `sync.rs:102-104` — the stale citation, verbatim above.
  - `draw.rs:1746` — `if image_fence != vk::Fence::null() && image_fence != self.frame_sync.in_flight[frame] {`
    (guard confirmed present and correct; this is a PASS on the invariant itself).
  - `draw.rs:180` is not a fence site.
- **Impact**:
  Documentation only. The cost is real but bounded: `sync.rs:107-112` explicitly warns that *"if
  `draw_frame` ever drops to a single-slot fence wait, this guard becomes load-bearing"* — i.e. the
  next person to touch the fence-wait pair is told to go read a line that does not exist, in the
  one comment that explains why the guard cannot be removed.
- **Related**: #1200 (CLOSED — REN-DIM15-02, *"cites stale triangle.frag:1321 — use symbol, not
  line number"*), #2757 (CLOSED — same shape for a `helpers.rs` → `triangle.frag` anchor). Both
  established the "anchor by symbol, not line" convention this violates.
- **Suggested Fix**:
  Replace the line-number anchor with a symbol anchor, per the #1200 / #2757 convention:
  `"The aliasing guard `image_fence != in_flight[frame]` in `VulkanContext::draw_frame`
  (`context/draw.rs`, immediately after `acquire_next_image`) then prevents …"`.

---


### SAFE-D6-01: `GpuCamera`'s own `#[repr(C)]` doc and its size-pin test both still say 352 B (live: 368 B)
- **Severity**: LOW
- **Dimension**: 6 — R1 material table layout (GpuCamera size-pin sub-check)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:335`; `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:50-51`; `docs/engine/renderer.md:271,582-583`
- **Status**: NEW (the *in-code* sites; the `.md` / SKILL sites are Existing: #3447 / #3450)
- **Description**: `#3323` grew `GpuCamera` 352 → 368 B by appending `exterior_sky_tint: [f32; 4]`, and the size pin was correctly renamed and re-asserted (`gpu_camera_is_368_bytes`, asserts 368, passes). Three prose sites were left behind, two of them *inside the renderer crate* — including the headline of the struct's own doc comment and the doc comment of the pin test itself. `docs/engine/renderer.md:582-583` additionally names two tests that no longer exist under those names (`gpu_camera_is_352_bytes`, `gpu_instance_is_128_bytes_std430_compatible`; live: `gpu_camera_is_368_bytes`, `gpu_instance_is_160_bytes_std430_compatible`).
- **Evidence**:
  - `gpu_types.rs:335` — `/// GPU-side camera data (**352 bytes**, std140-compatible).` — immediately contradicted by its own next paragraph (`:336-340`), which correctly derives 368 B from eleven trailing `vec4` and names `gpu_camera_is_368_bytes`. `:337-342` also stops the size history at "336 → 352 B", omitting the 352 → 368 step.
  - `gpu_instance_layout_tests.rs:50-51` — `/// \`GpuCamera\` must stay 352 B — three \`mat4\` (192 B) plus ten` / `/// trailing 16-byte vectors (160 B)` — while the assertion five lines below (`:68-70`) asserts **368** and the message says "368 B (352 B + 16 B exterior_sky_tint vec4, #3323)". Ten vec4 is also wrong; there are eleven.
  - `docs/engine/renderer.md:271` — "the camera UBO (`GpuCamera`, 352 bytes)"; `:582-583` — "`gpu_instance_is_128_bytes_std430_compatible`, `gpu_camera_is_352_bytes` (the live 352-byte `GpuCamera` layout)".
- **Impact**: No runtime effect — the assertion, the `.spv` reflection pin (`camera_ubo_size_matches_gpu_camera_in_every_shader`) and all six re-declaring shaders are correct at 368 B. The cost is audit-integrity: this exact doc-rot pattern has now produced #1526, #1565, #1623, #1754, #3201, #3447 and #3450, because a reader (or an audit) that trusts the struct's own headline computes a 16-byte-short UBO and "finds" a phantom drift. Fixing the two in-crate sites removes the most authoritative wrong number.
- **Related**: #3323 (the growth), #3201 (CLOSED, the 336 → 352 instance of the same rot), #3447 (OPEN, `shader-pipeline.md` + `memory-budget.md`), #3450 (OPEN, two audit SKILL files), #1526 / #1565 / #1623 / #1754 (prior instances)
- **Suggested Fix**: In `gpu_types.rs:335` change `**352 bytes**` → `**368 bytes**` and extend the history line to `… then 352 → 368 B with exterior_sky_tint (#3323)`. In `gpu_instance_layout_tests.rs:50-51` change `must stay 352 B` → `must stay 368 B` and `ten trailing 16-byte vectors (160 B)` → `eleven trailing 16-byte vectors (176 B)`. In `docs/engine/renderer.md` update the size and both test names. Consider whether #3447 should be widened to cover `renderer.md` rather than filing separately.


### SAFE-D6-03: `GpuMaterial::as_bytes` / `hash_material_slice` safety docs cite a nonexistent `Hash` impl, nonexistent pad fields, and a stale line anchor
- **Severity**: LOW
- **Dimension**: 6 — R1 material table layout (dedup soundness sub-check)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:394-398`; `crates/renderer/src/vulkan/material.rs:729-733`; `crates/renderer/src/vulkan/material.rs:55-58`
- **Status**: Regression of #2491 (CLOSED — "`hash_material_slice` docstring cites a `GpuMaterial::Hash` impl that does not exist, with stale line anchors")
- **Description**: Three `unsafe`-adjacent doc comments describe a `GpuMaterial` that no longer exists in two respects. (a) They name "the byte-level `Hash`/`Eq` impls" — `GpuMaterial` has `impl PartialEq` (`material.rs:745`) and `impl Eq` (`:751`) and **no `Hash` impl at all**, derived or manual (`grep 'impl.*Hash for GpuMaterial\|derive(Hash' material.rs` → no match); hashing is the free function `hash_gpu_material_fields` (`:1014`), which walks named fields and never touches `as_bytes`. (b) They justify determinism by "explicit padding fields the producer always initialises" — `GpuMaterial` has **zero** pad fields today; all 108 members are live named scalars (108 × 4 == 432 == `size_of`), and the last `_pad_falloff` was repacked into `greyscale_lut_index` under #890. (c) `descriptors.rs:396` points the reader at "`GpuMaterial::as_bytes` doc at vulkan/material.rs:653-666" — that range is now the middle of the `MODEL_SPACE_NORMALS` flag documentation; `as_bytes` and its doc are at `:729-741`.
- **Evidence**:
  - `descriptors.rs:394-398` — `// SAFETY: \`GpuMaterial\` is \`#[repr(C)]\` with f32/u32 fields and` / `// explicit padding fields the producer always initialises (see` / `// \`GpuMaterial::as_bytes\` doc at vulkan/material.rs:653-666).`
  - `material.rs:729-733` — `/// Byte view used by the byte-level \`Hash\`/\`Eq\` impls below. … all padding bytes are named fields the producer always initialises`.
  - `material.rs:55-58` (struct doc) — `Pad explicitly with named pad fields so the byte-level \`Hash\`/\`Eq\` impls below are deterministic.`
  - reading lines 653-666 of `crates/renderer/src/vulkan/material.rs` → `MODEL_SPACE_NORMALS` / `#2826` prose, not `as_bytes`.
  - #2491 fixed this same pair of errors in the `hash_material_slice` **docstring**; the `SAFETY` comment two lines below it, and the two `material.rs` sites it points at, were not swept.
- **Impact**: Documentation only — the actual invariant (no padding holes → deterministic bytes → sound `as_bytes`/`PartialEq`/`hash_material_slice`) **holds**, and is now stronger than the comment claims. The cost is that the `unsafe` block's SAFETY justification rests on a premise ("explicit padding fields") that no longer describes the type, and a reader who follows the anchor to verify it lands on unrelated text. Per the severity table an `unsafe` block whose safety comment is wrong is a MEDIUM floor; rated LOW because the comment is *understated*, not *unsound* — the real reason (zero padding) is a strictly stronger guarantee than the one written.
- **Related**: #2491 (CLOSED, same function, same two errors), #890 (removed the last pad field), #2415 / #1566 (prior `GpuMaterial` doc-rot on field counts and sizes)
- **Suggested Fix**: In `descriptors.rs:394-398` drop the line-number anchor (name the item, not the line) and replace "explicit padding fields the producer always initialises" with "no padding bytes at all — every one of its members is a named 4-byte scalar, so `size_of` equals the sum of the fields". Apply the same correction at `material.rs:729-733` and `:56-59`, and replace "`Hash`/`Eq` impls" with "`PartialEq`/`Eq` impls (hashing goes through `hash_gpu_material_fields`, which walks named fields)". Consider adding a one-line `const _: () = assert!(size_of::<GpuMaterial>() == 108 * 4);`-style compile-time no-padding check so the SAFETY premise is machine-enforced rather than prose.

---

# Verified PASS — regression guards confirmed intact

Most of this skill's checklist is a set of regression guards. Per the shared
protocol, a guard confirmed intact is recorded as a PASS, not re-filed as a
finding. Each entry below was checked against current code with file:line
evidence; the full per-dimension evidence is reproduced here.

## Dimensions 1, 2, 11
### Dimension 1 — FFI lifetime safety
- **`crates/fsr3-sys` is the only live FFI crossing and its contracts are complete.**
  Both `unsafe fn`s carry `# Safety` sections stating a real lifetime contract:
  `Context::create` (`crates/fsr3-sys/src/lib.rs:363-379`) requires live, mutually
  compatible Vulkan handles that outlive the result **and** an idle device before
  `Drop`; `Context::dispatch` (`:399-408`) requires every handle to belong to the
  creating device and to remain live through command-buffer execution. No other
  `unsafe fn` / `unsafe impl` exists in the crate (`grep -n "unsafe fn|unsafe impl"`
  returns exactly those two).
- **Every `SAFETY` claim in `fsr3-sys` was checked against the native shim.**
  `Drop`'s comment (`lib.rs:481-497`) asserts the native side frees the wrapper and
  nulls `raw` on *every* path including a failed `ffxDestroyContext`; confirmed at
  `crates/fsr3-sys/native/byro_fsr3.cpp:281-302` (`delete wrapper; *context = nullptr;`
  unconditionally, `result` still returned). `Error::fmt`'s claim of a static
  NUL-terminated string for *every* code is backed by the `default:` arm at
  `byro_fsr3.cpp:313`. `Context::create`'s "the native shim copies `desc`" is backed by
  the parameter-validation + copy at `byro_fsr3.cpp:167-205`.
- **`Context` is correctly `!Send + !Sync`.** It wraps `NonNull<RawContext>`, and a
  workspace-wide `grep -rn "unsafe impl"` finds **no** `unsafe impl Send`/`Sync`
  anywhere (only three `AnyBitPattern` markers in `crates/nif`). Nothing smuggles the
  SDK context across threads.
- **Device-outlives-context is enforced structurally.** `VulkanContext::drop`
  (`crates/renderer/src/vulkan/context/teardown.rs:171-176`) calls `device_wait_idle`
  first, then retires the FSR SDK context at `:239-241` via
  `FrameUpscaler::destroy_device_objects` — deliberately *outside* the
  `Some(allocator)` guard (#2158) — and only reaches `self.device.destroy_device(None)`
  at `:385`. The resize path pays `device_wait_idle` at
  `crates/renderer/src/vulkan/context/resize.rs:37` before the single
  `upscaler.recreate(...)` call site at `:1023`, and the runtime upscaler switch pays
  its own at `:1236`.
- **The vendored-SDK control-flow assumption is pinned, not assumed.**
  `crates/fsr3-sys/src/lib.rs:626-744` (`vendored_sdk_contract_tests`) scans the
  vendored FidelityFX v1.1.4 sources and fails an SDK bump that introduces an error
  return at or after the first function that records GPU work — which is what makes
  `FrameUpscaler::record`'s dispatch-failure recovery sound.
- **Ruffle/wgpu boundary (`crates/ui`) is safe Rust end to end.**
  `SwfPlayer::render` (`crates/ui/src/player.rs:431-491`) returns
  `Option<&[u8]>` borrowed from its own owned `self.pixel_buffer`, so the borrow
  checker — not a comment — bounds the slice's lifetime; the consumer
  `TextureRegistry::update_rgba` (`crates/renderer/src/texture_registry.rs:1596-1643`)
  takes `pixels: &[u8]`, stages and uploads within the call, and lets nothing escape.
  There is no `unsafe` block anywhere in `crates/ui/src`.
- **No teardown-ordering coupling between Ruffle and `VulkanContext`.** `SwfPlayer`
  runs on its own wgpu instance/device created by `shared_descriptors`
  (`crates/ui/src/player.rs:94-108`), a process-wide `OnceLock<Arc<Descriptors>>`, not
  on the engine's `ash::Device`. It therefore cannot outlive-or-underlive
  `VulkanContext` in a way that touches a destroyed logical device.
- **cxx-bridge scope guard holds.** `crates/cxx-bridge/src/lib.rs` still exposes exactly
  one bridge fn, `native_hello() -> String`, with no `*const`, `&[u8]`, `Box<…>`, or
  reference-taking `unsafe extern "C++"` signature. Not a live FFI boundary; no
  speculative findings filed against it.

### Dimension 2 — memory corruption / UB
- **ECS cached-pointer contract (#35 + #1367) intact.** `World::get`
  (`crates/core/src/ecs/world.rs:358`) returns `ComponentRef<'_, T>`, not a raw pointer
  with a dropped guard. All four cached derefs (`crates/core/src/ecs/query.rs:64`,
  `:135`, `:143`, `:289`) carry SAFETY comments whose stated invariant matches the
  field layout: in each of `QueryRead`, `QueryWrite` and `ComponentRef` the lock guard
  is the **first** declared field and is `#[allow(dead_code)]` — never read directly —
  so the raw pointer derived from it in `new()` is never invalidated by a competing
  borrow, and drop order releases the lock only after the wrapper is gone. `&mut *self.storage`
  is gated on `&mut self` (`:139`). No refactor has let a guard drop before its pointer.
- **`crates/core` has exactly six `unsafe` blocks and all six are sound.** The two
  outside `query.rs` are `str::from_utf8_unchecked` over an ASCII-lowercased copy of a
  `&str` (`crates/core/src/string/mod.rs:177` — `make_ascii_lowercase` cannot break a
  multi-byte sequence) and a test-only `as_bytes_mut` writing ASCII `b'x'` (`:302`).
- **NIF bulk POD reads intact.** The single `unsafe` site is
  `crates/nif/src/stream.rs:790-822` (`read_pod_vec_from`), reached from both
  `NifStream::read_pod_vec` (`:438`) and `header::read_pod_vec_from_cursor`
  (`crates/nif/src/header.rs:367`). Both wrappers apply `checked_mul` overflow guards
  (`stream.rs:439`, `header.rs:371`) **and** an allocation bound (`check_alloc` at
  `stream.rs:383`, `check_header_alloc` at `header.rs:384`) before the raw read.
  `T` is sealed behind the `unsafe trait AnyBitPattern` (`stream.rs:47`); the
  `pod_marker_covers_every_instantiated_type` test (`stream.rs:836`) makes a new
  element type without an `unsafe impl` a compile error. Every marked type is
  `#[repr(C)]` and padding-free — verified individually for `NiPoint3`
  (`crates/nif/src/types.rs:15-21`, 3×f32) and for `BoneWeight` / `Meshlet` /
  `CullData` (`crates/nif/src/blocks/bs_geometry.rs:312-352`, 2×u16 / 4×u32 / 2×[f32;3]).
  `set_len` runs only after `read_exact` returns `Ok`.
- **`sfmaterial` enum decode is a checked match, not a transmute.**
  `BuiltinType::from_u32` (`crates/sfmaterial/src/types.rs:37-57`) matches the sixteen
  `0xFFFFFF##` tags with `_ => return Err(Error::UnsupportedBuiltin { raw })`, and
  `grep -rn transmute crates/sfmaterial/` returns nothing. The module doc (`:10-11`) now
  describes the checked match rather than the old aspirational "transmute" prose.
- **`pex` opcode transmute guard holds on both preconditions.**
  `crates/pex/src/opcode.rs:131` range-checks `byte >= MAX_OPCODE` before the
  `transmute` at `:136`, and the enum is genuinely contiguous: mechanically extracting
  the `#[repr(u8)] pub enum OpCode` body yields **51 variants with exactly one explicit
  discriminant (`Nop = 0`)**, i.e. `0..=50`, against `MAX_OPCODE = 51` (`:68`). No gap.
- **`crates/bsa` untrusted-input bounds are centralised and used.**
  `crates/bsa/src/safety.rs` still owns `MAX_ENTRY_COUNT` (10 M, `:26`),
  `MAX_CHUNK_BYTES` (1 GiB, `:37`), `checked_entry_count` / `checked_chunk_size` /
  `checked_chunk_size_usize`, and `inflate_bounded` (`:114-131`, `take(declared + 1)`
  with a short decode deliberately still `Ok`).
- **The LZ4 `safe-decode` pin survives.** `Cargo.toml:151-157` pins
  `lz4_flex = { version = "0.11", default-features = false, features = ["std",
  "safe-encode", "safe-decode", "frame", "checked-decode"] }` with the #3392 rationale
  in-line at `:140-150`; `cargo tree -p byroredux-bsa -e features` confirms
  `safe-decode` and `checked-decode` are actually resolved, and `crates/bsa` is still
  the sole dependent. The `catch_unwind` defence-in-depth is still at
  `crates/bsa/src/ba2.rs:793` and is itself source-scan-pinned at `:1796`.
- **#3391's byte-range `&str` fix holds, and the pattern sweep is clean.**
  `canonical_mesh_path` (`crates/nif/src/import/mesh/bs_geometry.rs:39-64`) tests both
  head and tail via `as_bytes()`. I swept every `&ident[a..b]` in `crates/nif`,
  `crates/bsa`, `crates/bgsm`, `crates/sfmaterial`, `byroredux/src/asset_provider` and
  `crates/plugin/src/esm` and checked each `&str` (not `&[u8]`) hit individually — all
  are boundary-safe by construction: `normalize_material_path` / `normalize_texture_path`
  (`byroredux/src/asset_provider/archive.rs:218`, `:284`) index past five verified ASCII
  bytes; `numeric_sibling_paths` (`:405`, `:407`, `:419`, `:430`) slices only at offsets
  proven ASCII by a `strip_suffix` on a byte-length-preserving `to_ascii_lowercase` or
  by an `is_ascii_digit` char check; `idle_animation_candidates`
  (`byroredux/src/asset_provider/animation.rs:140-143`) uses `event.get(..4)` +
  `eq_ignore_ascii_case` so `&event[4..]` is only reached on an ASCII prefix;
  `filename_stem_lower` (`crates/nif/src/kfm.rs:317-330`) slices at `rfind` boundaries.
- **No unbounded recursion in the walkers.** `MAX_GRUP_NESTING_DEPTH = 64`
  (`crates/plugin/src/esm/reader.rs:68`) is enforced centrally by
  `bounded_group_content_end` (`:859-877`), which every recursive GRUP walker routes
  through (`records/grup_walker.rs`, `cell/support.rs`, `cell/walkers.rs`,
  `cell/wrld.rs`). The three remaining raw `group_content_end` production sites
  (`records/mod.rs:276`, `cell/wrld.rs:28`, and the definition itself) are iterative
  top-level loops that delegate into the bounded walkers, not recursive frames. NIF
  shape resolution is capped by `MAX_COLLISION_SHAPE_DEPTH`
  (`crates/nif/src/import/collision/shape.rs:20`, bail at `:56-61`, #1385).

### Dimension 11 — sandboxed mod runtime trust boundary
- **Absence of WASI is real, not promised.** `Cargo.toml:108` pins
  `wasmtime = { version = "47.0.3", default-features = false, features = ["anyhow",
  "component-model", "cranelift", "runtime", "std"] }`.
  `cargo tree -p byroredux-mod-runtime | grep -i "wasi|cap-std|preview"` returns
  **nothing**, and no other crate in the workspace depends on `wasmtime` (so feature
  unification cannot re-enable it). Better still, the guarantee is enforced at runtime
  rather than by inspection: `wasi_imports_are_absent_by_default`
  (`crates/mod-runtime/src/tests.rs:249`) instantiates a component that imports WASI and
  asserts it fails, because the `Linker` has nothing to satisfy it with.
- **Capability gating is check-before-act and denial is an error, not a no-op.**
  `logging::Host::log` (`crates/mod-runtime/src/runtime.rs:334-340`) tests
  `self.grants.contains(LOG_CAPABILITY)` first and `wasmtime::bail!`s on a missing
  grant; with `imports: { default: trappable }` (`crates/mod-runtime/src/bindings.rs:6`)
  that becomes a guest trap, which `enter` turns into `InstanceStatus::Quarantined`.
  The other two host functions (`context::principal_id`, `context::has_capability`,
  `:392-399`) expose only the calling principal's own identity and grant set, which is
  not host authority. `CapabilityId`/`PrincipalId` are validated ASCII identifiers
  (`identity.rs:114-131`) — `"../escape"` and embedded spaces are rejected.
- **Per-instance isolation holds.** Each `ModInstance` owns its own
  `Store<HostState>` with its own `Principal`, `CapabilitySet`, `StoreLimits` and log
  buffer (`runtime.rs:170-196`). A workspace grep for `static`/`thread_local`/
  `OnceLock`/`lazy_static`/`Arc<` across `crates/mod-runtime/src/` finds only two
  `&'static str` error payloads — **no shared mutable state at all**. Pinned by
  `denied_host_call_quarantines_only_its_instance` (`tests.rs:167`).
- **Resource limits reject degenerate configs in both directions.**
  `SandboxConfig::validate` (`limits.rs:111-204`) enforces floors *and* ceilings on
  every field: `MAX_SANE_LIMIT` (1 GiB) for counts/sizes, `MAX_WASM_STACK_BYTES_CEILING`
  (1 MiB, `:47`, guarding wasmtime's documented host-*abort* on stack overrun), and
  `MAX_FUEL_PER_ENTRY` (1e12, `:58`). `every_resource_field_rejects_usize_max` is green.
  Fuel is re-armed per entry (`runtime.rs:285`) with `consume_fuel(true)` (`:123`), so
  exhaustion is a trap → `FaultInfo` → terminal `Quarantined`, never a hang — pinned by
  `fuel_exhaustion_quarantines_runaway_guest` (`tests.rs:197`).
- **The guest-controlled `logs()` `Vec` is capped, and the cap is drainable.**
  `max_log_entries` / `max_log_bytes` / `max_log_message_bytes` are enforced in
  `logging::Host::log` (`runtime.rs:341-375`) with `checked_add` on the byte total;
  `take_logs` (`:236-240`) returns budget so a well-behaved long-running mod is not
  quarantined for uptime (#3050), and a budget overrun is attributed as
  `FaultKind::LogBudgetExhausted` rather than being confused with a guest fault.
- **Lifecycle cannot be re-entered after a fault.** `initialize` requires
  `Ready` and `shutdown` requires `Active` (`runtime.rs:250-278`); `enter`'s failure
  path sets `Quarantined` before returning `Err` (`:288-299`), so a trapping guest is
  quarantined, not retried, and neither entry point accepts a `Quarantined` or
  `Stopped` instance. `shutdown` is not literally idempotent — a second call returns
  `SandboxError::InvalidLifecycle` — but that is an explicit rejection, not a
  double-teardown, so it is not a safety defect.
- **Hostile input at compile time is an error, not a panic.**
  `SandboxRuntime::compile` (`runtime.rs:143-154`) checks `max_component_bytes`
  *before* `Component::new` and maps every failure to `SandboxError::Compile`; pinned
  by `compile_rejects_hostile_input_without_panicking` (`tests.rs:384`),
  `compile_rejects_a_valid_core_module_that_is_not_a_component` (`:437`), and
  `component_byte_limit_is_checked_before_compilation` (`:450`).
- `cargo test -p byroredux-mod-runtime` → **23 passed, 0 failed.**

**Dimension 11 produced no findings.** The crate contains no `unsafe`, and every
guarantee its docstring claims is backed by a test rather than by convention. The
one gap it documents itself (`limits.rs:65-78`: `max_component_bytes` is the only
pre-emptive bound on Cranelift compilation cost, because wasmtime 47.0.3 exposes no
compile-time fuel or deadline) is recorded in-code with the reason it is not closed,
which is the correct disposition for a contract with no consumer yet.

**Dimension 1 produced no findings.**

---


## Dimensions 3, 10
### Dimension 3

**D3-1 — Rapier bodies on cell unload (regression guard #1520 / #1531 / #3380) — PASS**
- `byroredux/src/cell_loader/unload.rs:319-329` — the `#1520 DROP` comment block and the
  `release_victim_rapier_bodies(world, &victims)` call still sit *before*
  `world.despawn_batch(victims)` (`unload.rs:335`).
- `byroredux/src/cell_loader/unload.rs:564-596` — the sweep walks BOTH `RapierHandles`
  and `Ragdoll` component rows, then `pw.remove_body(h.body)` / `pw.remove_ragdoll(r)`
  under a single `PhysicsWorld` write lock.
- `crates/physics/src/world.rs:255-275` — `remove_body` passes `islands`, `colliders`,
  `impulse_joints`, `multibody_joints` and `remove_attached_colliders = true`, then sets
  `self.wake()` + `self.colliders_dirty = true` (#2863) so the broad-phase removal that
  rapier defers to `pipeline.step()` is not stranded by the static-scene fast path.
- `crates/physics/src/world.rs:529-535` — the fast-path early return still rebuilds
  `query_pipeline` when `colliders_dirty`; `world.rs:632-636` does the same on the stepped path.
  Query-pipeline residency is therefore derived state, correctly invalidated.
- `crates/physics/src/ragdoll.rs:569-573` — `remove_ragdoll` cascades every body through
  `remove_body` (which drags colliders + multibody joints out).
- Guard tests still assert emptiness:
  `byroredux/src/cell_loader/rapier_release_tests.rs:64-68` (`body_count() == 0`),
  `:83-87` (`colliders.len() == 0`),
  `:236-252` (ragdoll: bodies 0, colliders 0, `multibody_joints.get(h).is_none()`),
  `:296-315` / `:321-338` (idempotence over duplicated victim lists, #3380).
- No production site inserts into `impulse_joints` (grep: zero hits outside `world.rs`
  construction/removal), so the untested joint set is a non-path.

**D3-2 — Deferred-destroy drain — PASS (both the tick-after-fence ordering and the shutdown sweep)**
- Primitive: `crates/renderer/src/deferred_destroy.rs:47` `DEFAULT_COUNTDOWN = MAX_FRAMES_IN_FLIGHT`;
  `tick` (`:88-101`) and `drain` (`:109-113`).
- Production instantiations confirmed:
  - Mesh vertex/index buffers — `crates/renderer/src/mesh.rs:360` field doc, drained by
    `drain_deferred_destroy` at `mesh.rs:1854` (inside `destroy_all`).
  - BLAS entries — `crates/renderer/src/vulkan/acceleration/mod.rs:208`
    (`pending_destroy_blas: DeferredDestroyQueue<BlasEntry>`), pushed from
    `blas_static.rs:60` and `blas_static.rs:1094` (mid-batch eviction) and
    `blas_skinned.rs:733`.
  - BLAS scratch buffers — `acceleration/mod.rs:225`
    (`pending_destroy_scratch: DeferredDestroyQueue<GpuBuffer>`), pushed at
    `blas_static.rs:518` and `memory.rs:92` / `memory.rs:113`.
- **Tick runs AFTER the fence wait (#418)** — `crates/renderer/src/vulkan/context/draw.rs:1622-1637`
  is the dual-slot `wait_for_fences(&[in_flight[frame], in_flight[prev]], …)`; the tick block is at
  `draw.rs:1775-1800` (`texture_registry.begin_frame` then `mesh_registry` /
  `texture_registry` / `accel` `tick_deferred_destroy`). A source-shape pin exists at
  `draw.rs:4538-4554`.
- **Shutdown sweep drains (#732)** — three independent covers:
  `VulkanContext::flush_pending_destroys` (`context/mod.rs:1913-1937`: `device_wait_idle`
  → `accel.drain_pending_destroys` → `mesh_registry.drain_deferred_destroy` →
  `texture_registry.drain_pending_destroys`), called from `byroredux/src/app_events.rs:61`;
  `AccelerationManager::destroy` calls `drain_pending_destroys` itself
  (`acceleration/mod.rs:351`) and additionally drains `blas_entries`, all `tlas` slots
  (accel + buffer + both instance buffers), `skinned_blas` (#1138), `scratch_buffers` and
  `blas_scratch_buffer` (`mod.rs:352-403`); `MeshRegistry::destroy_all` drains at
  `mesh.rs:1854`; `TextureRegistry::destroy` drains at `texture_registry.rs:1819`.
- **Other free mechanisms are fence-safe:**
  - Textures use per-`TextureEntry` frame-id timestamps rather than countdowns
    (`deferred_destroy.rs:23-30`). `begin_frame` bumps `current_frame_id`
    (`texture_registry.rs:1434-1443`) and the expiry predicate is
    `should_destroy_pending(current, queued) = current.wrapping_sub(queued) >= MAX_FRAMES_IN_FLIGHT`
    (`texture_registry.rs:1953-1955`). Called from the post-fence block, so safe.
  - Skin/morph slots are freed *immediately* (`skinned_blas_refit.rs:263`, `:754`, `:816`),
    not deferred. That is sound here because `draw_frame` waits **both** in-flight fences
    (`draw.rs:1626-1636`, `MAX_FRAMES_IN_FLIGHT == 2`), so at `record_skinned_blas_refit`'s
    call site (`draw.rs:2600`) no submitted command buffer is still executing.
    Source-shape pins for this sweep live at `skinned_blas_refit.rs:995-1068`.

**D3-3 — AllocatorResource drop ordering (regression guard #1406 / #1477) — PASS, including panic-unwind**
- Orderly path: `byroredux/src/app_events.rs:64-67` removes
  `AllocatorResource` from the ECS World, then `app_events.rs:68` `self.renderer.take()`.
- Panic-unwind / any-other-exit path: `byroredux/src/main.rs:440-466` — `impl Drop for App`
  performs the same `remove_resource::<AllocatorResource>()` **then** `self.renderer.take()`,
  with the INVARIANT comment naming #1406 / #1477 / REG-08. `App` declares `renderer`
  before `world`, so this explicit body is load-bearing and is present.
- Re-insert on `resumed`: `app_events.rs:141-146` (`if let Some(ref alloc) = ctx.allocator`).
- Downstream guard intact: `crates/renderer/src/vulkan/context/teardown.rs:341-370` still
  does `allocator.take()` → `Arc::try_unwrap` → `into_inner()` drop *before*
  `destroy_device`, with the #665 / LIFE-L1 leak-the-handles-instead-of-UAF fallback on the
  `Err(arc)` arm.

**D3-4 — GPU allocation inventory — PASS**
- Every the *vulkan/* modules module that calls `create_image` / `create_buffer` / `create_image_view`
  has a matching `destroy` fn (scan: bloom, buffer, caustic, composite, compute, exposure,
  frame_upscaler, gbuffer, morph_compute, placeholder, restir, skin_compute, ssao, svgf,
  swapchain, taa, texture, volumetrics, water_caustic, water — all `destroy_fns >= 1`).
  `create_sampler`/`destroy_sampler` and `create_descriptor_pool`/`destroy_descriptor_pool`
  parity holds (destroy counts >= create counts, the excess being error-path cleanups).
- `VulkanContext::drop` covers every `Option<…>` subsystem field:
  `teardown.rs:28-166` (`texture_registry`, `scene_buffers`, `image_health_buffers`,
  `skin_slots`, `morph_slots` (#3231), `accel_manager`, `cluster_cull`, `skin_compute`,
  `ssao`, `placeholder_ao`, `placeholder_caustic_sink`, `exposure`,
  `frame_upscaler.destroy_allocations`, `composite`, `caustic`, `volumetrics`, `bloom`,
  `water_caustic_accum`, `svgf`, `reservoir_buffers`, `taa`, `gbuffer`) plus
  `teardown.rs:182-260` (`egui_pass`, `presentation`, `gpu_timers`, `skin_palette`, `water`,
  `frame_upscaler.destroy_device_objects`, screenshot/depth staging, `frame_sync`, fences,
  command pools, framebuffers) and `teardown.rs:296-338` (pipelines, layout,
  `mesh_registry.destroy_all`, pipeline cache, render pass, swapchain).
- FSR3 two-half teardown ordering is correct: `destroy_device_objects` (`teardown.rs:240`,
  allocator-independent block) runs *before* `destroy_allocations` (`teardown.rs:129`, reached
  via `destroy_allocator_owned_resources` at `teardown.rs:267`) — matching the contract in
  `frame_upscaler.rs:1051-1058`.
- **Recreate-on-resize paths all free the old allocation first**, checked individually:
  `caustic.rs:1189-1200` (drain slots → destroy view/image → `allocator.free`),
  `water_caustic.rs:424-429` + `destroy_slot` at `:446-457`,
  `restir.rs:136-140` (`buf.destroy` before re-push),
  `frame_upscaler.rs:1012-1024` (`self.destroy(device, allocator)` then `*self = Self::new(…)`),
  `gbuffer.rs:456-468` (all seven attachments destroyed before realloc),
  `svgf.rs:1449-1460`, plus `resize.rs:737-748` (bloom: `old_bloom.destroy` then `= None`),
  `resize.rs:789-797` (volumetrics: `take()` then `destroy`), `resize.rs:1238-1246` (TAA),
  `resize.rs:394-399` (SSAO), `resize.rs:337-342` (water).
- Eviction thresholds cross-checked (not re-derived) against `docs/engine/memory-budget.md`:
  `acceleration/constants.rs` — `BLAS_REBUILD_SLACK_BYTES` 16 MB, `TLAS_SCRATCH_SLACK_BYTES`
  256 KB, `TLAS_REBUILD_SLACK_BYTES` 1 MB, `MIN_TLAS_INSTANCE_RESERVE` / `WORKING_SET_FLOOR`
  8192, `MIN_BLAS_BUDGET_BYTES` 256 MB, `SKINNED_BLAS_REFIT_THRESHOLD` 600,
  `BATCH_EVICTION_CHECK_INTERVAL` 64. No drift found; the doc's per-subsystem tables
  (Instance/Indirect/Material SSBO, bone buffers, SVGF/TAA/caustic/ReSTIR/volumetric
  per-resolution rows) are pinned by their own `*_matches_documented_memory_budget` tests.

**D3-5 — CPU-side unbounded growth — PASS except SAFE-D3-01 below**
- `MaterialTable` dedup map: `byroredux/src/render/mod.rs:666` `material_table.clear();`
  is still the first statement of the per-frame build. Confirmed, moved on.
- `MaterialProvider` (`byroredux/src/asset_provider/material.rs`): `bgsm_cache` is a
  `TemplateCache::new(256)` (`:536`); `bgem_cache` + `bgem_cache_order` half-evict on
  overflow (`:463-471`, `:759-762`); `failed_paths` + `failed_paths_order` half-evict
  (`:474-477`, `:725-727`); `csg_cache` (`:523`) is keyed by plugin path — bounded by load
  order and discarded with the provider. Module-scope `sf_cdb_cache` is capped by
  `SF_CDB_CACHE_MAX_ENTRIES` with eviction at `material.rs:162-170` (#3054 fix intact).
- `NifImportRegistry` (`byroredux/src/cell_loader/nif_import_registry.rs:286-308`): default
  LRU cap **2048**, `BYRO_NIF_CACHE_MAX` override, `BYRO_NIF_CACHE_MAX=0` explicitly warns.
  Cap enforced in `insert` (`:471`+).
- `name_lookup.rs`: holds no map of its own — it borrows a caller's `HashMap` and has a
  bounded `MAX_CASE_FALLBACK_WARNINGS = 20` warn latch (`:29`, `:47-55`).
- `TextureRegistry`: refcounted, `drop_texture` / `drop_textures`
  (`texture_registry.rs:1283-1376`) purge `path_map` via `retain` on refcount → 0; cell
  unload drives it from `unload.rs:302-303`.
- `MeshRegistry`: refcounted, `drop_mesh` / `drop_meshes` (`mesh.rs:997-1024`) purge
  `mesh_cache` via `retain`; `MAX_MESH_SLOTS = 1 << 24` slot guard at `mesh.rs:572` / `:778` / `:871`.
- `App::ui_reported_host_methods` is capped by
  `byroredux_ui::MAX_DISTINCT_HOST_METHOD_NAMES` with a one-shot latch (`main.rs:230-238`).
- `morph_slots` has the `pending_morph_unload_victims` sibling of the skin-slot drain
  (`context/mod.rs:1598-1607`, `unload.rs:291-296`, eviction at
  `skinned_blas_refit.rs:802-818`) — #3231 wiring intact.

### Dimension 10

**D10-1 — `DebugUiState` is CPU-only — PASS**
- `crates/debug-ui/src/lib.rs:136-159`: fields are `visible: bool`, `egui_ctx: egui::Context`,
  `egui_winit: egui_winit::State`, `last_output: Option<egui::FullOutput>`,
  `panels: PanelState`, `game_menu: GameMenuState`, `player_message: Option<(String, Instant)>`.
  No `vk::` / `ash::` type appears anywhere in the file (grep on `vk::|ash::` returns only
  prose mentions at `:4` and `:191`). `impl Resource for DebugUiState {}` at `:179`.
- Its one bounded collection, `PanelState::console_history`, is capped at
  `CONSOLE_HISTORY_CAP = 200` (`:177`) and the cap **is** enforced (`:217-219`).

**D10-2 — `EguiPass` Vulkan resources freed before the device — PASS**
- Ownership: `VulkanContext::egui_pass: Option<EguiPass>` (`context/mod.rs:1853`);
  `EguiPass` owns `render_pass: vk::RenderPass` + `framebuffers: Vec<vk::Framebuffer>` +
  the `egui_ash_renderer::Renderer` (its own descriptor pool / images / buffers) and takes the
  shared `Arc<Mutex<Allocator>>` (`egui_pass.rs:53-89`, `:90-97`).
- `teardown.rs:182-184`: `if let Some(mut pass) = self.egui_pass.take() { pass.destroy(&self.device); }`
  — this frees framebuffers + render pass, and `pass` (hence the `Renderer` and its
  allocator `Arc` clone) drops at the end of that `if let`. That is at line 182, i.e.
  **before** `destroy_allocator_owned_resources` (`teardown.rs:267`), before
  `self.allocator.take()` + `Arc::try_unwrap` (`teardown.rs:341-345`), and before
  `destroy_device` at the bottom. The allocator-before-device rule holds.
- `EguiPass::destroy` (`egui_pass.rs:302-336`) also flushes the last frame's `pending_free`
  through `renderer.free_textures` while the device is still alive (#1427).
- Constructor error paths hand back the raw render pass and the framebuffers rather than
  leaking them (`egui_pass.rs:99-153`) — #2685 fix intact.
- Resize: `resize.rs:905-962`. Format-stable arm calls `recreate_framebuffers`
  (`egui_pass.rs:186-204`, which drains + destroys the old framebuffers before creating new
  ones) and, on failure, `pass.destroy(&self.device)` instead of `?`-ing through with the
  pass already `take()`n (#2685). Format-change arm destroys and rebuilds via `EguiPass::new`
  (#2475). Both arms are pinned by source-shape tests at `resize.rs:1660-1735`.

**D10-3 — one-frame deferred texture free survives — PASS**
- `egui_pass.rs:83-88` field doc; `dispatch` frees the *previous* frame's list first
  (`:231-237`) and only stashes this frame's `textures_delta.free` at the very end (`:295`).
- The safety anchor is real: `dispatch` is invoked from `draw.rs:3729-3747`, ~2100 lines after
  the dual-slot `wait_for_fences` at `draw.rs:1626-1636`.
- Skipped-frame accumulation (#2247) is intact: `submit_egui_frame`
  (`context/mod.rs:2387-2395`) merges via `merge_egui_pending_output`
  (`:2401-2413`, `older.append(output)`) instead of overwriting, so a dropped frame's
  `textures_delta.free` is not silently discarded. Pinned by
  `context/mod.rs:2438-2450`.

**D10-4 — allocator/queue mutex hold minimised during egui texture upload (CONC-D1-01 / #1713) — PASS**
- `EguiDispatchCtx::queue: &'a Mutex<vk::Queue>` (`egui_pass.rs:47`) — passed as the mutex, not
  a bare handle.
- `egui_pass.rs:245-252`: the lock is acquired inside an
  `if !output.textures_delta.set.is_empty()` block that contains **only** the
  `set_textures` call; the guard `q` drops at the end of that block.
  `tessellate` (`:256`) and the `cmd_begin_render_pass` / `cmd_draw` / `cmd_end_render_pass`
  sequence (`:261-292`) run with the queue released. No widening.
- The allocator mutex itself is taken inside `egui-ash-renderer` for the duration of its own
  allocate/free calls only; ByroRedux never holds it across the egui dispatch.

---


## Dimensions 4, 5
### Environment / validation-layer feasibility

A Vulkan ICD **is** present and `vulkaninfo` works on this box:

- `/usr/share/vulkan/icd.d/` → `nvidia_icd.json`, `radeon_icd.json`, `intel_icd.json`, `lvp_icd.json`, …
- `vulkaninfo --summary` → Instance 1.4.341; **GPU0 = NVIDIA GeForce RTX 4070 Ti** (driver 580.173.02),
  GPU1 = RADV RAPHAEL_MENDOCINO iGPU, plus llvmpipe.
- `VK_LAYER_KHRONOS_validation 1.4.341` is installed and enumerable.

So a validation-layer run **is** technically possible on this machine. It was **not** attempted:
the project rule forbids spawning a parallel/headless `byroredux` while the user may have their own
instance running. Everything below is therefore static analysis plus `cargo test`, except where a
finding is explicitly flagged as needing a validation-layer run to confirm the *runtime* symptom
(the *static* premise of SAFE-D5-01 is proven from the shipped SPIR-V binaries, not inferred).

`spirv-dis` / `spirv-val` are also present at `/usr/bin/` and were used indirectly — the capability
dumps below come from decoding the `OpCapability` (opcode 17) stream of the committed `.spv` files
directly.

---

### Verified PASS

**Dimension 4**

- **Every `unsafe {` block in the workspace carries a safety justification — 724 / 724.**
  Scanned every `.rs` file under *crates/*, every `.rs` file under *byroredux/src/*, *tools/* (excluding `target/`).
  A deliberately strict pass (SAFETY only in the *contiguous* comment block immediately above the
  `unsafe {` line, or the *contiguous* comment lines immediately inside it — no enclosing-fn
  fallback) returned **9** candidates; reading all 9 showed every one is a false positive, where
  the SAFETY comment sits one line higher, above the `let x: T =` binding whose RHS is the
  `unsafe` block:
  `byroredux/src/cell_loader/unload.rs:377` (comment at :371),
  `crates/nif/src/stream.rs:813` (:797), `crates/renderer/src/texture_registry.rs:424` (:420),
  `crates/renderer/src/vulkan/scene_buffer/descriptors.rs:277` (:273), `:400` (:394), `:424` (:420),
  `:437` (:435), `:461` (:458), `:487` (:484),
  `crates/fsr3-sys/examples/vulkan_context_smoke.rs:166`.
  → **No batched MEDIUM to file.** Per the brief, the retired #2692 "SAFETY-vs-unsafe count gap"
  work item was not re-litigated.
- **All 80 `unsafe fn` declarations carry a `# Safety` doc section** — 0 missing.
  #2684 (`SAFE-D4-03`, six undocumented `unsafe fn`) and #2544 (fsr3-sys example) are **intact**,
  including `crates/fsr3-sys/examples/vulkan_context_smoke.rs`.
- **#2683 / SAFE-D4-01 regression guard INTACT.** `debug_assert_flush_range_bounded`
  (`crates/renderer/src/vulkan/buffer.rs:579-594`) + the pure `flush_range_within`
  (`:601-608`) still exist and are called from all three flush sites
  (`flush_if_needed:1077`, `invalidate_if_needed:1131`, `write_mapped:1196`, `flush_range:1244`).
  The three SAFETY comments now correctly describe the *superset* semantics of
  `aligned_flush_range` rather than the false "contained in the allocation" claim the issue
  retired. `NON_COHERENT_ATOM_SIZE = 256` (`:531`) with a `debug_assert` in
  `device::create_logical_device` pinning it. `crates/renderer/src/vulkan/buffer.rs:1947`
  `flush_range_within_accepts_contained_and_rejects_overshoot` and `:1972`
  `non_coherent_atom_size_is_power_of_two` cover it in `cargo test`.
- **Mapped-memory write invariants hold at every site checked** (all `copy_nonoverlapping` /
  `from_raw_parts` over mapped GPU memory in `scene_buffer/upload.rs`):
  - `upload_lights` (`upload.rs:85-104`): SAFETY claims `count` clamped to `MAX_LIGHTS` and the
    buffer sized `LightHeader + MAX_LIGHTS*GpuLight` — **true**: clamp at `:28`, allocation
    formula at `buffers.rs:461-462`.
  - `upload_bone_worlds` (`upload.rs:246-255`): clamp `min(MAX_TOTAL_BONES)` at `:237`, buffer
    sized `size_of::<[[f32;4];4]>() * MAX_TOTAL_BONES` at `buffers.rs:466` — **true**.
  - `upload_bind_inverses` (`upload.rs:347-356`): claims
    `bytes_this_mesh ≤ slot_byte_stride` and `offset + stride ≤ staging size` — **true**:
    `min(MAX_BONES_PER_MESH)` at `:345`, `capped ≤ MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME`
    at `:336`, staging sized `MAX_PENDING(1366) × MAX_BONES_PER_MESH × 64 B` at
    `buffers.rs:616-619`.
  - `seed_persistent_bind_inverses_identity` (`upload.rs:505-523`): 9216 B < the 65536 B
    `vkCmdUpdateBuffer` limit — **true** (144 × 64).
- **`crates/nif/src/stream.rs:790-822` `read_pod_vec_from`** — the `slice::from_raw_parts_mut` over
  `Vec::with_capacity` + `set_len`-after-`read_exact` reasoning is correct, and the
  `debug_assert_eq!(byte_count, count * size_of::<T>())` is not load-bearing in release: both
  callers (`NifStream::read_pod_vec:439` and `header.rs::read_pod_vec_from_cursor:371`) derive
  `byte_count` from `count.checked_mul(size_of::<T>())`, so the equality is structural. `T:
  AnyBitPattern` + the `target_endian = "big"` compile gate cover validity.
- **`crates/pex/src/opcode.rs:130-137` `transmute::<u8, OpCode>`** — the stated invariant
  ("`#[repr(u8)]`, contiguous discriminants `0..MAX_OPCODE`") is **true**: 51 variants, only
  `Nop = 0` given an explicit discriminant, `#[repr(u8)]` present, `MAX_OPCODE = 51`,
  `OPCODES` table length 51, and `opcode.rs:165-173` tests it.
- **`crates/renderer/src/mesh.rs:1867-1884`** `vertex_slice_bytes` / `index_slice_bytes` — `Vertex`
  is 104 B (20 f32 + 4 u32 + 8 u8, align 4) with no implicit padding, as the comment claims.
- Push-constant byte casts all pin their size with a test:
  `presentation.rs:572-579` (`size == 128` asserted at `:822`), `skin_compute.rs:685` / `:1032`
  (`push_constants_size_is_12_bytes` / `skin_palette_push_constants_size_is_4_bytes`),
  `svgf.rs:1351`, `water.rs:700`, `material.rs:734-742` (`GpuMaterial::as_bytes`).
- **`shrink_blas_scratch_to_fit` (`acceleration/memory.rs:92-138`)** — the SAFETY comment at
  `byroredux/src/cell_loader/unload.rs:371-375` claims retirement is deferred by frames-in-flight
  per #1782; **true**: both the early-return path (`memory.rs:92`) and the realloc path (`:113`)
  push the old buffer into `pending_destroy_scratch` with `DEFAULT_COUNTDOWN`.
- High-risk-pattern sweep across the whole workspace: `align_to`, `MaybeUninit`, `assume_init`,
  `get_unchecked`, `Box::from_raw`, `Box::into_raw`, `static mut`, `write_unaligned` — **zero
  occurrences**. `transmute` → 1 real site (pex, verified above). `set_len` → 1 real site (nif,
  verified above). `read_unaligned` → 2 sites, both correctly justified by the mapped slice's
  lack of typed alignment (`scene_buffer/descriptors.rs:249`, `:277`).

**Dimension 5**

- **SPIR-V reflection test PASSES.**
  `cargo test -p byroredux-renderer scene_descriptor_reflection` → **5 passed, 0 failed**
  (`rt_enabled_layout_matches_triangle_shaders`, `rt_disabled_layout_matches_triangle_shaders_with_optional_tlas`,
  `rt_enabled_layout_matches_water_shaders`, `rt_disabled_layout_matches_water_shaders_with_optional_tlas`,
  `dropping_instance_binding_fails_with_diagnostic`). The Rust descriptor layout matches the
  shader-declared bindings in both RT-enabled and RT-disabled permutations.
- **#1390 TLAS-resize `device_wait_idle` regression guard INTACT.**
  `crates/renderer/src/vulkan/acceleration/tlas.rs:984-989`:
  `let retiring_old = …; if retiring_old { let _ = device.device_wait_idle(); }` sits **before**
  the `self.tlas[frame_index].take()` destroy at `:990`, with the invariant note at `:974-983`.
- **TLAS UPDATE geometry/instance-count parity holds.**
  `predicates.rs:188-229 decide_use_update` returns `false` on an empty current list (`:215`) and
  requires `cached_addresses.len() == current_addresses.len()` plus element-wise address equality
  (`:223-227`). `tlas.rs:127-129` adds a second belt-and-braces gate
  (`use_update && instance_count != tlas.built_primitive_count → use_update = false`), and the
  UPDATE arm at `tlas.rs:316-344` `debug_assert_eq!`s the two counts before submitting
  `range_primitive_count = tlas.built_primitive_count`. `built_primitive_count` is assigned only
  in the BUILD arm (`:348`) and reset to 0 with `needs_full_rebuild: true` on resize
  (`:1032`, `:1041`).
- **Skinned-BLAS refit parity holds.** `blas_skinned.rs:467-509` validates *both* halves of
  VUID-…-pInfos-03667 before the UPDATE: `validate_refit_flags(built_flags, SKINNED_BLAS_FLAGS)`
  and `validate_refit_counts(built_vertex_count, built_index_count, vertex_count, index_count)`;
  a mismatch **drops the entry and forces a fresh BUILD next frame** (`:502-509`) rather than
  submitting an illegal refit. Both predicates are pure and unit-tested in
  `acceleration/tests/predicates_tests.rs`. (A bone-count change does not alter vertex/index
  counts or topology, so it does not by itself invalidate an UPDATE; the geometry data pointer
  may legally change across an UPDATE.)
- **All AS build-input buffers carry the required usage flags.**
  Vertex/index: `mesh.rs:904-905`, `:1208-1209`, `:1395-1396` all add
  `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR | SHADER_DEVICE_ADDRESS` (behind an
  `rt_usage` gate). Skinned output verts: `skin_compute.rs:503-505` same trio.
  AS storage buffers: `blas_static.rs:446`, `:734`, `blas_skinned.rs:154`, `tlas.rs:897`, `:951`
  → `ACCELERATION_STRUCTURE_STORAGE_KHR | SHADER_DEVICE_ADDRESS`.
  TLAS instance buffer: `tlas.rs:834-836` →
  `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR | SHADER_DEVICE_ADDRESS | …`.
  Scratch: `memory.rs:119`, `:363`, `blas_static.rs:524`, `blas_skinned.rs:236` →
  `STORAGE_BUFFER | SHADER_DEVICE_ADDRESS`.
- **Drop ordering destroys children before parents.**
  `crates/renderer/src/vulkan/context/teardown.rs:171-390`: `device_wait_idle()` first (`:176`),
  then per-pipeline `destroy_device_objects`, screenshot/depth-capture staging, `frame_sync`,
  transfer fence + both command pools, framebuffers, allocator-owned resources, depth +
  depth-history, render-pass pipelines + layout, mesh registry, pipeline cache, render pass,
  `swapchain_state.destroy` (`:340`), allocator `Arc::try_unwrap` (`:346-384`),
  `destroy_device` (`:385`), `destroy_surface` (`:386`), debug messenger (`:388`),
  `destroy_instance` (`:390`). Surface-after-device is legal (surface is instance-level; all
  swapchains created from it were destroyed at `:340`). The `Arc::try_unwrap` failure arm
  deliberately leaks and `return`s rather than destroying the device under live allocations.
- **`vkCreate*` / `vkDestroy*` pairing** — a mechanical count over `crates/renderer/src`
  (comment lines stripped) finds a destroy counterpart for every created object kind
  (`acceleration_structure`, `buffer`, `command_pool`, `debug_utils_messenger`, `descriptor_pool`,
  `descriptor_set_layout`, `device`, `fence`, `framebuffer`, `image`, `image_view`, `instance`,
  `pipeline_cache`, `pipeline_layout`, `query_pool`, `render_pass`, `sampler`, `semaphore`,
  `shader_module`, `swapchain`, `surface`). `create_graphics_pipelines` /
  `create_compute_pipelines` map onto the 25 `destroy_pipeline` calls.
- **Queue submission ordering is correct and per-image.**
  `sync.rs:53-112` documents (and implements) `image_available` **per frame-in-flight** and
  `render_finished` **per swapchain image** — the shape that avoids
  VUID-vkQueueSubmit-pSignalSemaphores-00067 under MAILBOX discard.
  `draw.rs`: both-slots `wait_for_fences` (`:1628`) → `acquire_next_image` into
  `image_available[frame]` (`:1720-1723`) → **image-fence aliasing guard INTACT** at `:1746`
  (`image_fence != vk::Fence::null() && image_fence != in_flight[frame]`) → `images_in_flight[img]`
  stamped (`:1761`) → `reset_fences` immediately before submit (`:3811`, the #952 placement) →
  `queue_submit` waiting `image_available[frame]` (`:3778`) and signalling
  `render_finished[img]` (`:3792`) with fence `in_flight[frame]` (`:3841`) →
  `queue_present` waiting `render_finished[img]` (`:3952`). Wait-before-signal holds; queue
  access is serialized by the `graphics_queue`/`present_queue` mutexes
  (VUID-vkQueueSubmit-queue-00893).
- **`VOLUMETRIC_OUTPUT_CONSUMED` read, not assumed.** `volumetrics.rs:546` is
  `pub const VOLUMETRIC_OUTPUT_CONSUMED: bool = true;` and `context/post_passes.rs:506` gates
  the whole volumetrics record block (`if let Some(ref mut vol) = self.volumetrics { … vol …}`)
  on it. The live pass is correctly wired.
- **`initialize_layouts` covers every mip / FIF slot, and every owner is called from both the
  init and the resize path.** All 7 implementations iterate their complete owned image set:
  `bloom.rs:496-503` (`MAX_FRAMES_IN_FLIGHT × (BLOOM_MIP_COUNT + BLOOM_MIP_COUNT-1)`, every
  `down_mips` + `up_mips` of every frame), `svgf.rs:1127-1134` (`indirect_history` +
  `moments_history` + `atrous_color` — the complete owned set),
  `taa.rs:645-647` (all `history`), `water_caustic.rs:245-247` (all `slots`),
  `caustic.rs:818-830` (all `slots`), `gbuffer.rs:403-418` (7 attachments × every image),
  `volumetrics.rs:1845-1860` (lighting + integrated + emission-history + 3 combustion volumes,
  plus the two noise volumes). Callers: init (`init.rs:795, 957, 994, 1059, 1142, 1212, 1320`)
  and resize (`resize.rs:507, 673, 762, 814, 1363`, plus the self-calls at
  `svgf.rs:1556`, `caustic.rs:1242`, `taa.rs:883`). No missed slot found.
- **CLEAR-before-COMPUTE invariant holds.** `caustic_splat.comp` is the only shader using
  `imageAtomicAdd` (2 sites) and its R32_UINT accumulator gets either an exponential decay or a
  full `cmd_clear_color_image` with pre/post barriers every frame
  (`caustic.rs:898-1043`), plus a one-shot `clear_for_skip` (`:1101-1145`).
  `volumetrics_inject.comp` uses `imageStore` only (0 `imageAtomicAdd`), fully overwriting each
  froxel, so no per-frame clear is required; the one-time zero-init at
  `volumetrics.rs:1894-1931` covers first-sight reads.

**Dimension 4 produced 1 finding (MEDIUM). Dimension 5 produced 2 findings (1 HIGH, 1 LOW).**

---

## Dimensions 6, 7
### Dimension 6 — R1 material table layout

| Guard | Evidence | Verdict |
|---|---|---|
| Size pin exists, name matches assertion, matches reality | `crates/renderer/src/vulkan/material.rs:1494` — `fn gpu_material_size_is_432_bytes()` asserts `size_of::<GpuMaterial>() == 432`. Name says 432, assertion says 432, test **passes**. Struct has exactly 108 fields × 4 B = 432 B. | PASS |
| Per-field offset pin (#806) covers EVERY field | `crates/renderer/src/vulkan/material.rs:1727-1900`. Mechanically diffed: 108 field names parsed from `pub struct GpuMaterial` vs 108 distinct `offset_of!(GpuMaterial, …)` assertions — **set difference empty in both directions**. Last assertion `back_lighting_map_index == 428` → 432 total. | PASS |
| All fields flat scalar f32/u32, no `[f32; 3]` | No `[f32; N]` of any kind in `GpuMaterial`. BGSM translucency suite (`translucency_subsurface_r/g/b:260/264/268`, `_transmissive_scale:272`, `_turbulence:276`) and Disney lobe (`ior:280`, `subsurface:284`, `sheen:288`, `sheen_tint:292`, `anisotropic:296`) are all bare `f32` at 4-byte offsets. `material.rs:1500` also pins `align_of == 4`. | PASS |
| No padding holes; `Default` zeroes every scalar | `impl Default for GpuMaterial` (`material.rs:380`) sets **all 108** fields explicitly (mechanical diff: no field missing, none extra). `GpuMaterial` has **zero** anonymous pad bytes — 108 × 4 == `size_of` — so `as_bytes` is fully determined. Non-zero defaults are deliberate live values (`roughness 0.5`, `ior DEFAULT_DIELECTRIC_IOR`, `fresnel_power 5.0`, `glass_blur_scale 0.4`, …), not uninit holes. | PASS |
| Byte-Hash/Eq dedup is sound | `material.rs:729-751`: `as_bytes()` → `slice::from_raw_parts(self, size_of)`, consumed by `impl PartialEq` + `impl Eq`. Dedup key is `hash_gpu_material_fields` (`material.rs:1014`), which walks **all 108** fields (mechanical diff vs struct: empty both ways). Debug builds re-construct and `debug_assert!` byte-equality on every hash hit (`material.rs:1341-1352`), so producer-hash drift panics rather than miscolouring. Slot 0 is pre-seeded with `GpuMaterial::default()` by `seed_neutral_default` (`material.rs:1262-1266`), called from both `new()` and `clear()` → default materials always dedup to slot 0. | PASS |
| `to_gpu_material` populates every field | `crates/renderer/src/vulkan/context/mod.rs:421`. All 108 fields written explicitly; **zero** `..Default::default()` in the body, so no field can silently inherit a stale default and split the dedup key. | PASS |
| Intern cap (#797) | `material.rs:1360-1373`: `if self.materials.len() >= MAX_MATERIALS { overflow_count += 1; INTERN_OVERFLOW_WARNED.call_once(warn); return 0; }` — one-shot `Once`-gated warn, over-cap returns id 0, which resolves to the neutral seeded slot. Cap is `>=`, so max `len()` is exactly `MAX_MATERIALS`; highest reachable id is `MAX_MATERIALS - 1`. **No off-by-one.** | PASS |
| Upload truncation in lockstep with the same constant | `crates/renderer/src/vulkan/scene_buffer/upload.rs:651-657`: hard `assert!(materials.len() <= MAX_MATERIALS, …)` (stronger than the `debug_assert` the brief expected) **plus** `let count = materials.len().min(MAX_MATERIALS);`. Both read `MAX_MATERIALS` from `scene_buffer/constants.rs:192` (`= 16384`), the same import `material.rs:29` uses. Buffer is sized `size_of::<GpuMaterial>() * MAX_MATERIALS` (`buffers.rs:472-473`). Three sites, one constant. | PASS |
| `GpuInstance.material_id` guaranteed in range (SSBO index, CRITICAL class) | Every producer traced: `byroredux/src/render/static_meshes.rs:888`, `byroredux/src/render/particles.rs:308` both assign from `material_table.intern_by_hash(...)` (capped, above). The only literal assignments are `material_id: 0` (`crates/renderer/src/vulkan/water.rs:1239`, `context/mod.rs:2591`, `context/draw.rs:4425`, `render/static_meshes.rs:854`, `render/particles.rs:265` — all `DrawCommand` initialisers overwritten immediately after, or the UI/water quad which legitimately wants slot 0). `draw.rs:3071` copies `draw_cmd.material_id` verbatim. **No path can emit an id ≥ table len.** | PASS |
| GLSL mirror `struct GpuMaterial`, field by field | `crates/renderer/shaders/include/bindings.glsl:109-242` is the sole declaration; `triangle.frag` `#include`s it. Pinned three ways, all green: `gpu_material_glsl_field_order_matches_rust_struct` (`shader_contract_tests.rs:1383`) asserts **count + order + per-field scalar TYPE** (`f32↔float`, `u32↔uint`, #2688) against the Rust struct; `gpu_material_glsl_field_names_pinned` asserts the GLSL names; `gpu_material_field_offsets_match_shader_contract` asserts the Rust offsets. | PASS |
| `ui.vert` / `triangle.vert` / `water.vert` / `caustic_splat.comp` GpuInstance mirrors (#785 regression class) | All five declarations dumped and compared by hand — **byte-identical field lists in identical order**, through `morphDeltaAddress`/`morphWeightAddress`/`morphTargetCount`/`_reserved2a-c` (#3231). `ui.vert:` mirror is current and does **not** read the material table; `ui_vert_reads_texture_index_from_instance_not_material_table` and `gpu_instance_glsl_copies_stay_in_lockstep` (`shader_contract_tests.rs:1750`, five-way + Rust leg, #2748) both pass. **#785 is NOT regressed.** | PASS |
| `GpuInstance` size pin + no vec3 | `gpu_instance_layout_tests.rs:30` — `gpu_instance_is_160_bytes_std430_compatible`, passes. Padding is three separate `u32` (`_reserved2a/b/c`, `gpu_types.rs:201-204`) precisely to avoid the `uvec3` std430 footgun; `Default` zeroes all of them. `u64` members land on 8-aligned offsets 112/128/136. | PASS |
| `GpuCamera` size pin + no vec3 | `gpu_instance_layout_tests.rs:66` — `gpu_camera_is_368_bytes` asserts 368 and passes; struct is 3×`mat4` + 11×`[f32;4]`. `.spv`-level reflection pin `camera_ubo_size_matches_gpu_camera_in_every_shader` (`vulkan/reflect.rs`) also passes. No `[f32; 3]` anywhere in `gpu_types.rs`. | PASS (see SAFE-D6-01 for doc rot only) |
| `GpuLight` vec3 rule + GLSL mirror currently correct | `gpu_types.rs:270-281`: four `[f32; 4]`, no vec3. `gpu_light_glsl_copies_stay_in_lockstep` (`shader_contract_tests.rs:1681`, four GLSL copies) passes, and the four GLSL declarations match the Rust field list **today**. | PASS on current state (see SAFE-D6-02 for the missing guard) |

### Dimension 7 — RT IOR / glass refraction guards

| Guard | Evidence | Verdict |
|---|---|---|
| Glass-passthrough identity keyed on `materialKind` (#789, since `a09d2b76`) | `crates/renderer/shaders/triangle.frag:2027-2029` — `bool hitIsGlass = (materials[hInst.materialId].materialKind == MATERIAL_KIND_GLASS);`. The old texture-equality test is gone; `triangle.frag:1939-1947` carries an explicit "do not reintroduce texture keying" note (#2692). Terminus re-test at `:2161-2165` uses the same kind check. | PASS |
| Compile-time unbounded-recursion cap is the loop bound | `triangle.frag:1951` — `const int MAX_REFRACT_PASSTHRUS = 8;` and `:1994` — `for (int passthru = 0; passthru <= MAX_REFRACT_PASSTHRUS; ++passthru)`. It **is** the loop bound, not a decorative constant. Pinned by `shader_constants.rs:1416`. | PASS |
| Adaptive 2/4/6/8 budget keyed on quality tier (NOT a fixed 2) | `triangle.frag:1793-1796` — `int refractPassthruBudget = 2; … uint budgetTier = min(rayBudget.qualityTier, 3u); refractPassthruBudget = 2 + int(budgetTier) * 2;` → **2 / 4 / 6 / 8** at tiers 0-3. Early exit at `:1994-1996` (`if (passthru > refractPassthruBudget) break;`) and the continue-gate at `:2052` (`passthru < refractPassthruBudget`). Pinned by `shader_constants.rs:1414`. | PASS |
| The `REFRACT_PASSTHRU_BUDGET = 2` phantom stays dead (#3052) | `crates/renderer/src/shader_constants.rs:1424-1427` — `assert!(!src.contains("REFRACT_PASSTHRU_BUDGET = 2"), …)`; the enclosing test `triangle_frag_scales_glass_interface_depth_with_honest_ray_cost` (`:1411`) **passes**. Tree-wide `grep` finds the string only in that assertion and in historical *docs/audits/*. `.claude/commands/audit-safety/SKILL.md:275-285` now names `MAX_REFRACT_PASSTHRUS = 8` correctly — **#3052 is NOT regressed.** | PASS |
| `GLASS_RAY_BUDGET` Rust↔generated-GLSL lockstep, read by name | `crates/renderer/src/shader_constants_data.rs:282` — `pub const GLASS_RAY_BUDGET: u32 = 2_097_152;`; generated `crates/renderer/shaders/include/shader_constants.glsl:111` — `#define GLASS_RAY_BUDGET 2097152u`. Emitted by `crates/renderer/build.rs:421` and pinned by the `shader_constants.rs:201` expectation table (test green). Sibling `GLASS_RAY_COST = 4` / `#define GLASS_RAY_COST 4u` (`shader_constants_data.rs:283`, `shader_constants.glsl:112`) likewise. | PASS |
| `GLASS_RAY_BUDGET` enforced at every glass call site | There is exactly **one** IOR glass call site (`triangle.frag:1792-2000`, gated by `glassIORAllowed`). Enforcement is two-legged and both legs are live: (a) the CPU derives every tier's `glass_ray_limit` from the constant — `ray_budget.rs:196-213` (`/8`, `/4`, `/2`, full), pinned by `glass_ray_limit_tiers_derive_from_glass_ray_budget` (`ray_budget.rs:227-245`, #2686); (b) the shader consumes the resulting `rayBudget.qualityTier` as the coherent frame-wide interface-depth limiter. **Note the deliberate design change**: `glassRayLimit` is retained in the GPU ABI (`bindings.glsl:408`) for telemetry only and is read by no shader — per-fragment atomic admission was removed because unordered winners split alpha glass between IOR and Fresnel paths into permanent stipple (`shader_constants_data.rs:268-281`, and `shader_constants.rs:1419-1422` asserts the old admission expression is absent). The runaway-recursion cap is therefore the compile-time `MAX_REFRACT_PASSTHRUS`, which is present. **#1438's unconditional-`atomicAdd` overshoot at `triangle.frag:1801` is acknowledged and NOT re-reported.** | PASS |
| Frisvad orthonormal basis (#820) on the IOR roughness spread | `triangle.frag:1894-1905` — the roughness-spread branch calls `buildOrthoBasis(refractDir, rRight, rUp)` with an in-code note that `cross(refractDir, N_geom_view)` degenerates to zero at normal incidence. `buildOrthoBasis` (`crates/renderer/shaders/include/math_common.glsl:123-130`) is the branchless Frisvad (2012) construction — no `cross(N, up)`, no `normalize` of a possibly-zero vector. No NaN can reach `rayQueryInitializeEXT` from this site. | PASS |
| IOR miss fallback uses cell-ambient in interiors (#1125) | `triangle.frag:2187-2189` — `refrColor = isExteriorGlass ? (skyTint.xyz * 0.5 + sceneFlags.yzw * 0.5) : sceneFlags.yzw;`. Interiors read cell ambient **only**; no global sky tint leakage. The sibling reflection miss at `:1849-1851` uses the identical gate. | PASS |
| `DBG_VIZ_GLASS_PASSTHRU = 0x80` uncollided | `crates/renderer/src/shader_constants_data.rs:641`. Enumerated **all 32** `pub const DBG_*: u32 = 0x…` values in that file: they occupy bit positions 0-31 **exactly once each**, no duplicates. Full list verified: `0x1 BYPASS_POM`, `0x2 BYPASS_DETAIL`, `0x4 VIZ_NORMALS`, `0x8 VIZ_TANGENT`, `0x10 BYPASS_NORMAL_MAP`, `0x20 RESERVED_20`, `0x40 VIZ_RENDER_LAYER`, **`0x80 VIZ_GLASS_PASSTHRU`**, `0x100 DISABLE_SPECULAR_AA`, `0x200 RESERVED_200`, `0x400 BYPASS_VERTEX_COLOR`, `0x800 DISABLE_AO`, `0x1000 LEGACY_LIGHT_ATTEN`, `0x2000 DISABLE_MULTISCATTER`, `0x4000 DISABLE_ATROUS`, `0x8000 DISABLE_RESTIR`, `0x10000 DISABLE_SPATIAL`, `0x20000 VIZ_MOTION`, `0x40000 DISABLE_TEMPORAL`, `0x80000 VIZ_RAW_INDIRECT`, `0x100000 VIZ_MATERIAL_STATE`, `0x200000 VIZ_GI_BOUNCE`, `0x400000 VIZ_FSR_TEMPORAL`, `0x800000 VIZ_NONFINITE`, `0x1000000 VIZ_SHADOW_OFFSET`, `0x2000000 VIZ_NORMAL_DIVERGENCE`, `0x4000000 VIZ_DIRECT`, `0x8000000 DISABLE_DIRECT_SHADOWS`, `0x10000000 DISABLE_GI_RAYS`, `0x20000000 DISABLE_REFLECTION_GLASS_RAYS`, `0x40000000 DISABLE_ALL_MAIN_RAYS`, `0x80000000 VIZ_SELECTED_LIGHT`. The three compound views (`VIZ_MATERIAL_LOBES`, `VIZ_RT_LOD`, `VIZ_SHADOW_VISIBILITY`, `:867/873/881`) are documented ORs of existing bits, not new allocations. `DBG_VIZ_GLASS_PASSTHRU` is present in the `DBG_BITS` emit catalog (`:949`) and consumed at `triangle.frag:2126` and `:2386`. | PASS |

### Not re-reported (OPEN issues confirmed still live)
- **#2795** (OPEN) — `byroredux/src/app_frame.rs:182-191` still `debug_assert_eq!`s `overflow_count() == 0`, panicking debug builds on the graceful over-cap degrade path. Premise verified against current code; existing issue, skipped per dedup rule.
- **#3452** (OPEN) — FO4 `Rimlight Power` FLT_MAX sentinel carried into `GpuMaterial`. Out of this dimension's layout scope; not re-reported.
- **#3447 / #3450** (OPEN) — stale 352 B `GpuCamera` in `docs/engine/shader-pipeline.md`, `docs/engine/memory-budget.md` and two audit SKILL files. See SAFE-D6-01 for the *in-code* sites those two issues do not cover.

---


## Dimensions 8, 9
### Dimension 8

**D8.1 — #772 B-spline pose-fallback FLT_MAX sentinel: INTACT end-to-end.**
- Sentinel constant + rationale (FO3 `TestQAHairM` 31→0, FNV Doc Mitchell finger bones) —
  `crates/nif/src/anim/channel.rs:466-479` (`pub const FLT_MAX_SENTINEL: f32 = 3.0e38;`).
- Predicate — `crates/nif/src/anim/keys.rs:10-12` (`is_flt_max`), `:21-23` (`is_key_value_sane`,
  the #1443 mainline-stream sibling), `:29-35` (`sane_tangent`).
- Static-pose gate (`NiLookAtInterpolator` / `NiTransformInterpolator` constant channel) —
  `crates/nif/src/anim/transform.rs:126-172`: translation, all 4 quaternion lanes, and scale
  each drop to an EMPTY key vector (bind-pose fallback), not a materialised key.
- B-spline fallback gates — `crates/nif/src/anim/bspline.rs:176-182` (float static fallback
  returns `None` on sentinel), `:344-357` (translation), `:376-386` (rotation), `:404-412`
  (scale), and `static_transform_channel` at `:427+`.
- Mainline keyframe-stream filters (#1443) — `crates/nif/src/anim/keys.rs:41-45`, `:65-70`,
  `:223`; colour/float channels `crates/nif/src/anim/channel.rs:64, 81, 164, 192, 222, 264`.
- Regression tests present and asserting: `crates/nif/src/anim/tests/transform.rs:242-275`
  (per-axis "no pose" semantics), `:349-403` (#772 — full `-f32::MAX` TRS pose must produce
  an EMPTY `TransformChannel`, not keys), `:407-420` (partial-sentinel sibling),
  `crates/nif/src/anim/tests/channel.rs:902-939`, `crates/nif/src/anim/tests/sanitize.rs`.
  All three test modules are wired: `crates/nif/src/anim/tests/mod.rs:16,18,20` and
  `crates/nif/src/anim/mod.rs:67`.
- The clip-level companions are also live: `byroredux/src/anim_convert.rs:299-303`
  (`sanitized_clip_frequency`, #3258) and `:527-530` (non-finite `phase` rejected).

**D8.2 — `AnimationClipRegistry` case-insensitive interning: INTACT.**
- Insert path folds case in place, allocation-free on an already-lowercase key —
  `crates/core/src/animation/registry.rs:98-113`.
- Lookup path canonicalises symmetrically — `:70-76` + `canonicalise` at `:207-214`.
- Regression tests: `:255-274` (#790 dedup — `reg.len() == 1` after 3 inserts, `build_clip`
  panics on a memo hit), `:290-311` (#866 case-insensitivity across `MESHES\IDLE.KF` /
  `Meshes\Idle.KF` / `meshes\idle.kf`), `:317-322` (`add()` deliberately does not populate
  the path map).
- Deduped call paths verified: `byroredux/src/npc_spawn.rs:520-521`
  (`get_or_insert_by_path`, static lowercase `kf_path`), `byroredux/src/asset_provider/
  animation.rs:76` (`get_by_path` early-out) + `:123-124` (`get_or_insert_by_path`).
- Eviction exists and is wired: `AnimationClipRegistry::release` at
  `crates/core/src/animation/registry.rs:154-190` (clears keyframe arrays, keeps the slot
  addressable so no live `clip_handle: u32` ever aliases, and `retain`s the reverse path-map
  so the next `get_or_insert_by_path` rebuilds). Callers: `byroredux/src/streaming_helpers.rs:542-544`
  and `byroredux/src/cell_loader/references/mod.rs:134-136` (LRU eviction, #863).
- The residual monotonic *slot-header* growth is already tracked — see "Existing" below.

**D8.3 — `MAX_TOTAL_BONES` overflow guard fires; no SSBO over-index reachable.**
- Pool refuses past capacity and warns once — `crates/core/src/ecs/resources/skin_slot_pool.rs:162-189`
  (`overflow_warned` latch at `:175-183`, `overflow_attempt_count` saturating-add at `:174`,
  accessor at `:216-218`). Constructor asserts `max_skinned >= 1` at `:135-139`.
- Capacity is derived, not hardcoded: `byroredux/src/main.rs:586-590` —
  `(MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1 = floor(196608/144) - 1 = 1364`.
- Overflowed entities fall back to bind pose, not over-index —
  `byroredux/src/render/skinned.rs:93-98` (`continue` leaves `skin_offsets` unset) and
  `byroredux/src/render/static_meshes.rs:284` (`skin_offsets.get(&entity).copied().unwrap_or(0)`
  → slot 0 = identity).
- Guard tests intact and still asserting the bound —
  `byroredux/src/render/bone_palette_overflow_tests.rs:63-88` (at-capacity: palette length
  `== (max_skinned+1) × MBPM` AND `<= MAX_TOTAL_BONES`) and `:91-112` (over-capacity: at
  least one mesh dropped, `palette.len() <= MAX_TOTAL_BONES`). Module wired at
  `byroredux/src/render/mod.rs:1008`. The test harness recomputes capacity from the same
  two constants as `main.rs` (`:41-44`), so a constant bump cannot desync test from product.
- SSBO write path is bounded at every hop:
  - `crates/renderer/src/vulkan/scene_buffer/upload.rs:237` — `bone_world.len().min(MAX_TOTAL_BONES)`
    clamps the `copy_nonoverlapping` byte count against the `MAX_TOTAL_BONES`-sized staging buffer
    (`buffers.rs:466`); flush is range-limited at `:257`.
  - `upload.rs:400-408` — `debug_assert!((slot_id+1) * MBPM <= MAX_TOTAL_BONES)` (#1193 /
    SAFE-D7-NEW-03) plus the pool-side construction bound; `record_pending_bind_inverse_copies`
    `dst_offset = slot_id × slot_byte_stride` therefore ends at ≤ 196,560 × 64 B < the 2 MB
    persistent SSBO.
  - `upload.rs:343-357` — `upload_pending_bind_inverses` caps per-mesh bytes at
    `bind_inverses.len().min(MAX_BONES_PER_MESH)` and the pending count at
    `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME`; overflow entries stay queued
    (`drain_pending` doc, `skin_slot_pool.rs:227-240`) rather than being silently dropped.
  - `byroredux/src/app_frame.rs:398-410` — `padded.resize(MAX_BONES_PER_MESH, IDENTITY)`
    truncates as well as pads, so an over-long `bind_inverses` cannot widen a slot.
  - Shader side: `crates/renderer/shaders/skin_vertices.comp:195-209` — the #651 / SH-6
    clamp `uvec4 bIdx = min(boneIdx, uvec4(MAX_BONES_PER_MESH - 1u))` bounds the read to
    `bone_offset + 143`; with `bone_offset <= 1364 × 144`, the maximum read index is
    196,559 < `MAX_TOTAL_BONES` (196,608). Zero-weight vertices take `bones[bone_offset]`
    at `:193`.

**D8.4 — `byroredux/src/render/skinned.rs` palette-offset arithmetic: bounded.**
- `skin_offsets.insert(entity, slot * MBPM)` (`:100`) — `slot <= 1364`, product 196,416, no
  `u32` overflow.
- `required_slots = (pool.max_used_slot() + 1) * MBPM` (`:148`) with
  `max_used_slot() = next_slot.saturating_sub(1)` (`skin_slot_pool.rs:355-357`), and
  `next_slot` is only ever bumped while `<= max_slot` — so every allocated slot (including
  free-list reuse, which only recycles ids `< next_slot`) satisfies
  `start + MBPM <= bone_world_out.len()`.
- `end = start + MAX_BONES_PER_MESH.min(skin.bones.len())` (`:179`) and the write loop
  `.take(MAX_BONES_PER_MESH)` (`:180`) double-bound the per-entity write; `SkinnedMesh::new`
  itself asserts `bones.len() <= MAX_BONES_PER_MESH`
  (`crates/core/src/ecs/components/skinned_mesh.rs:124-128`).
- The `#1794` "don't re-identity-fill" optimisation's invariant (a vertex's bone index is
  bounded by its OWN mesh's bone count) is enforced structurally by the shader clamp above,
  so the retained stale padding is genuinely unreachable.

### Dimension 9

**D9.1 — every renderer-bound `Material` producer resolves or constructs finite PBR scalars.**
Enumerated producers (non-test):
| Producer | Guard | Evidence |
|---|---|---|
| `translate_material` (NIF import — cell + loose paths) | `resolve_pbr()` | `byroredux/src/material_translate.rs:467` (literal, NaN seed at `:563`) → `:590` |
| `translate_texture_only_material` | `resolve_pbr()` | `material_translate.rs:697` (NaN seed `:703`) → `:727` |
| `Material::default()` | finite literals | `crates/core/src/ecs/components/material.rs:559-560` — `metalness: 0.0`, `roughness: 0.5` |
| Save restore | `sanitize_finite()` per component | `crates/save/src/driver.rs:145-158` (#2687); prevention half `crates/save/src/validate.rs:454` |
| `mat.set` console command | `resolve_pbr()` (#2489) | `byroredux/src/commands/scene.rs:907-927` |
| Studio host `SetMaterial` | explicit `clamp(0.0, 1.0)` + finiteness gate | `byroredux/src/studio_host.rs:88-94`, `:174` |
| `apply_glass_behavior` (post-resolve mutation) | finite constants only | `byroredux/src/helpers.rs:114-128` |
| Cornell harness (`matte`/`pbr`/`glass`/`emissive`/`fire_refraction`) | finite literals | `byroredux/src/cornell.rs:1432-1560` |
| Particle billboards | finite literals | `byroredux/src/render/particles.rs:195-196` (`roughness: 1.0`, `metalness: 0.0`) |
The two ECS `world.insert(entity, material)` sites both carry a `translate_material` output:
`byroredux/src/scene/nif_loader.rs:959` → `:1083`, and
`byroredux/src/cell_loader/spawn/mesh_instance.rs:634` → `:831`.
`Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs:1165-1191`) still detects
`is_nan()` on both fields and ends with the unconditional
`clamp(0.0,1.0)` / `clamp(0.04,1.0)` pair.
**The `static_meshes.rs` no-`Material` fallback still constructs finite defaults directly** —
`byroredux/src/render/static_meshes.rs:367-370` (`0.5` roughness, …), confirmed intact.

**D9.2 — collision translate finiteness: INTACT, and double-gated.**
- `BhkMultiSphereShape` — per-sphere `if !center.is_finite() || !radius.is_finite() { continue; }`
  (`crates/nif/src/import/collision/shape.rs:110-137`, NIFAL-S4 / #1409); an all-corrupt
  multi-sphere collapses to `None` → trimesh fallback, not an empty `Compound`.
- `BhkConvexListShape` — `crates/nif/src/import/collision/shape.rs:235-253`; children are
  produced by the recursive `resolve_shape`, so each child inherits its own arm's
  `finite()`/`finite_vec()` gate; an empty residue returns `None` (the #1269 empty-`Compound`
  fix is preserved).
- Siblings all gated: `Sphere` `:87-91`, `Box` `:139-151`, `Capsule` `:154-164`,
  `Cylinder` `:167-176`, `ConvexVertices` `:179-191`, `TransformShape` `:258-268` (#2862),
  `segment_shape` `:773`, trimesh `:715` (#1779). Helpers `finite`/`finite_vec` at
  `crates/nif/src/import/collision/mod.rs:530-538`.
- **Blast radius is additionally closed at the Rapier boundary** (release-profile backstop, so
  this does not rely on `debug_assert!`): `crates/physics/src/convert.rs:26-32`
  `clamp_shape_extent` maps non-finite → `1e-3` and clamps finite values into
  `[1e-3, MAX_SANE_SHAPE_EXTENT = 1_048_576]` (`:20`); applied to Ball `:217-222` (#3238),
  Cuboid `:222-251` (#2543), Capsule/Cylinder following. Compound child transforms are
  neutralised to `Vec3::ZERO`/`Quat::IDENTITY` at `:201-213`. Pinned by
  `huge_finite_ball_radius_clamps_to_sane_ceiling` (`:544-548`) and the Cuboid/Capsule
  siblings. `sanitize_scale` (`:42-48`) closes the scale lane (#2860).
  Collision shapes never reach the BLAS builder — BLAS geometry comes from render meshes —
  so the only downstream is Rapier, and it is bounded.

**D9.3 — typed particle emitter params are finite, non-negative, and spawn-capped.**
- `extract_emitter_params` — `crates/nif/src/import/walk/mod.rs:821-855`: an 11-field
  `is_finite()` sweep (incl. the #1445 `planar_angle*` and #1775 `radius_variation` late
  additions) plus `life_span > 0.0 && initial_radius >= 0.0`; failure returns `None`
  (heuristic preset), it does not leak partial garbage. `base_scale` separately filtered at
  `:812` (`is_finite() && > 0.0`).
- `extract_emitter_rate` — `:921-930`: `sane(r) = (r.is_finite() && 0.0 < r && r < 3.0e38)`,
  which rejects the `FLT_MAX` "use the keyed data" sentinel (#1364) as well as NaN/±Inf/0/negative.
- Colour curve gate `:740` (`is_finite() && x < 3.0e38`).
- `apply_emitter_params` (`byroredux/src/systems/particle.rs:29-46`) copies only pre-validated
  fields; `apply_emitter_overlays` clamps the authored pool budget to
  `min(authored, MAX_PARTICLES_CEILING)` at `:118` (`MAX_PARTICLES_CEILING = 256`,
  `crates/core/src/ecs/components/particle.rs:83`).
- **The per-frame spawn count is NOT an unbounded-allocation channel** —
  `byroredux/src/systems/particle.rs:439-479`: the spawn block is gated on
  `rate.is_finite() && rate > 0.0 && start_size.is_finite() && start_size > 0.0` (#1382), and
  the loop body breaks the moment `em.particles.len() >= cap`, so at most `cap` (≤ 256)
  pushes occur regardless of `spawn_count`. `life` floored at `0.05` (`:468`), `size` floored
  at `0.0` (`:475`). Pinned by the NaN `rate`/`start_size` tests at `:760-790`.

---


---

# Deduplication notes

**#3512 (OPEN, `medium`) — "FO4-2026-08-27-D1-02: `CsgArchive::chunk_bytes` inflates a
.csg chunk into an unbounded Vec and range-checks it only afterwards" — appears to have
been fixed by #3410 and should be re-checked before it is worked.**
`chunk_bytes` no longer uses an unbounded `read_to_end`: `crates/bsa/src/csg.rs:277-289`
now calls `crate::safety::inflate_bounded(ZlibDecoder::new(&comp[..]), CSG_CHUNK_SIZE,
…)`, which caps the decoder at `declared + 1` bytes and raises `InvalidData` **at** the
ceiling rather than past it. The `raw.len() > CSG_CHUNK_SIZE` check at `:290-298` is
explicitly retained as defence-in-depth and to preserve this arm's message, per the
in-code #3410 comment. The finding above (`read_psg`) is a *different* site and a
different mechanism (up-front capacity vs. decompression bomb), so it does not
duplicate #3512 — but #3512 itself reads stale.

## Existing (reported, not re-filed)

- **Existing: #2689** — `SAFE-D8-01: AnimationClipRegistry slot vector grows monotonically —
  release() never returns a slot, stranding one clip header per evict/reload cycle` (OPEN,
  labelled `low`). This is the answer to D8.2's "is there eviction at all" question:
  `release()` (`crates/core/src/animation/registry.rs:154-190`) clears a slot's *contents* but
  never returns the index to a free list, and `release_drops_path_binding_so_next_get_or_
  insert_rebuilds` (`:334-352`) confirms a post-release reload allocates a **new** slot. So
  `clips: Vec<AnimationClip>` grows by one empty header per evict→reload cycle for the
  process lifetime. Premise re-verified against current code; already tracked, not re-filed.


---

## Next step

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-08-30.md
```
