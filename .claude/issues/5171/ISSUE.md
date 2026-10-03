# #5171: EXT-D1-2026-10-02-03: No test pins the Starfield LGTM unit lift — #5002 added four height-field lifts to `normalize` with only a wire-decode test, and LGTM fog/fade lifting has never been pinned

**Labels**: low,terrain-exterior,esm-plugin,test-gap,bug,game:starfield
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: LOW
- **Dimension**: EXAL boundary discipline (test coverage of the parse-boundary lift)
- **Location**:
  - `crates/plugin/src/esm/records/spatial_units.rs:106-124` (LGTM arm, including the #5002 lines 117-123);
  - `crates/plugin/src/esm/records/misc/world.rs` test `starfield_lgtm_data_decodes_the_sf_xcll_layout` (calls `parse_lgtm` directly: wire values, no normalize);
  - `crates/plugin/src/esm/records/spatial_units_tests.rs` (no LGTM or XCLL case).
- **Status**: NEW
- **Tier Violated**: n/a (test gap)
- **Game Affected**: Starfield
- **Description**:
  - `spatial_units_tests.rs` pins REFR/LIGH/SCOL (`:55`), WTHR (`:158`) and WATR (`:245`). It has no case for `index.lighting_templates`, and none for CELL XCLL `lighting()`: fog near/far, fog_clip, light fades, the SF height mid/ranges.
  - Deleting the whole LGTM arm, or just #5002's four height lines, leaves every test green.
  - The baseline's suggested-fix text ("a spatial_units test like the XCLL/LGTM ones") assumed tests that do not exist.
- **Impact**: A regression to metric LGTM/XCLL fog (the ~70× class of #5134) would go undetected. 6 vanilla LGTMs and every Starfield interior XCLL depend on this arm.
- **Suggested Fix**:
  - Add a `parse_esm` fixture with a 108-B Starfield LGTM, using `ShipInteriorLT` values plus distinct heights, and a cell with a 108-B XCLL.
  - Assert fog/clip/fade/heights ×70 and the scales/gravity untouched.
  - Add a non-Starfield companion that asserts the authored values.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
