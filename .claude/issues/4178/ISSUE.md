# ESM-2026-09-11-D6-06: has_length_prefix mismatch would still parse Ok with a silently discarded, unvalidated length prefix

URL: https://github.com/matiaszanolli/ByroRedux/issues/4178
Labels: bug, low, esm-plugin

---

**Severity**: LOW
**Dimension**: Localized Strings
**Record / Sub-record**: —
**Location**: `crates/plugin/src/esm/strings_table.rs:136-162`
**Status**: NEW (re-derived fresh this session; previously described in `AUDIT_ESM_2026-08-13.md`; dedup search against the live open-issue list found no matching GitHub issue)

**Description**: `StringsTable::parse` takes `has_length_prefix` as a caller-supplied boolean and never validates it against the data — the prefixed arm reads the 4-byte length and explicitly discards it, scanning for the NUL terminator instead. A mismatch would still parse `Ok` and yield text shifted by 4 bytes, which for `.STRINGS`-shaped data stays human-readable enough to pass a casual eyeball check.

**Evidence** (`strings_table.rs:136-155`): no comparison between declared length and discovered NUL offset.

**Impact**: Latent — today unreachable since the flag is a fixed per-extension literal at all three call sites. Becomes a live trap the moment a caller derives the flag from something other than a hardcoded literal. The prior corpus measurement found 100% agreement across 116,825 real entries, so any future disagreement is a real signal worth surfacing.

**Related**: ESM-2026-09-11-D6-01 (a language-token fix that adds call paths would touch this code).

**Suggested Fix**: Compare the declared length against the actual NUL offset and `log::warn!` once on disagreement.

## Completeness Checks
- [ ] **TESTS**: A fixture with a deliberately-wrong length prefix pins the new warning

