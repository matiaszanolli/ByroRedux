# #7179 — PAR-D4-2026-11-21b-02: the meshes-prefix scan still misses several spellings, including one the issue listed

Labels: low,import-pipeline,bug,test-gap

- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `byroredux/src/workspace_hygiene_tests.rs` `no_production_code_prefixes_meshes_by_hand`
- **Status**: NEW (residual of #7166, closed). The close comment said the scan was "mutation-checked against the six spellings"; the issue listed seven, and `format!("{}\\{}", "meshes", x)` is still not flagged.
- **Description**: re-implemented and fed snippets. Missed:
  - `format!("{}\\{}", "meshes", x)`;
  - `r#"meshes\{x}"#`;
  - a leading separator (`"\\meshes\\{x}"`, `"/meshes/{x}"`);
  - `["meshes", x].join("\\")`;
  - `Path::new("meshes").join(x)`;
  - a placeholder before the root (`"{base}meshes\\{x}"`).

  No live offender. The `mitsuba_export.rs` allow-listing is justified, but it exempts the whole file rather than the one line. The matcher has no unit test of its own.
- **Suggested Fix**: move the matcher into a pure function with positive and negative cases, and handle `r#"`, a leading separator, and a bare `"meshes"` used as a `format!` / `join` argument.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
