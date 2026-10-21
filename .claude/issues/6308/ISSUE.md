# #6308: PAR-D6-02: The loose tier folds keys ASCII-only while the archive readers fold with Unicode, and case-variant duplicates on a case-sensitive filesystem silently last-win in `read_dir` order

**Labels**: low,bug,import-pipeline

**Source**: docs/audits/AUDIT_PARSERS_2026-10-21.md · **Severity**: LOW · **Dimension**: I/O & Paths · **Location**: `byroredux/src/asset_provider/archive.rs` (`LooseDir::walk`, `LooseDir::key`); `crates/bsa/src/archive/mod.rs` (`normalize_path`)

- **Location**:
  - `byroredux/src/asset_provider/archive.rs:81-103` (`walk`, `key`: `to_ascii_lowercase`).
  - The archive folds: `crates/bsa/src/archive/mod.rs:124` and `crates/bsa/src/archive/open.rs:279` (`to_lowercase`).
- **Status**: NEW (48e422a71)
- **Trigger Input**: a loose override named with a non-ASCII uppercase letter (for example `textures\Ölfass.dds`) referenced in lowercase, or `Data/textures/x.dds` beside `Data/Textures/x.dds` on Linux.
- **Description**:
  - Dim 6's contract is that open-time and lookup keys agree across tiers. The archives fold Unicode, so `Ö` becomes `ö`. The loose index and its lookup fold ASCII only, so a non-ASCII loose override is never found where the archive copy is.
  - Two loose files that normalise to the same key overwrite each other in `files.insert` in nondeterministic `read_dir` order, and nothing is counted or warned. The BSA and BA2 readers count and warn on in-archive duplicates (#4671).
- **Suggested Fix**:
  - Fold with the same `to_lowercase().replace('/', "\\")` the archive readers use, ideally through one shared helper.
  - Count and warn once for duplicate loose keys.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
