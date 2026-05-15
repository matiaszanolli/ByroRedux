# #1050 — Tech-Debt: Test hygiene [batch]

**Labels**: enhancement, low, tech-debt
**Source audit**: `docs/audits/AUDIT_TECH_DEBT_2026-05-13.md` (TD6-001 through TD6-010)

## Status per sub-finding

| ID | Status | Commit / Note |
|----|--------|---------------|
| TD6-001 | FIXED | 5196627d — deleted print-only `extract_and_parse_nif` |
| TD6-002 | FIXED | 5196627d — renamed `bench_*` → `manual_bench_*` |
| TD6-003 | RESOLVED | #833 pinned by `bulk_readers_decode_le_byte_order`; #823 by lock_tracker tests:410+. #824/#828/#830/#832 are perf-only — downgraded to "documented patterns" (no meaningful unit invariant to assert) |
| TD6-004 | FIXED | 5196627d — reframed mtidle_motion_diagnostic.rs module doc as "Regression pin" |
| TD6-005 | FIXED | #1058 — `crates/plugin/src/esm/test_paths.rs` centralises all game-data paths |
| TD6-006 | DEFERRED | Golden-frame coverage expansion — substantial, tracked in ROADMAP |
| TD6-007 | DEFERRED | Synthetic NIF corpus for CI — substantial, tracked in ROADMAP |
| TD6-008 | FIXED | 5196627d — added skinning_e2e row to smoke-tests README |
| TD6-009 | SKIP | Rust integration tests can't import from `src/` without pub visibility; `tests/common/mod.rs` IS the correct convention |
| TD6-010 | FIXED | `scripts/test-summary.sh` — prints pass/fail/ignored counts with per-category breakdown and ignore-count sentinel |
