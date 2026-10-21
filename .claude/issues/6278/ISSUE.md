# #6278: TOOL-D2-2026-10-21-01: The `metrics_response_mirrors_core_snapshot` guard cited by `DebugResponse::Metrics` does not exist, and two core metrics fields never reach the wire

**Labels**: low,tech-debt,test-gap,bug

**Source**: docs/audits/AUDIT_TOOLING_2026-10-21.md · **Severity**: LOW · **Dimension**: Protocol & Registry · **Location**: `crates/debug-protocol/src/lib.rs:238-244` (variant doc), `crates/debug-protocol/src/lib.rs:257-278` (`MetricsPayload`), `crates/debug-server/src/evaluator.rs:168-198` (`eval_metrics`), `crates/core/src/ecs/metrics.rs` (`MetricsSnapshot.cpu_pass_ms`, `.top_systems_ms`)

- **Severity**: LOW
- **Dimension**: Protocol & Registry
- **Exposure**: developers only (`byro-dbg metrics` / TUI dashboard)
- **Location**: `crates/debug-protocol/src/lib.rs:238-244` (variant doc), `crates/debug-protocol/src/lib.rs:257-278` (`MetricsPayload`), `crates/debug-server/src/evaluator.rs:168-198` (`eval_metrics`), `crates/core/src/ecs/metrics.rs` (`MetricsSnapshot.cpu_pass_ms`, `.top_systems_ms`)
- **Status**: NEW. The skill has carried "report it until a test lands" since #5822; no earlier report filed it, and there is no issue (open or closed) for the guard name.
- **Description**: The variant doc says the field set is pinned so "a one-sided drift breaks the build", and the `f64a299ec` (#5822) commit message says the evaluator's conversion is "pinned as before by ... the server's metrics_response_mirrors_core_snapshot guard". That test exists nowhere in the tree, and `git log -S` finds no trace of it ever existing. The drift it was meant to catch is already present. Core's `MetricsSnapshot` has two more fields than `MetricsPayload`: `cpu_pass_ms` (Phase 8 CPU frame breakdown, `8d4c14522`, 2026-05-24) and `top_systems_ms` (Phase 11 per-system scheduler timings, `1138a5b19`). `eval_metrics` maps neither. The in-engine debug-ui panel shows them (`crates/debug-ui/src/panels.rs:1571-1601`), but the wire client cannot.
- **Evidence**: `grep -rn metrics_response_mirrors_core_snapshot crates byroredux tools` → only the doc comment at `lib.rs:242`. Field list of `MetricsSnapshot` (17 `pub` fields) vs `MetricsPayload` (15) → missing `cpu_pass_ms`, `top_systems_ms`.
- **Impact**: someone triaging a frame-time gap from `byro-dbg` (the "atw_scheduler is 500 ms, which system?" case Phase 11 was built for) sees GPU pass times only, and the doc tells them the payload is complete. The next field added to core will be dropped the same way, with nothing to catch it.
- **Related**: #5822, #2513
- **Suggested Fix**: Add `cpu_pass_ms` / `top_systems_ms` to `MetricsPayload` (serde-default for older clients) and map them in `eval_metrics`, then write the guard the doc cites. One way is an exhaustive destructure of `MetricsSnapshot` in the evaluator (`let MetricsSnapshot { sampled_at_secs, .., top_systems_ms } = &*snap;` with no `..`), which makes a new core field a compile error.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_TOOLING_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
