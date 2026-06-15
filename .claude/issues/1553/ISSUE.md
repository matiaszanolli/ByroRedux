**Severity**: LOW (cosmetic — does not block rendering) · **Dimension**: Multi-Master Load Order + TES5 Cell-Load
**Location**: `crates/plugin/src/esm/strings_table.rs` (complete, 7 tests); zero non-test call sites for `StringTableSet::load` / `StringsTableGuard::new`
**Source**: AUDIT_SKYRIM_2026-06-14 (SK-D4-02)

## Description
`crates/plugin/src/esm/strings_table.rs` implements the `.STRINGS`/`.DLSTRINGS`/`.ILSTRINGS` table format and is fully unit-tested, but `StringTableSet::load` / `StringsTableGuard::new` have **no production call sites** — only doc-comment and test references (`grep` confirms zero non-test call sites). With no guard installed, `resolve_lstring` (`records/common.rs:129`) always returns `None`, so every localized name emits the `<lstring 0xNNNNNNNN>` placeholder.

Status note: this is the evolution of prior **SK-D6-NEW-01** (2026-05-12 audit: "no `.STRINGS` loader exists"). The loader now *exists* but is unwired, so the user-visible symptom is unchanged. No open GitHub issue tracks it.

## Evidence
`common.rs` references `StringTableSet` only via the thread-local + RAII guard; nothing calls `::load`/`Guard::new` outside docs/tests. `resolve_lstring` at `common.rs:129`, consumed at `:172` (`return resolve_lstring(id).unwrap_or_else(|| format!("<lstring 0x{:08X}>", id))`). All seven vanilla/DLC/CC Skyrim SE masters are Localized-flagged → hits 100% of Skyrim content at runtime.

## Impact
Cell titles, NPC names, book/faction names display as `<lstring 0x000…>`. UI legibility, not a rendering blocker.

## Suggested Fix
Install `StringsTableGuard` per-plugin during ESM load when `header.localized` is set (resolve the language tag, call `StringTableSet::load`, hold the guard across the record walk). ~20 LOC of wiring; loader + tests already exist.

## Completeness Checks
- [ ] **SIBLING**: Wiring covers all three table kinds (`.STRINGS`/`.DLSTRINGS`/`.ILSTRINGS`)
- [ ] **TESTS**: An integration test asserts a localized name resolves (not the placeholder)
