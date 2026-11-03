# RT-2026-10-31-09: `m34-day-night`'s multi-day assertion is wall-clock sensitive and fails at today's frame cost

Labels: low,tech-debt,test-gap,bug
Source: docs/audits/AUDIT_RUNTIME_2026-10-31.md

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-31.md` RT-2026-10-31-09 (report HEAD `670f81917`).
**Re-verified at HEAD `7f2d98944`**: `m34-day-night.sh` is unchanged since the report.

- **Severity**: LOW
- **Dimension**: Harness integrity
- **Location**: `docs/smoke-tests/m34-day-night.sh:89-92,129`
- **Status**: NEW
- **Current**: expects `time.advance: day=2 hour=0.000 clock=00:00 phase=night`, observes `hour=0.001`. The script resumes the clock at 120× for the gap between `time.resume` and the next `time.pause` (about 30 ms, which at 120× is 3.6 game-seconds, so 23.001 h), then `time.advance 25` lands at 0.001. Every other leg (sunrise, noon, night, pause/resume) passes.
- **Suggested Fix**: Compare `hour` within 0.01, or pause first and `time.set 23:00` again before `time.advance`.

## Completeness Checks
- [ ] **TESTS**: The gate is re-run live and passes (or SKIPs with 77 on missing data)
