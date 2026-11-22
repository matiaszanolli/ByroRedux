# #7166 — PAR-D6-2026-11-21-01: `no_production_code_prefixes_meshes_by_hand` only catches a name placeholder followed directly by `}"`

Labels: low,import-pipeline,bug,test-gap

- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `byroredux/src/workspace_hygiene_tests.rs`
- **Status**: NEW (residual of #7145, closed)
- **Description**: the scan misses all of these spellings:
  - `format!("meshes\\{stem}.nif")`
  - `format!("meshes\\{}.nif", x)`
  - `format!("meshes\\{dir}\\{file}")`
  - `format!("{}\\{}", "meshes", x)`
  - `push_str("meshes\\")`
  - `format!("meshes/{x}.nif")`

  No live offender exists today; the remaining `meshes\` formats in production are fixed paths.
- **Suggested Fix**: flag any string literal that starts with `meshes[\\/]` and then interpolates, or that ends at the separator, outside `core/src/paths.rs`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
