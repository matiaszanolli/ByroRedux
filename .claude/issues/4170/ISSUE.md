# ESM-2026-09-11-D6-03: .STRINGS family is still decoded via String::from_utf8_lossy, corrupting real cp1252 content

URL: https://github.com/matiaszanolli/ByroRedux/issues/4170
Labels: bug, medium, game:fo4, game:starfield, esm-plugin

---

**Severity**: MEDIUM
**Dimension**: Localized Strings
**Record / Sub-record**: `FULL`/`DESC`/`NAM1` (any lstring-bearing field)
**Location**: `crates/plugin/src/esm/strings_table.rs:155,161`
**Status**: NEW (re-derived fresh this session; previously described in `AUDIT_ESM_2026-08-13.md`; dedup search against the live open-issue list found no matching GitHub issue)

**Description**: Both the prefixed and non-prefixed decode arms in `StringsTable::parse` call `String::from_utf8_lossy`. Bethesda's companion files are Windows-1252; any byte in `0x80..=0xFF` that isn't a valid UTF-8 continuation becomes U+FFFD. Measured corrupting real English content: 75 FO4 + 228 Starfield entries with U+FFFD (curly apostrophes, ellipses, RobCo glitch-art characters); 0/67,414 on Skyrim.

**Evidence** (`strings_table.rs:145-161`):
```rust
let str_start = offset + 4;
...
String::from_utf8_lossy(&blob[str_start..str_start + nul_pos]).into_owned()
} else {
    ...
    String::from_utf8_lossy(&blob[offset..offset + nul_pos]).into_owned()
};
```
No cp1252 mapping anywhere in the file.

**Impact**: Latent behind the language-token bug (ESM-2026-09-11-D6-01) today for FO4/FO76/Starfield (nothing resolves there yet), but becomes immediately visible the moment that fix lands — every apostrophe-bearing name would render with a replacement-character glyph, reading as a new bug introduced by that fix.

**Related**: ESM-2026-09-11-D6-01 (fixing that exposes this).

**Suggested Fix**: Decode with a cp1252→`char` mapping instead of `from_utf8_lossy`; optionally probe UTF-8 first and fall back for genuinely-UTF-8 community tables.

## Completeness Checks
- [ ] **TESTS**: Fixtures with real cp1252 apostrophe/ellipsis bytes pin the corrected decode

