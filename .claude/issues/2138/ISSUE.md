# 2138: CONC-D4-NEW-02: vulkan-validation CI job swallows the boot-time access-invariant debug_assert failures

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2138
**Labels**: bug, medium, sync

---

## Severity
MEDIUM

## Dimension
Scheduler Access Declarations — `/audit-concurrency` 2026-07-25

## Location
`.github/workflows/ci.yml:163-172`; guards at `byroredux/src/boot.rs:1002-1030`

## Description
The three #1394/#1602 guards (`undeclared_parallel_count`/`known_conflict_count`/`unknown_pair_count`, all `debug_assert_eq!(..., 0)`) live in `install_runtime_registries`, called from `App::new` — before the event loop, so they do execute in the `vulkan-validation` job. But the step runs `OUTPUT=$(... cargo run ... 2>&1 || true)` and fails **only** if the output contains the literal substring `[Vulkan]`. A `debug_assert` panic's text contains no such substring, so the job goes green on a tripped guard.

## Evidence
Confirmed against current `.github/workflows/ci.yml`:
```bash
OUTPUT=$(xvfb-run --auto-servernum cargo run -p byroredux -- --bench-frames 5 2>&1 || true)
echo "$OUTPUT"
if echo "$OUTPUT" | grep -qF '[Vulkan]'; then
  exit 1
fi
```
`|| true` swallows any non-zero exit code (including a panic); the sole failure predicate is the `[Vulkan]` substring match. Panic text from `boot.rs:1011/1023/1029` contains no `[Vulkan]` marker.

## Impact
These guards are the primary regression pin for the whole scheduler-access-declaration dimension, and they are currently enforced by nothing in CI: `cargo test` never calls `build_scheduler` (it's `pub(crate)`, sole caller `App::new`), and the one job that does call it discards the exit code. A future `add_to()` or a new conflicting pair (the exact #1601 shape) would reach `main` with a green CI. Today's state is fine (verified statically), so this is a guard-integrity gap, not a live defect.

## Related
#1394 (closed), #1601 (closed), #1602 (closed), `byroredux/src/scheduler_access_tests.rs`, CONC-D4-NEW-01 (same job, adjacent gap, filed separately).

## Suggested Fix
Cheapest — also fail the step on a `panicked at` substring, or capture the real exit code (`set -o pipefail`, keep `|| true` only for the known "no suitable device" bail, matched explicitly). Sturdier — since `scheduler_access_tests.rs` is already compiled into the bin's test binary, add a real `cargo test` asserting the three counts are 0, replacing the `include_str!`-grep proxies.

## Completeness Checks
- [ ] **TESTS**: A regression test / CI change pins this specific fix
