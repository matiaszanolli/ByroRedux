# #327 / N1-02 — NiGeometryData Keep/Compress Flags version threshold

## Root cause

`parse_geometry_data_base_inner` (tri_shape.rs:648) gates `keep_flags` /
`compress_flags` on `stream.version() >= NifVersion(0x0A000100)`, i.e.
**10.0.1.0**.

Per nif.xml (line 3886-3887):

```xml
<field name="Keep Flags"     type="byte" since="10.1.0.0" ... />
<field name="Compress Flags" type="byte" since="10.1.0.0" />
```

`since="10.1.0.0"` is `0x0A010000`, not `0x0A000100`. Files in the
`[10.0.1.0, 10.1.0.0)` window (non-Bethesda Gamebryo) currently consume
2 phantom bytes before `has_vertices`, corrupting every NiGeometryData
downstream.

## Games affected

Non-Bethesda Gamebryo content in the `10.0.1.x` minor range. Target
games (Oblivion 20.0.0.5, FO3/FNV/Skyrim 20.2.0.7, SSE 20.2.0.7, FO4+
BSVER-based) are all above 10.1.0.0 and unaffected.

## Fix

```rust
// crates/nif/src/blocks/tri_shape.rs:648
-if stream.version() >= NifVersion(0x0A000100) {
+if stream.version() >= NifVersion(0x0A010000) {
     let _keep_flags = stream.read_u8()?;
     let _compress_flags = stream.read_u8()?;
 }
```

## Sibling check

`group_id` on line 641 is also gated at `>= 0x0A000100`. Per nif.xml
line 3882 it's `since="10.1.0.114"` (`0x0A010072`). That's **N1-01**,
filed separately as #326 and out of scope for this issue.

`parse_psys_geometry_data_base` shares the same inner routine
(`parse_geometry_data_base_inner`) so the fix applies to both paths.

## Regression test

New `nigeometry_data_version_tests` module in tri_shape.rs:
- **10.0.1.0** (in the gap): stream must NOT consume keep/compress bytes.
- **10.1.0.0** (nif.xml threshold): stream MUST consume them.
