# EXT-D7-2026-09-19-07: exterior matrix covers 5 of 7 games; W1 traversal route exists only for FNV

- **ID**: EXT-D7-2026-09-19-07
- **Labels**: low,terrain-exterior,bug,test-gap,game:fo76,game:starfield
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4507

**Severity**: LOW · **Dimension**: Acceptance harness · **Game Affected**: FO76, Starfield (matrix); all but FNV (W1 traversal)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D7-2026-09-19-07)

**Location**: `docs/smoke-tests/m-exteriors.sh:896-913` (game case list); `docs/smoke-tests/fixtures/` (only `fnv.env` declares `W1_WATER_SOURCE`, line 133)

**Description**
The exterior readiness matrix — the harness the audit's Dim 1-6 verdicts lean on — has no FO76/Starfield profile, and the WATAL traversal gate (`w1-water-traversal.sh`) has exactly one measured route (FNV deep profile); `skyrim_se`, the default game, is a SKIP. Partially by design: FO76/Starfield exterior support is documented open scope (watal.md; terrain-LOD scheme "none" for both — though see EXT-D6-2026-09-19-02 for FO76's shipped `.bto` family). The scripts do not overclaim in their own usage text.

**Impact**
Any FO76/Starfield exterior work lands with zero harness backstop; "W1 passed" in a session summary usually means FNV only.

**Suggested Fix**
When FO76/Starfield exterior support lands, add `fixtures/<game>.env` + water fixture rows (and revisit with EXT-D6-2026-09-19-02). Until then the README's honest-skip note is adequate.

## Completeness Checks
- [ ] **TESTS**: New fixture rows pin per-game gates when support lands
