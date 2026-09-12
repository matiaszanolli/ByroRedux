# ESM-2026-09-11-D6-05: companion-file paths are still composed with exact case and probed with a single fs::read

URL: https://github.com/matiaszanolli/ByroRedux/issues/4176
Labels: bug, low, esm-plugin

---

**Severity**: LOW
**Dimension**: Localized Strings
**Record / Sub-record**: —
**Location**: `crates/plugin/src/esm/strings_table.rs:233-251`
**Status**: NEW (re-derived fresh this session; previously described in `AUDIT_ESM_2026-08-13.md`; dedup search against the live open-issue list found no matching GitHub issue)

**Description**: The loose-file companion-strings candidate is built from the plugin's on-disk stem, the literal directory name `"Strings"`, and an upper-case extension. Packed archives are immune (path normalization lower-cases them); the loose-file override path — the one that must win for mods — has no case-insensitive fallback.

**Evidence** (`strings_table.rs:233-251`): single exact-case `fs::read` attempt, no retry.

**Impact**: A correctly-cased-per-Bethesda-canon mod override on a case-sensitive filesystem misses the loose file entirely and silently falls back to the archive, reversing the intended override order.

**Related**: ESM-2026-09-11-D6-01, and the existing #4073 (silent zero-table resolution) which is precisely what makes this failure invisible.

**Suggested Fix**: Probe case-insensitively via a directory listing + `eq_ignore_ascii_case`, or try a lower-case variant as a second candidate.

## Completeness Checks
- [ ] **TESTS**: A case-sensitive-filesystem fixture pins the corrected fallback behavior

