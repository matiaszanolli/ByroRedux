# #5904: PAR-D4-2026-10-15-04: the BGSM/BGEM sweeps pin no corpus size, so a listing regression that drops files stays at "100%"

- **Labels**: low,bug,nifal,test-gap,game:fo4
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

- **Severity**: LOW
- **Dimension**: Corpus Gates
- **Location**: `crates/bgsm/tests/parse_all.rs:364-368` (FO4, `assert!(stats.total > 0)`) and `:466-469` (FO76, the same). The FO76 JSON bucket is counted but not pinned (`:437-440`), although the comment at `:470-472` says "exactly 2 known JSON files".
- **Status**: NEW. `MIN_SUCCESS_RATE` 1.0 is confirmed at `parse_all.rs:52`, and drift is asserted empty. The size half of contract (d) is missing.
- **Trigger Input**: n/a. Regression class: a `Ba2Archive` name-table or `list_files` regression that loses entries (for example, one GNRL page), or a reader change that starts `{`-prefixing binary payloads. The rate is computed over whatever is listed, and JSON-classified files are not failures.
- **Description**:
  - Every other parser census now pins exact counts: HKX, CDB, menu DDS, CSG and SE Meshes0. The two BGSM sweeps alone can pass on one file.
  - Measured figures already sit in the comments: FO4 "0/36,888 files" (`:357`); FO76 "29,989/29,991 OK with exactly 2 known JSON files" (`:470`).
- **Evidence**: code reading. No probe was needed: the assertions are one-sided by inspection.
- **Impact**: a weaker gate than its siblings. No current failure.
- **Related**: #5011, #4665, #5671 (the same one-sided-to-exact fix on HKX).
- **Suggested Fix**: assert the exact per-variant FO4 BGSM/BGEM totals, and assert FO76 `total == 29,991` and `json_text == 2`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
