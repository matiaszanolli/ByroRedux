# #7178 — PAR-D4-2026-11-21b-01: the ledger test's remaining blind spots

Labels: low,import-pipeline,bug,test-gap

- **Severity**: LOW (all latent; no field is falsely credited at HEAD)
- **Dimension**: Corpus Gates
- **Location**: `byroredux/src/asset_provider/tests/bgsm_merge.rs` `every_parsed_sidecar_field_is_merged_or_in_the_ledger`
- **Status**: NEW (residual of #7165, closed)
- **Description**:
  - **Ledger credit ignores which struct a field belongs to.** `glowmap`, `emittance_color` and the `adaptive_emissive_*` fields exist in both BGSM and BGEM, so one mention credits both.
  - **A sentence that excludes a field still credits it.** `receive_shadows` is credited by a line that says it is *not* in the ledger.
  - **The template-read check counts comments.** It searches `template.rs` with comments still in it.
  - **The comment stripper is hand-rolled.** It tracks only `"` per line, so `'"'` or `"\\"` desynchronise it; it ignores `/* */` and raw strings.
  - **`.base.` is unanchored on the left.** It would credit `BaseMaterial` for any future struct with a `base` field.
- **Suggested Fix**: key ledger entries per struct (`BGEM glowmap`), strip comments from the template text too, and move a real comment stripper into `byroredux_core::source_scan`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
