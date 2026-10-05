# #5303: ESM-2026-10-05-D6-01: The regression test for the bare-`--esm` strings-archive fix re-implements the fix instead of calling it, so it cannot fail

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5303
- **Labels**: low,esm-plugin,bug,test-gap
- **Source**: `docs/audits/AUDIT_ESM_2026-10-05.md` (ESM-2026-10-05-D6-01)

_From `docs/audits/AUDIT_ESM_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW. This is a test gap.
- **Dimension**: Localized Strings
- **Record / Sub-record**: — (`.STRINGS` archive discovery)
- **Location**: `byroredux/src/cell_loader/load_order.rs:1581-1606` (`string_archive_discovery_treats_a_bare_plugin_name_as_the_cwd`); the fix is at `:452-463` (`ArchiveStringSource::discover`)
- **Status**: NEW. It is a residual of c7e53a20a, which has no issue.
- **Description**: c7e53a20a correctly makes `discover()` treat `Path::new("Skyrim.esm").parent() == Some("")` as the cwd. Without that, `read_dir("")` fails and every lstring in a bare-name launch stays `<lstring 0x…>`. The test, however, copies the `.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(".")` expression inline and asserts `read_dir` on its own copy. It never calls `discover()` or `ArchiveStringSource::read()`. Reverting the fix leaves the test green, the same vacuous-guard shape `_audit-common.md` warns about for source-scan tests. The loose-file leg (`StringTableSet::load_with_archive`, `strings_table.rs:369-372`) never had the bug, because `"".join("Strings")` resolves relative to the cwd.
- **Evidence**: The test body builds `cwd_plugin = Path::new("Cargo.toml")` and calls only `std::fs::read_dir` on its own copy of the expression. No production function appears in it.
- **Impact**: A future refactor of `discover()` can silently reintroduce the placeholder-text regression that `p4-quest-route.sh` caught live on 2026-09-30.
- **Related**: c7e53a20a, #4073 (zero-tables diagnostic).
- **Suggested Fix**: Extract the directory derivation into a helper that `discover()` calls, and test that helper. Alternatively, call `discover()` on a bare relative name inside a serialized temp-dir test holding a `<stem> - Interface.bsa` fixture.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
