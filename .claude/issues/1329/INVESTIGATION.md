# Investigation — #1329 Oblivion bhkConvexSweepShape/bhkMeshShape truncation

## Symptom (as filed)
3 vanilla Oblivion meshes (`handscythe01.nif`, `oar01.nif`,
`ungrdltraphingedoor.nif`) truncate at block 3 because
`bhkConvexSweepShape` / `bhkMeshShape` have no dispatch arm; on sizeless
Oblivion (v10.0.1.0, no `block_sizes`) the failed parse can't be skipped,
discarding all following render geometry.

## Root cause is DEEPER than the missing dispatch arms
Adding the two parsers stops the hard error, but the files still cascade —
because the **whole v10.0.x Havok chain is misaligned by 4 bytes per block**.

`parse_block_inner` (the #688 fix, mod.rs:234-237) consumes a 4-byte
`groupID` for **every** NiObject in file-version `[10.0.0.0, 10.1.0.114)`.
nifly reads this field in `NiObject::Get` — but the `bhkSerializable`
Get-chain (`bhkShape : bhkSerializable : bhkRefObject`, all `NiCloneable`,
not `NiCloneableStreamable`) does **not** reach `NiObject::Get`, and openmw's
`bhk*::read` bodies never read a groupID either. So Havok blocks carry **no**
groupID, yet ByroRedux consumes one for them at v10.0.x.

This was never caught because:
- v20.x (FO3+/Skyrim) Oblivion-Havok content is `> 10.1.0.114` → no groupID
  consumed → existing bhk parsers correct.
- The rare v10.0.1.0 Oblivion files that DO hit bhk blocks all truncated at
  the first *undispatched* shape (block 3) before reaching the misaligned
  downstream blocks.

### Hexdump proof (handscythe01, block region @126)
Decoding the bhkBoxShape→bhkConvexSweepShape→bhkTransformShape chain
**without** a per-block groupID yields fully sane values:
- box @126 ends @162 (material=5, radius=0.1)
- sweep @162: shape_ref=2 (→box), material=5, radius=0.1
- transform @190: shape_ref=3 (→sweep), material=5, valid 4×4 rotation matrix
- box @278: material=5, radius=0.1

**With** the groupID consumed, every field is garbage-shifted (shape_ref=76M,
material=0.1, radius=0.0). Confirmed across multiple blocks → not coincidence.

`NiTriStripsData` (door block 2, `Ni`-prefixed, NiObject-derived) DOES carry
the groupID — block 3 lands correctly at @5578 only when it's consumed. So
the distinction is precisely **Havok serializables (`bhk*`/`hk*`) lack the
groupID; everything else keeps it.**

## Three-part fix (all in the nif crate)
1. **mod.rs `parse_block_inner`**: gate the v10.0.x groupID consume to skip
   `bhk*`/`hk*` block types. Low risk: only affects v10.0.x Havok blocks
   (these rare Oblivion files); #688's NiNode-rooted 154 files are unaffected.
2. **bhkConvexSweepShape / bhkMeshShape**: new openmw-exact parsers
   (`Shape Ref + HavokMaterial + Radius + skip12`; and `skip8 + Radius +
   skip8 + Scale + props + skip12 + strips`). bhkMeshShape uses the true
   openmw `skip(8)` (an earlier `skip(4)` was only compensating for the
   groupID bug and is reverted).
3. **bhkMoppBvTreeShape**: gate its `hkpMoppCode` `Offset`(origin) read on
   `version >= 10.1.0.0` — nif.xml/openmw say `since="10.1.0.0"`, but the
   existing parser read it unconditionally, over-reading 16 bytes on
   v10.0.1.0. (Separate pre-existing bug, exposed once the chain aligns.)

## Verification oracle
`trace_block` over each of the 3 files must reach full `num_blocks`
(47 / 17 / 31) with zero ERR and sane references — proves byte-exact
alignment on sizeless content.

## Cross-check sources
- nif.xml `bhkConvexSweepShape` (3117), `bhkMeshShape` (3179),
  `hkpMoppCode` (3140, `Offset since=10.1.0.0`), `HavokMaterial` (2293).
- openmw `components/nif/physics.cpp` read bodies.
- nifly `include/BasicTypes.hpp:972` (groupID gate) + `include/bhk.hpp` class
  hierarchy (bhk bases are `NiCloneable`, no Get override).
