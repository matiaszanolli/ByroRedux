# OBL-D5-2026-10-14-03: `substitute_numbers` mis-renders a flag-prefixed `%0.f` (precision falls back to 6)

**Labels**: low,game:oblivion,legacy-compat,scripting,bug
**Source**: docs/audits/AUDIT_OBLIVION_2026-10-14.md

Found by `/audit-oblivion` (docs/audits/AUDIT_OBLIVION_2026-10-14.md, HEAD `adbd468e2`). Finding ID: `OBL-D5-2026-10-14-03`. Re-verified against current code before filing.

- **Severity**: LOW (forward-looking; no quest script is affected today)
- **Dimension**: 5 (shared formatter in the M47.2 message path; routes to `/audit-scripting`)
- **Location**: `byroredux_scripting::messages::substitute_numbers`
- **Status**: NEW
- **Description**: the spec scan collects `[.0-9]*` into `spec`, then derives precision with `spec.strip_prefix('.')`. For `%0.f` the spec is `"0."`, `strip_prefix('.')` is `None`, so precision defaults to 6: value 3 renders `3.000000` instead of printf's `3`. Any width or flag digit before the dot (`%2.0f`) has the same shape.
- **Evidence**: `Oblivion.esm` script text census of Message / MessageBox specifiers: `%.0f` ×39, `%0.f` ×3, `%.2f` ×3, `%.1f` ×1 (+ one source typo `%Of`). The three `%0.f` sites are `SEHillofSuicidesSCRIPT` (commented out) and `XPKnotboneFactionFixerSCRIPT` ×2, all object scripts, none a quest script. The unit test covers only `%.0f`.
- **Impact**: cosmetic wrong text ("1.000000 taken out") once object scripts run on Oblivion; any MESG / Papyrus text using a width.
- **Suggested Fix**: parse `[flags][width][.precision]` (take the part after `.` as precision, ignore leading digits) and add a `%0.f` row to the message tests.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, sibling parsers/tests)
- [ ] **TESTS**: A regression test pins this specific fix
