# SCR-D4-2026-10-17-04: `check_reference_script` / `check_alias_script` run outside the panic net the rest of the `.pex` path uses

Issue: #6000
Labels: low, scripting, bug, safety

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D4-2026-10-17-04: `check_reference_script` / `check_alias_script` run outside the panic net the rest of the `.pex` path uses

- **Severity**: LOW
- **Dimension**: Attach & Reference Identity
- **Location**: `byroredux/src/asset_provider/script.rs:151-163` (reference), `:246-258` (alias, per frame), `crates/scripting/src/translate/mod.rs:338-380` (net-coverage pin lists only translate/populate entry points)
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: The 3948/1816 contract wraps every untrusted-derived stage in `catching_panics`, and a source pin enforces it for the translate and populate entry points. The M47.2 checkers consume the decompiler's AST and are called from the cell-load attach path and from a per-frame system (`check_alias_scripts_dispatch`), with no net and no pin. A panic there would abort cell load or the frame loop instead of declining.
- **Evidence**: A grep of `checker.rs` and `script_model.rs` found no `unwrap`/`expect`/`unreachable!`/`panic!`, and the two index expressions (`owners[0]`, `arg_kinds[1]`) are guarded. This is hardening, not a demonstrated panic. A stack overflow is uncatchable regardless, but the frame sizes above show it is not reachable.
- **Impact**: Defense-in-depth gap on mod-supplied bytes.
- **Related**: #3948, #1816, #3287.
- **Suggested Fix**: Wrap the two `check_*` calls in `catching_panics("check_reference_script", ..)` and add the entry points to the pin in `translate/mod.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
