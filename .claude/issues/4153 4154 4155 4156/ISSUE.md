# Batch: #4153, #4154, #4155, #4156

## #4153 — NIF-D2-2026-09-11-03: NiMaterialProperty reuses FLAGS_U32_THRESHOLD for an unrelated field's cut
**Severity**: MEDIUM · **Location**: `crates/nif/src/blocks/properties.rs:44`; `crates/nif/src/version.rs:432-436`

`FLAGS_U32_THRESHOLD` is named/documented for the `NiAVObject.flags` u16→u32 widen (`bsver > 26`);
`NiMaterialProperty`'s compact-color cut reuses it via `>=` for nif.xml's independent `bsver < 26`
condition. Correct only because both constants happen to equal 26 today.

Fix: Add a distinct `MATERIAL_COMPACT_COLORS = 26` constant with its own doc and a two-sided test.

## #4154 — NIF-D3-2026-09-11-01: d5_coverage coverage-probe tool silently skips all .bto/.btr files
**Severity**: MEDIUM · **Location**: `crates/nif/examples/d5_coverage.rs:91,116`; `crates/nif/tests/block_coverage_baselines.rs:126`

The dispatch-coverage probe tool hardcodes a `.nif`-only filter instead of the shared
`corpus::is_nif_entry` (which also matches `.bto`/`.btr`). Against a `.bto`-dominant archive it
reports a vacuous "100% coverage" for 0 files actually opened.

Fix: Replace both hardcoded filters with `byroredux_nif::corpus::is_nif_entry`.

## #4155 — NIF-D5-2026-09-11-01: HavokPackfile::content_range/absolute_end add two u32 fields before widening
**Severity**: MEDIUM · **Location**: `crates/nif/src/blocks/collision/havok_packfile.rs:133-148`

`absolute_end`/`content_range` add two raw `u32` header fields before casting to `usize`, unlike
the rest of `parse_havok_packfile`. A section header summing past `u32::MAX` overflow-panics in
debug builds. Currently unreachable (no live caller yet), but a real hazard once wired up.

Fix: Widen to `usize` before adding. Add a unit test with fields summing past `u32::MAX`.

## #4156 — NIF-D5-2026-09-04-01: FO4+ bhkRigidBody reads the Skyrim CInfo body against a CInfo2014 wire layout
**Severity**: MEDIUM · **Location**: `crates/nif/src/blocks/collision/rigid_body.rs:107-113,173-178`

`BhkRigidBody::parse`'s FO4+ arm skips 4 bytes then reads fields at Skyrim's field order, by the
code's own comment admission that CInfo2014's real layout is "very different" and "knowingly
incomplete." Any FO4/FO76/Starfield NIF using the classic `bhkRigidBody` chain (not the NP/
`BhkSystemBinary` chain from #3809) yields garbage mass/friction/motion_type feeding the PHYSAL
solver's classification.

Fix: Decode `bhkRigidBodyCInfo2014` per nif.xml, or make the gap measurable via
`summarize_collision_authoring` if a full decode isn't warranted right now.
