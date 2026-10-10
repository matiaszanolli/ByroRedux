# #5533: SF-2026-10-09-D4-02: `sf_smoke` counts PKIN-based REFRs as "resolved" through the nominal empty-model `StaticObject`, so the Cydonia resolve rate hides the whole pack-in gap

**Labels**: bug, game:starfield, legacy-compat, low, test-gap

**Source**: `docs/audits/AUDIT_STARFIELD_2026-10-09.md` — finding `SF-2026-10-09-D4-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW (measurement / test gap)
- **Dimension**: ESM Resolve Rate + Cell Bring-up
- **Location**: `crates/plugin/src/esm/cell/support.rs:892-918` (nominal `StaticObject`, `model_path: String::new()`, `RecordType::PKIN`); `byroredux/src/sf_smoke.rs:185-195` (any `statics` hit counts as resolved)
- **Status**: NEW
- **Description**:
  - `parse_pkin_group` registers every PKIN as an empty-model static, so that the expander can find the base.
  - `sf_smoke` treats any `statics` hit as resolved, so the 370 Cydonia PKIN REFRs count toward the 91.2% headline even though they spawn nothing.
  - Their 12,218 template children never enter the metric.
  - This is why four Starfield reports (06-23, 07-25, 08-16, 08-24) listed "PKIN 370" as resolved without anyone noticing SF-D4-01.
- **Evidence**: As above; the by-type rows in the 06-23, 07-25, 08-16 and 08-24 reports.
- **Impact**: The skill's Dim 4 regression gate cannot see SF-D4-01, or a fix for it.
- **Related**: SF-D4-01, #2637 (the precedent for a separate known-but-not-rendered bucket).
- **Suggested Fix**: Report PKIN-based REFRs in their own bucket with their template child count ("pack-in: N REFRs → M template children, instanced / not instanced"), not as resolved.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
