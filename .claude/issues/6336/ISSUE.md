# PERF-D8-2026-10-23-01: 994620123 harness edit has no disposition

- **GitHub**: #6336
- **Severity**: LOW
- **Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-23.md

Found by `/audit-performance` 2026-10-23 (PERF-D8-01).

## Location
`scripts/check-bench-harness-provenance.sh` (the `DISPOSITION` table, ~line 30); the edit itself is `scripts/fsr-bench-matrix.sh` (header `printf` field + `num("gpu_cloud_shadow")`).

## Description
`994620123` (#6316) appends the `gpu_cloud_shadow` column to the bench matrix. Its siblings `8fdd9ae56`, `a4ede5fa0` and `fc83608d3` each carry a one-line `DISPOSITION` (#5451) so a reader sees "diverged, and here is what was established". This one has none, so every record older than it prints an unannotated DIVERGED.

## Evidence
A no-argument run lists `994620123` as the newest harness-pair edit under `a3a7a43b0` (now 2 edits) and `a37fcba3c` (now 4) with nothing beneath it. The diff is a pure column append — no run arguments, gates or arithmetic changed. ROADMAP records the control only in prose (`docs/audits/CONTROL_stepped-camera_a3a7a43b0_under-994620123.tsv`).

## Impact
Tooling/doc rot only; the live record `d9a201c71` is byte-stable against HEAD (script verdict, exit 0). Without the line, the next comparison against the outgoing record sees a bare DIVERGED for an edit already shown harmless.

## Suggested Fix
Add `[994620123]="by inspection: appends the gpu_cloud_shadow column only (#6316; a bracket nested inside gpu_sky_cube); no run arguments, gates or arithmetic changed; d9a201c71 was controlled under it (docs/audits/CONTROL_stepped-camera_a3a7a43b0_under-994620123.tsv)"`. Also update the audit-performance skill's DIVERGED edit counts (2 and 4).

## Completeness Checks
- [ ] **TESTS**: `scripts/check-bench-harness-provenance.sh` output shows the disposition under both older records
