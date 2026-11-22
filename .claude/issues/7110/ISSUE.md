# #7110 — ESM-D3-05: `parse_condition_list`: public `&None` entry point to a remapping decoder, no production caller; stale exemption

Labels: low,esm-plugin,tech-debt,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D3-05)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D3-05**

- **Severity**: LOW (no live caller; same shape as the fixed ESM-D3-03 / #6788)
- **Dimension**: 3 (guard blind spot)
- **Record / Sub-record**: CTDA (any record)
- **Location**: `crates/plugin/src/esm/records/condition.rs:473-479` (`push_ctda(sub, &None, ...)`), exempted at `records/tests.rs:2996` (`("condition.rs","parse_condition_list")` in `EXEMPT_NO_U32_READS`, no comment).
- **Status**: NEW (pre-dates the baseline, not previously flagged; #6988 "delete 21 pub items nothing references" missed it).
- **Description**: The exemption row passes the mechanical "no u32 reads" test only because the function delegates to `push_ctda`. The guard `no_parser_with_a_remap_in_scope_passes_an_identity_remap` only fires when a remap is in scope. `grep -rn parse_condition_list crates byroredux` finds no use outside `tests.rs`.
- **Impact**: None today. A future caller would get plugin-local `param_1`/`reference_form_id`/`Use Global` comparands, which silently mismatch global ids on every multi-plugin load.
- **Suggested Fix**: delete the function and its exemption row, or give it a `remap` parameter.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
