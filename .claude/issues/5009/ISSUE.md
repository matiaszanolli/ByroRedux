# PAR-D2-2026-09-29-02: The debug-load NIF resolver keeps the pre-#4658 raw extract loop, with first-listed-wins order and BSA-only opening

**Labels**: low,bug,import-pipeline,tech-debt

**Source report**: `docs/audits/AUDIT_PARSERS_2026-09-29.md`

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `byroredux/src/debug_load.rs:185-212` (`resolve_nif_bytes`)
- **Status**: NEW (owner `/audit-tooling`, per `_audit-owners.md`)
- **Trigger Input**: a `byro-dbg` NIF load request whose path is present but corrupt in a `--bsa` archive, or any `--bsa` that is a BA2.
- **Description**:
  - The loop is `for window in args.windows(2) { … BsaArchive::open(archive_path) … if let Ok(data) = archive.extract(path) { return Some(data) } }`.
  - It has the exact shape #4658 removed from the providers: non-`NotFound` errors are dropped silently.
  - It is first-listed-wins, which inverts #3637.
  - It re-opens (and re-indexes) every `--bsa` per request.
  - A BA2 `--bsa` fails `BsaArchive::open` with "not a BSA file", which is logged as a failure to open.
- **Evidence**: see the location. `every_content_provider_resolves_collisions_last_wins` scans providers only, so it cannot see this loop.
- **Impact**: debug tooling only. A present-but-corrupt override reads as "not found", or silently resolves to an earlier archive's copy. On FO4 or Starfield (`--bsa *.ba2`), archive-backed debug NIF loads never resolve.
- **Related**: #4658, #3637, #4752 (the same function's path policy, a security issue, OPEN)
- **Suggested Fix**: resolve through `build_texture_provider(&args).extract_mesh(path)`, which the cell-request branch already builds, or through `extract_first` over `Archive::open`.

**Validated at HEAD 9fcfdc3fc**: `resolve_nif_bytes` in `byroredux/src/debug_load.rs` (after its new loose-file root check) still loops `args.windows(2)` over `--bsa`, `BsaArchive::open` per request, returns the first `Ok(extract)` and drops non-`NotFound` extract errors silently.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
