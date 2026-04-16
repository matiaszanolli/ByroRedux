# #326 / N1-01 — NiGeometryData Group ID threshold

## Root cause

`parse_geometry_data_base_inner` (tri_shape.rs:641-643) gates the
optional `group_id` i32 on `stream.version() >= NifVersion(0x0A000100)`
(10.0.1.0).

Per nif.xml line 3882:
```xml
<field name="Group ID" type="int" since="10.1.0.114">Always zero.</field>
```

`since="10.1.0.114"` → `NifVersion(0x0A010072)`. Files in the
`[10.0.1.0, 10.1.0.114)` window read 4 phantom bytes, misaligning
every downstream NiGeometryData field.

This is the third (and last) N1-* version-gate finding from the
2026-04-15 NIF audit — same class of bug as #327 (N1-02 keep/compress)
and #328 (N1-04 bounding volume). The parser's inner comment already
anticipated this fix with the note "tracked separately as N1-01".

## Games affected

Non-Bethesda Gamebryo pre-Civ IV era (rough boundary at 10.1.0.114,
which is when Gamebryo shipped the feature). All target games
(Oblivion 20.0.0.5 onward) are above 10.1.0.114 and unaffected.

## Fix

```rust
// crates/nif/src/blocks/tri_shape.rs:641-643
-if stream.version() >= NifVersion(0x0A000100) {
+if stream.version() >= NifVersion(0x0A010072) {
     let _group_id = stream.read_i32_le()?;
 }
```

Comment rewritten to reference the nif.xml `since="10.1.0.114"` gate
and the #326 audit cross-reference.

## Regression tests

Extended `nigeometry_data_version_tests` with:

- `nigeometry_data_at_10_1_0_113_skips_group_id` — at 10.1.0.113
  (one minor below threshold) the 4 bytes must NOT be consumed.
- `nigeometry_data_at_10_1_0_114_reads_group_id` — at the threshold
  they MUST be consumed.

The existing #327 fixture grew an `include_group_id` flag so each
test controls each version gate independently.
