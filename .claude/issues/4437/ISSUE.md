# #4437: SF-2026-09-16-D4-01: Starfield ESM coverage docs point at the pre-split dispatch site, and the #4278 drift history lists LCTN as a live arm when it has none

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4437
- **Labels**: low,esm-plugin,documentation,doc-rot,game:starfield,legacy-compat
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW
- **Dimension**: 4
- **Location**:
  - `crates/plugin/examples/sf_smoke.rs:10-12`
  - `crates/plugin/src/esm/records/parse.rs:33-35`
  - `docs/engine/starfield-esm-phase0-baseline.md:9, 23, 165, 175, 232`
  - `docs/engine/starfield-esm-roadmap.md:73, 239`
- **Status**: NEW
- **Description**:
  - **Stale dispatch site**: `sf_smoke`'s module doc still locates the
    catch-all at "`records/mod.rs:925`". `eaa94b49d` moved the dispatch to
    `records/parse.rs`, and `records/mod.rs` is now a 145-line barrel. The
    Phase 0 baseline doc still calls `DISPATCH_HANDLED_FOURCCS` "the
    hand-maintained slice in `sf_smoke.rs`", which #4278 made false.
  - **LCTN is not a live arm**: `DISPATCH_HANDLED_FOURCCS`'s own doc (and
    #4278's body) says the hand list "drifted three times (LCTN, then …),
    each time reporting live dispatch arms as 'skip'". LCTN has **no**
    top-level dispatch arm. `rg 'b"LCTN"' crates/plugin/src/esm/records`
    finds nothing, `sf_smoke` correctly reports `LCTN … 6017 … skip` today,
    and `starfield-esm-phase0-baseline.md:223` agrees: "Locations (LCTN)
    silently skipped at the top level".
- **Impact**: The doc drift misleads the coverage tooling's readers. The LCTN
  line invites someone to "fix" the list by re-adding LCTN, which would make
  the tool over-report coverage (the regression class #4278 closed, inverted).
- **Suggested Fix**: Repoint the three doc sites at `records/parse.rs` and
  replace LCTN with SECH/AOPF + OMOD/LVSP/SCEN in the drift history.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
