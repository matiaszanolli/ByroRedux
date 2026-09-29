# #5104: FO4-D1-02: e593770f0 inserted a new test inside another test's doc comment — resolve_precombine_owner_follows_form_id_mod_index lost the first line of its doc

**Labels**: documentation, low, legacy-compat, tech-debt, game:fo4

**Source report**: `docs/audits/AUDIT_FO4_2026-09-29.md`
**Severity**: LOW
**Dimension**: M49 precombines (doc hygiene)

## Location
`byroredux/src/cell_loader/precombined.rs` (test module: `oc_nif_paths_are_already_canonical_cache_keys` / `resolve_precombine_owner_follows_form_id_mod_index`).

## Description
e593770f0 inserted the new test `oc_nif_paths_are_already_canonical_cache_keys` inside another test's doc comment. The #1590 doc line now sits on top of the new test, and its continuation sits alone above the test it belongs to.

## Evidence
```
    /// #1590 (a) — the CSG + subdir follow the cell's owning plugin (form-id
    /// The streaming worker pre-parses precombines under the key the drain
    ...
    fn oc_nif_paths_are_already_canonical_cache_keys() {
    ...
    /// mod-index byte → load order), not the last-loaded `--esm`.
    #[test]
    fn resolve_precombine_owner_follows_form_id_mod_index() {
```

Validated at HEAD 9fcfdc3fc: the spliced comment is present exactly as above.

## Impact
Cosmetic. Both tests' rationale reads garbled, and the #1590 attribution sits on the wrong test.

## Suggested Fix
Move the `/// #1590 (a) — …` line down to rejoin its continuation above `resolve_precombine_owner_follows_form_id_mod_index`.

## Completeness Checks
- [ ] **SIBLING**: Other e593770f0 test insertions checked for the same splice

