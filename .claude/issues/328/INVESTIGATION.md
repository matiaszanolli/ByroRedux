# #328 / N1-04 — NiAVObject bounding-volume branch triggers outside legitimate window

## Root cause

`NiAVObjectData::parse` (base.rs:108-113):

```rust
let collision_ref = if stream.version() >= NifVersion(0x0A000100) {
    stream.read_block_ref()?
} else {
    skip_bounding_volume(stream)?;
    BlockRef::NULL
};
```

Per nif.xml (line 3492-3494):

```xml
<field name="Has Bounding Volume" type="bool"          since="3.0" until="4.2.2.0" />
<field name="Bounding Volume"     type="BoundingVolume" cond="Has Bounding Volume"
                                  since="3.0" until="4.2.2.0" />
<field name="Collision Object"    type="Ref" template="NiCollisionObject"
                                  since="10.0.1.0" />
```

Three legitimate branches:

| Version range              | Field present                |
|----------------------------|------------------------------|
| `<= 4.2.2.0`               | `Has Bounding Volume` + body |
| `(4.2.2.0, 10.0.1.0)` gap  | *neither*                    |
| `>= 10.0.1.0`              | `Collision Object`           |

Current code collapses the first two ranges into one, consuming a
phantom `has_bv` bool (and optionally a full BoundingVolume body) in
the `[4.2.2.1, 10.0.0.x]` gap window.

## Games affected

Non-Bethesda Gamebryo files in the pre-10.0.1.0 range that aren't the
Morrowind-era NetImmerse. All target games (Oblivion through Starfield)
are well above 10.0.1.0 — unaffected.

## Fix

```rust
// crates/nif/src/blocks/base.rs:108
let collision_ref = if stream.version() >= NifVersion(0x0A000100) {
    stream.read_block_ref()?
} else if stream.version() <= NifVersion(0x04020200) {
    skip_bounding_volume(stream)?;
    BlockRef::NULL
} else {
    // Gap window [4.2.2.1, 10.0.0.x]: neither Bounding Volume nor
    // Collision Object is serialized per nif.xml. See #328.
    BlockRef::NULL
};
```

## Sibling check

`NiAVObjectData::parse_no_properties` (line 127) is BSTriShape-only
(Skyrim+, version 20.2.0.7). Always reads `collision_ref`
unconditionally — correct, not affected.

## Regression test

New `#[cfg(test)] mod tests` in base.rs:
- **Gap window (10.0.0.0)**: no bounding-volume bool and no
  collision_ref; parser must consume exactly the NiObjectNET + flags +
  transform + properties_list bytes.
- **Pre-Gamebryo (4.2.2.0)**: bounding-volume bool (and velocity vector)
  still consumed.
