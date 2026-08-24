# 3282: TD1-2026-08-24-01: draw_frame re-grew to 2498 LOC - 51% of a 4909-line file

**Severity**: LOW · **Report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-24.md` (TD1-2026-08-24-01)

## Description

`draw_frame` is 2498 LOC in a 4909-line file — the single largest function in the codebase's primary Dim-1 bucket. Not tangled: a linear pipeline with clear phase boundaries, already delegating its tail to `record_geometry_pass`/`record_post_passes` (#2258). What remains inline: frame-sync, command-buffer begin, TLAS build, camera/light/TAA/DOF assembly, skin/bone GPU upload + dispatch, cluster-cull dispatch, instance-SSBO build+upload, material/terrain reupload.

## Location

`crates/renderer/src/vulkan/context/draw.rs:1522-4020` (`pub fn draw_frame`)

## Evidence

`wc -l` → 4909 total; `draw_frame` is 51%. `record_geometry_pass`/`record_post_passes` calls at `:3611`/`:3655`. #2255 (closed 2026-07-25) covered an earlier regrowth of this same function — this is the pattern recurring a second time.

## Impact

Maintenance cost only. `draw.rs` grew 4730→4909 total lines in four days entirely inside this function.

## Related

#2255 (closed, prior instance), #2258/#2259 (extractions to mirror).

## Suggested Fix

Extract along existing phase boundaries into private `VulkanContext` helpers: `sync_and_acquire_frame`, `begin_frame_recording`, `assemble_camera_and_lights`, `dispatch_skin_and_cluster`, `build_and_upload_instances`. Mechanical — preserve barrier/dispatch order verbatim; needs a live-engine smoke run to confirm no behavioral drift.

## Completeness Checks
- [ ] **DROP**: Vulkan object teardown/lifetime unaffected by the extraction
- [ ] **SIBLING**: Extraction pattern matches `record_geometry_pass`/`record_post_passes`
- [ ] **TESTS**: Live-engine smoke run confirming no behavioral drift
