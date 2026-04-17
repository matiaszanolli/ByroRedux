# #124 / NIF-513 — bhkNPCollisionObject family skip-only

## Root cause

Fallout 4 replaced the classic `bhkCollisionObject → bhkRigidBody → shape tree` chain with a new "NP" physics subsystem. Every FO4 NIF references collision through `bhkNPCollisionObject`; that block holds a ref to a `bhkSystem` subclass (`bhkPhysicsSystem` or `bhkRagdollSystem`) whose `ByteArray` carries the Havok-serialised physics tree.

The previous dispatch (`blocks/mod.rs:585-600`) routed all three types through a common skip-only arm that discarded their contents and returned a `NiUnknown` placeholder — so every FO4 parse dropped 100% of collision data.

## nif.xml reference

```xml
<niobject name="bhkNPCollisionObject" inherit="NiCollisionObject" versions="#FO4# #F76#">
  <field name="Flags"   type="bhkCOFlags" default="0x80"/>
  <field name="Data"    type="Ref" template="bhkSystem"/>
  <field name="Body ID" type="uint"/>
</niobject>

<niobject name="bhkPhysicsSystem" inherit="bhkSystem" versions="#FO4# #F76#">
  <field name="Binary Data" type="ByteArray"/>
</niobject>

<niobject name="bhkRagdollSystem" inherit="bhkSystem" versions="#FO4# #F76#">
  <field name="Binary Data" type="ByteArray"/>
</niobject>
```

`NiCollisionObject` contributes `target_ref: Ptr<NiAVObject>` (4 B). `ByteArray` is `u32 data_size; byte data[data_size]`.

## Fix

New parsers in `crates/nif/src/blocks/collision.rs`:

- `BhkNPCollisionObject { target_ref, flags, data_ref, body_id }` — 14 B wire size (+ anything `block_size` tells us is trailing, but nif.xml has nothing further for FO4/FO76).
- `BhkSystemBinary { type_name, data }` — single struct used for both `bhkPhysicsSystem` and `bhkRagdollSystem` since their layouts are identical (only the semantic role differs; `type_name` tags the concrete subclass). Keeps the raw bytes verbatim so a Havok parser can consume them later without re-parsing the outer NIF.

Dispatch in `blocks/mod.rs` now routes the three type names through the new parsers. The skip-only arm is gone.

## Sibling check

Grepped for other Havok blocks still in the skip-only fallback: none. `bhkNPCollisionObject`, `bhkPhysicsSystem`, and `bhkRagdollSystem` were the full set flagged by the 2026-04-05b audit.

## Scope considerations

- **No physics wiring yet**: the new structs store the refs + raw bytes, but `byroredux-physics` has no FO4 NP adapter. The physics bridge is still classic bhk → Rapier. Surfacing the blocks to the NIF→ECS importer is a follow-up when the FO4 physics adapter lands (tracked implicitly by M28 Phase 2).
- **Binary data is untouched**: parsing the inside of `ByteArray` requires a Havok HKX decoder, which is out of scope. We only need to keep the stream position correct and preserve the blob for future hand-off.

## Regression tests

Three new tests in `blocks::dispatch_tests`:

- `fo4_bhk_np_collision_object_dispatches_and_consumes` — builds a 14-byte synthetic payload (target_ref + flags + data_ref + body_id), confirms dispatch returns a `BhkNPCollisionObject` and the stream is consumed exactly.
- `fo4_bhk_physics_system_keeps_byte_array_verbatim` — confirms `bhkPhysicsSystem` dispatches through `BhkSystemBinary::parse` and preserves the byte-array payload unchanged.
- `fo4_bhk_ragdoll_system_keeps_byte_array_verbatim` — same assertion for `bhkRagdollSystem`, with a different payload and type-name tag.
