//! Tuning constants for BLAS / TLAS lifecycle decisions.
//!
//! Split out from the monolithic `acceleration` module so all sizing
//! and threshold values live in one place. Consumers are the sibling
//! submodules inside `acceleration`.

use ash::vk;

/// Slack margin on BLAS-build scratch shrink. The persistent scratch
/// buffer shrinks only when it's both >2× the new peak AND the
/// absolute excess exceeds this margin — keeps shrink decisions
/// stable across adjacent cell loads with similar high-water marks.
/// 16 MB is the scale BLAS scratch lives at (a single 80–200 MB
/// build is plausible; the slack is ~10% of that). See `#495` and
/// `scratch_should_shrink`.
pub(super) const BLAS_REBUILD_SLACK_BYTES: vk::DeviceSize = 16 * 1024 * 1024;

/// Slack margin on TLAS instance-buffer shrink (`#645` / MEM-2-3).
/// TLAS instance buffers are 64 B/entry and live at MB scale, so the
/// BLAS-scratch 16 MB slack would effectively never trigger — a
/// 32 K-instance peak buffer is only ~2 MB. 1 MB ≈ 16 K instances:
/// wide enough to absorb adjacent-cell-load variance, narrow enough
/// to actually fire when a big exterior peak settles back into a
/// small interior working set. See `tlas_instance_should_shrink`.
pub(super) const TLAS_REBUILD_SLACK_BYTES: vk::DeviceSize = 1024 * 1024;

/// Lower bound on TLAS instance-buffer capacity. The build path
/// pre-sizes to `max(2 × instance_count, MIN_TLAS_INSTANCE_RESERVE)`
/// — covers interior cells (~200-800) and exterior cells (~3000-5000)
/// without resizing on cell-streaming transitions through
/// low-instance frames. Trades ~1 MB BAR per FIF slot on small cells
/// for stable build performance. See REN-D8-NEW-10 / REN-D2-NEW-02.
pub(super) const MIN_TLAS_INSTANCE_RESERVE: u32 = 8192;

/// Lower bound on the post-shrink TLAS working-set capacity. Matches
/// the build-path floor `MIN_TLAS_INSTANCE_RESERVE` so a shrink
/// targeting a tiny working set can't churn below the floor — the
/// next build would just re-pad back to it and we'd burn a
/// free+create cycle for no behavioural change.
pub(super) const WORKING_SET_FLOOR: u32 = MIN_TLAS_INSTANCE_RESERVE;

/// Minimum BLAS-budget floor. Computed budget is `device_local / 3`
/// capped no lower than this — keeps the 90% eviction trigger
/// meaningful even on small-VRAM devices where `total / 3` would be
/// a small absolute number. 256 MB matches the typical cell BLAS
/// footprint. See `compute_blas_budget`.
pub(super) const MIN_BLAS_BUDGET_BYTES: vk::DeviceSize = 256 * 1024 * 1024;

/// REFIT-count threshold beyond which a skinned BLAS is dropped and
/// rebuilt to reset the BVH bounds. 600 frames ≈ 10 s @ 60 FPS —
/// long enough to amortise the rebuild cost over many cheap refits,
/// short enough that the worst-case animation cycle doesn't drift
/// far past the original BVH. See #679 / AS-8-9.
pub const SKINNED_BLAS_REFIT_THRESHOLD: u32 = 600;

/// How often to check the eviction threshold inside the batched BLAS
/// build. Every N buffers created we test
/// [`should_evict_mid_batch`]; eviction runs only when needed, so the
/// idle cost is one add + one compare per N iterations.
pub(super) const BATCH_EVICTION_CHECK_INTERVAL: usize = 64;
