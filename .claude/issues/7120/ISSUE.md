# #7120 — ESM-D8-03: CTDA `param_1 = 0xFFFFFFFE` sentinel (fn 1/32) remapped on Starfield: 175 warn lines, value unchanged

Labels: low,esm-plugin,bug,game:starfield
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D8-03)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D8-03**

- **Severity**: LOW
- **Dimension**: Real-Data Validation
- **Record / Sub-record**: `CTDA` (QUST 147, SCEN 21, PACK 4, INFO 1), function 1 (172 of 174) and function 32 (1)
- **Location**: `crates/plugin/src/esm/records/condition.rs:638-641` (`remap_condition_form_ids`), `param1_is_form_id` at `condition.rs:542-546`
- **Status**: NEW, same family as the closed #6786-class "inverse-shape remap" and Dim 3's `GetActorValue` finding
- **Description**: `param1_is_form_id(1)` marks function 1 (`GetDistance`, target FormID in Skyrim/FO4) as FormID-bearing. On Starfield every occurrence in `Starfield.esm` carries the sentinel `0xFFFFFFFE` (a "none/self" marker), which `remap_starfield` cannot resolve and logs. The function table is the Skyrim/FO4 one; Starfield re-uses these indices for other meanings.
- **Evidence**: probe over every CTDA/CTDT in `Starfield.esm`: 174 conditions where `param1_is_form_id(fn)` is true and `param_1 >> 24 != 0`, all `param_1 == 0xFFFFFFFE`, `param_2 == 0`.
- **Impact**: 175 warns per Starfield load and a no-op pass-through. The value is left unchanged, so no condition is evaluated differently. Log noise only.
- **Suggested fix**: skip `0xFFFFFFFE` (and `0xFFFFFFFF`) in `remap_condition_form_ids`, or gate `param1_is_form_id` on `GameKind` (there is no Starfield function catalog yet).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
