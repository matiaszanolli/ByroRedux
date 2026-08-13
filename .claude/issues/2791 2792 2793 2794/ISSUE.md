# Batch: #2791 #2792 #2793 #2794

## #2791 — REN-D5-04: allocate_scene_render_buffers bind-inverse staging comment understates size by 87x (144 KB vs ~12.6 MB)

**Labels**: documentation, renderer, low
**Location**: `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`allocate_scene_render_buffers`)

The bind-inverse staging comment computes "16 × 144 × 64 ≈ 144 KB"; the
constant is 1366, making it ≈ 12.6 MB — an 87× understatement next to
the second-largest host-visible allocation the renderer makes.
`constants.rs` and `memory-budget.md` are both correct; only this site
is stale.

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D5-04).

---

## #2792 — REN-D15-09: submersion_system WaterVolume-absent exit leaves stale submersion state

**Labels**: bug, renderer, low
**Location**: `byroredux/src/systems/water.rs` (`submersion_system`)

Two "no water data" exits with opposite behaviour: the `WaterPlane`-absent
exit resets `SubmersionState` to default with a comment explaining why;
the `WaterVolume`-absent exit twenty lines later is a bare `return`, so
the camera keeps `head_submerged: true` and a stale `material` that
`compute_underwater_params` then feeds indefinitely. Only separable via
`spawn_lod_water_plane` (#2449), which inserts `WaterPlane` without
`WaterVolume` — a state in which the camera cannot already be submerged,
so today's outcome is "no reset needed". Defence-in-depth gap.

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D15-09).

---

## #2793 — REN-D5-05: collect_image_health writes through mapped_slice_mut with no invalidate/flush primitive documented

**Labels**: bug, renderer, low, memory
**Location**: `crates/renderer/src/vulkan/context/resources.rs` (`collect_image_health`), `crates/renderer/src/vulkan/buffer.rs`

The health counter is read and rewritten through `mapped_slice_mut` with
neither invalidate nor the flush `mapped_slice_mut`'s own doc mandates;
`GpuBuffer` has no invalidate primitive at all. Benign only because
gpu-allocator 0.27 puts `HOST_COHERENT` in the required flag set for
`CpuToGpu` — and nothing in the source says so.

Documentation half of REN-D4-04 / REN-D4-05.

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D5-05).

---

## #2794 — REN-D5-06: deferred_destroy.rs module doc claims two production users, there are three (pending_destroy_scratch omitted)

**Labels**: documentation, renderer, low
**Location**: `crates/renderer/src/deferred_destroy.rs`

Module doc claims "two production users"; there are three —
`pending_destroy_scratch` (#1782's fix) is omitted, so a reader auditing
deferred-destroy coverage concludes the shared BLAS scratch is *not* on
the countdown path, which is the exact wrong conclusion that produced
#1782. Both `DEFAULT_COUNTDOWN` cross-references are rotted.

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D5-06).
