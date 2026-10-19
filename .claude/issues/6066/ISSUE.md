# #6066 (snapshot as filed)

Split from #5015.

**Split from #5015** (FO4 ragdoll decode, `e1bd40bcf`).

## Description
`decode_np_ragdoll` reads each FO4 ragdoll body's mass, shape, pose and joints, but not its friction, restitution or damping. The blob stores them (a material row and a motion-properties row per body, identical for all 18 human bodies), yet their field identities are not established, so `extract_np_ragdoll` gives every FO4 body the values the Skyrim humanoid ragdoll authors on all but one body: linear damping 0.0996, angular damping 0.0498, friction 0.3, restitution 0.8 (`NP_*` constants in `crates/nif/src/import/collision/ragdoll.rs`).

## Evidence
Human skeleton blob: the 80-byte rows at `hknpRagdollData+16` and the 64-byte rows at `+32` are byte-identical across all 18 bodies. They hold `bfloat16`-style values (upper 16 bits of an `f32`): `0.6016` ×2, `0.2002`, `0.0498` in the first; `1.0, 1.0, 104.375, 31.57, 0.4004, 0.6016, 0.17, 0.4905, 0.0025, 0.0025, 1.0` in the second. `0.0498` is exactly Skyrim's authored angular damping, which suggests the encoding but not which field is which. Nothing in the corpus names them (`__types__` is empty in every blob).

## Why it matters
Restitution 0.8 on a corpse is high and damping is the dial `ragdoll_extra_angular_damping` / #5697 acts on. FO4 ragdolls will feel like Skyrim's rather than FO4's, and creature rigs with different authored values are not distinguished.

## Suggested approach
Identify fields by varying evidence rather than by guessing: compare rows across skeletons (creatures, robots, power armor) to see which values vary and correlate with what (a heavy robot vs a rat), and against the Skyrim `bhkRigidBody` values for the same body in the draugr/human rigs. Only wire a value once two independent checks agree (No Guessing policy).

## Related
#5015, #5697, `docs/engine/physal.md`.

## Completeness Checks
- [ ] **TESTS**: decoded values pinned on the human rig and one creature rig
- [ ] **SIBLING**: classic Skyrim damping/friction path unchanged
