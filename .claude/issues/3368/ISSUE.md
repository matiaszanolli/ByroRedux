# SKY-2026-08-27-D5-02: the header's folder-records offset (bytes 8..12) is never read — the folder-table walk hardcodes an implicit 36

Labels: low,import-pipeline,bug,game:skyrim,legacy-compat

- **Severity**: LOW
- **Confidence**: CONFIRMED (code-read + verified the field's value on all 23 shipped archives)
- **Location**: `crates/bsa/src/archive/open.rs:28-62` (header slice reads), first folder
  record read at `open.rs:118-120`
- **Description**: `BsaArchive::open` reads a fixed 36-byte header and then reads folder
  records straight out of the same `BufReader` at whatever position that left it (36).
  Bytes `[8..12]` — the folder-records offset — are the only header word other than the
  trailing file-flags that is never sliced at all, so the parser has no way to notice or
  honour an archive whose folder table does not begin immediately after the header.
  openmw explicitly seeks to it before touching the folder table:

  ```cpp
  // reference/openmw/components/bsa/compressedbsafile.cpp:67
  input.seekg(mHeader.mFoldersOffset);
  if (input.fail())
      fail("Failed to read compressed BSA folder record offset: " + ...);
  ```
- **Evidence**: A throwaway raw re-parser that printed the field whenever it was not 36
  produced no output across all 23 Skyrim SE archives, i.e. every shipped archive has
  `folders_offset == 36`. The debug-build run over Meshes0 + Textures0 + Textures7 +
  Textures8 + Misc (46,692 files) also emitted zero `"BSA folder offset mismatch"`
  warnings from the existing `#[cfg(debug_assertions)]` check at `open.rs:185-193` —
  the *per-folder* offsets are self-consistent, which is a different field and does
  not cover this one. (The logger was self-tested: a deliberate `log::warn!` at startup
  printed, so the zero-warning result is real and not a dead sink.)
- **Impact**: None on any shipped Bethesda archive. A third-party archive with a padded
  or extended header fails loudly (the folder-name length byte reads as garbage →
  `read_exact` error or an `InvalidData` from `checked_entry_count`), so this is a
  robustness/diagnosability gap, not a corruption vector.
- **Suggested Fix**: Read `header[8..12]` and `reader.seek(SeekFrom::Start(offset))`
  before the folder loop (BufReader needs `Seek`, already imported under
  `cfg(debug_assertions)` — promote it), or at minimum validate `offset == 36` and
  return a clear `InvalidData` naming the field.
- **Related**: nothing in the 300-issue dedup baseline (84 open, fetched 2026-08-27) touches the BSA header layout.
  #586 / FO4-DIM2-01 hardened the *count* fields in this same header but did not add
  the offset.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
