# EXT-D3-2026-10-27-01: The temperate retune (c9433d501) left the old 9–21 BU envelope in the rustdoc and in exal-groundcover.md

Labels: low, terrain-exterior, documentation, doc-rot
Source: docs/audits/AUDIT_EXTERIOR_2026-10-27.md

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-27.md` finding `EXT-D3-2026-10-27-01` (Dimension 3, ground cover) · Severity LOW (doc-rot) · Status NEW.

**Location**: `crates/core/src/ecs/components/groundcover.rs:232-246` (`GroundCoverSpecies::DEFAULT_TEMPERATE` rustdoc and in-body comment); `docs/engine/exal-groundcover.md:1056-1058`, `:1900`, `:2003-2008`.

## Description
The temperate retune (`c9433d501`, 11-26 / 1.2-2.35) left the old 9-21 BU envelope in the rustdoc and in `exal-groundcover.md`:

- The `DEFAULT_TEMPERATE` rustdoc still says "so a 9–21 unit blade is shin-height".
- The in-body comment still opens with "The fallback stays deliberately short", after a sentence cut off mid-clause ("…for its four-ribbon tuft to"). It sits directly above the new "1.25x taller" paragraph.
- §12.16 states the temperate draw as "15 units, CV 0.231". The shipped draw is `moment_matched_length((11.0, 26.0))`: mean 18.5, CV ~0.234.
- `:1900` presents "temperate 9-21" as current.
- `:1056-1058` derives `GROUNDCOVER_CANOPY_LEAF_AREA_DENSITY = 0.4` from "the ~10-unit canopy the default temperate species stands up", a premise that moved with the ~1.23x height.

## Evidence
`groundcover.rs:254-255`: `blade_length_mean: moment_matched_length((11.0, 26.0)).0, blade_length_cv: moment_matched_length((11.0, 26.0)).1`. `exal-groundcover.md:1900` reads "Default blades are 1.5x taller (temperate 9-21", and `:2008` "15 units, CV 0.231".

## Impact
Readers tuning ground cover or deriving LAI from the doc start from numbers the code no longer uses. No behavioural effect.

## Related
#6248 (width growth tracked as performance), #6201, #6413.

## Suggested Fix
Update the rustdoc and the §12.16 numbers, mark `:1900` as historical, and re-state or re-derive the LAI premise.

## Completeness Checks
- [ ] **SIBLING**: The arid species' doc numbers checked for the same drift
- [ ] **TESTS**: Doc-number scan (if one exists) updated; otherwise none needed

