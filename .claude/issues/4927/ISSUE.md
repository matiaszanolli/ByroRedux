# EXT-D4-2026-09-27-05: skyal.md does not document the interior outdoor-sky lane

**Issue**: #4927
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: low,terrain-exterior,documentation,doc-rot

**Severity**: LOW (doc)
**Dimension**: Sky, weather, sun
**Tier Violated**: n/a
**Game Affected**: n/a
**Status**: NEW. It is distinct from #4875 (volumetrics doc) and #4861 (guard).
**Location**:
`docs/engine/skyal.md` (the only interior mention is line 413)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- The per-frame interior cube/SH bake is unspecified, as are the `sky_lower.w` 1/2 modes, the `jitter.w` gating rule, #4839's portal-cloud lighting and `portal_sun`.
- The #2226/#3323 isolation rule has been superseded in practice, and nothing specifies its replacement. EXT-D4-01 follows from this gap.

## Suggested Fix
Add a SKYAL section with a consumer/gate table.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **TESTS**: Doc text matches the code it describes (re-grep the cited symbols)
