# #2377 — Exterior runtime readiness epic

Top-level epic ("Canonical execution plan: docs/engine/exterior-readiness-plan.md")
listing 9 dependency issues (#2368, #2375, #2376, #2374, #2370, #2371,
#2373, #2369, #2372) plus 2 already-closed/related (#2340, #2348). Not a
scoped bug — no code fix possible here; this session's work was pure
triage/investigation.

## Dependency-chain audit (2026-08-31)

Initial `gh issue view --json state` sweep showed all 9 dependencies as
CLOSED. Deeper investigation found this was misleading:

- **#2369 (EX-14/15)** was closed by an **accidental commit-keyword
  auto-close** — commit `a47dcf0c`'s subject `Fix #2369: ...` triggered
  GitHub's closing-keyword scan even though the commit's own body and the
  issue's own last comment both explicitly said "#2369 is a large epic...
  remains open... EX-14 ground cover/trees and EX-15's parent-worlds/
  FO4-spatial-data halves are untouched." **Reopened.** Filed a memory
  note on this failure mode (`feedback_multi_issue_commit_close.md`).
- **#2372 (EX-16)** was closed last turn (this session) as
  "superseded/split into sub-issues," NOT as completed — 0 of its 6
  acceptance criteria were fully done at closure time.
- The other 7 dependencies (#2368, #2370, #2371, #2373, #2374, #2375,
  #2376) were checked for the same accidental-close pattern (their close
  events all have `commit_id: null`, i.e. closed manually via `gh issue
  close`, not by keyword auto-close) and their closing comments describe
  genuine, verified completion (live validation runs, acceptance criteria
  enumerated and checked). No contradiction found — these 7 appear
  legitimately done.

## Duplicate-issue cleanup

While auditing #2372's split, found that 3 of the 6 sub-issues I filed
last turn (#3801, #3803, #3804) duplicated **pre-existing** open issues
that already covered the same scope, better:
- #3801 (REGN Weather/Grass/Landscape/Objects) → duplicate of **#3301**
  ("EX-16 items 1+5 remainder"). Closed, pointed at #3301.
- #3803 (actor/package migration) → duplicate of **#3299** ("EX-16 item
  4: actor/package state snapshot/restore"). Closed, pointed at #3299 —
  also noted #3299 is now actionable since its step-2 blocker (#2369 item
  C2) landed.
- #3804 (per-emitter audio ownership) → duplicate of **#3301**. Closed,
  cross-posted its ownership/reclaim framing onto #3301 so it isn't lost.

#3802 (cross-tile NAVM) and #3806 (boundary/soak tests) were NOT
duplicates — confirmed via search, kept open as filed.

## True current state of #2377's definition-of-done

Genuinely NOT all done, despite the dependency checklist reading
"complete":
- #2369 (EX-14/15) — reopened, real remaining scope (ground cover/trees,
  parent-worlds/FO4-spatial-data).
- #2372 (EX-16) — closed-as-split; remaining scope lives in #3299
  (actor/package snapshot-restore, now unblocked), #3301 (REGN
  Weather/Map/Landscape/Objects/Grass/Imposter + incidental audio
  emitter), #3802 (cross-tile NAVM geometric join), #3806 (boundary/soak
  tests, blocked on #3299 and #3802).
- Adjacent open issues under the same `terrain-exterior` label not part
  of the original 9-dependency list but relevant to "join without overlap
  or gaps": #3307 (active VWD culling), #3308 (reversed-Z depth) — both
  deferred pending GPU validation per an earlier session's explicit
  decision; #3306 (a live FO4 terrain-seam crack the `terrain.seams` gate
  caught, unrelated to this triage); #3142 (a perf finding, unrelated).

## Decision
Leave #2377 open. Posting an honest status comment correcting the
"all dependencies closed" appearance and pointing at the real remaining
work. Not closing it — that would misrepresent completion the same way
the naive dependency-checklist read would have.
