# FNV-2026-08-26-D7-03

**Issue**: #3330
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 7 — PHYSAL Ragdoll
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/nif/src/import/collision/ragdoll.rs:204-215` (the `BhkConstraintData::Other` arm)
and `:142-155` (the `BhkBreakableConstraint` arm)

**Premise verified**: both drop sites are present and *loud* at HEAD (the #1539 / #1850 warnings are
intact — see "Regression guards verified"). What is **not** tracked anywhere open is that the
un-decoded edge actually severs the articulation on real FNV content. `BhkConstraint::parse`
(`crates/nif/src/blocks/collision/constraints.rs`) decodes only `bhkRagdollConstraint`,
`bhkLimitedHingeConstraint`, and the malleable wrapper's inner 7/2; `bhkHingeConstraint`,
`bhkPrismaticConstraint` and `bhkStiffSpringConstraint` reach `extract_ragdoll` as
`BhkConstraintData::Other` and are dropped.

**Evidence** — corpus scan flagged exactly 3 files that lose authored edges; union-find over the
surfaced joint graph:

```
creatures\protectron\skeleton.nif        12 authored -> 9 surfaced   (2x bhkPrismatic + 1x breakable)
  connected components: 4
    ["Bip01 NonAccum","Bip01 L Thigh","Bip01 L Calf","Bip01 R Thigh","Bip01 R Calf",
     "Bip01 Spine","Bip01 L UpperArm","Bip01 L Forearm","Bip01 R UpperArm","Bip01 R Forearm"]
    ["Bip01 Head"]          <- severed
    ["Bip01 Head Dome"]     <- severed
    ["Bip01 Spine Brain"]   <- severed
creatures\sentryturret\skeleton.nif       3 authored -> 2 surfaced   (1x bhkHingeConstraint)
  connected components: 2   ["Bip01 NonAccum"] | ["Bip01 Yaw","Bip01 Pitch","Bip01 Brain"]
creatures\minisentryturret\skeleton.nif   3 authored -> 2 surfaced   (1x bhkHingeConstraint)
  connected components: 2   ["Bip01 NonAccum"] | ["Bip01 Yaw","Bip01 Pitch","Bip01 Brain"]
```

Every other FNV ragdoll (217 / 220, incl. `_male`) is a single connected component.

**Impact**: exactly the failure the #1539 warning text predicts, now confirmed on shipped content.
A destroyed Protectron's head, head dome and spine-brain each become an independent free-falling
multibody; a destroyed Sentry Turret's 1000-mass base separates from its yaw/pitch/brain assembly.
Small blast radius (3 creature skeletons), but it is a hard visual break, not a fidelity nuance.

**Fix sketch**: `bhkHingeConstraint` is a `bhkLimitedHingeConstraint` without the angle limits —
decode it into `LimitedHingeCInfo` (nif.xml `bhkHingeConstraintCInfo`, FO3+ 8×Vec4 = 128 B, the
size `BhkBreakableConstraint::wrapped_payload_size(1, false)` already knows) and surface it as
`ImportedJointKind::LimitedHinge { min_angle: -PI, max_angle: PI }` — the "long-term" note already
written in the `Other` arm's comment. `bhkPrismatic` needs a canonical prismatic joint kind and is
a larger change; the breakable-wrapped edge needs the wrapped CInfo retained at parse (#1850's own
note).

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
