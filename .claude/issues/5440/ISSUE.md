# #5440: NIF-D3-2026-10-08-03: #5259's source scan only matches the `ends_with(".nif")` spelling, so `nif_stats`' directory mode still walks a `.nif`-only corpus

**Labels**: low,nif-parser,nif,bug,test-gap
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5440

**Source**: `docs/audits/AUDIT_NIF_2026-10-08.md` — `NIF-D3-2026-10-08-03` (HEAD `00f580e09`)

- **Severity**: LOW (test-gap / tooling)
- **Dimension**: Block Dispatch Coverage (corpus definition)
- **Game Affected**: loose extracted corpora containing renamed-NIF LOD meshes (`.bto`/`.btr`: Skyrim, FO4, FO76)
- **Location**:
  - `crates/nif/examples/nif_stats.rs:558-578` (`process_dir`: `path.extension()… .eq_ignore_ascii_case("nif")`);
  - `crates/nif/tests/common/mod.rs:717-741` (`parse_all_nifs_in_dir`: `if lower != "nif"`, which has no callers);
  - the guard is at `crates/nif/src/corpus.rs:142-188` (needle `ends_with(".nif")` at `:171`).
- **Status**: NEW. #5259 (closed) and #4629/#4154 established the single-definition rule.
- **Description**: the corpus definition is meant to live once, in `corpus::is_nif_entry`. #5259's scan rejects the literal `ends_with(".nif")` under `tests/` and `examples/`, but an `extension()`-based `.nif` test is invisible to it. Two such sites remain:
  - `nif_stats`' directory walker, which is the `--corpus <path>` route of this skill and the tool's loose-file mode. Its archive mode uses `is_nif_entry` (`:589`, `:617`), so the two modes of one tool disagree on what a NIF is.
  - `parse_all_nifs_in_dir`, which is dead.
- **Impact**: `nif_stats <extracted-dir>` silently excludes every `.bto`/`.btr` LOD mesh. That is the #2587/#4154 under-reporting shape, on the path an auditor uses for an extracted corpus. Archive-mode gates are unaffected.
- **Related**: #5259, #4629, #4154, #2587.
- **Suggested Fix**: route `process_dir` through `is_nif_entry(path.to_string_lossy())`, and delete or fix `parse_all_nifs_in_dir`. Widen the scan to also reject `extension()`-based `"nif"` comparisons, or add a positive assertion that `process_dir` calls `is_nif_entry`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (any other `extension()`-based `.nif` test under `tests/` and `examples/`)
- [ ] **TESTS**: A regression test pins this specific fix
