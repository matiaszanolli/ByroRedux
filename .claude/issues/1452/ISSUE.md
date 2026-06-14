# FO4-D5-LOW-02: nif_stats.rs default gate and comment claim all 7 games parse at 100% — wrong for FO4

**Severity**: LOW · **Source**: AUDIT_FO4_2026-06-02 (D5-LOW-02)

**Location**: `crates/nif/examples/nif_stats.rs:54-60`

## Description

The constant and its doc-comment read:
```rust
/// Default success rate gate. All 7 supported games ship at 100%
/// (ROADMAP "Full-archive parse rates: ALL 7 games at 100%") — any drop
/// is a vanilla regression. Override via `NIF_STATS_MIN_SUCCESS_RATE`
const DEFAULT_MIN_SUCCESS_RATE: f64 = 1.0;
```

Both claims are incorrect:
1. The quoted ROADMAP phrase does not exist — no such sentence appears in ROADMAP.md.
2. FO4 ships at **96.46% clean / 100% recoverable** (33,757 / 34,995 NIFs). Running `cargo run -p byroredux-nif --example nif_stats -- "Fallout4 - Meshes.ba2"` without `NIF_STATS_MIN_SUCCESS_RATE=0.96` exits non-zero with "parse success rate 96.46% is below the 100.00% threshold" — a false failure signal that would mislead a developer investigating parse regressions.

Note: `parse_real_nifs.rs` uses `MIN_RECOVERABLE_RATE = 1.0` (which FO4 does meet), not the clean rate. The nif_stats tool applies the clean rate gate, which is a different metric.

## Fix

Update `nif_stats.rs:54-60`:
```rust
/// Default success rate gate. Games with a known FaceGen/drift truncation tail
/// (FO4 at 96.46%, FO76, Oblivion, Starfield) require `NIF_STATS_MIN_SUCCESS_RATE`
/// override; games with fully-clean archives (FO3, FNV, Skyrim SE) pass at 1.0.
/// Override via `NIF_STATS_MIN_SUCCESS_RATE=<0.0..=1.0>` env var.
///
/// See ROADMAP compatibility matrix for per-game clean-parse floors.
const DEFAULT_MIN_SUCCESS_RATE: f64 = 1.0;
```

Or lower the default to ~0.96 with a note that FO4+ require the override.

## Completeness Checks
- [ ] **SIBLING**: Check if any CI script invokes `nif_stats` without the env override against FO4 data
- [ ] **TESTS**: n/a (comment-only fix)

_Filed from [docs/audits/AUDIT_FO4_2026-06-02.md](../blob/main/docs/audits/AUDIT_FO4_2026-06-02.md)_
