# REN-D5-01: the Bink movie overlay texture has no `memory-budget.md` ledger row

Labels: low, renderer, memory, documentation, doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/7030

**Severity**: LOW
**Source**: `docs/audits/AUDIT_RENDERER_2026-11-16.md` (renderer audit, delta since 2026-11-09)
**Location**: `VideoPlayer::texture` / `tick` (`byroredux/src/video.rs`, added in `6e64c453b`); `docs/engine/memory-budget.md` ("MenuXml HUD overlay texture", "Dynamic RGBA staging arenas")
**Status**: NEW (validated against HEAD; no matching open issue)

## Description
One `register_rgba` handle is kept for the session, plus a retained host pixel copy and a grow-only staging arena sized by the movie frame (about 8.3 + 8.3 + up to 16.6 MB at 1080p). `grep -i "video|movie|bink"` in `memory-budget.md` returns nothing and the staging-arena consumer list omits the movie player.

**Impact**: ledger gap, not a leak.

## Suggested Fix
Add the row and list the movie player as a consumer; optionally release the handle in `VideoPlayer::finish`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
