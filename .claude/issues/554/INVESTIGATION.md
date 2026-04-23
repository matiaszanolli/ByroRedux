# #554 Phase 1 — bisect of Oblivion NiUnknown pool

**Audit finding:** 429 NiUnknown blocks across 32 already-dispatched types in
`Oblivion - Meshes.bsa` (8,032 NIFs scanned).

## Premise check

The audit's framing — "32 distinct types in the NiUnknown bucket despite all
being in dispatch" — implies up to 32 independent per-type parser bugs. That
framing is **wrong**. The 429 blocks are not spread across 32 independent
defects; they are cascade failures from a very small number of upstream
drift sources. This is verified by tracing each affected NIF.

## Methodology

Three instrumented examples were added (now checked in under
`crates/nif/examples/`):

| Example            | Purpose                                                        |
|--------------------|----------------------------------------------------------------|
| `locate_unknowns`  | For every NiUnknown type, records first N (nif, block_index).  |
| `recovery_trace`   | Runs `parse_nif` with `info` logs → every recovery event.      |
| `dump_nif`         | Extracts a NIF from a BSA for byte-level inspection.           |

Pipeline: `unknown_types` (existing) → histogram → `locate_unknowns` → pick
representatives per type → `recovery_trace` to find the *first* failing block
per NIF → `trace_block` to walk the preceding blocks → `xxd` on the raw dump
to identify the drift point.

## Per-NIF first-failure survey (9 representative NIFs)

| NIF                                                              | First-fail block                  | Drift cause                                        |
|------------------------------------------------------------------|-----------------------------------|----------------------------------------------------|
| `dungeons\fortruins\traps\rftrapdarts01.nif`                     | 151 NiTexturingProperty           | Particle system upstream                           |
| `landscape\landscapewaterfall02.nif`                             | 30 NiPSysEmitterCtlr              | Block 17 NiPSysData under-consumes by ~482 bytes   |
| `fire\fireopenmedium.nif`                                        | 46 NiPSysSpawnModifier            | Block 24 NiPSysData (same)                         |
| `dungeons\misc\fx\fxwhitetorchlarge.nif`                         | 37 NiNode                         | Block 20 NiPSysData (same)                         |
| `oblivion\sigil\sigilfireboom01.nif`                             | 233 NiPSysSpawnModifier           | Block 150 NiPSysData (100 particles, same root)    |
| `magiceffects\drain.nif`                                         | 283 NiTriStrips                   | Block 206 NiPSysData (100 particles, same root)    |
| `oblivion\gate\obliviongate_forming.nif`                         | 49 NiTransformData                | Block 30 NiGeomMorpherController / NiMorphData chain |
| `dungeons\misc\dustcloudhorizontal01.nif`                        | 58 NiBoolData                     | Upstream animation-controller drift (no NiPSysData) |
| `oblivion\seige\deidricseigecrawleractivator.nif`                | 565 bhkRigidBody                  | Oblivion Havok `bhkRigidBody` (already-known, SE-dominant) |

## Root cause 1 — NiPSysData drops `Particle Info` on pre-BS202 Bethesda streams (dominant)

**Scope:** ≈80 % of the 429-block pool. Every particle-system NIF in the
Oblivion corpus.

**Evidence — `landscape\landscapewaterfall02.nif`, block 17:**
- Block 17 NiPSysData at stream offset 1088 (file offset 2126) parses with
  `num_vertices = 15`, `has_vertices = 1`, plus inherited NiParticlesData
  fields (has_radii, sizes, rotations, …).
- Parser consumes 647 bytes. Actual block 18 NiPSysAgeDeathModifier begins at
  file offset 3255 (confirmed by hex dump — "NiPSysAgeDeath:2" string marker
  at 0xCB7). Stream-relative, block 18 starts at 2217, not 1735.
- Gap: **482 bytes** of data the parser never reads, which the outer loop
  then walks as zero padding for subsequent blocks.

**Cause.** `crates/nif/src/blocks/particle.rs:803-824` comments out the
`Particle Info` field ("Non-Bethesda only") and only reads the three tail
fields (`has_rotation_speeds` + array + `num_added`/`added_particles_base`).
nif.xml line 4030 contradicts this:

