# #5341: EXT-D5-2026-10-05-05: WATAL / EXAL doc rot after the fix wave (bundle)

**Labels**: low,terrain-exterior,water,documentation,doc-rot
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5341

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-05.md` — `EXT-D5-2026-10-05-05` (HEAD `a2c24b16e`)

- **Severity**: LOW (doc rot)
- **Dimension**: Water translation (WATAL), plus one Dim 1 doc and one census tool
- **Location / Status**: per item
- **Tier Violated**: no-fabrication (documentation)
- **Game Affected**: Oblivion, FO3/FNV, FO76, Starfield (docs only)
- **Description**:
  1. **watal.md §2 Oblivion row re-settles an OPEN frame** (Regression of #5182 by #5136's `f75ac66cb`).
     - The rewritten per-game table (`docs/engine/watal.md:421`) gives Oblivion "Conversion: none applied" with "frame follows from the field's definition, not a census".
     - The #5182 table about 100 lines later (`:518`) says "**un-rotated, OPEN** … does not establish φ = θ". The doc now contradicts itself, which is exactly the defect #5182 removed.
  2. **The parser field doc was never updated** (incomplete #5136).
     - `crates/plugin/src/esm/records/misc/water.rs:312-322` still says the translate boundary "applies the one +90° rotation to either" convention.
     - It also lists FO76, Starfield and FO3/FNV `DNAM[100]` as wind-FROM bearings. Code (`env_translate.rs:603-610`) and watal.md now say otherwise. The baseline's D5-01 fix suggestion named this exact site.
  3. **watal.md:352** (#5185 text, landed after #5169) says "(`spatial_units::normalize`, Starfield-gated, FO76 untouched)". The #5169 sentence four lines below says FO76's absorption *is* lifted. `spatial_units.rs:1-4` and `:85-101` agree with #5169.
  4. **exal.md §5.4** (`docs/engine/exal.md:516-526`) still describes "a single worldspace-wide LOD water quad (a hole-cut annulus …) … a fixed entry-time snapshot". Since #5243 it is a per-cell mesh rebuilt on every grid crossing.
  5. **watal.md:788-790** "Older games naturally use the same path" is false for Oblivion (D5-02).
  6. **`crates/plugin/examples/xclw_census.rs:89-91`**: the #5244 per-ring bucket computes `(dx).abs().max(dy)`, with no `.abs()` on the y term. Cells south of the probe grid land in the wrong ring or are dropped by the `d > 0` filter. #5244's issue cites this tool's per-ring counts as its reconciliation evidence.
- **Suggested Fix**:
  - Make the §2 Oblivion row "none applied, **OPEN**" and point it at the #5182 row.
  - Rewrite the parser doc to say "per-game; see watal.md §2", and drop the FO76/Starfield/FO3-FNV bearing claim.
  - Fix line 352 to "Starfield-gated, FO76 absorption only (#5169)".
  - Rewrite exal.md §5.4 to describe the per-cell rebuild model.
  - Add `.abs()` to the census.

## Publisher note

Item 1 is a doc regression of the closed #5182 (re-introduced by #5136's `f75ac66cb`). Item 6 is a real bug in a census example (`crates/plugin/examples/xclw_census.rs`, missing `.abs()` on the y term), not doc text — it can ride the same commit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
