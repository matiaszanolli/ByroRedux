# SF3-02: .mat arm silently falls to generic unsupported-format warn when the CDB fails to parse

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1831
**Labels**: bug, import-pipeline, low, legacy-compat

**Severity**: LOW
**Dimension**: CDB materials (defense-in-depth / diagnosability, Starfield audit Dim 3)
**Location**: `byroredux/src/asset_provider/material.rs` (`load_starfield_cdb` warn+drop; `.mat` gate; unknown-extension fallback warn)
**Related**: Distinct from #1289 (Phase-2 per-field CDB extraction, tracked separately as SF3-01 in the same audit, not re-filed). This finding is about diagnosability of CDB *parse failure*, not about the missing value walk.

## Description

`load_starfield_cdb` warns and drops on parse failure, leaving `sf_cdbs` possibly empty. If the ONLY CDB fails (e.g. a future patch bumps CDB fileVersion past 4 → `UnsupportedVersion`, a hard bail per the #1569 pins), `has_starfield_cdb()` returns false, the `.mat` gate is skipped, and every `.mat` mesh falls through to the unknown-extension arm — logging "unsupported format (Starfield .mat?)" per path. The operator sees generic per-material spam that does not point back at the single upstream CDB failure (logged once, far earlier).

## Evidence

The `load_starfield_cdb` failure branch logs once and returns (`byroredux/src/asset_provider/material.rs:245-256`); the downstream `.mat` warning at line 1081 (`"material path '{}' is not a .bgsm/.bgem — unsupported format (Starfield .mat?); mesh will use NIF defaults"`) never references CDB state. The two log sites are disjoint.

## Impact

Diagnosability only. Content still renders (NIF-default Lambert). A future Starfield update changing the CDB version would present as thousands of "unsupported .mat" warnings rather than one clear degradation line.

## Suggested Fix

In the `.mat` fallback, when the path ends `.mat` AND `!has_starfield_cdb()`, emit a distinct once-only warning naming the likely cause ("Starfield .mat encountered but no CDB loaded/parsed — check --materials-ba2 and CDB version").

## Completeness Checks
- [ ] **SIBLING**: Same once-only-warning pattern applied consistently across other CDB/BGSM-gated fallback arms
- [ ] **TESTS**: A regression test pins this specific fix (CDB parse failure produces the distinct warning, not per-mesh spam)

