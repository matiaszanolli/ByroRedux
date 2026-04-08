# Issue #172 Investigation

## Problem
`read_string()` uses `version >= 0x14010003` (20.1.0.3) to dispatch to the string-table path. nif.xml says `Num Strings`/`Max String Length`/`Strings` are `since="20.1.0.1"` (0x14010001).

## Affected sites (this fix)
- `crates/nif/src/stream.rs:153` — `read_string` dispatch
- `crates/nif/src/header.rs:147` — header string table parse

Both must stay in sync or a file in the 20.1.0.1/20.1.0.2 band would have the strings table populated by the header but fall through to inline-string reads in `read_string` (corruption).

## Not changed (different semantic, same hex literal coincidence)
- `crates/nif/src/blocks/tri_shape.rs:124` — NiGeometry "Has Shader" + "Shader Name" upper bound. nif.xml gates the inline shader fields `until="20.1.0.3"` literally.
- `crates/nif/src/blocks/properties.rs:244,277` — NiTexturingProperty shader texture flags format change. Different field, separate xml gate.

## Test impact
Existing stream.rs tests use `V20_2_0_7` (>= both thresholds) or `V4_0_0_2`/`0x0A000100` (< both thresholds). No regression. Add one new test at exactly `0x14010001` to lock the threshold.
