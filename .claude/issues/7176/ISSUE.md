# #7176 — PAR-D2-2026-11-21b-05: BSA, BA2 and CSG open-time table reads still fail with a bare "failed to fill whole buffer"

Labels: low,import-pipeline,bug

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**:
  - `crates/bsa/src/archive/open.rs`: the header, folder records, folder names, file records and file names reads;
  - `crates/bsa/src/ba2.rs`: the header and name-table reads;
  - `crates/bsa/src/csg.rs`: the header and chunk-table reads.
- **Status**: NEW (sibling of #7159, closed: it covered the extract-time reads only)
- **Trigger Input**: a truncated archive whose declared counts run past the file.
- **Description**: these reads name no table or index. #7159's issue also asked to drop the doubled outer labels: the plugin's "Compressed record X: ESM record X", and the provider's "'p' failed to extract: BA2 … of 'p'". The commit kept them, giving reasons.
- **Suggested Fix**: route the open-time reads through `read_exact_at_for` (or a cursor-side equivalent) with the table and index in the label.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
