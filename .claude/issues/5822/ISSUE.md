# TD2-2026-10-15-21: The metrics snapshot exists in four hand-synced copies and three hand conversions

Labels: low,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-21`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/core/src/ecs/metrics.rs:28` (`MetricsSnapshot`), `crates/debug-protocol/src/lib.rs:244` (`DebugResponse::Metrics`), `tools/byro-dbg/src/tui.rs:180` (`MetricsView`, built at `:232`), `crates/debug-ui/src/panels.rs:385` (`MetricsSnapshotView`, built at `byroredux/src/main.rs:1463`); conversion at `crates/debug-server/src/evaluator.rs:174-197`
- **Status**: NEW
- **Age**: `0925f7926` (2026-09-26) added fields across 9 files
- **Effort**: small
- **Description**: ~20 fields each. The debug-ui copy's doc claims "the debug-ui crate doesn't depend on core's resource types directly", but `crates/debug-ui/Cargo.toml:7` depends on `byroredux-core`, so `MetricsSnapshotView` and the 20-line `main.rs` conversion can simply go. The protocol crate genuinely cannot depend on core, so the wire variant stays, but `MetricsView` in byro-dbg is a third flatten of it; one pinned test covers only core <-> wire.
- **Evidence**: `git log -S driver_vram_total_mb --name-only` -> 9 files in one commit.
- **Impact**: each new telemetry field is 9 files and the unpinned copies can lag silently.
- **Related**: #2513.
- **Suggested Fix**: debug-ui reads `byroredux_core::ecs::MetricsSnapshot` directly (delete the view and the `main.rs` mapping); in `debug-protocol` make the variant `Metrics(MetricsPayload)` with a `#[derive(Serialize, Deserialize)]` struct that byro-dbg stores as-is (delete `MetricsView`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
