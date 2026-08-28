# SKY-2026-08-27b-D4-01: `list_cells.rs`'s `.STRINGS` doc comment describes a gap that the archive fallback closed — "Skyrim SE hits this for every cell" is false at HEAD

- **Severity**: LOW
- **Dimension**: 4 (multi-master load order)
- **Location**: `byroredux/src/list_cells.rs:130-138`
- **Confidence**: CONFIRMED (both halves of the claim checked against code and archives)

## Description

The comment reads:

```rust
/// A localized plugin's FULL sub-record holds a string-table ID, not
/// text; when the companion table can't be found the resolver hands
/// back a `<lstring 0xNNNNNNNN>` placeholder. Skyrim SE hits this for
/// every cell — it ships its `.STRINGS` inside `Skyrim - Interface.bsa`
/// rather than as loose `Data/Strings/` files, which
/// `esm::StringTableSet::load` is the only thing that reads.
```

Both load-bearing statements are now wrong. `list_cells::run` calls `parse_record_indexes_in_load_order` (`byroredux/src/cell_loader/load_order.rs:206-213`), which installs an `ArchiveStringSource` and routes through `StringTableSet::load_with_archive` (`:118`), not `::load`. `ArchiveStringSource::discover` (`:143-175`) matches `Skyrim - Interface.bsa` twice over — once as a `plugin_archive` (`" - interface"` suffix on the `skyrim` stem) and once as a `shared_archive` (`stem.ends_with(" - interface")`), the latter covering `Update.esm` / `Dawnguard.esm` / `HearthFires.esm` / `Dragonborn.esm`, whose tables all live in that same archive.

## Evidence

`Skyrim - Interface.bsa` carries 138 `strings\…` entries including `strings\skyrim_english.strings`, `strings\update_english.strings`, `strings\dawnguard_english.strings` and `strings\dragonborn_english.strings`; `_ResourcePack.bsa` and each `ccBGSSSE*.bsa` carry their own 27 apiece, all reachable through the exact-stem match. The behaviour is pinned by the real-data test `real_skyrim_load_order_preserves_categories_and_resolves_archive_strings` (`byroredux/src/cell_loader/load_order.rs:540`).

## Impact

Documentation only. The `is_unresolved_lstring` helper the comment introduces is still correct and still useful (a non-localized or table-less plugin does produce the placeholder) — but a reader is told a live, fixed subsystem is broken, which is exactly the false premise the project's audit-hygiene rule exists to prevent.

## Suggested Fix

Rewrite the comment to say the placeholder is what a *non-localized or table-less* plugin yields, and point at `ArchiveStringSource` for where Skyrim's tables actually come from.

## Related

#1553 (the `.STRINGS` wiring), `db5bb149` (the multi-plugin load-path invocation the `/audit-skyrim` Dimension 4 checklist names).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)

---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27b.md` (`/audit-skyrim`).*
