# TD3-205: feature-matrix.md's Fire-refraction row (yesterday's own fix) cites #2236/#2237 as open — both closed same day

**Severity**: MEDIUM
**Dimension**: 3 (Stale Documentation & Comments)
**Location**: `docs/feature-matrix.md:51`
**Labels**: medium, renderer, tech-debt, documentation
**Source**: `docs/audits/AUDIT_TECH-DEBT_2026-08-03.md`

## Description
Yesterday's TD3-202 fix (`b9a68eea`) added a Fire refraction row citing #2236/#2237
as "known consistency gaps." Both issues were closed the same day via `cd6a8338`
("Fix #2234, #2235, #2236, #2237: renderer audit findings"), confirmed via
`gh issue view` (`closedAt: 2026-08-03T02:06Z` for both). The row went stale within
roughly six hours of being written — the fastest doc-rot turnaround observed across
any cycle of this audit.

## Evidence
`docs/feature-matrix.md:51`; `gh issue view 2236 2237 --json state,closedAt`.

## Impact
This is the 4th consecutive cycle (07-16, 07-25, 08-02, 08-03) the exact same
feature-matrix-lags-shipped-code pattern has recurred — worth treating as a process
gap rather than four independent doc bugs.

## Related
TD3-101, TD3-NEW-03, TD3-202 (all closed, same file, same pattern, third-through-fourth
recurrence).

## Suggested Fix
Remove or update the "known consistency gaps" clause to reflect #2236/#2237 as
resolved; if any residual gap remains, name it specifically rather than by
now-closed issue number.

## Age / Effort
~18 hours. Effort: trivial.
