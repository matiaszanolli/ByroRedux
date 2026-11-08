# ESM-D6-02: the loose-override case-insensitive probe covers the file name, not the `Strings` directory

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6797
Labels: low, esm-plugin, bug, game:skyrim
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

**Related**: Incomplete fix of closed #4176 (file-name segment fixed, `Strings` directory not).

- **Severity**: LOW
- **Dimension**: 6
- **Location**: crates/plugin/src/esm/strings_table.rs:370-398 (`strings_dir = ...join("Strings")`, `loose_candidate`)
- **Status**: NEW (incomplete fix of #4176, which closed after fixing only the file-name segment; the regression test `loose_companion_file_is_found_case_insensitively` builds a `Strings/` dir with a lowercase file name only)
- **Description**: `loose_candidate` tries `Strings/<name>` exactly, then `read_dir("<dir>/Strings")` and matches file names case-insensitively. If the directory itself is spelled `strings/` (what BSArch-style extractors and many mod packagers emit, and exactly the archive's own lowercase `strings\` spelling) on a case-sensitive filesystem, both `is_file()` and `read_dir()` fail and the loose file is never seen, so the archive wins: the same inverted Bethesda override order #4176 set out to close.
- **Evidence** (probe `dircase`, loose file with id 1 = "Loose Override", archive callback returns "Archive"):
  `dir Strings -> Some("Loose Override")` / `dir strings -> Some("Archive")` / `dir STRINGS -> Some("Archive")`.
- **Impact**: a mod's lower-case-directory translation/override of a vanilla string table is silently ignored on Linux (the project's dev platform); #4073's warning does not fire because the archive supplies tables.
- **Suggested fix**: resolve the directory component the same way (case-insensitive `read_dir` of the plugin directory for an entry equal to `Strings`), then the file name; extend the existing test to a lowercase directory.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
