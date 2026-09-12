# ESM-2026-09-11-D6-04: parse_term still reads terminal text from the wrong sub-records and never reads ITXT/BTXT/RNAM

URL: https://github.com/matiaszanolli/ByroRedux/issues/4171
Labels: bug, medium, game:fnv, game:fo3, game:fo4, esm-plugin

---

**Severity**: MEDIUM
**Dimension**: Localized Strings
**Record / Sub-record**: `TERM` — `ANAM`, `DNAM`, `MNAM` (wrongly read); `ITXT`, `BTXT`, `RNAM` (never read)
**Location**: `crates/plugin/src/esm/records/misc/world.rs:1504-1541`
**Status**: NEW (re-derived fresh this session; previously described as FO4-only in `AUDIT_ESM_2026-08-13.md`, extended with FNV evidence in `AUDIT_ESM_2026-09-09.md`; dedup search against the live open-issue list found no matching GitHub issue)

**Description**: `parse_term` assigns `password` from `ANAM`, `footer_text` from `DNAM`, and pushes `MNAM` onto `menu_items` whenever non-empty. None of these three is authored text on FO4 or FNV; the real body/menu/result text lives in `ITXT`/`BTXT`/`RNAM`, none of which has a match arm anywhere in the crate.

**Evidence** (`world.rs:1519-1536`):
```rust
match &sub.sub_type {
    b"ANAM" => out.password = read_zstring(&sub.data),
    b"DNAM" => out.footer_text = read_zstring(&sub.data),
    b"BSIZ" if !sub.data.is_empty() => { out.body_size = sub.data[0]; }
    b"MNAM" => { ... out.menu_items.push(text); }
    _ => {}
}
```
`grep -rn 'ITXT' crates/plugin/src/esm/` returns only a doc-comment mention.

**Impact**: Every `TERM` record in FO3/FNV/FO4 gets a garbage `password`, an empty/wrong `footer_text`, and (on FO4) spurious one-character `menu_items` entries, while the actual displayed terminal text is dropped entirely — and fixing the language-token bug (ESM-2026-09-11-D6-01) alone would not surface FO4's `ITXT`/`BTXT`/`RNAM` lstring ids since this parser never routes them through the lstring reader.

**Related**: ESM-2026-09-11-D6-01.

**Suggested Fix**: Read `ITXT`/`BTXT`/`RNAM` through `read_lstring_or_zstring`; drop or re-source the `ANAM`/`DNAM`/`MNAM` arms against a cited xEdit layout.

## Completeness Checks
- [ ] **TESTS**: A fixture with real `ITXT`/`BTXT`/`RNAM` payloads pins the corrected field mapping

