# #5000: CONC-D7-2026-09-28-02: BSA and CSG went lock-free with no multi-threaded extract regression; only BA2 has one

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,import-pipeline,concurrency,test-gap,bug
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW (test gap)
- **Dimension**: Worker Threads
- **Location**: `crates/bsa/src/archive/extract.rs:36-66,106` (BSA); `crates/bsa/src/csg.rs:309-367` (CSG `chunk_bytes`); the only concurrent test is `crates/bsa/src/ba2.rs:1201-1266` (`concurrent_extracts_read_their_own_entries`)
- **Status**: NEW
- **Description**:
  - **What changed.** `1b8b21f3f` removed `Mutex<File>` from all three readers, and only BA2 gained a thread-scoped extract test. The BSA reader serves the streaming worker's pool tasks for 4 of 5 games (Oblivion/FO3/FNV/Skyrim). It also serves main-thread texture resolves and the new prefetch tasks, all on one handle. The CSG reader is now hit by concurrent precombine-decode tasks (`e593770f0`, `streaming.rs:1348-1356`, `precombined.rs` `CsgHandleCache`).
  - **The hazard is future regression, not current code.** `impl Read for &File` and `impl Seek for &File` both exist. A later edit such as `(&self.file).seek(..)` followed by `read_exact` compiles without `&mut` and without a lock, and silently reintroduces the shared-cursor race the Mutex used to prevent. Nothing on the BSA or CSG side would catch it.
- **Evidence**: `git show 1b8b21f3f -- crates/bsa/src/archive/tests.rs crates/bsa/src/csg.rs` adds no threaded test. `grep thread::scope crates/bsa/src` returns only `ba2.rs:1250`.
- **Trigger Conditions**: A future edit to BSA or CSG extract.
- **Impact**: Silent wrong-bytes corruption: NIF parse failures, or wrong precombine geometry, that shows only under parallel streaming.
- **Verification Path**: Add 8 threads × N rounds that extract from a synthetic compressed + embed-name BSA and a multi-chunk CSG, and assert byte equality. The BA2 test is the template.
- **Related**: #3659, #1170, #877
- **Suggested Fix**: Clone the BA2 `concurrent_extracts_read_their_own_entries` shape for `BsaArchive` (compressed + `embed_file_names`) and for `CsgArchive::read_psg` across chunk boundaries.

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D7-2026-09-28-02) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
