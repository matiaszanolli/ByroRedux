# Issue #989: SK-D6-NEW-01: No .STRINGS companion-file loader

**State:** OPEN  
**Labels:** enhancement, low, legacy-compat

## Summary

Skyrim localized names render as `<lstring 0xNNNNNNNN>` placeholders because the
`.STRINGS`/`.DLSTRINGS`/`.ILSTRINGS` companion-file loader (Phase 2) was never
implemented. Phase 1 (#348, CLOSED) wired the placeholder; this issue implements
the resolver.

## Location

- `crates/plugin/src/esm/records/common.rs` — `read_lstring_or_zstring`, `LocalizedPluginGuard`
- To create: `crates/plugin/src/esm/strings_table.rs`

## Plan

1. Create `strings_table.rs` with `StringsTable` + `StringTableSet`
2. Add thread-local `CURRENT_STRINGS_TABLE` + `StringsTableGuard` in `common.rs`
3. Update `read_lstring_or_zstring` to resolve via thread-local table
4. Export from `mod.rs`
5. Add regression test
