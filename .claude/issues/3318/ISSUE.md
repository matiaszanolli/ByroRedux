# FNV-2026-08-26-D7-02

**Issue**: #3318
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: HIGH
**Dimension**: 7 — PHYSAL Ragdoll
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/ragdoll.rs:135-143` (vs. the contradictory sibling reading at
`crates/nif/src/import/collision/mod.rs:378-386`)

**Premise verified**: current code at HEAD. `template_from_imported` treats an `is_t` body's CInfo
transform as a **skeleton-root/rest-space world pose** and subtracts the bone rest pose out of it:

```rust
let (local_translation, local_rotation) = if b.is_t {
    let inverse_bone_rotation = rest.rotation.inverse();
    (
        inverse_bone_rotation * (b.translation - rest.translation) / rest.scale,
        inverse_bone_rotation * b.rotation,
    )
} else {
    (Vec3::ZERO, Quat::IDENTITY)
};
```

The *other* consumer of the identical `BhkRigidBody::{is_t, translation, rotation}` fields —
`extract_from_classic`, the architecture-collider path, #2316 — reads them as a **node-local shape
transform**:

```rust
let has_offset = body.is_t && offset_is_finite && (...);
if has_offset {
    shape = CollisionShape::Compound {
        children: vec![(body_translation, body_rotation, Box::new(shape))],
    };
}
```

Two contradictory interpretations of the same field. The FNV data says the node-local one is right:

1. The shape geometry underneath a T body is unambiguously **node-local** — see D7-01's capsule
   dump: `Bip01 R Thigh`'s capsule runs `(29.6, 0, 0) → (0.17, 0, 0)`, i.e. from the bone origin
   *down the bone*, not around a world position. A transform applied on top of a node-local shape
   is node-local.
2. Under the root-space reading, the resulting "bone-local offset" magnitude comes out ≈ the bone's
   own distance from the skeleton root — a physically impossible local offset for a limb.
3. `#2336` (which introduced the subtraction) was diagnosed on **non-T** bodies, which `#2447` has
   since gated out of this branch entirely. The subtraction now has *only* T bodies flowing
   through it, i.e. the exact case it was never validated against.

**Evidence** — census over all 220 FNV ragdoll NIFs. `cinfo_t` is the authored value;
`root-space-local` is what `template_from_imported` produces today; `|rest_t|` is the bone's
distance from the skeleton root:

```
FNV ragdoll bodies: 1815 total, 351 bhkRigidBodyT, in 160 files
T bodies with |root-space local| > 1 BU: 351/351;  > 10 BU: 297/351

 378.761 creatures\robobrain\skeleton.nif :: 'Bip01 Head'
         cinfo_t=(17.650, 0.927, 3.908) |18.101|  ->  local=(-378.305,-17.650, 5.780)   |rest_t|=379.256
 318.151 creatures\robobrain\skeleton.nif :: 'Bip01 Neck1'
         cinfo_t=(-6.117, 0.000, 0.000) | 6.117|  ->  local=(-318.092,  6.117,-0.031)   |rest_t|=318.092
 196.883 creatures\deathclaw\skeleton.nif :: 'Bip01 Brain'
         cinfo_t=( 4.473, 0.000, 5.501) | 7.090|  ->  local=(-192.948,  4.473,-38.910)  |rest_t|=194.305
 132.453 creatures\minisentryturret\skeleton.nif :: 'Bip01 NonAccum'
         cinfo_t=( 4.001, 0.000, 0.000) | 4.001|  ->  local=(-132.393,  4.001, 0.000)   |rest_t|=132.393
  90.631 creatures\protectron\skeleton.nif :: 'Bip01 Spine Brain'
         cinfo_t=( 0.000, 0.116, 0.000) | 0.116|  ->  local=(   0.005,-88.334,-20.276)  |rest_t|=90.744
```

The pattern is exact: `|root-space-local| ≈ |rest_t|` while `|cinfo_t|` stays small (0.1–26 BU,
the scale of a genuine local shape offset). The single counter-example proves the rule —
`clutter\questitems\mq04chandellier01.nif :: 'MQ04Chandellier01 NonAccum'` has `|rest_t| = 0.000`
(host node at the origin), so the two readings coincide and `local == cinfo_t == (1.41, 343.95, 5.34)`,
a chandelier genuinely hanging 344 units above its node.

Cross-check that the **non**-T path is fine: on `_male\skeleton.nif` all 18 bodies are non-T and
their (ignored) `cinfo_t` matches the bone's world rest pose to within **0.12 BU** max — so `#2447`'s
zero-offset fallback is correct to ~1.7 mm at FNV scale. `dog\skeleton.nif` shows the split inside
one file: 18 non-T bodies at |Δ| ≤ 0.010 BU, and 3 T bodies (`Bip01 Spine0/Spine/Spine2`) at
|Δ| = 50–56 BU with a 120° rotation error.

**Impact**: 351 of 1815 FNV ragdoll bodies (19.3 %), in 160 of 220 ragdoll files, are seeded by
`activate_ragdoll` (`ragdoll.rs:298-299`, `body = bone_global ∘ local`) tens-to-hundreds of units
away from the bone they belong to, with a rotation error up to 168°. Affected content is
FNV's whole creature roster — dog, deathclaw, radscorpion/roboscorpion, robobrain, protectron,
sentry bot, Mister Gutsy, securitron, cazador, gecko, centaur, spore plant, queen ant — plus
several `*_go.nif` armour ragdolls. Rapier's multibody forward kinematics resolves the joint pivots
(which *are* scaled and placed correctly) on the very first step, snapping the limb back tens of
BU; the writeback inverse then carries that displacement onto the bone `GlobalTransform` and into
the GPU bone palette. Symptom: creature corpses that explode, stretch, or fling a limb across the
room on death. The humanoid `_male` reference path is unaffected (all non-T), which is exactly why
it has never surfaced in the existing gates.

**Fix sketch**: in `template_from_imported`, for `b.is_t` use `(b.translation, b.rotation)`
**verbatim** as the bone-local offset — the same reading `extract_from_classic` uses — and delete
the rest-pose subtraction (it has no remaining correct consumer once non-T is gated off). Re-derive
`ragdoll::tests::imported_root_space_body_pose_converts_to_bone_local_once` accordingly, and add a
real-data pin asserting no FNV ragdoll body's local offset exceeds e.g. 40 BU.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
