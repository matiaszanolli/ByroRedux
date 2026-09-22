# PAR-D4-2026-09-21-04: archive_with_payload leaks one temp BSA per test per run

Labels: low,bug,import-pipeline,tech-debt

## Description
`crates/bsa/src/archive/tests.rs:289-316`: the `archive_with_payload` test helper writes `byroredux-bsa-#352-<pid>-<entry>.bsa` into `temp_dir()` and never removes it; the path is not returned to the caller so nothing downstream can clean it up either. The sibling `write_temp_v105` helpers in the same file do call `remove_file` after use.

Verified unchanged at HEAD `ee6d3fb39`: `archive_with_payload` still ends with `BsaArchive { file: Mutex::new(file), ... }` and no cleanup of `path`.

## Evidence
This audit's own unit run (`TMPDIR=/mnt/data/tmp`, 20:07) added 6 such files. 24 sat in `/mnt/data/tmp` from 4 prior runs (Sep 14 x3, Sep 21) at the time of the audit.

## Impact
Temp-dir litter that accumulates on the default tmpfs `/tmp` every `cargo test -p byroredux-bsa` run.

## Related
#352 (the regression this helper exists to cover)

## Suggested Fix
Return the path (or a guard that deletes on drop) and remove it after `BsaArchive` construction; the open file handle keeps the data readable on Unix even after unlink.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D4-2026-09-21-04)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix