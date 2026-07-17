# 1999: NIF-D1-02: BhkSimpleShapePhantom / BhkAabbPhantom miss the bhkWorldObject Unknown Int for v10.0.1.0-10.0.1.2 files

https://github.com/matiaszanolli/ByroRedux/issues/1999

Labels: high, nif-parser, nif, bug

**Severity**: HIGH · **Dimension**: Stream Position Integrity
**Location**: `crates/nif/src/blocks/collision/phantom_action.rs:21-46` (`BhkSimpleShapePhantom::parse`), `:63-89` (`BhkAabbPhantom::parse`)
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIF_2026-07-16.md (NIF-D1-02)

## Description
Per nif.xml, `bhkWorldObject` is `Shape(Ref) + Unknown Int(uint, until="10.0.1.2") + HavokFilter + WorldObjectInfo`. `BhkRigidBody::parse_oblivion_old` correctly reads this 4-byte field; the two phantom parsers in the same module read `shape_ref` then jump straight to `havok_filter`, never consuming it.

## Evidence
```rust
// phantom_action.rs:22-26
let shape_ref = stream.read_block_ref()?;
let havok_filter = stream.read_u32_le()?;   // reads the phantom "Unknown Int" as HavokFilter on v10.0.1.0-.2
stream.skip(20)?; // bhkWorldObjectCInfo
```
vs. the sibling fix already in the same module (`rigid_body.rs:232-234`, #1329):
```rust
let shape_ref = stream.read_block_ref()?;
stream.skip(4)?; // Unknown — only present `<= VER_OB_OLD` (10.0.1.x)
let havok_filter = stream.read_u32_le()?;
```

## Impact
If a `bhkSimpleShapePhantom` or `bhkAabbPhantom` block appears in a v10.0.1.0-10.0.1.2 file (no `block_sizes` table), the parser misreads the offset and the stream ends 4 bytes short, cascading unrecoverably into every following block.

## Related
#1329, #1337 (same version band, same module, different block types)

## Suggested Fix
Add the same `stream.version() <= NifVersion::V10_0_1_2 { stream.skip(4)? }` gate (or a shared `read_bhk_world_object_prefix` helper) to both parsers, and correct the stale doc comment.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in related files (other `bhk*` collision block parsers sharing the `bhkWorldObject` base)
- [ ] TESTS: A regression test pins this specific fix (v10.0.1.x fixture for both phantom types)
