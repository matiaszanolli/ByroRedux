# #3809 — FO4 precombine collision, Havok `BhkSystemBinary`

## The premise was stale in two directions

The issue body ("verified 2026-08-31") describes the blob as fully opaque and
the work as "greenfield format work". Two corrections, in order of how much
they changed the job:

1. **The container was already cracked.** `83565a9b` ("Triage #3809 #3810")
   landed `crates/nif/src/blocks/collision/havok_packfile.rs` — header,
   section table, class names — on 2026-08-31, a few hours *after* the status
   line in the issue body was written. It has **zero consumers**: only the
   `pub use` re-export. So the decoder existed and nothing could reach it.
2. **`__data__` is not compressed.** That prior pass's finding 6 called the
   data section "high-entropy … an already-compressed/bit-packed object
   stream". It is a plain relocatable object image. What made it look
   high-entropy is that the section's three *fixup tables* — sitting between
   `local_fixups_offset` and `exports_offset` — were being read as part of the
   payload. Corrected in place in that file's notes.

## What the fixup tables give

Decoding them turns the payload from bytes into a typed object graph:

| table | entry | what it yields |
|---|---|---|
| virtual | `(data_offset, section, class_name_offset)` | the exact start **and runtime class** of every top-level object |
| global | `(src, section, dst)` | cross-section pointer relocations |
| local | `(src, dst)` | intra-section pointers — the array-member data pointers inside each object |

Strides: local 8 B, global and virtual 12 B each. Tables are padded to their
declared span with `0xFFFFFFFF`, so the reader stops on the sentinel.

The class-name offset in a virtual fixup points at the **name text**, not the
record start — 5 bytes past it, past the record prefix. That is why
`parse_classname_records` now returns offsets alongside names.

## Corpus validation

`Fallout4 - MeshesExtra.ba2`, all **4,484** `_physics.nif` blobs. Six
independent cross-referential checks, all **100 %**:

| check | share |
|---|---:|
| `parse_havok_packfile` succeeds | 100 % |
| last section `absolute_end()` == blob length | 100 % |
| every virtual fixup resolves to a declared class | 100 % |
| objects in strictly ascending offset order | 100 % |
| every global fixup names a real section | 100 % |
| every local fixup lands inside its section | 100 % |

None of these is assertive — they are arithmetic or cross-referential, and a
wrong field order would not survive thousands of files of differing sizes.

Every blob carries the **same five objects in the same order**:
`hknpPhysicsSystemData`, `hknpCompressedMeshShape`, `hkRefCountedProperties`,
`hknpBSMaterialProperties`, `hknpCompressedMeshShapeData`. Local-fixup counts
scale 14 → 4,098 with mesh complexity, exactly as array pointers should.
Uniform header: `hk_2014.1.0-r1`, file version 11, 3 sections, layout rules
`8/1/0/1`.

## What remains, precisely

Not "decode a Havok binary format" — the field layout of five *named* classes,
and in practice one: `hknpCompressedMeshShapeData`, which carries the
bit-packed mesh. `__types__` is empty in every blob, so the file ships no
reflection metadata; layouts must come from corpus inference. That inference
now starts from known object bounds and known array-pointer locations instead
of an undifferentiated byte run.

Issue scope items 2 (extract into the collision pipeline) and the mesh codec
stay open — deliberately not attempted here; this is the spike.

## Non-vacuity

| injected fault | tests caught |
|---|---:|
| virtual/global fixup stride 12 → 8 | 3 |
| ignore the `0xFFFFFFFF` padding sentinel | 1 |

Tests use a hand-built synthetic fixture; no real game bytes are committed.
The existing 2-arg fixture builder became a thin wrapper over a
fixup-carrying one, so the five pre-existing tests are untouched.
