# Safety Audit — 2026-05-11

## Scope

Ten dimensions per `/audit-safety`: unsafe Rust, Vulkan spec compliance, GPU/CPU
memory, threading, cxx FFI, RT pipeline, new compute pipelines (TAA / Caustic /
Skin), R1 material table, RT IOR-refraction, and NPC / animation spawn.

## Dedup pass

`gh issue list --limit 1000 --state all` → 948 issues. Open issues that overlap
the safety surface and that this audit therefore re-affirms rather than re-reports:

| Dim | Issue | Title (short) |
|---|---|---|
| D2 | #908 | `current_frame=0` reset after resize can submit on un-waited fence |
| D2 | #909 | Composite UBO host barrier isolated from bulk host barrier |
| D2 | #910 | `acquire_next_image` error path leaks `image_available` semaphore signal |
| D2 | #913 | `recreate_swapchain` does not reset `frame_counter` → stale TAA jitter |
| D2 | #947 | Outgoing subpass dep omits `EARLY_FRAGMENT_TESTS` |
| D2 | #948 | Packed depth-stencil fallback uses DEPTH-only view with combined layout |
| D2 | #949 | `gbuffer::initialize_layouts` uses deprecated `TOP_OF_PIPE` source stage |
| D3 | #850 | `SoundCache` has no eviction — long-session unbounded growth |
| D3 | #856 | streaming worker `JoinHandle` never joined (relies on Arc-drop semantics) |
| D3 | #858 | non-looping `ActiveSound` survives entity despawn until natural termination |
| D6 | #661 | `compute_to_blas` barrier uses legacy `ACCELERATION_STRUCTURE_READ_KHR` |
| D6 | #911 | First-sight skin compute prime + sync BLAS BUILD stalls per-frame cmd buf |
| D10 | #946 | `BSDynamicTriShape` "vanilla never fires" comment empirically false (warn spam) |

Closed and verified still-in-place during this audit:
**D7** #871 (skin slot rollback) at `skin_compute.rs:349`.
**D8** #797 / SAFE-22 over-cap return-0 at `material.rs:595`; #806 / R1-N6 per-field
offset pin at `material.rs:675`; #785 / R-N1 ui.vert lockstep at `ui.vert:48`.
**D9** #789 texture-equality identity check at `triangle.frag:1684`; #820 / REN-D9-NEW-01
Frisvad basis at `triangle.frag:316-322`; #916 GLASS_RAY_BUDGET=8192 at `triangle.frag:1534`;
interior cell-ambient fallback at `triangle.frag:1775` (bb53fd5).
**D10** #790 / E-N1 `clip_handles_by_path` + case-insensitive memoisation at
`registry.rs:46-117`; #772 / FLT_MAX sentinel at `anim.rs:2017-2020`.
**D6** #907 / REN-D12-NEW-01 `validate_refit_counts` at `acceleration.rs:120-135`;
#679 / AS-8-9 `should_rebuild_skinned_blas_after` threshold at `acceleration.rs:98`.

## NEW findings

### SAFE-25: Main raster pipeline lacks SPIR-V descriptor-set reflection validation
- **Severity**: MEDIUM
- **Dimension**: D7 (new compute pipelines invariant generalised), D8 (R1 lockstep risk)
- **Location**: `crates/renderer/src/vulkan/pipeline.rs:130-142` (`build_triangle_pipeline_layout`)
- **Status**: NEW

**Description.** Every compute pipeline in the renderer
(`bloom`, `caustic`, `ssao`, `svgf`, `taa`, `skin_compute`, `composite`,
`compute`, `volumetrics`) calls
`reflect::validate_set_layout(...)` against its SPIR-V before
`vkCreateDescriptorSetLayout`, so a binding-type / count / stage drift
between shader source and Rust layout is caught at startup. The **main
raster pipeline** — the one containing `triangle.vert` + `triangle.frag`,
where `GpuInstance` (set 1, binding 4), `MaterialBuffer` (set 1,
binding 7), the global vertex/index SSBOs, the bindless `textures[]`
array, the TLAS handle, and every R1 lockstep field live — does **not**.
It constructs its `PipelineLayout` from two opaque
`DescriptorSetLayout` handles handed in by the caller and never
cross-checks them against the SPIR-V bindings the fragment / vertex
stage actually declares.

**Evidence.**

