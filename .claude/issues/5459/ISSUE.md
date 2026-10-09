# #5459: SAVE-D2-2026-10-08-02: the `FORMAT_MAJOR` bump ledger in `snapshot.rs` stops at v32 while the constant is 33

**Labels**: low,save-load,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5459

**Source**: `docs/audits/AUDIT_SAVE_2026-10-08.md` — `SAVE-D2-2026-10-08-02` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: the ledger's last entry is `v31 -> v32`; `FORMAT_MAJOR` is `33`.

- **Severity**: LOW
- **Dimension**: Format & Schema Discipline
- **Data-Loss Class**: none (doc-rot)
- **Location**: `crates/save/src/snapshot.rs:246-262`; v33's rationale exists only in `serde_default_guard_tests.rs:739-743`.
- **Status**: NEW
- **Description**: `14cff35ae` changed only the constant line. The skill and the crate doc treat this comment as *the* bump ledger. The v33 note in the guard comment also says pre-v33 saves can "legitimately re-qualify" a line after load, but pre-v33 saves are rejected outright, by both the major and the schema fingerprint.
- **Suggested Fix**: Add a `v32 -> v33 (#5367 Phase L): new saved resource DialogueSpokenInfoForms …` entry, and correct the guard comment's wording.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
