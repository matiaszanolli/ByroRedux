# SKY-2026-09-11-D6-02: ROADMAP.md prose Parser-coverage summary contradicts its own compatibility matrix on file counts and per-game percentages

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4254

**Severity**: LOW
**Dimension**: 6 — Specialty Blocks + Real-Data Rendering
**Location**: `ROADMAP.md:519` (prose "Parser coverage" summary) vs `ROADMAP.md:687,1514` (compatibility matrix)
**Status**: NEW

**Description**: `ROADMAP.md`'s prose "Parser coverage" summary contradicts its own compatibility matrix on three numbers: 184,886 (prose) vs. the matrix's cited totals (the matrix explicitly names 184,886 as superseded by #3369/#3466), and stale 100%/FO76-clean/Starfield-99.99% claims vs. the matrix's current FO76-98.18%-with-truncation-tail and Starfield-99.98%.

**Evidence**: Confirmed in current code — `ROADMAP.md:519-523` states "NIF parses across seven games (184 886 files on the latest sweep...)" and "Starfield at 99.99% aggregate", while `ROADMAP.md:687` cites Starfield at **99.98%** aggregate (120,524/120,543) and `ROADMAP.md:1514` cites **FO76 98.18%** with a named truncation tail (#3466) — directly contradicting the prose summary's implied FO76-clean claim.

**Impact**: Documentation-only, but it's precisely the premise a future audit would cite, risking a stale figure propagating into a new report.

**Suggested Fix**: Update the prose "Parser coverage" summary (line ~519) to match the compatibility matrix's current, sourced figures (Starfield 99.98%, FO76 98.18% with truncation tail), and cite the matrix as the single source of truth rather than restating numbers that can drift out of sync.

## Completeness Checks
- [ ] **TESTS**: N/A (documentation-only fix)
