# #4993: CONC-D3-2026-09-28-05: The `lock-order-check` lane captures `tee`'s exit status, so it is green on every failure except a detected cycle, including a test-build compile error

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,concurrency,test-gap,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW. This is a CI-guard vacuity. The grep gate still catches real `lock-order cycle` panics.
- **Dimension**: ECS Lock Ordering (CI dynamic supplement)
- **Location**: `.github/workflows/ci.yml:218-228`.
- **Status**: NEW. The pattern arrived with the #4603 rewrite.
- **Description**:
  - The step runs `cargo test --workspace --no-fail-fast --exclude byroredux-ui 2>&1 | tee /tmp/lockorder_test.log`, then `status=$?`, and later `exit $status`.
  - No `shell:` key appears anywhere in `ci.yml`. GitHub's default `run` shell is therefore `bash -e {0}`, with no `pipefail`.
  - `$?` is `tee`'s status, which is always 0. `exit $status` is dead, and only the `grep -q "lock-order cycle"` branch can fail the job.
  - The #4603 comment says the goal is for "a lock failure [to] be distinguishable from any other red". The other reds were meant to stay red.
- **Evidence**:
  - At a070baaad, CI "ABBA lock-order detector" passes. "Test + Check + Clippy" fails on the same tree, which has a failing `cli_args::tests::renderer_config_defaults_to_fsr_quality`.
  - The local `BYRO_LOCK_ORDER_CHECK=1 cargo test --workspace --no-fail-fast` reproduces that one failure.
- **Trigger Conditions**: Any compile error in a test target, or any test failure that is not a detector cycle.
- **Impact**:
  - A compile break in the test build means zero tests run under the detector while the lane reports green.
  - This is the exact "lane can't tell" failure mode #4603 was fixing.
  - Same-thread reentrancy panics (`ECS deadlock detected: …`, `lock_tracker.rs:103/206/213`) do not contain "lock-order cycle". They are also caught by the Test job, so they are not lost, but this lane does not flag them.
- **Verification Path**: Push a branch with a deliberate `#[test] fn f(){panic!()}`. The lane stays green.
- **Related**: #4603, #4595, CONC-D3-2026-09-28-03.
- **Suggested Fix**:
  - Use `status=${PIPESTATUS[0]}`, or add `set -o pipefail` or `shell: bash`.
  - Optionally extend the grep to `ECS deadlock detected`.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D3-2026-09-28-05) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related systems / CI steps
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition and the `docs/engine/ecs.md` canonical order are preserved
- [ ] **TESTS**: A regression test pins this specific fix
