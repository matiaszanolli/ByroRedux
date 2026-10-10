# #5519: EXT-D1-2026-10-09-04: Climate / LOD doc rot after the 10-08 fix wave: the "Oblivion-era REGN CNAM" claim that #5421 refuted survives, a dead intra-doc link, and #5374 / #5388 / #5222 / #5387 are not in exal.md

**Labels**: doc-rot, documentation, low, terrain-exterior

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-09.md` — finding `EXT-D1-2026-10-09-04` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW (doc rot)
- **Dimension**: EXAL boundary discipline (docs)
- **Location**:
  - `docs/engine/exal.md:128-138` (climate paragraph).
  - `byroredux/src/env_translate.rs:489-505` (`resolve_exterior_climate` doc), including the link at `:501`.
  - exal.md §5.2 (`:480-496`, Fallout legacy blocks).
- **Status**: NEW. This is distinct from #5341, whose item 4 (exal.md's LOD-water "entry-time snapshot" text) is not re-reported.
- **Tier Violated**: no-fabrication (documentation)
- **Game Affected**: all (docs)
- **Description**:
  1. exal.md:133 says the region chain is the REGN `CNAM` that "only Oblivion-era regions author (#5421)". env_translate.rs:497-499 says the same, and `:503-504` says "the only TamrielClimate references on disk are one special-case worldspace and one Shivering-Isles region".
     - #5421 established that xEdit defines no REGN `CNAM` and that vanilla authors none.
     - This census agrees: REGN `CNAM` occurs 0 times in Skyrim.esm (317 REGN), Fallout4.esm (106), Fallout3.esm (139) and Oblivion.esm (211).
     - The rung is inert on all vanilla content. `4e32d33ec` was written after `bd052048a` and misattributes it.
  2. env_translate.rs:501 links [`named_or_richest_climate`], which no longer exists: `217bcbc9f` renamed it to `oblivion_climate_rungs`.
  3. exal.md's climate text predates #5388. It describes only the `"<worldspace>Climate"` naming, not the inherit-all step or the naming along the whole `WNAM` chain.
  4. Neither exal.md nor watal.md records #5374's rule that pre-FO3 children inherit everything (it governs Oblivion default water and climate).
  5. exal.md §5.2 has no text for #5222's authored-quad index (`LegacyLodQuadIndex`) or #5387's majority-residue prune. Both now decide what FO3/FNV distant LOD draws.
- **Suggested Fix**:
  - Rewrite the region-rung sentence (in exal.md and the code doc) as "xEdit-undefined; 0 vanilla occurrences in any game; kept for mod data". Fix the link.
  - Add the #5374/#5388 inheritance and chain-naming rungs, and a §5.2 paragraph on the index and the prune (citing D6-01's tie caveat).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
