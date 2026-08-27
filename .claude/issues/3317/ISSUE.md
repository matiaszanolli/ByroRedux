# FNV-2026-08-26-D7-01

**Issue**: #3317
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: HIGH
**Dimension**: 7 — PHYSAL Ragdoll
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/nif/src/import/collision/shape.rs:154-164` (sibling cylinder arm `:167-176`),
consumed at `crates/physics/src/convert.rs:253-264`

**Premise verified**: current code at HEAD, reached by *both* the ragdoll path
(`extract_ragdoll` → `resolve_shape`) and the architecture path (`extract_from_classic` →
`resolve_shape`):

```rust
let p1 = finite_vec(havok_to_engine(s.point1[0], s.point1[1], s.point1[2]) * scale)?;
let p2 = finite_vec(havok_to_engine(s.point2[0], s.point2[1], s.point2[2]) * scale)?;
// p1/p2 are finite, so the derived half_height is finite too.
let half_height = (p2 - p1).length() * 0.5;
let radius = finite(s.radius1.max(s.radius2) * scale)?;
return Some(CollisionShape::Capsule { half_height, radius });
```

`CollisionShape::Capsule` (`crates/core/src/ecs/components/collision.rs:31-34`) carries only
`{half_height, radius}` — there is no field for the capsule's centre or axis — and
`convert.rs:259` builds it as `SharedShape::capsule_y(...)`, i.e. **always centred on the part
origin and always along +Y**. Both the midpoint `(p1+p2)/2` and the axis direction `p2-p1` are
lost at the NIF→canonical boundary and cannot be recovered downstream.

nif.xml is explicit that these are two real points, not an extent
(`nif.xml:3079-3086`, `bhkCapsuleShape`): `First Point` / `Second Point` = "First/Second point on
the capsule's axis" — they define placement *and* orientation in the shape's own (host-node) space.

**Evidence** — raw `bhkCapsuleShape` points from `meshes\characters\_male\skeleton.nif`
(havok_scale 7, engine units, one row per ragdoll bone):

```
bone=Bip01 R Thigh     p1=[29.604, 0.027, 0.367]  p2=[ 0.174, 0.027, 0.367]
     midpoint=[14.889, 0.027, 0.367]  |mid|=14.893  axis=(-1.000, 0.000, 0.000)  len=29.43  r=5.65
bone=Bip01 L UpperArm  p1=[19.687, 0.000, 0.000]  p2=[-0.096, 0.000, 0.000]
     midpoint=[ 9.795, 0.000, 0.000]  |mid|= 9.795  axis=(-1.000, 0.000, 0.000)  len=19.78  r=1.86
bone=Bip01 R Calf      p1=[27.557,-0.816,-0.603]  p2=[-0.532,-0.816,-0.603]
     midpoint=[13.512,-0.816,-0.603]  |mid|=13.550  axis=(-1.000,-0.000, 0.000)  len=28.09  r=4.17
bone=Bip01 Spine2      p1=[ 7.094,-0.638, 3.967]  p2=[ 7.094,-0.638,-3.964]
     midpoint=[ 7.094,-0.638, 0.001]  |mid|= 7.123  axis=( 0.000, 0.000,-1.000)  len= 7.93  r=10.18
```

Corpus-wide over all 20 746 FNV NIFs: **1363 `bhkCapsuleShape` blocks; 1321 (96.9 %) have a
midpoint further than 0.5 BU from the body origin (worst 512.07 BU); 1070 (78.5 %) have an axis
that is not engine ±Y**, across 111 files. (`bhkCylinderShape`: 0 instances in FNV, so its
identical collapse at `shape.rs:167-176` is FNV-inert but is the same latent defect.)

**Impact**: every FNV ragdoll limb capsule is built at the wrong place and the wrong orientation —
the R Thigh collider sits at the hip joint instead of half-way down the thigh (14.9 BU off) and
lies along Y (vertical) instead of along the bone's −X. The visible result on a shot NPC is limbs
whose collision volume does not follow the visible limb: corpses that sink into / stand off the
floor, that interpenetrate each other and world geometry, and joint pivots (which *are* decoded in
the correct authored space) fighting a collider whose centre of mass is at the joint rather than
mid-limb. It also corrupts `.mass(part_mass)`-derived inertia (a Y-capsule's inertia tensor is not
the X-capsule's). This is the reference title's dominant ragdoll primitive: 18 of 18 `_male` bodies
are capsules. The same loss applies to FNV static/clutter colliders built from `bhkCapsuleShape`.

**Fix sketch**: give `CollisionShape::Capsule` (and `Cylinder`) the authored `a`/`b` endpoints (or
keep `{half_height, radius}` and emit `CollisionShape::Compound { children: [(midpoint,
rot_from_y_to_axis, Capsule)] }` from `resolve_shape`, which needs no core type change and no
`convert.rs` change). Add a fixture pinning a −X-axis, offset capsule so the round-trip is
regression-guarded.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
