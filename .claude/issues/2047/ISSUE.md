# TD3-101: feature-matrix.md still lists NPC AI/behavior as entirely unstarted despite 7 shipped M42 procedure runtimes

**GitHub Issue**: #2047
**Labels**: medium,tech-debt,documentation

**Severity**: MEDIUM
**Dimension**: 3 (Stale Documentation & Comments)
**Location**: `docs/feature-matrix.md:73,172`

## Description
Last touched `1d3190fb` (2026-07-03). Since then, seven M42 AI-package procedure runtimes shipped (Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol, all opt-in via `BYRO_*` env flags) — documented in ROADMAP.md and `docs/engine/npc-spawn-ai-packages.md` (kept current), but not here. The NPC Spawning table's "AI / behavior" row still reads `✗ | ✗ | ✗ | ✗`; the "What Doesn't Work Yet" table still lists the whole category as a live gap. Same failure mode as 4 already-closed feature-matrix issues (#1699/#1703/#1756/#1818) — this specific gap is new/unreported.

## Evidence
`docs/feature-matrix.md:73`: `| AI / behavior | ✗ | ✗ | ✗ | ✗ |` confirmed still present. `ROADMAP.md`'s M42 row (verified in full) documents Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol procedure runtimes landing 2026-07-14 through 2026-07-16, all opt-in gated (`BYRO_SANDBOX_SIT`/`BYRO_WANDER`/`BYRO_TRAVEL`/`BYRO_FOLLOW`/`BYRO_ESCORT`/`BYRO_GUARD`/`BYRO_PATROL`).

## Impact
A reader using feature-matrix.md as the "what works today" reference would wrongly conclude NPCs have zero behavior/AI.

## Related
#1699, #1703, #1756, #1818 (closed, same file, different rows).

## Suggested Fix
Update both rows to `~` (partial) with a footnote naming the 6-7 opt-in-gated procedures and their v0 scope limits (spawn-time-only selection, no per-frame re-evaluation); name the still-genuinely-missing ~10 procedures in the gaps table instead of the whole category.

**Age**: doc last touched 2026-07-03; drift accumulated over the 07-15/16 M42.3–M42.8 commits.
**Effort**: small

## Completeness Checks
- [ ] **TESTS**: N/A (documentation-only fix) — no regression test applicable beyond visual review
