## #4168 [OPEN] ESM-2026-09-11-D6-01: companion-strings language token is still hardcoded "english" — FO4/FO76/Starfield ship _en and resolve nothing
labels: bug, high, game:fo4, game:fo76, game:starfield, esm-plugin

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


## #4169 [OPEN] ESM-2026-09-11-D5-01: PlacedRef.group_type is still 6 for every real placement — regression of #3728
labels: bug, medium, esm-plugin, doc-rot

**Severity**: MEDIUM
**Dimension**: CELL / WRLD Walkers & Placement Data
**Record / Sub-record**: CELL/WRLD child GRUPs 6/8/9/10 — `REFR`/`ACHR`/`ACRE`/`PGRE`/`PHZD`/`PMIS`
**Location**: `crates/plugin/src/esm/cell/walkers.rs:161-177,702-738`; `crates/plugin/src/esm/cell/wrld.rs:276-329`; `crates/plugin/src/esm/cell/mod.rs:391-406,320-322`; `crates/plugin/src/esm/cell/tests/cell.rs:198-199`
**Status**: Regression of #3728 (CLOSED) — re-verified still present at HEAD by direct source re-read. Not superseded by `6c3584ec`/#4077, which fixed a related but distinct false-topology premise in the immediately neighbouring code, but left this bug and its stale legend comments untouched.

**Description**: `parse_refr_group_inner` receives `group_type` once, at the point the outer 6/8/9 container group is entered, and threads that same value unchanged through its own nested-group recursion without ever re-reading `sub.group_type`. Real Bethesda content always nests 8 (Persistent) / 9 (Temporary) / 10 (Visible Distant) *inside* a type-6 "Cell Children" container — never as its direct sibling — so every placement in every shipped master is stamped `group_type = 6`, discarding exactly the persistent/temporary/visible-distant distinction #3728 was filed to add.

**Evidence** (`walkers.rs:702-728`):
```rust
fn parse_refr_group_inner(
    ...
    group_type: u8,   // threaded UNCHANGED through recursion
    depth: u32,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub = reader.read_group_header()?;
            ...
            parse_refr_group_inner(
                reader, sub_end, refs, landscape, navmeshes, pathgrids, deleted,
                group_type,  // <-- never re-derived from `sub.group_type`
                depth + 1,
            )?;
            continue;
        }
```
The comment immediately above this parameter still documents the bug as intended behaviour ("Fixed for the entire recursion … threaded unchanged (unlike `depth`, which increments)"). The regression fixture (`tests/cell.rs:198-199`) still builds two direct CELL children (type 6 and type 8 as siblings) — a shape that occurs zero times in any shipped master, which is why the test stays green while the bug stays live. No live consumer exists yet (`grep -rn '\.group_type'` outside walker/reader/tests finds only two `0xFF`-sentinel test fixtures).

**Impact**: A field that looks authoritative, is guarded by a green but unrepresentative test, and is uniformly wrong. It will silently misinform the first streaming-residency or save-restore consumer that reads it.

**Related**: #3728 (closed, inert fix), #4077/`6c3584ec` (fixed the neighbouring false-topology premise but not this bug). Includes ESM-2026-09-11-D5-02's stale-comment cleanup (four sites: `walkers.rs:161`, `wrld.rs:311`, `mod.rs:391-393,320-322`) as part of the same fix.

**Suggested Fix**: Re-derive `group_type` at each nested group inside `parse_refr_group_inner` — when `sub.group_type` is 8/9/10, pass that value into the recursive call instead of the inherited one. Correct the stale legend comments at all four sites and rebuild the test fixture as CELL → GRUP 6 → {GRUP 8, GRUP 9, GRUP 10}, the only nesting shape the real corpus contains.

## Completeness Checks
- [ ] **TESTS**: Rebuild `tests/cell.rs:198-199`'s fixture to the real GRUP-6-wraps-{8,9,10} nesting shape and pin the corrected `group_type` per placement


## #4170 [OPEN] ESM-2026-09-11-D6-03: .STRINGS family is still decoded via String::from_utf8_lossy, corrupting real cp1252 content
labels: bug, medium, game:fo4, game:starfield, esm-plugin

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


## #4171 [OPEN] ESM-2026-09-11-D6-04: parse_term still reads terminal text from the wrong sub-records and never reads ITXT/BTXT/RNAM
labels: bug, medium, game:fnv, game:fo3, game:fo4, esm-plugin

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


