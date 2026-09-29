# PAR-D3-2026-09-29-01: #4664/#4672 BGSM diagnostics never fire on the production path, because the template resolver parses with the non-diagnostic `parse_bgsm` (regression of #4664)

**Labels**: low,bug,import-pipeline,game:fo4,game:fo76

**Source report**: `docs/audits/AUDIT_PARSERS_2026-09-29.md`

**Regression of #4664** — the fix for closed #4664 is incomplete; this is filed as a new issue rather than reopening.

- **Severity**: LOW
- **Dimension**: Version Gating
- **Location**: `crates/bgsm/src/template.rs:27`, `crates/bgsm/src/template.rs:257`; `byroredux/src/asset_provider/material/provider.rs:358-411`
- **Status**: Regression of #4664 (incomplete fix, `f97a5d183`). The same gap covers #4672's lossy-string warning.
- **Trigger Input**: a BGSM leaf or template with trailing bytes, `version > 22`, or a non-UTF-8 string.
- **Description**:
  - `MaterialProvider::resolve_bgsm` calls `self.bgsm_cache.resolve(&mut reader, &key)`.
  - `TemplateCache::resolve_depth` parses every file in the chain with `parse_bgsm(&bytes)`, which discards `ParseDiagnostics`.
  - `parse_bgsm_diag` and its three `warn!`s (lossy strings, unconsumed bytes, version over ceiling) exist only in the `ResolveError::DepthLimit` recovery arm. The same function describes that arm as "effectively dormant", since vanilla depth is at most 3.
  - The BGEM path (`provider.rs:614`) is correct.
- **Evidence**: the commit message of `f97a5d183` says "the production provider warns once per file with the path". In the code, a normal BGSM load never reaches `parse_bgsm_diag`.
- **Impact**: layout drift, post-v22 content or lossy paths in BGSM files (leaf or template) decode silently. This is the exact silence #4664 was filed to end. Vanilla is zero-noise either way.
- **Related**: #4664, #4672, PAR-D4-2026-09-29-01
- **Suggested Fix**:
  - Have `TemplateCache` call `parse_bgsm_diag` and surface the diagnostics per resolved file, either returned alongside the `ResolvedMaterial` or through a warn callback on `TemplateResolver`.
  - Add a provider-level test with a trailing-junk BGSM.

**Validated at HEAD 9fcfdc3fc**: `crates/bgsm/src/template.rs` imports and calls plain `parse_bgsm(&bytes)` in the resolver; the only `parse_bgsm_diag` call in `byroredux/src/asset_provider/material/provider.rs` is inside the `ResolveError::DepthLimit` recovery arm.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
