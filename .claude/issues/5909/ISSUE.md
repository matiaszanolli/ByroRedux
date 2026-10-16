# #5909: PAR-D6-2026-10-15-01: 0f07131a8 (#5739) spliced the new helper's doc into `resolve_profile_root`'s doc comment

- **Labels**: low,documentation,doc-rot,tech-debt
- **Filed from**: `docs/audits/AUDIT_PARSERS_2026-10-15.md`

**Full title**: 0f07131a8 (#5739) spliced the new helper's doc into `resolve_profile_root`'s doc comment; `resolve_profile_root` is now undocumented and `absolutise_against_cwd` documents the wrong function

- **Severity**: LOW
- **Dimension**: I/O & Paths (doc integrity of the moved `--profiles` reader)
- **Location**: `crates/game-detect/src/profiles.rs:52-63`.
  - Lines 52-55 are `resolve_profile_root`'s original doc ("Compose a profile's data directory from `games_root` + … Phase 20.").
  - Lines 56-63 are the #5739 text.
  - Both precede `pub fn absolutise_against_cwd` (`:64`).
  - `pub fn resolve_profile_root` (`:91`) now has no doc at all.
- **Status**: NEW. It is the same defect class as #5029 (fixed for menuxml in 511f8f4b1) and open #5704.
- **Trigger Input**: n/a.
- **Description**: rustdoc for `absolutise_against_cwd` opens with "Compose a profile's data directory from `games_root` + the profile's `subdir` field…", which describes `resolve_profile_root`.
- **Evidence**: `git show 0f07131a8 -- crates/game-detect/src/profiles.rs`. The hunk at `@@ -53,6 +53,41 @@` inserts below line 55, inside the existing `///` block.
- **Impact**: doc rot on a public API used by two tools.
- **Related**: #5739, #5029, #5704.
- **Suggested Fix**: move lines 52-55 back above `resolve_profile_root` (`:91`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
