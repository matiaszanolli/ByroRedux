# PERF-D9-02: GPU timer readback issues up to 12 separate driver round-trips per frame

**Labels**: low, performance, renderer, bug

**Severity**: LOW
**Dimension**: Telemetry & Origin Cost
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`crates/renderer/src/vulkan/gpu_timers.rs:245-345`

## Description
GPU timer readback issues up to 12 separate driver round-trips per frame via individual `read_bracket` calls gated on `active_bits`, rather than one batched `get_query_pool_results(pool, 0, 24, ...)` call.

Verified current: `read_and_reset` (`crates/renderer/src/vulkan/gpu_timers.rs`) still reads brackets individually per `active_bits` flag rather than in one batched query-pool read.

## Impact
Minor host-side driver overhead only.

## Suggested Fix
A single `get_query_pool_results(pool, 0, 24, ...)` batched read would replace up to 12 per-bracket calls; would need to handle the "unwritten queries never become available under WAIT" caveat already documented in the code for a full-pool read.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix if implemented (e.g. asserting a bounded number of `get_query_pool_results` calls per frame)
