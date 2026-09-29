# TD1-2026-09-29-05: `streaming.rs` crossed to 2069 (+316)

**Labels**: low,terrain-exterior,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 1 · **Status**: NEW · **Effort**: small
- **Location**: `byroredux/src/streaming.rs`
- **Age**: `a3632909a` (texture prefetch, 09-27) and the #3659/#4999/#5000 archive-read fixes
- **Suggested Fix**: split three ways.
  - Telemetry (`StreamingLatencySummary`, `StreamingTelemetry`, `phase_distribution`; 48–470) →
    `streaming/telemetry.rs`.
  - Worker + pre-parse pipeline (`join_with_timeout` … `pre_parse_cell`, `ParseInputBudget`; 1114–1937)
    → `streaming/pre_parse.rs`.
  - State and deltas stay.

**Validated at HEAD 9fcfdc3fc**: `prod_loc byroredux/src/streaming.rs` = 2069.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
