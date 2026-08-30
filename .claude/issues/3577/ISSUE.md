# #3577 — REN-2026-08-30-D2-03: `shader-pipeline.md`'s Set-1 descriptor table is two rows out of date — binding 11 is documented as 8 × `u32` / 32 B but is 17 × `u32` / 68 B, and binding 19 (`SelectedRayProbeBuffer`) is absent entirely

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3577 --json state`.

---

- **Severity**: LOW
- **Dimension**: SSBO/Indexing (authoritative-doc divergence)
- **Location**: `docs/engine/shader-pipeline.md:439`
  (binding 11 row) and `:444` (table ends at binding 18). Ground truth:
  `crates/renderer/src/vulkan/scene_buffer/ray_budget.rs:12-29`
  (`GpuRayBudget`, `WORDS = 17`),
  `crates/renderer/shaders/include/bindings.glsl:406-424`
  (`RayBudgetBuffer`) and `:479-489` (`SelectedRayProbeBuffer`),
  `crates/renderer/src/vulkan/scene_buffer/buffers.rs:852`
  (`write_storage_buffer(set, 19, …)`).
- **Status**: NEW rows. **Adjacent to `#3447`** (`REN-2026-08-27-D3-01`, open:
  "shader-pipeline.md still documents GpuInstance at 128 B and GpuCamera at
  352 B") — same document, same defect class, but different rows; `#3447`'s title
  and body scope it to the `GpuInstance`/`GpuCamera` size literals and
  `memory-budget.md`. Fold these two rows into `#3447` rather than filing
  separately if that issue is being worked. The `GpuCamera` 352 B → 368 B drift I
  re-confirmed (doc `:193`, `:427` vs `gpu_camera_is_368_bytes`) is **already
  `#3447`** and is not re-filed here.
- **Description**: the doc's binding-11 row reads *"`GpuRayBudget` — 8 × `u32`
  (32 B): `rayBudgetCount`, `glassRayLimit`, `directShadowSamples`,
  `maxPathSegments`, `maxShadedHits`, `volumetricLightCap`, `qualityTier`,
  reserved. Only word 0 is the CPU-zeroed atomic counter; sizing a
  range/flush/barrier from `u32` is 28 B short."* The struct now carries nine
  further RT-LOD telemetry words (`lod_fragments`, `lod_bin_0..3`,
  `reflection_traced`, `reflection_lod_culled`, `gi_traced`, `gi_lod_culled`),
  matching `bindings.glsl:415-423` one-for-one — 17 words, 68 B. Nor is word 0
  still the only shader-written word: `triangle.frag:795-800`, `:2627-2628` and
  `:3576-3577` `atomicAdd` into the telemetry tail, and
  `collect_rt_lod_telemetry` (`scene_buffer/descriptors.rs:237-251`) reads it
  back. Separately, Set 1 Binding 19 has existed since the selected-ray-probe
  work but never reached the table, so a reader adding a binding would pick 19
  as the next free slot.
- **Evidence**: `GpuRayBudget::WORDS = 17` and the 17-element `words()` array
  (`ray_budget.rs:33-54`); `bindings.glsl` declares 17 `uint` members;
  `buffers.rs:852` writes binding 19 and `bindings.glsl:479` declares it.
  Verified the *code* is internally consistent and safe: every size, range and
  flush already derives from `std::mem::size_of::<GpuRayBudget>()`
  (`descriptors.rs:221`, `buffers.rs:824`), so the doc's own "28 B short" warning
  is describing a hazard the code does not have — only the doc is stale. **No
  Vulkan change is proposed by this finding.**
- **Impact**: documentation only. The hazard is prospective: the stale row is
  precisely the one that warns a future author about sizing a barrier from the
  wrong type, and it now understates the struct by 36 B rather than the 28 B it
  names; an absent binding-19 row invites a descriptor-slot collision.
- **Suggested Fix**: update the binding-11 row to 17 × `u32` (68 B), list the
  telemetry words, and correct "only word 0 is CPU-zeroed / shader-written"; add
  a binding-19 row for `SelectedRayProbeBuffer` (`GpuSelectedRayProbe`, 144 B,
  pinned by `selected_ray_probe_is_144_bytes_std430_compatible`,
  `gpu_instance_layout_tests.rs:80`). `#3447`'s own suggested remedy — an
  automated size-literal check instead of a sixth manual sweep — would have
  caught all four rows at once and remains the better fix.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D2-03

## Dedup cross-reference

Adjacent to **#3447** (`shader-pipeline.md`, same document, different rows). #3447 scopes
the `GpuInstance`/`GpuCamera` size literals; this is the binding-11 `GpuRayBudget` row and
the absent binding-19 row. Fold into #3447 rather than fixing separately if that issue is
in flight.


## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
