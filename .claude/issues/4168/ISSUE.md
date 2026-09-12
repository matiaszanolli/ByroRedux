# ESM-2026-09-11-D6-01: companion-strings language token is still hardcoded "english" — FO4/FO76/Starfield ship _en and resolve nothing

URL: https://github.com/matiaszanolli/ByroRedux/issues/4168
Labels: bug, high, game:fo4, game:fo76, game:starfield, esm-plugin

---

**Severity**: HIGH
**Dimension**: Localized Strings
**Record / Sub-record**: every lstring site (`FULL`/`DESC`/`NAM1`/`CNAM`/`NNAM`/`BPTN`/…)
**Location**: `byroredux/src/cell_loader/load_order.rs:533-534`; consumed at `crates/plugin/src/esm/strings_table.rs:242-251`
**Status**: NEW (re-derived fresh this session; this exact defect was previously described in `AUDIT_ESM_2026-08-13.md` and `AUDIT_ESM_2026-09-09.md` but a dedup search against the live open-issue list found no matching GitHub issue — see below)

**Description**: `load_order.rs` builds the companion-file language segment from one literal, `"english"` (overridable only via `BYRO_STRINGS_LANG`). Skyrim SE's tables are named `<stem>_english.STRINGS`; every Creation-Engine title from Fallout 4 onward instead ships a two-letter ISO tag (`_en`, `_de`, `_ja`, `_ptbr`, `_zhhans`, `_zhhant`). There is no alias table and no per-game mapping anywhere in the tree.

**Evidence**:
```rust
// byroredux/src/cell_loader/load_order.rs:533-534
let strings_language =
    std::env::var("BYRO_STRINGS_LANG").unwrap_or_else(|_| "english".to_string());
```
```rust
// crates/plugin/src/esm/strings_table.rs:242-251 — exactly one candidate name, then give up
let mut load_file = |ext: &str, has_prefix: bool| -> Option<StringsTable> {
    let name = format!("{stem}_{language}.{ext}");
    ...
```
Measured archive contents confirm the discovery half is fine and only the language segment is wrong: `Fallout4 - Interface.ba2` holds `strings\fallout4_en.strings` etc. (35 `strings\` files, zero containing `"english"`); `Starfield - Localization.ba2` holds `strings\starfield_en.*`; `SeventySix - Localization.ba2` holds `strings\seventysix_en.*`/`nw_en.*`. Test coverage still only exercises the Skyrim spelling.

Confirmed via `gh issue list --search` (multiple keyword variants) that no open GitHub issue matches this defect at time of filing.

**Impact**: On Fallout 4, Fallout 76 and Starfield, every localized field is `<lstring 0xNNNNNNNN>` — cell display names, NPC names, item names, book/terminal/message text, quest log entries. This is the single largest localization defect in the dimension by blast radius (three of four modern titles, every lstring field).

**Related**: CLOSED #2912 (fixed the loose-file-vs-archive discovery half); companion findings in this same audit: cp1252 decode corruption (filed separately), silent zero-table resolution (existing #4073).

**Suggested Fix**: Replace the single literal with a small per-game (or per-plugin) candidate list — try the two-letter tag first, fall back to the long English spelling — and log which candidate hit. A one-line alias table (`english`⇄`en`, `german`⇄`de`, …) covers all six shipped languages.

## Completeness Checks
- [ ] **TESTS**: Fixtures for the FO4/FO76/Starfield `_en`-style spelling alongside the existing Skyrim `_english` fixture

