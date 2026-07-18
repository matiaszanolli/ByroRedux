# Issue batch: 1999, 2000, 2001, 2003

## #1999 — NIF-D1-02: BhkSimpleShapePhantom / BhkAabbPhantom miss the bhkWorldObject Unknown Int for v10.0.1.0-10.0.1.2 files
- Severity: HIGH (bug, nif-parser, nif)
- Location: `crates/nif/src/blocks/collision/phantom_action.rs:21-46` (`BhkSimpleShapePhantom::parse`), `:63-89` (`BhkAabbPhantom::parse`)
- `bhkWorldObject` base is `Shape(Ref) + Unknown Int(uint, until="10.0.1.2") + HavokFilter + WorldObjectInfo`. `BhkRigidBody::parse_oblivion_old` (rigid_body.rs:230-266, fixed under #1329) correctly skips this 4-byte field for old Oblivion; the two phantom parsers never do, reading shape_ref then havok_filter unconditionally.
- Suggested fix: add the same version gate (`stream.version() <= NifVersion::V10_0_1_2 { stream.skip(4)? }`) to both parsers, fix the stale doc comment claiming the field is never present.

## #2000 — NIF-D1-03: NiGeomMorpherController reads Morpher Flags / Num Interpolators without their nif.xml version gates
- Severity: HIGH by impact, narrow trigger (bug, nif-parser, nif)
- Location: `crates/nif/src/blocks/controller/morph.rs:30-36` (`NiGeomMorpherController::parse`)
- nif.xml gates `Morpher Flags` `since="10.0.1.2"` and `Num Interpolators`/`Interpolators` `since="10.1.0.106"`. Both read unconditionally — same bug class #1329/#1337 fixed for BhkRigidBody/bhkMoppBvTreeShape/bhkNiTriStripsShape/NiSkinData in the identical version window, never applied here.
- Suggested fix: gate `morpher_flags` behind `version() >= V10_0_1_2` (default 0 below), gate `num_interpolators`/ref loop behind `version() >= V10_1_0_106` (default empty).

## #2001 — NIF-D1-01: NiPersistentSrcTextureRendererData aliased to NiPixelData's parser — missing Pad Num Pixels + Platform fields
- Severity: HIGH on Oblivion (unrecoverable cascade) / MEDIUM on FO3+ (masked by block_sizes) (bug, nif-parser, nif)
- Location: `crates/nif/src/blocks/mod.rs:604-606` (dispatch alias), `crates/nif/src/blocks/texture.rs:236-306` (`NiPixelData::parse`)
- `NiPersistentSrcTextureRendererData` is dispatched to the same `NiPixelData::parse`, but diverges after the shared `NiPixelFormat` prelude: it additionally has `Pad Num Pixels` (since 20.2.0.6) and an unconditional 4-byte `Platform` field NiPixelData lacks. Current tail reads `num_pixels` then misreads `Pad Num Pixels` as `num_faces`, wrong byte-length multiplier, drops `Platform`.
- Suggested fix: give `NiPersistentSrcTextureRendererData` its own parser (or shared prelude + two-way tail split): `Num Pixels` → `Pad Num Pixels` (since 20.2.0.6) → `Num Faces` → `Platform`/`Renderer` → `Pixel Data`.

## #2003 — NIF-D1-04: NiShadeProperty.Flags read unconditionally, but nif.xml gates it to bsver <= FO3
- Severity: MEDIUM (Skyrim+ has block_sizes, masks drift) (bug, nif-parser, nif)
- Location: `crates/nif/src/blocks/properties.rs:547-556` (`NiFlagProperty::parse`), dispatch at `crates/nif/src/blocks/mod.rs:592-601`
- `NiSpecularProperty`/`NiWireframeProperty`/`NiDitherProperty`/`NiShadeProperty` all dispatch to the same `NiFlagProperty::parse`, but nif.xml gates `NiShadeProperty.Flags` to `vercond="#NI_BS_LTE_FO3#"` — the other three have no such gate. Currently `flags` read unconditionally for all four.
- Suggested fix: split `NiShadeProperty` into its own thin parser gated on `bsver <= FO3`, or branch inside `NiFlagProperty::parse` on `type_name == "NiShadeProperty" && bsver > FO3` to skip the read and default `flags`.
