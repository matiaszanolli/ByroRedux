# 2137: CONC-D4-NEW-01: The only CI job that boots the real engine does not enable the ABBA lock-order detector

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2137
**Labels**: bug, medium, sync

---

## Severity
MEDIUM

## Dimension
Scheduler Access Declarations — `/audit-concurrency` 2026-07-25

## Location
`.github/workflows/ci.yml:131-173` (`vulkan-validation`) vs `.github/workflows/ci.yml:75-87` (`lock-order-check`)

## Description
`BYRO_LOCK_ORDER_CHECK=1` is set only on the `lock-order-check` job (`cargo test --workspace`, single-threaded hand-built `World`s). The `vulkan-validation` job — the only CI job that boots the actual engine (`cargo run -p byroredux -- --bench-frames 5` under lavapipe, debug profile so `global_order` is compiled in) — does not set the env var, so the detector is compiled in but inert.

Confirmed against current `.github/workflows/ci.yml`: the `vulkan-validation` step exports only `VK_ICD_FILENAMES`, `VK_INSTANCE_LAYERS`, `RUST_LOG` — no `BYRO_LOCK_ORDER_CHECK`.

## Evidence
```yaml
# ci.yml vulkan-validation step
export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json
export VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation
export RUST_LOG=error
```
No `BYRO_LOCK_ORDER_CHECK` anywhere in that job. `ENABLED` is a `LazyLock<AtomicBool>` seeded from `std::env::var_os` at first touch (`lock_tracker.rs:216-217`).

## Impact
The live 5-frame run is the only place in CI where rayon actually dispatches the real parallel batch across worker threads against a real loaded world — precisely the workload the cross-thread graph was built for — and it is the one place the detector is switched off. This is the structural reason CONC-D5-01/-02/-03 (#2134, #2135, and the sibling issue filed above) were never caught: they require a live `PhysicsWorld`/real cell, which only this job provides, and this job runs with the detector disabled.

## Related
#1410 (TS-02, closed — built the detector), same class as CONC-D5-01 (#2134), CONC-D5-02 (#2135), CONC-D5-03, CONC-D4-NEW-03 (filed separately).

## Suggested Fix
Add `BYRO_LOCK_ORDER_CHECK: 1` to the `vulkan-validation` job's `env:` block (or add a second `--bench-frames` invocation with it set). Cost is negligible for a 5-frame run.

## Completeness Checks
- [ ] **TESTS**: Re-run CI to confirm the detector now fires on the current (pre-fix) code and stays quiet once CONC-D5-01/-02/-03 land
