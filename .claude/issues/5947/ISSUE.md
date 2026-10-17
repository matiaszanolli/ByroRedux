# REN-VOL-L-02: `initialize_layouts` destroys the noise staging buffers unconditionally, ignoring the #4891 `MaybeInFlight` policy

Labels: low,renderer,vulkan,memory,bug
Source: docs/audits/AUDIT_RENDERER_2026-10-17.md

- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (Volumetrics)
- **Location**: `volumetrics/init.rs`, `initialize_layouts` tail (`for buffer in staging.iter_mut() { buffer.destroy(…) }` before `upload_result?`); same pattern in `cloud_noise.rs` (SKYAL, shared `noise::create_density_noise_staging`). The callers then destroy the whole pipeline on that error (`context/init.rs` volumetrics init arm, `context/resize.rs` `recreate_bloom_and_volumetrics`).
- **Status**: NEW
- **Description**: the comment says the one-time submit "waits for the queue before returning, so neither staging buffer can still be referenced". That is false for `OneTimeCommandError::MaybeInFlight`: a failed `queue_submit`, or a `vkWaitForFences` failure other than DEVICE_LOST (#5270), leaves the copies possibly pending. The repo's rule (#4891; `buffer.rs`, `blas_static.rs`) is "destroy on `NotSubmitted`, leak on `may_be_in_flight`". The helper itself leaks `cmd` and the fence on that class, so the staging free here is the one remaining non-conforming disposal. Reachable only on device loss / OOM during init or resize.
- **Suggested Fix**: `if OneTimeCommandError::may_be_in_flight(&e) { mem::forget(staging) }` else destroy; decide the same for the froxel images the caller destroys. Apply to both consumers of the shared helper. Failure-path only and not testable on a device-free run, so frame it as policy conformance.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other init/teardown sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_RENDERER_2026-10-17.md` (area-scoped `volumetrics-deep` suite run, HEAD e16a624e9). Fix with `/fix-issue <number>`._
