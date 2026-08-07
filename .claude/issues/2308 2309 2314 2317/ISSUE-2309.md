title:	TD3-205: feature-matrix.md's Fire-refraction row (yesterday's own fix) cites #2236/#2237 as open — both closed same day
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	documentation, medium, renderer, tech-debt
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	2309
--
## Description
Yesterday's TD3-202 fix (`b9a68eea`) added a Fire refraction row citing #2236/#2237 as "known consistency gaps." Both issues were closed the same day via `cd6a8338` ("Fix #2234, #2235, #2236, #2237: renderer audit findings"). The row went stale within roughly six hours of being written — the fastest doc-rot turnaround observed across any cycle of this audit.

## Evidence
- `docs/feature-matrix.md:51` still says: "`MATERIAL_KIND_FIRE_REFRACTION` (103, Session 62) — normal-driven heat-haze distortion proxy; known consistency gaps in shadow masking, G-buffer overwrite, and composition sort order (#2236, #2237)".
- `gh issue view 2236 2237 --json state,closedAt` confirms both `state: CLOSED`, `closedAt: 2026-08-03T02:06:00Z` / `2026-08-03T02:06:02Z`.

## Impact
This is the 4th consecutive audit cycle (07-16, 07-25, 08-02, 08-03) the exact same feature-matrix-lags-shipped-code pattern has recurred — worth treating as a process gap rather than four independent doc bugs. This time it's not just docs lagging shipped code, but a same-day doc fix lagging a same-day issue-closure.

## Related
TD3-101, TD3-NEW-03, TD3-202 (all closed, same file, same pattern, third-through-fourth recurrence).

## Suggested Fix
Remove or update the "known consistency gaps" clause in `docs/feature-matrix.md:51` to reflect #2236/#2237 as resolved; if any residual gap remains, name it specifically rather than by now-closed issue number.

## Completeness Checks
- [ ] **SIBLING**: Check other feature-matrix.md rows citing issue numbers for the same lag pattern before closing
- [ ] **TESTS**: Doc-only fix; not applicable
