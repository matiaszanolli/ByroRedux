# #5416: CONC-D3-2026-10-08-03: The #5261 `rt-integrity` assertion in the vulkan-validation lane has no source pin

**Labels**: low,concurrency,bug,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5416

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-08.md` — `CONC-D3-2026-10-08-03` (HEAD `00f580e09`)

- **Severity**: LOW (test gap; the assertion itself works — see below).
- **Dimension**: ECS Lock Ordering (the CI-lane half of Dim 3)
- **Location**: `.github/workflows/ci.yml:477-481` (the assertion); the sibling pins in `byroredux/src/scheduler_access_tests.rs:424-445` (`vulkan_validation_job_requires_a_selected_device`, `vulkan_validation_job_fails_on_a_panic`, `vulkan_validation_job_enables_the_lock_order_detector`, `…resolves_lavapipe_and_fails_on_init_failure`).
- **Status**: NEW. `grep -rn 'rt_flag=1\|tlas_build=1'` over `byroredux/` and `crates/` matches only a comment in `scene.rs` and a unit-test string in `ecs/resources/mod.rs`.
- **Description**: `59115f54c` (#5261) restored RT in the lane and added `grep -qE 'rt-integrity:.*rt_flag=1 .*tlas_build=1'`. The assertion works: the run 37833008690 log carries `rt-integrity: frame=7 … rt_flag=1 tlas_build=1 tlas_eligible=4 tlas_emitted=4`, and the `println!` that produces it is on stdout so `RUST_LOG` cannot hide it. But every other assertion in that job is pinned by a `vulkan_validation_job_*` test, and this one, the only guard against the lane silently going blind to RT again (the #4596 / #4987 / #5261 class), is not. A workflow edit that drops it passes `cargo test`.
- **Impact**: The class of regression the last two baseline findings were about can return silently.
- **Related**: #5261, #4987, #4596.
- **Suggested Fix**: Add a `vulkan_validation_job_requires_live_rt` test beside the others asserting the job contains `rt_flag=1 .*tlas_build=1`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