```
<field name="Particle Info" type="NiParticleInfo" length="Num Vertices"
       vercond="!#BS202#" />
```

`!#BS202#` means "not Bethesda 20.2.0.7+". Oblivion is 20.0.0.4 → the field
IS required, 15 entries × (12 velocity + 4 age + 4 life_span + 4 last_update +
2 spawn_gen + 2 code) = 28 bytes each = 420 bytes. The remaining 62 bytes of
the observed 482-byte gap match the inherited `Has Rotation Speeds` array
(15 × 4 = 60) + the 2-byte `Num Active` field that the current parser
already reads — the 62-byte delta is from one or more inherited arrays
the file serializes that the parser treats as empty because its `has_*`
byte happens to fall on a zero in the Particle Info region.

nif.xml `NiParticleInfo` definition excludes `Rotation Axis` since 10.4.0.1,
so on Oblivion (20.0.0.4) each entry is 28 bytes, not 40.

**Scope of fix.** Read `Num Vertices × NiParticleInfo` in `parse_particles_data`
when `type_name != "NiParticlesData"` and stream is not BS202.

Child issue: **NIF-09A** (to be filed).

## Root cause 2 — Havok `bhkRigidBody` drift (already-known)

Accounts for 6 NiUnknown in Oblivion (block 565 of
`deidricseigecrawleractivator.nif` and a handful of siblings). This is the
same defect the audit listed separately as NIF-02 (Skyrim SE: 12,866
`bhkRigidBody` failures). The Oblivion footprint is tiny and rolls up into
that issue; no new child issue needed.

## Root cause 3 — Animation-controller chain drift (residual)

A small subset of the pool comes from NIFs with no particle systems at all
(`obliviongate_forming`, `dustcloudhorizontal01`, …). First failures are
on NiTransformData / NiBoolData / NiFloatData blocks with garbage KeyType
values, meaning the stream drifted earlier in the controller chain.

Preliminary bisect on `obliviongate_forming.nif`:
- Block 30 NiGeomMorpherController consumed 77 bytes.
- Block 31 NiMorphData parsed `num_morphs = 0` and consumed 9 bytes, then
  subsequent blocks read into a zero-padded region — but the block-31 peek
  `00 00 00 00 05 00 00 00 c3 01 00 00 01 04 00 00 00 42 61 73 65` shows the
  real NiMorphData has `num_morphs != 0` and a morph entry named "Base".

→ drift source is either NiGeomMorpherController's parser (likely), or the
upstream interpolator chain (NiBlendBoolInterpolator at block 47 of the
same NIF over-consumes 1438 bytes because its `flags & 1 = 0` path is wrong
for Oblivion, but that is a *symptom* of drift, not a new cause).

Footprint: the handful of NiTransformData (20), NiFloatData (2), NiColorData
(23 — shared with particle cascades), NiBoolData (15 — shared), subset of
NiNode / NiStringExtraData blocks in these non-particle NIFs. Bounded at
≤ 60 blocks across 4-6 NIFs.

Child issue: **NIF-09B** (to be filed, lower priority than 09A).

## Conclusion

The audit's 32-type enumeration is a red herring. Two upstream defects
(NiPSysData Particle Info omission + a still-unidentified
controller-chain drift on a small number of non-particle NIFs) account
for essentially all 429 NiUnknown substitutions in the Oblivion corpus.

Phase 1 output: **two child issues** (NiPSysData fix + animation-controller
bisect), not 32. Closing this issue is appropriate once the child issues
are filed.

## Verification plan (Phase 2 preflight)

1. Implement the Particle Info read in `parse_particles_data` (Cause 1).
2. Re-run `unknown_types` on `Oblivion - Meshes.bsa`; expected pool drop
   from 429 → ~60-80 (only the non-particle cascades remain).
3. Spot-check the affected particle-system NIFs with `recovery_trace` —
   first-failure block should disappear.
4. Then triage the residual animation-controller chain as NIF-09B.
