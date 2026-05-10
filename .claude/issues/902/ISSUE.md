# #902 — FNV-D5-NEW-03: R6a-stale-7 gating clause "M41 hasn't landed" is falsified

**Severity**: LOW (observability / rationale-text drift)
**Source audit**: `docs/audits/AUDIT_FNV_2026-05-08.md` § Dimension 5
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/902
**Status**: NEW · CONFIRMED at HEAD `318fcaf`

## Location

`ROADMAP.md` → "Open — Risk-reducers (2026-04-22)" → R6a-stale-7 row (around line 512)

## Summary

R6a-stale-7's rationale reads "Refresh still deferred until M41 lands the visible-actor workload that exercises the new code paths." That precondition is met — M41-EQUIP B.2 landed in session 31 (#896 Phases A.0 → B.2); Prospector entity count grew 1200 → 2562 at HEAD `318fcaf`, FPS dropped 172.6 → 143.7.

Bench-staleness itself is not a finding (R6a-stale-7 is already filed). This narrowly corrects the falsified rationale text so the next reader doesn't rely on a stale precondition.

The rationale text being in the way of recognising the deferral cost real signal is load-bearing in practice — see #900 for a regression a fresh bench would have caught.

## Fix path

Replace R6a-stale-7's rationale text with:

> "M41-EQUIP B.2 landed in session 31 (#896 Phases A.0 → B.2); Prospector entity count grew from 1200 to 2562 at HEAD `318fcaf`, FPS dropped from 172.6 to 143.7 (-17%) on the new entity volume. Bench refresh is overdue, not deferred — see #900 / FNV-D5-NEW-01 for a defect this would have caught earlier."

## Related

- ROADMAP R6a-stale-7 — parent Known Issue, not a separate GitHub issue
- #896 — M41-EQUIP work that falsified the precondition
- #900 — the regression a fresh bench would have caught
