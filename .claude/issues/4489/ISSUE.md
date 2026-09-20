# EXT-D7-2026-09-19-02: m-exteriors and m34-day-night exit 0 PASS when every profile SKIPs

- **ID**: EXT-D7-2026-09-19-02
- **Labels**: medium,tech-debt,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4489

**Severity**: MEDIUM · **Dimension**: Acceptance harness · **Game Affected**: all
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D7-2026-09-19-02)

**Location**: `docs/smoke-tests/m-exteriors.sh:788,919-923`; `docs/smoke-tests/m34-day-night.sh:31-36`; contract at `docs/smoke-tests/README.md:7-8`

**Description**
README: "Missing game data is an explicit `SKIP` with exit code `77`, never a pass." `w1-water-traversal.sh` honours this (exit 77, promoted to a CI error by `playable-smoke.yml`). `m-exteriors.sh` instead records a SKIP row and `return 0` per profile; `m34-day-night.sh` `exit 0`s its SKIP directly. When no game data is present — the exact misconfiguration a runner variable typo produces — m-exteriors prints "exterior-smoke: PASS - every installed selected profile passed" and exits 0.

**Impact**
A data-less run is indistinguishable from a green run to anything consuming the exit code (CI, an audit lane, a release checklist).

**Related**: README lines 7-8, 296-298 (the 77 contract and its CI promotion)

**Suggested Fix**
Exit 77 (not 0) when the summary contains SKIP rows and no profile ran; keep per-profile SKIP rows visible. Mirror the `playable-smoke.yml` 77-to-error promotion wherever these scripts get automated.

## Completeness Checks
- [ ] **TESTS**: A data-less dry run of each script exits 77 with a SKIP banner
