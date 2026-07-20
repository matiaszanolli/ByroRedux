# #2111: D8-01: Streaming worker re-parses the whole NIF header just to read bsver

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2111
**Labels**: bug, nif-parser, medium, performance

---

**Severity**: medium
**Dimension**: NIF Parse
**Location**: `byroredux/src/streaming.rs:517-522`
**Status**: NEW

## Description
`parse_one_nif` calls `NifHeader::parse(&bytes)` a **second** time (after `parse_nif` already succeeded) only to read `user_version_2` — but the just-parsed `NifScene` already retains it as `scene.bsver` (`crates/nif/src/scene.rs:124`, set at `crates/nif/src/lib.rs:829`). The re-parse re-walks and re-allocates the entire header string + block-type tables (one `Arc<str>` per string), not the "~60 bytes" the inline comment claims. Runs for every NIF on the background cell-load streaming worker; Skyrim SE/FO4/FO76/Starfield meshes carry hundreds of strings, doubling header allocation count per NIF on that path.

## Evidence
```rust
// byroredux/src/streaming.rs:517-522
// NifScene doesn't retain the header, so re-parse it (~60 bytes)
// to read BSVER for the game-era-gated BSXFlags bit-5 check
// the drain step applies — mirrors `parse_and_import_nif`.
let bsver = byroredux_nif::header::NifHeader::parse(&bytes)
    .map(|(h, _)| h.user_version_2)
    .unwrap_or(0);
```
`scene.bsver` is already set at `crates/nif/src/lib.rs:829: bsver: header.user_version_2,` inside the same `parse_nif` call this function already made — the comment's premise ("NifScene doesn't retain the header") is stale.

## Impact
Doubles per-NIF header allocation count (string table + block-type table) on the background cell-load streaming worker, for every NIF loaded via streaming rather than direct load.

## Suggested Fix
Replace with `let bsver = scene.bsver;` (binding already in scope at streaming.rs:509) and correct/remove the stale comment. Zero extra allocation.

**dhat bound**: extend `crates/nif/tests/heap_allocation_bounds.rs` with a fat (~200-entry) string-table fixture and pin `max_blocks < baseline + num_strings` (not `+ 2*num_strings`) to catch a re-introduced double-parse.

## Completeness Checks
- [ ] **SIBLING**: Check `parse_and_import_nif` (the function this comment says it "mirrors") for the same pattern
- [ ] **TESTS**: A regression test / dhat bound pins this specific fix

