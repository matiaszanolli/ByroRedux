# SCR-D4-2026-10-17-03: `.pex` is re-extracted, parsed, preflighted and decompiled for every scripted REFR; only the VM verdict is cached

Issue: #5999
Labels: low, scripting, bug, performance

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D4-2026-10-17-03: `.pex` is re-extracted, parsed, preflighted and decompiled for every scripted REFR; only the VM verdict is cached

- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/cell_loader/references/attach.rs:765-790` (`resolve_pex` + `translate_pex_detailed_with_providers` per REFR per script), `byroredux/src/asset_provider/script.rs:131-172` (cache only after translate)
- **Status**: NEW
- **Untrusted-Input**: Yes (the work is proportional to attacker-supplied script size)
- **Description**: `VmReferenceScriptChecks` caches the checker verdict per script name, but each REFR carrying the same script still pays archive extraction (zlib / LZ4), `byroredux_pex::parse`, the compatibility preflight, `decompile_script` and the provider lowering before the cache is consulted. The recognizer result depends on per-REFR properties, but the decompiled AST does not.
- **Evidence**: No cache in `resolve_pex` or `translate_pex_detailed_with_providers`. A cell with N REFRs of one script does N decompiles. This is unmeasured; #3938 shows decompile cost on this path was a real hotspot earlier.
- **Impact**: Cell-load CPU scales with scripted-REFR count instead of unique-script count. Modded cells with thousands of scripted REFRs of one script stall the budgeted loader.
- **Suggested Fix**: Cache `(script name -> Arc<ast::Script> + PexCompatibility)` next to `VmReferenceScriptChecks` and run `lower_provider_program` / `translate_script` against the cached AST. Measure first with the `m47-triggers.sh` timing.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