`crates/renderer/src/vulkan/reflect.rs:11` — module doc:
```rust
//! calls [`validate_set_layout`] before `vkCreateDescriptorSetLayout`
```
Call sites (grep `validate_set_layout`):
- `vulkan/bloom.rs:231, 275`
- `vulkan/caustic.rs:44, …`
- `vulkan/compute.rs:139`
- `vulkan/composite.rs`
- `vulkan/ssao.rs`
- `vulkan/skin_compute.rs`
- `vulkan/svgf.rs`
- `vulkan/taa.rs:263`
- `vulkan/volumetrics.rs`
- **`vulkan/pipeline.rs` — none.**

```rust
// pipeline.rs:130-142
fn build_triangle_pipeline_layout(
    device: &ash::Device,
    descriptor_set_layout: vk::DescriptorSetLayout,
    scene_set_layout: vk::DescriptorSetLayout,
) -> Result<vk::PipelineLayout> {
    let set_layouts = [descriptor_set_layout, scene_set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);
    unsafe {
        device
            .create_pipeline_layout(&layout_info, None)
            .context("Failed to create pipeline layout")
    }
}
```

**Impact.** R1 / Shader Struct Sync (#318 / #417 / #806) pins the
**byte layout** of `GpuInstance` and `GpuMaterial`, and the offset-pin
test at `material.rs:675` catches within-vec4 reorders. None of those
tests cover the case where the descriptor **binding** itself changes:
adding a new SSBO binding to `triangle.frag` without the matching
`DescriptorSetLayoutBinding` on the Rust side (or vice-versa) compiles,
links, and runs — and either reads garbage at the wrong binding index
or silently uses a default-initialised resource. That is exactly the
class of bug `reflect::validate_set_layout` was added to catch.

The fragment shader currently has ≥ 12 bindings across set 0 + set 1
(samplers, instance SSBO, material SSBO, vertex SSBO, index SSBO,
light SSBO, cluster SSBO, ray-budget atomic, TLAS, etc.); any silent
drift between Rust layout creation and shader source is exactly the
binding-type-mismatch failure mode the reflection layer is the only
sound guard against.

**Suggested Fix.** Hook `validate_set_layout` into the call site that
constructs `descriptor_set_layout` and `scene_set_layout` (where the
SPIR-V bytes are still available — `triangle.vert.spv` + `triangle.frag.spv`
are `include_bytes!` in `pipeline.rs` already). The cost is one
reflection pass per shader at startup; the alternative is waiting for
a vertex-shader-side drift to manifest as a wrong-binding read or
device-lost. Two new shader stages `ReflectedShader::vertex(...)` +
`ReflectedShader::fragment(...)` already exist (see `bloom.rs:231`
for the multi-stage pattern).

---

### SAFE-26: `bgem_cache` and `failed_paths` in BgsmProvider grow unbounded across sessions
- **Severity**: LOW
- **Dimension**: D3 (memory safety)
- **Location**: `byroredux/src/asset_provider.rs:461-473` + `:535-549`
- **Status**: NEW (sibling of closed #863 and open #850)

**Description.** `BgsmProvider.bgem_cache: HashMap<String, Arc<BgemFile>>`
and `failed_paths: HashSet<String>` are populated by every cell load
that touches a BGEM-material reference. There is no eviction path; in
a long exterior-streaming session the maps grow with the union of
every BGEM seen across every loaded cell (~couple thousand for a full
Skyrim/FO4 archive). Per entry is small (a parsed `BgemFile` is a
few hundred bytes, paths are short ASCII), so the absolute footprint
is bounded by archive contents (low MB), but the pattern mirrors the
two cases that have already been fixed/filed:

- `AnimationClipRegistry` was grow-only until #790 / #863 added
  `release()` and case-insensitive memoisation.
- `SoundCache` is the open mirror at #850.

**Suggested Fix.** Either:
1. Hook BGEM/BGSM eviction into the same M40 cell-unload path that
   already drops NIF imports and BLAS entries (see
   `cell_loader.rs:330` `freed_meshes` loop). The cache is per-cell
   addressable by the same cell-root → resource set the cell loader
   tracks.
2. Add a soft LRU cap analogous to the BLAS eviction policy.

Option 1 is cheaper and keeps the eviction model uniform with the
existing M40 cleanup. Bundle with #850's eventual fix.

## RE-AFFIRMED invariants

### D1 — Unsafe Rust blocks

516 `unsafe` occurrences across 34 files. Distribution skews to
renderer (~470 of them are ash bindings — unsafe by API design,
the convention is bulk-trusted at this layer). Non-trivial unsafe
operations (raw-pointer / `from_raw_parts` / slice casts) all carry
explicit `SAFETY:` comments with sound invariants:

- `crates/nif/src/stream.rs:284-316` — `read_pod_vec<T>` — LE-host
  compile-error gate + POD requirement documented.
- `crates/nif/src/header.rs:340-364` — `read_pod_vec_from_cursor` —
  same SAFETY block, cross-references `stream.rs`.
- `crates/renderer/src/vulkan/buffer.rs:640-660`, `:808-820` —
  `write_mapped<T: Copy>` — slice-to-bytes cast for Vulkan upload.
- `crates/renderer/src/vulkan/scene_buffer.rs:1640-1652` —
  `hash_material_slice` — cross-references `GpuMaterial::as_bytes`.
- `crates/renderer/src/vulkan/material.rs:317-331` —
  `GpuMaterial::as_bytes` — `repr(C)` + named-pad invariant
  explicitly documented.
- `crates/renderer/src/vulkan/skin_compute.rs:462-470` —
  `cmd_push_constants` byte cast — repr(C) 12-byte invariant pinned
  by `push_constants_size_is_12_bytes` test.
- `byroredux/src/cell_loader.rs:339-348` —
  `shrink_blas_scratch_to_fit` — main-thread + no-build-in-flight
  invariant called out.
- `crates/core/src/string/mod.rs:104-110` —
  `ascii_lowercase_into_buf` — UTF-8 ASCII-only guarantee documented.

`World::get()` raw-pointer extension (called out in the audit prompt)
no longer exists. Issue #35 replaced it with `ComponentRef` at
`crates/core/src/ecs/query.rs:188-235` — `RwLockReadGuard`-owning
wrapper, sound by construction.

### D2 — Vulkan spec compliance

VulkanContext Drop at `vulkan/context/mod.rs:1836-2008` does the
reverse-create teardown correctly. The #665 / LIFE-L1 allocator-
outstanding-references case at `:1961-1998` correctly leaks the
device + surface + instance + debug messenger in release rather than
double-freeing — verified by #927 closeout.

`VK_LAYER_KHRONOS_validation` enabled in debug at
`instance.rs:11, 36, 41`. `VK_KHR_ray_query` + `acceleration_structure`
+ `deferred_host_operations` enumerated at `device.rs:104-109` and
gated behind `caps.ray_query_supported` at every consumer
(`device.rs:338, 353, 363, 366, 369-380`). Feature struct
`ray_query_features` correctly chained via `push_next` at `:380`.

Per-frame-in-flight `render_finished` semaphores landed at 913f804
(#906 / REN-D1-NEW-02 closeout — was the swapchain-image-count-keyed
bug; the per-frame fix is now baseline).

Open D2 issues (#908 / #909 / #910 / #913 / #947 / #948 / #949) are
the residual surface and reported as Existing above.

### D3 — Memory safety

GPU side. `MaterialBuffer` SSBO sized for `MAX_MATERIALS = 4096`
(`scene_buffer.rs:63`) with upload truncation at `:1180`
(`materials.len().min(MAX_MATERIALS)`) — over-cap intern returns
material 0 at `material.rs:604` (#797 / SAFE-22), so the on-GPU SSBO
read is always in bounds even if the CPU table over-runs. The
intern-cap and upload-cap are consistent.

`MaterialTable::clear()` (`material.rs:493-498`) is called once per
frame at the top of `build_render_data`
(`byroredux/src/render.rs:289`). The dedup HashMap retains capacity
across frames (Rust HashMap semantics) — bounded by `MAX_MATERIALS`,
no growth path.

M40 cell unload at `cell_loader.rs:200-360` does:
- `freed_meshes` → `accel.drop_blas` (per-mesh BLAS slot drop)
- `accel.shrink_blas_scratch_to_fit` (#495 — BLAS scratch
  hysteresis)
- `mesh_drops` → `mesh_registry.drop_mesh`
- `texture_drops` → `texture_registry.drop_texture`
- `world.despawn(eid)` for every victim

`NifImportRegistry` is process-lifetime (#381) but per-handle backed
by `Arc<...>`; cell-unload drops the surviving Arc references via
the despawn loop. AnimationClipRegistry is `release()`-aware (#790).

CPU side. Two new findings above (SAFE-25 is descriptor-layout
reflection, SAFE-26 is BGEM cache growth). No other unbounded growth
identified in the cell-streaming hot path.

### D4 — Thread safety

`TypeId`-sorted lock acquisition explicitly enforced in
`crates/core/src/ecs/systems.rs:64` (`Systems` runner) and via
`lock_tracker::TrackedRead` / `TrackedWrite` scope guards at every
`World::query` / `World::resource` site
(`crates/core/src/ecs/world.rs:247, 350, 288, 341, 278`).
`ComponentRef` ownership transfer of the tracker entry at
`query.rs:215-242` is sound: the `Drop` impl untracks read, and the
`new()` constructor scope-defuses only on `Some`.

`Arc<Mutex<Allocator>>` lock-hold sites: every `GpuBuffer::create_*`
and `Texture::create_*` takes the allocator lock for the duration of
one `vkAllocateMemory`/`vkBindBufferMemory`/`vkBindImageMemory`
sequence and releases. No multi-allocator-lock patterns observed.

`Send + Sync` bounds on `Component` / `Resource`: enforced at trait
definition (`ecs/component.rs`, `ecs/resource.rs`).

### D5 — FFI safety

`crates/cxx-bridge/src/lib.rs` is currently a 35-line placeholder:
one `String` round-trip across the boundary (`native_hello`),
one POD struct (`EngineInfo`) returned from Rust. cxx handles
`String` ownership transfer correctly; no raw pointers, no
shared-lifetime hazards. D5 is effectively N/A until the bridge
gains real consumers.

### D6 — RT pipeline safety

- BLAS / TLAS device-address sites: every buffer queried via
  `vkGetBufferDeviceAddress` is created with `SHADER_DEVICE_ADDRESS`
  usage flag (`buffer.rs::create_*`, `skin_compute.rs:328`,
  `acceleration.rs::create_*`).
- `instance_custom_index` encoding: `acceleration.rs:2047`
  `Packed24_8::new(ssbo_idx, 0xFF)` where `ssbo_idx` is the
  shared-SSBO-compaction map index (#419) — by-construction within
  the SSBO bounds.
- Global vertex / index SSBO indexing is **not GPU-bounds-checked**
  (`triangle.frag:340-358`). CPU-side encoding via
  `mesh.vertex_offset` / `mesh.index_offset` is the only guard —
  this is the documented contract, not a bug.
- Ray-query origin bias: every `rayQueryInitializeEXT` uses
  `tMin ∈ [0.001, 0.05]` with origin = `fragWorldPos ± N * bias`
  (see `triangle.frag:1653` for IOR, `:373` for shadow, `:2341` for
  reflection).
- TLAS UPDATE-mode safety:
  `last_blas_addresses` zip-compare at `acceleration.rs:282-301`
  short-circuits to full BUILD on any address-list change.
  `validate_refit_counts` at `:120-135` defends
  VUID-vkCmdBuildAccelerationStructuresKHR-pInfos-03667 against
  vertex/index-count drift on `entity_id → mesh` remap. Skinned
  BLAS refit threshold (`SKINNED_BLAS_REFIT_THRESHOLD = 600`)
  forces full rebuild at `:91` / `:1260`.
- Skin compute output buffer usage at `skin_compute.rs:327-329`:
  `STORAGE_BUFFER | SHADER_DEVICE_ADDRESS |
  ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`. The audit
  prompt notes "M29.3 also re-adds `VERTEX_BUFFER`" — Phase 3
  (raster reading skinned output as VBO) is **not yet landed**;
  the comment at `:316-322` is explicit. Current usage mask is
  correct for Phase 2 (RT-only consumer). Re-add `VERTEX_BUFFER`
  in the M29.3 commit per the in-code marker.
- Bone palette overflow guard at `byroredux/src/render.rs:325-338`
  fires before the renderer's `upload_bones` clamp; once-per-session
  warn via `BONE_PALETTE_OVERFLOW_WARNED.call_once`.

### D7 — New compute pipeline safety

- **TAA**: history images held in GENERAL across both read + write
  paths (`taa.rs:509, 512, 691-692, 729-730`). Initial transition
  UNDEFINED → GENERAL at `:607` (one-time, post-create). Force-reset
  predicate `should_force_history_reset` at `svgf.rs` consumed at
  `taa.rs:654`; first-frame produces no NaN read because the
  reset branch zeroes the sample, not the history mix.
- **Caustic**: R32_UINT accumulator with `imageAtomicAdd` only
  (`caustic.rs:57`, shader at `caustic_splat.comp:243`). CLEAR-before-
  COMPUTE invariant at `caustic.rs:816` is unconditional and
  fenced by both pre- and post-clear pipeline barriers
  (`:783-803`, `:825-840`). No persistent-ghost path.
- **Skin compute**: `SkinPushConstants` is 12 B at
  `skin_compute.rs:43-56`, pinned by
  `push_constants_size_is_12_bytes` at `:526`. Far under the
  128 B Vulkan-guaranteed minimum.
- **SPIR-V reflection**: every compute pipeline uses
  `reflect::validate_set_layout` (call-site enumeration at the
  top of SAFE-25). The exception is `pipeline.rs` — see SAFE-25.

### D8 — R1 material table

- `gpu_material_size_is_260_bytes` test pinned at
  `material.rs:647` (#804 / R1-N4 dropped `avg_albedo` 12 B from
  the pre-260 272 B).
- `gpu_material_field_offsets_match_shader_contract` test
  pinned at `material.rs:675` (#806 / R1-N6). Every named field on
  the Rust side has an explicit `offset_of!` assertion against the
  shader-side vec4 group.
- All fields are scalar f32/u32; no `[f32; 3]` vec3 alignment trap.
- Named pad fields explicit; `GpuMaterial::as_bytes` byte cast at
  `material.rs:317-331` is deterministic (verified by
  hash-dedup test corpus).
- `material_id` bounds: instance points at a slot the table
  upload already truncated to `MAX_MATERIALS`. Over-cap intern
  returns 0 at `material.rs:604` so the GPU read is always
  in-bounds even on overflow.
- `MaterialTable::intern` cap-and-warn at `material.rs:595-604`
  with `INTERN_OVERFLOW_WARNED.call_once`.
- Upload truncation at `scene_buffer.rs:1180`
  (`count = materials.len().min(MAX_MATERIALS)`) — in lockstep
  with the intern cap.
- `ui.vert:48` reads per-instance `inst.textureIndex` — not
  `materials[0].textureIndex` (#785 / R-N1 / regression of #776
  fixed).

### D9 — IOR refraction safety

- Frisvad orthonormal basis at `triangle.frag:316-322` is the
  active code path for IOR refraction roughness spread, called
  from `:1609`. The legacy `cross(N, world-up)` formulation is
  gone (#574). Singularity at exactly `dir.z = -1.0` is the only
  degenerate case and is not reachable from the camera-facing
  refraction rays in our Z-up→Y-up converted scene.
- Texture-equality identity check for glass passthrough at
  `triangle.frag:1684`
  (`bool sameTexture = (hInst.textureIndex == selfTexture);`)
  prevents the #789 infinite-loop class. `REFRACT_PASSTHRU_BUDGET = 2`
  at `:1651` caps the loop body iterations; outer ray-budget
  gate (`GLASS_RAY_BUDGET = 8192` at `:1534`, `GLASS_RAY_COST = 4`
  at `:1539`) is enforced at the single fragment-entry site
  `:1541-1544`.
- IOR miss fallback at `:1775` uses
  `fog.xyz * 0.5 + sceneFlags.yzw * 0.5` — half-fog + half-cell-
  ambient. Cell ambient `sceneFlags.yzw` is sky-derived in
  exteriors and room-mood in interiors; no hard-coded daylight
  leak into dungeons (bb53fd5).
- `DBG_VIZ_GLASS_PASSTHRU = 0x80u` at `:717`. Full debug-bit
  catalog (`:659-741`): 0x1, 0x2, 0x4, 0x8, 0x10, 0x20, 0x40, 0x80,
  0x100, 0x200 — all distinct powers of 2, no collisions.

### D10 — NPC / animation spawn

- B-spline pose fallback FLT_MAX sentinel at `anim.rs:2017-2020`
  (`is_flt_max(v) := v.abs() >= 3.0e38`). Applied to every TRS
  axis at `:1050, 1061-1064, 1074, 1299, 1465, 1503` so empty
  key lists fall back to bind-pose value per axis — no
  whole-NPC-disappearance (#772 fix).
- `NiBSplineCompTransformInterpolator` dispatch at `anim.rs:1003`.
  Reachable on FNV / FO3 as well as Skyrim+ per
  `feedback_bspline_not_skyrim_only.md` — the audit-era assumption
  that this is Skyrim-only is wrong; the dispatch is gameset-
  agnostic and the FLT_MAX sentinel guard applies uniformly.
- `AnimationClipRegistry` dedup at `registry.rs:46-117`:
  case-insensitive memoisation via `clip_handles_by_path` map.
  `get_or_insert_by_path` ASCII-lowercases keys before lookup
  (avoiding allocation when the caller already passed a
  canonical key). `release()` at `:156-191` clears the slot in
  place AND retains the path-map invariant (drops reverse-map
  entries pointing at the released handle, so a subsequent
  `get_or_insert_by_path` for the same key rebuilds rather than
  reading the empty stub). #790 / #863 fix confirmed in-place.

## Report Finalization

Two NEW findings (SAFE-25 MEDIUM, SAFE-26 LOW). 13 existing OPEN
issues re-affirmed under their original numbers. All previously-fixed
invariants spot-checked against the current code paths; no
regressions detected.

Suggested next:

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-05-11.md
```
