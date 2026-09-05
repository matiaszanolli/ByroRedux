# #3829: REN-2026-09-05-D2-01: volumetrics_inject.comp's GpuBoundaryInstance is a stale 128-byte mirror of the now-160-byte GpuInstance, corrupting every boundary-geometry read past instance index 0

Filed from `docs/audits/AUDIT_RENDERER_2026-09-05.md` (REN-2026-09-05-D2-01) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3829 --json state`.

---

**Severity**: CRITICAL — SSBO index mismatch (the severity floor for this dimension)
**Dimension**: SSBO/Indexing (volumetrics × SSBO × ray queries)
**Source**: `docs/audits/AUDIT_RENDERER_2026-09-05.md` (`REN-2026-09-05-D2-01`), `/audit-suite volumetrics-deep`

## Location

- `crates/renderer/shaders/volumetrics_inject.comp` — `struct GpuBoundaryInstance` and its `BoundaryInstanceBuffer` (binding 19) declaration; consumed by `rigidBoundaryNormal`, queried from `combustionPathBlocked`
- Bound by `VolumetricsPipeline::write_boundary_geometry` (`crates/renderer/src/vulkan/volumetrics.rs`), from the call site in `record_volumetrics_pass` (`crates/renderer/src/vulkan/context/post_passes.rs`)
- Ground truth: `GpuInstance` in `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`
- The guard that does **not** cover it: `gpu_instance_glsl_copies_stay_in_lockstep` (`crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`)

## Description

`write_boundary_geometry` binds binding 19 directly to the **same** `vk::Buffer` the main scene uses for its per-frame `GpuInstance[]` SSBO, and `rigidBoundaryNormal` indexes it with the `instanceIndex` returned by `rayQueryGetIntersectionInstanceCustomIndexEXT` on a committed hit against the same TLAS the rest of the renderer builds. The shader therefore assumes `boundaryInstances[i]` and the raster/RT pass's `instances[i]` address the same record at the same byte offset. **They do not.**

`GpuBoundaryInstance` ends at `uint surfaceId` (offset 108) followed by a single `uvec4 _boundaryTail` — total **128 B** std430 stride. Its own comment still asserts the now-false invariant: *"preserves the exact 128-byte std430 stride of the canonical Rust `GpuInstance`."*

The canonical Rust struct is **160 B**: after `surfaceId` it carries `skinned_vertex_address` (112), `_reserved` (120 → 128, the old end), then `morph_delta_address` (128), `morph_weight_address` (136), `morph_target_count` (144) and three reserved `u32`s (148 → 160) — pinned by `gpu_instance_is_160_bytes_std430_compatible`.

#3231 (`5f4dea46`, 2026-08-23) grew the real struct by exactly the 32 bytes `_boundaryTail` never gained. The boundary-geometry feature (`715b9230`, 2026-08-18) predates that growth by five days — verified via `git merge-base --is-ancestor 715b9230 5f4dea46` — so this mirror was correct when written and has been silently wrong for the ~13 days since.

### Why no test caught it

`gpu_instance_glsl_copies_stay_in_lockstep`'s `SOURCES` is a hardcoded 5-file list (`include/bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp`) that does not include `volumetrics_inject.comp`. Its companion completeness guard `assert_mirror_list_is_complete` discovers mirrors by searching for the literal declaration `"struct GpuInstance"` — which does not match `"struct GpuBoundaryInstance"` as a substring, so the discovery half cannot see it either. (`volumetrics_inject.comp` *is* in the SOURCES of `gpu_light_glsl_copies_stay_in_lockstep`, but that test pins `struct GpuLight`, an unrelated struct.) This is a sixth mirror sitting entirely outside the tracked set, not a symptom of a wider pattern.

The Rust-side doc comment on the `_reserved2a/b/c` padding already documents this exact failure mode from when it was hit *inside* the five tracked mirrors during #3231's own development: *"Confirmed the hard way (#3231): this exact substitution produced a GPU device-lost hang with zero validation-layer diagnostic — every instance past the first read at the wrong offset."* That lesson did not propagate to the untracked mirror.

## Evidence

Byte-by-byte std430 offset comparison of both live declarations (independently re-verified at publish time, not taken on the audit's word):

- GLSL: `mat4 model` (0..64), seven `uint`/`float` scalars and three albedo floats (64..108), `uint surfaceId` (108), `uvec4 _boundaryTail` (112..128) → **128 B**, already 16-aligned so no tail padding
- Rust: **160 B** per `gpu_instance_is_160_bytes_std430_compatible`
- Binder confirmed by `grep -rn "instance_buffers()\[" crates/renderer/src/vulkan/` → a single hit in `post_passes.rs`
- `write_boundary_geometry`'s own doc states binding 19 is the "current GpuInstance table"

## Impact

Scoped to the volumetrics combustion-transport boundary-collision feature — fire/explosion smoke advection near solid geometry, gated on `carriesCombustion(...)`, i.e. only while an active or lingering fire/explosion fog volume is actually transporting chemistry. It does **not** affect the main raster/RT shading path, whose `instances[]` reads go through the correctly-updated `bindings.glsl` copy.

When triggered, for any TLAS hit whose `instance_custom_index != 0`, every recovered field (`boneOffset`, `vertexOffset`, `indexOffset`, `vertexCount`, `model`) is read from the wrong byte range, misaligned by 32 bytes per unit of instance index.

**The failure mode is silently wrong data, not an OOB access or device loss**: the affected fields feed bounds checks and a triangle-normal lookup rather than a raw pointer dereference, the descriptor's bound range is the full (larger) instance buffer, and `instanceIndex` only ever originates from a real TLAS hit. Garbage `boneOffset` frequently trips the "assume skinned, bail to conservative normal" path; when garbage values instead pass the loose bounds checks, `rigidBoundaryNormal` returns a plausible-looking but wrong triangle normal sourced from an unrelated instance's geometry.

Visible effect: fire/smoke plumes near architecture tunnel through walls that should block them, get blocked by walls that are not there, or deflect off the wrong surface normal — for every instance except whichever entity happens to sort to draw index 0 that frame.

## Related

- #3231 — introduced the 128→160 B growth this mirror missed
- `715b9230` — introduced the now-stale mirror
- #2748 / `REN-D3-2026-08-12-01` — the lockstep test this mirror falls outside of

## Suggested Fix

1. Widen `_boundaryTail` from one `uvec4` (16 B) to the full 48 B the real struct now carries past `surfaceId` — three `uvec4` fields — restoring byte-identical 160 B stride. Follow `gpu_types.rs`'s own discipline of deliberately avoiding any type whose std430 alignment could silently drift.
2. Correct the now-false "128-byte" claim in the struct's comment.
3. **Close the structural gap so this cannot drift a third time.** Either teach `gpu_instance_glsl_copies_stay_in_lockstep` to accept a deliberate prefix-mirror under a different struct name and add `volumetrics_inject.comp` to `SOURCES`, or — cheaper and sufficient, since this mirror only needs to agree on *stride*, not on carrying every named field — add a narrow sibling test parsing `GpuBoundaryInstance`'s total std430 size and asserting it equals `size_of::<GpuInstance>()`.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files — is any *other* shader declaring a differently-named prefix-mirror of a Rust GPU struct outside the tracked SOURCES sets?
- [ ] **TESTS**: A regression test pins this specific fix (stride equality, not just field-by-field lockstep)
- [ ] **DROP**: n/a — no Vulkan object lifetimes change
- [ ] **LOCK_ORDER**: n/a
