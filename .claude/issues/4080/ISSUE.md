# #4080 — ESM-2026-09-09-D2-01

a stale 3 887-line `parse_real_esm.rs.orig` merge artifact is committed and tracked

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4080 --json state`).

---

- **Severity**: LOW
- **Dimension**: Sub-Record Byte Accounting (test corpus hygiene)
- **Record / Sub-record**: —
- **Location**: `crates/plugin/tests/parse_real_esm.rs.orig`
- **Status**: NEW
- **Description**: `git ls-files | grep -E '\.orig$|\.rej$'` returns exactly one path in
  the entire repository, and it is this one. It was added by `ef85f7be` ("Refactor code
  structure for improved readability and maintainability") and is a merge/patch artifact,
  not a deliberate fixture: it is a byte-for-byte older copy of the live
  `crates/plugin/tests/parse_real_esm.rs` (3 887 lines vs 3 973 — 86 lines behind,
  missing among other things the `#3923` GMST-leveling-overlay block), it contains no
  conflict markers, and nothing anywhere references it
  (`grep -rn 'parse_real_esm.rs.orig'` over `.rs`/`.toml`/`.md`/`.sh` → no matches).
  `.gitignore` has no `*.orig` rule, which is why it slipped in.
- **Evidence**: `wc -l` → `3973 parse_real_esm.rs`, `3887 parse_real_esm.rs.orig`;
  `git log --diff-filter=A -- crates/plugin/tests/parse_real_esm.rs.orig` → `ef85f7be`.
- **Impact**: not a correctness bug — Cargo compiles `tests/*.rs`, so a `.orig` extension
  is never built and the 947-test baseline is unaffected. The cost is (a) 3 887 lines of
  silently-rotting duplicate assertions that read as real coverage to anyone grepping the
  test corpus, and (b) the exact hazard *feedback_file_split_include_str* records — a
  repo-wide `grep` over `crates/plugin/tests` now returns two hits for every real-data
  assertion, one of them stale.
- **Related**: `ef85f7be`; *feedback_file_split_include_str*.
- **Suggested Fix**: `git rm crates/plugin/tests/parse_real_esm.rs.orig` and add
  `*.orig` / `*.rej` to `.gitignore`.

---
