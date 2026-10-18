# UI-D1-2026-10-17-02: SwfDecodeCounts::decompresses is a hard-coded literal and no prepare_movie test feeds a CWS movie

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/5967 (snapshot as filed 2026-10-17; GitHub is authoritative for current state)

**Source**: `docs/audits/AUDIT_UI_2026-10-17.md` (UI-D1-2026-10-17-02) · **Severity**: LOW (test-fidelity gap; the code is correct by reading) · **Dimension**: Profile, Prepare & Host Bridge Transport · **Profile**: both · **Status**: NEW (adjacent to closed #5479)

## Description
Both `prepare_movie` return arms construct `SwfDecodeCounts { decompresses: 1, .. }` unconditionally (`crates/ui/src/prepare.rs:310` and `:338`); the comment at `:259` says it "measures the real work". The three `SwfDecodeCounts` guards (`an_archive_menu_open_decompresses_and_parses_once` at `:402`, `a_loose_avm1_movie_is_decompressed_once_and_never_parsed` at `:430`, and the mismatch guard) build movies with `swf::write_swf`, which are uncompressed FWS, and assert the literal against itself. The only CWS pin is `conforming_cws_returns_its_decode_with_an_untouched_body`, which tests the shim in isolation, not `prepare_movie`.

## Evidence
`grep -n decompresses crates/ui/src/prepare.rs` shows only the two literals and test reads. No test passes a CWS-signed buffer to `prepare_movie`. Shipped content is CWS: 51 of 53 Skyrim and 311 of 311 FO4 interface movies.

## Impact
Nothing turns red if a stage re-inflates raw CWS bytes (the #5479 shape), or if the shim declines and the stock decode runs (which really inflates twice, stock plus Ruffle's `from_data`, while still reporting 1). The documented single-decode floor is not machine-checked on the compressed path that matters.

## Suggested fix
Add a `prepare_movie` test over a real zlib-compressed AVM2 and AVM1 movie asserting `prepared.data` starts with `FWS`. Derive `decompresses` from whether the shim returned `Some` for a CWS input, or count inflates through a wrapped reader, instead of a literal.

Related: #5479, UI-D1-2026-10-17-01 (ceiling gap).

## Completeness Checks
- [ ] **SIBLING**: The navigator's dependency path decode counts checked for the same literal
- [ ] **TESTS**: The new test fails when `prepare_movie` re-inflates a raw CWS input
