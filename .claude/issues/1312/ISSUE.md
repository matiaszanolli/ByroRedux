# #1312 -- OBL-D6-NEW-01: ROADMAP Oblivion exterior-blocked framing is stale

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: MEDIUM | **Dim 6** — Blockers & Game-Specific Quirks
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D6-NEW-01)

**Location**: `ROADMAP.md:161, :229, :304` (Known Issues / compat matrix / M32.5 row)

**Issue**: ROADMAP.md still presents "Oblivion exterior blocked on TES4 worldspace + LAND wiring" as the live top blocker. That work shipped (#965 `ac69ae5e` + the exterior cell loader). Empirically: Tamriel grid 0,0 radius-1 = 9 cells / 4886 entities / 150.6 FPS on RTX 4070 Ti. The stale framing misdirects contributors and silently perpetuates the pre-#699 "BSA v103" narrative.

**Suggested fix**: update ROADMAP:161/229/304 to reflect the current state — "Oblivion exterior renders end-to-end; residual gaps are render-completeness (ocean water `OBL-D6-NEW-02`, normal maps `OBL-D4-NEW-01`)."

## Completeness Checks
- [ ] **SIBLING**: check HISTORY.md + docs/engine/game-compatibility.md for the same stale framing
- [ ] **TESTS**: no code change; doc-only
- [ ] **CANONICAL-BOUNDARY**: doc-only
- [ ] **UNSAFE**: no unsafe involved
