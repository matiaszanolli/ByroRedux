# PHYSAL — Physics Abstraction Layer

**PHYSAL** (Physics Abstraction Layer; pronounced "FIZZ-al") is the canonical
translation tier for **simulated physics** — the articulated, dynamic bodies the
engine steps every frame: ragdolls today, then loose rigid bodies, phantoms /
trigger volumes, runtime constraints, and active (motorised) ragdolls. It is the
sibling of [`nifal.md`](nifal.md) and [`exal.md`](exal.md): where NIFAL
translates per-game **NIF geometry/material** data and EXAL translates per-game
**ESM environment** data, PHYSAL translates per-game **Havok physics** data into
one canonical articulated-physics representation that a solver builds and steps
identically for every game.

"Abstraction" is the brand; the mechanism is **canonical translation** (per-game
`Imported*` → one resolved, game-agnostic spec). The verbs stay `translate` /
`canonical` / `resolve` / `build`; **PHYSAL** names the layer as a whole.

**Status**: ACTIVE (opened 2026-06-14). The ragdoll slice is the reference
realisation — see §3. Generalises the M41.x FNV ragdoll work into a cross-game
layer; Oblivion decode landed alongside this doc.

**Goal**: every supported engine version (Oblivion / FO3 / FNV / Skyrim LE/SE /
FO4 / FO76 / Starfield) translates its native, per-game Havok authoring into
**one canonical, fully-resolved physics spec** through a single explicit
parse→canonical boundary, which **one** solver boundary then builds. The
simulation, writeback, and gameplay layers consume the canonical spec
**identically for every game** — no per-game branches downstream, no `Option`
"resolve-it-later" fallbacks. We are not emulating Havok; we are using its
authored data as a *baseline* and simulating it on our own solver to **fix what
the original engines never did right** (link separation, jitter, clunky limits).

This is the same doctrine NIFAL formalises
([`feedback_format_translation.md`](../../) — "never per-game branches downstream;
translate at the parser boundary"; the `format_abstraction.md` GameVariant
pattern), now applied to the physics pipeline.

---

## 1. PHYSAL is double-ended

NIFAL and EXAL abstract **one** axis: the source game. PHYSAL abstracts **two** —
because physics has a sink (the solver) as well as a source (the game):

```
        SOURCE axis (per game)                            SINK axis (per solver)
  ┌──────────────────────────────────┐          ┌──────────────────────────────┐
  Havok in any game's NIF ─parse─▶ Imported ─translate─▶ Canonical spec ─build─▶ Rapier
       (Oblivion order,              (engine-      (glam-native,        (RigidBody +
        FO3+ order, FO4 blob)         native graph) solver-agnostic)     multibody joints)
```

- **Source boundary** (parse + extract + translate): folds every per-game Havok
  quirk — constraint field order, `havok_scale`, collision-object kind — into one
  canonical spec. This is where Oblivion vs FO3+ vs FO4 differences live, and the
  *only* place they live.
- **Sink boundary** (`build`): the single site that lowers the canonical spec onto
  the backing solver. Swapping Rapier for another solver reimplements this one
  function and the pose readback — nothing upstream of it changes.

The payoff of the sink boundary is concrete: the canonical `RagdollSpec` /
`RagdollJointSpec` are plain glam types with no Rapier in their signatures, so the
"what to simulate" description is independent of "how it's simulated."

---

## 2. The tier model

```
                parse + extract                 translate()                    build()
  NIF bytes ────────────────────▶  Imported*  ──────────────▶  Canonical  ──────────────▶  Solver
            (per-game bhk* wire              (engine-native,    (entity-resolved          (Rapier rigid
             decode → Y-up, scaled           game-agnostic      ECS spec, no              bodies +
             articulation graph)             articulation)      Option leaks)             multibody joints)
```

| Tier | What it is | Where it lives | Rule |
|---|---|---|---|
| **Raw / `Imported*`** | A faithful decode of the Havok articulation — the constraint graph, per-body mass/damping/shape, joint geometry — converted to engine units (Y-up, `havok_scale`). May carry era-specific quirks. **Allowed to be messy.** | `crates/nif/src/blocks/collision/` (wire) + `crates/nif/src/import/collision/ragdoll.rs` (`ImportedRagdoll`) | Decode only; never the engine's source of truth. |
| **`translate()` boundary** | Resolves `ImportedRagdoll` against the live skeleton (bone names → `EntityId`) into the canonical ECS blueprint, then seeds a world-space build spec from the bones' current poses at activation. Exactly **one** site per concern. | `byroredux/src/ragdoll.rs` (`template_from_imported`, `activate_ragdoll`) | One producer; no duplicate construction. |
| **Canonical** | The game- and solver-agnostic spec the engine reasons about. `RagdollSpec`/`RagdollJointSpec` (build input) + the `Ragdoll` / `RagdollTemplate` ECS components (runtime state + dormant blueprint). | `crates/physics/src/ragdoll.rs` + `byroredux/src/ragdoll.rs` | The single source of truth. |
| **Solver build** | Lowers the canonical spec onto Rapier: dynamic bodies, colliders (mass split), constraint graph oriented into a kinematic tree, one multibody joint per edge. | `crates/physics/src/ragdoll.rs::build_ragdoll` | One solver boundary; the only place solver types appear. |

### The canonical-type rule (inherited from NIFAL)

> **Where an ECS component already serves the game- and solver-agnostic,
> engine-facing role, that component IS the canonical type.** Introduce a new
> canonical type only where none exists.

The runtime `Ragdoll` component (body handles + joint handles) and the dormant
`RagdollTemplate` (bone-relative blueprint) are the canonical types — there is no
parallel `Canonical*` struct they copy from. `RagdollSpec` is not a third type but
the **transient build argument** to the sink boundary: it exists only between
`activate_ragdoll` and `build_ragdoll`, carrying the world-space seed the dormant
template can't (the template is pose-independent; the spec is pose-resolved at the
instant of activation).

### Relationship to NIFAL

PHYSAL **consumes** NIFAL's canonical collision output: ragdoll body geometry is
NIFAL's `CollisionShape` (the `bhk*Shape` → `CollisionShape` resolution in
`import/collision/shape.rs::resolve_shape`, NIFAL's "collision" category). PHYSAL does
not re-decode shapes — it reuses the canonical ones and adds the *articulation*
(graph + joints + simulation) NIFAL stops short of. Clean dependency, zero
duplication.

`CollisionShape` geometry remains in canonical local units. Placement or bone
scale travels separately on `GlobalTransform` / `RagdollBodySpec` and the shared
`collision_shape_to_parts` converter applies it exactly once at the Rapier sink.
Producers must not bake that scale into vertices or primitive dimensions; doing
both produces scale² geometry while ragdoll articulation remains scale¹ (#3064,
#3065).

---

## 3. Ragdolls — the reference realisation

### Source boundary — per-game constraint decode

> **Not the only seam.** §1 enumerates three source-boundary seams and all three
> are live in this pipeline: the constraint decode described below, `havok_scale_for`
> (`crates/nif/src/lib.rs` — the compatibility table below depends on it for the
> Skyrim+ ×69.99 row), and collision-object-kind dispatch. All three sit at the
> parse→canonical boundary, which is what the doctrine requires; an earlier
> wording here claimed "the whole per-game seam" was the constraint decode alone,
> which contradicted §1 and the table twenty lines below it (#2883).
>
> `humanoid_skeleton_path(GameKind)` (`byroredux/src/npc_spawn.rs`) game-branches
> *which* skeleton asset feeds this pipeline. That is asset resolution, not
> physics translation — no physics semantics, not a fourth seam.

The per-game seam **in the constraint graph** is the typed decode of the
constraint CInfos (`crates/nif/src/blocks/collision/constraints.rs`). At HEAD that
is **seven wire types → six CInfo structs → three importer functions**, reachable
through **three wrappers** (bare, `bhkMalleableConstraint`-wrapped, and
`BhkBreakableConstraint`-wrapped):

| Wire type | CInfo struct | Importer | Landed |
|---|---|---|---|
| `bhkRagdollConstraint` | `RagdollCInfo` | `ragdoll_joint` | M41.x slice 1 |
| `bhkLimitedHingeConstraint` | `LimitedHingeCInfo` | `limited_hinge_joint` | M41.x slice 1 |
| `bhkHingeConstraint` | `LimitedHingeCInfo` (synthesized ±π limits, zero friction) | `limited_hinge_joint` | #3330 |
| `bhkPrismaticConstraint` | `PrismaticCInfo` | `prismatic_joint` | #3792 |
| `bhkBallAndSocketConstraint` | `BallAndSocketCInfo` | *none yet* | #4212 |
| `bhkStiffSpringConstraint` | `StiffSpringCInfo` | *none yet* | #4212 |
| `bhkBallSocketConstraintChain` | `BallSocketChainCInfo` | *none yet* | #4212 |

> Two counts, not one — a hinge is decoded into the *LimitedHinge* CInfo because
> an unlimited hinge is a limited one with the limits opened to ±π.
>
> **Decoded ≠ imported.** The last three rows are parsed and retained but have no
> importer: `extract_ragdoll` declines them exactly as it declines `Other`, because
> no canonical joint kind exists yet for a point-to-point or distance joint. They
> were decoded anyway because the alternative was a suppressed under-read —
> nif.xml gives all three a fixed size with no motor field, so each now consumes
> its whole body and its stream drift is 0. That is why #4212 also removed them
> from `is_havok_constraint_stub`: with nothing left unread, suppressing their
> drift telemetry would hide real parser drift rather than a by-design tail. The
> one type left in that list, `bhkGenericConstraint` (nif.xml carries a name and
> no field spec), has no dispatch arm and always lands as `NiUnknown` with an exact
> `block_size` skip, so #4626 deleted the list and its `stubbed_drift_histogram`
> altogether.
>
> `bhkMalleableConstraint` with an inner type outside this table still reads its
> base, skips its fixed payload, and stays `BhkConstraintData::Other`.

Each importer reads the **common subset** of fields — twist/plane/pivot + angle
limits for ragdoll; axis/perp/pivot + limits for hinge (the LimitedHinge perp-axis
zero-reference threads through to the canonical spec as of #2448 / PHYS-02);
sliding/rotation/pivot + min/max distance for prismatic — so only genuinely
era-only or unmodelled fields are decoded-and-discarded: the FO3+ **motors**, and
the friction scalars (`LimitedHingeCInfo::max_friction`, `PrismaticCInfo::friction`)
which are **captured but unused** by the importers today. `Perp Axis In B1` is
**not authored** in the Oblivion/Morrowind layout; the parser derives it as
`Perp B2 × Axis B` — nif.xml's own stated identity (`Perp Axis In B2` = `Axis B ×
Perp Axis In B1`, expressed in B's space), verified against FNV's authored B1
values to ~1e-7 (#5537). The pre-#5537 claim that zeroing it "left behavior on
that era unchanged" was false: the solver boundary's `frame_rot` fell back to a
synthesized perpendicular, an arbitrary zero-reference that put Oblivion bind
poses 1.5–1.8 rad outside the limits and tore the rig apart on its first solve.
`Axis A` is likewise **not serialized** for an Oblivion-era
hinge; the parser derives it as `Perp A1 × Perp A2`, nif.xml's own stated identity,
rather than guessing. One `RagdollCInfo` / `LimitedHingeCInfo` / `PrismaticCInfo`
therefore feeds every game:

| Game | Discriminator | Constraint layout | State |
|---|---|---|---|
| **Oblivion / Morrowind** | NIF ≤ 20.0.0.5 (`#NI_BS_LTE_16#`) | Ragdoll 6×Vec4 + 6×f32 (pivots-first, **no motors**); LimitedHinge 7×Vec4 + 3×f32 (**no Perp B1**); Hinge 5×Vec4 = 80 B (**no Axis A** — derived); Prismatic 8×Vec4 + 3×f32 = 140 B, pivots-first order, **no motor** | **decoded (2026-06-14; Hinge #3330, Prismatic #3792)** |
| **FO3 / FNV** | NIF 20.2.0.7, bsver ≤ 34 (`!#NI_BS_LTE_16#`) | Ragdoll 8×Vec4 + 6×f32 + motor; LimitedHinge 8×Vec4 + 3×f32 + motor; Hinge 8×Vec4 = 128 B + motor (no serialized limits); Prismatic 8×Vec4 + 3×f32 = 140 B + motor, `Sliding/Rotation/Plane/Pivot` A-then-B order (**not** the Oblivion order) | **decoded — slice 1 reference** |
| **Skyrim LE/SE** | NIF 20.2.0.7, bsver 83–127 | identical FO3+ layout; gated by NIF **version**, not bsver. `havok_scale` ×69.99 applied at import via `havok_scale_for(header)` | **decoded; version-gate pinned by test, real-data validation pending** |
| **FO4 / FO76 / Starfield** | `BhkNPCollisionObject` → `BhkSystemBinary` | Havok-serialised binary blob — the constraint graph is inside the blob, not as discrete `bhkRigidBody.constraints` | **container + object table decoded (#3809, `parse_havok_packfile`); FO4 skeleton ragdolls (`bhkRagdollSystem`) decoded and threaded (#5015, `decode_np_ragdoll`); precombined geometry blocked on `hknpCompressedMeshShapeData`'s field layout. Documented limitation, not a leak** |

Sources of truth (no-guessing): `/mnt/data/src/reference/nifxml/nif.xml`
(`bhkRagdollConstraintCInfo` / `bhkLimitedHingeConstraintCInfo` /
`bhkHingeConstraintCInfo` / `bhkPrismaticConstraintCInfo`, both version branches)
cross-checked against the sibling `BhkBreakableConstraint` byte tables in the same
file. Every decoder asserts exact stream advancement (byte-level tests in
`blocks/collision/bhk_constraint_tests.rs`).

> This paragraph has drifted twice now — #2883 corrected the seam *count*, #3970
> the *type* count after #3330 and #3792 landed without touching this file. If you
> add a decode arm, the table above and the `constraints.rs` docstrings are part of
> the same edit; `physal_seam_doc_tests` in that file fails the build if they are not.

### Extract — articulation graph (`crates/nif/src/import/collision/ragdoll.rs`)

`extract_ragdoll` is **already game-agnostic**: it walks `BhkRigidBody` blocks,
maps each to its host bone (`NiNode.collision_ref → BhkCollisionObject.body_ref →
host NiNode name`), resolves the shape via NIFAL's `resolve_shape`, and remaps
constraint entity refs (rigid-body block indices) to body-array indices. It
switches on `BhkConstraintData`, never on game. Output: `ImportedRagdoll { bodies,
constraints }` in Y-up, `havok_scale`-applied engine units. Requires ≥2 bodies +
≥1 joint or returns `None` (a lone collider isn't a ragdoll).

### Translate — bone resolution + activation (`byroredux/src/ragdoll.rs`)

At spawn, `template_from_imported` resolves bone names against the loaded
skeleton's `name→EntityId` map into a `RagdollTemplate` ECS component (bodies whose
bone fails to resolve are dropped, constraint indices remapped). On the
`ragdoll <id>` console trigger, `activate_ragdoll` reads each bone's current
`GlobalTransform`, seeds a world-space `RagdollSpec`, and calls the sink.

Production activation comes from `combat::reconcile_dead_actor`, reached for
live deaths, actors authored `Starts Dead`, and the save-load reconciliation
sweep; `ragdoll <id>` remains the manual debug path. Before activation, a
transient `AuthoredRagdollPose` may apply the placed actor's `XRGD` pose so the
corpse starts from its authored pose rather than the spawn pose. This mapping
is skeleton-keyed, not a `GameKind` branch: entry *k* maps to body *k* in the
18-body humanoid set, rotations use the empirically derived Gamebryo XYZ
product with negated angles, and entry zero places the COM at the reference.
The pose is consumed once and is not saved. Unknown skeletons, missing or
invalid entries, and other pose refusals leave the skeleton unchanged; normal
ragdoll activation then uses its current pose and can still collapse from the
spawn pose. See `byroredux/src/ragdoll_pose.rs` for the evidence and pinned
bone-name order.

### Build + step + writeback — solver (`crates/physics`)

`build_ragdoll` creates one dynamic body + collider per spec body (mass split
across compound parts), orients the constraint graph into a forest-safe kinematic
tree (BFS, loop back-edges dropped), and inserts a Rapier **multibody** joint per
edge — reduced-coordinate, constraint-by-construction, so links cannot visibly
separate under stress (the headline "clunkiness" of the original Havok ragdolls).
`ragdoll_writeback_system` (Stage::Late, after physics + propagation) copies the
stepped body poses onto the bone `GlobalTransform`s the skinned mesh already
reads — so the mesh crumples with no renderer change.

### Known approximation

Havok's cone + two-plane angular limit model does not map 1:1 onto Rapier's
per-axis angular limits. Slice 1 applies twist→twist-axis and cone→both swing
axes — good enough to switch an actor from bind-pose to a plausible ragdoll.
Limit fidelity has since been refined (#5536: the authored range applies to
the Havok twist convention — negate+swap on the non-flip edge, pass-through on
the flip edge — instead of the mirrored pre-fix branches), and the limits are
soft springs rather than hard clamps (#5538, below). Motors stay captured but
unused.

---

## 4. "Better than Havok" levers

Centralised in `crates/physics/src/config.rs::ContactConfig`, so the
"reimplement it properly" knobs are one resource, not scattered constants:

- **Multibody joints** (not impulse joints) — the structural win: no inter-link
  separation, which is most of what makes the original ragdolls read as clunky.
- `ragdoll_extra_angular_damping` — extra per-body angular damping on top of the
  authored value (inert at the `0.0` default); the biggest "less floppy than
  Havok" dial. Rapier skips `RigidBody` damping for multibody links (#5540), so
  both damping terms ride the user force/torque channel; the angular one is
  `-c·I_world·ω` (`PhysicsWorld::angular_damping_torque`, #5697), which decays
  spin at exactly the authored rate `c`. It was `-c·m·ω` first — about 60× too
  weak on a bone-sized capsule — on the mistaken premise that rapier exposes no
  inertia.
- **Soft joint limits** (#5538, done): `RAGDOLL_JOINT_SOFTNESS` is a 30 Hz,
  ζ = 1 spring on the locked DOF and the limits (rapier's 1e6 Hz default
  corrects a bind pose that sits a fraction of a radian outside its range in a
  single substep — 617 BU/s at tick 0 on FO3 — and tore the rig apart).
- **Solver iteration count** (#5546, deliberately *not* raised):
  `RAGDOLL_EXTRA_SOLVER_ITERATIONS = 0`. "Ragdolls want more than clutter" was
  the old premise; with soft limits +0 / +4 / +12 all stay bounded on the FO3
  gate, and any nonzero value is a whole-world cost (≈ 2× step cost with 500
  unrelated awake boxes, `a_ragdoll_does_not_change_an_unrelated_bodys_solve`).
  Do not re-raise it to chase limit stability.
- Motors stay **off** for the ragdoll slice (the `Motor A/B` + `bhkConstraintMotor`
  data is captured at parse for a future active-ragdoll slice).

---

## 5. Per-concern inventory

How close each physics concern is to the canonical contract.

### Ragdoll articulation — **converged for the classic-chain games (2026-06-14)**

Oblivion / FO3 / FNV / Skyrim all funnel through one `ImportedRagdoll` → one
`RagdollSpec` → one Rapier multibody. The reference realisation (§3).

### Loose rigid bodies / static collision — **owned by NIFAL + the physics runtime**

`CollisionShape` / `RigidBodyData` → `physics_sync_system` → Rapier is the existing
M28 path ([`physics.md`](physics.md)). PHYSAL does not re-abstract it; it reuses the
canonical shapes. If a future slice needs a single dynamic-rigid-body spec, it joins
PHYSAL under the same source/sink boundaries.

As of 2026-08-07, the NIF source boundary also retains a
`CollisionAuthoringSummary`. FO4+/FO76/Starfield packed authoring selects a
single placement-following AABB proxy for clutter/actors, while architecture
keeps its precise per-mesh static-triangle fallback. This is an explicit
compatibility approximation at the NIFAL→runtime boundary, not a second
physics abstraction or a claim that the packed body was decoded.

### Phantoms / trigger volumes — **parked (needs a `TriggerVolume` ECS path)**

`BhkPCollisionObject` phantoms are parsed but want a trigger-volume consumer, not a
rigid body — deferred, mirroring the NIFAL collision note. FO3 DLC
`bhkSPCollisionObject` now dispatches through the same phantom wrapper instead
of being misclassified as classic rigid-body authoring.

### FO4+ packed Havok — **container decoded; one payload class remains**

`BhkNPCollisionObject → BhkSystemBinary` holds the whole physics system
(bodies + constraints) as a serialised Havok binary. The **container** is no
longer opaque: `blocks::collision::parse_havok_packfile` decodes the classic
packfile header, section table, class-name table and — since #3809 — all three
fixup tables, which makes `HavokPackfile::objects()` return every top-level
object in `__data__` with its exact offset and runtime class name.

Validated over the whole FO4 precombine corpus (4,484 `_physics.nif` blobs in
`Fallout4 - MeshesExtra.ba2`), where six independent cross-checks hold at
100 %: parse success, last section's `absolute_end()` landing exactly on the
blob length, every virtual fixup resolving to a declared class, objects in
ascending offset order, every global fixup naming a real section, every local
fixup landing inside its section. Every blob carries the same five objects in
the same order — `hknpPhysicsSystemData`, `hknpCompressedMeshShape`,
`hkRefCountedProperties`, `hknpBSMaterialProperties`,
`hknpCompressedMeshShapeData`.

What remains is **not** "decode a Havok binary format" but the field layout of
those named classes, and in practice one of them: `hknpCompressedMeshShapeData`
carries the bit-packed mesh. `__types__` is empty in every blob, so the file
ships no reflection metadata to mine — the layouts have to come from corpus
inference. The local-fixup table does give the array-member pointers inside
each object, so that inference now starts from known object bounds and known
array locations rather than from an undifferentiated byte run.

**FO4 ragdolls (`bhkRagdollSystem`) — decoded and threaded (#5015).** A
skeleton's articulation is a different blob from the precombine one and does
not need the compressed-mesh layout: `hknpRagdollData` (bodies, joints),
`hknpCapsuleShape` / `hknpSphereShape` / `hknpConvexPolytopeShape` /
`hknpDynamicCompoundShape` bodies, and `hkpRagdollConstraintData` /
`hkpLimitedHingeConstraintData` joints (also behind an
`hknpBreakableConstraintData` wrapper).
`blocks::collision::decode_np_ragdoll` reads them into the same
`RagdollCInfo` / `LimitedHingeCInfo` records the classic `bhkConstraint` path
produces, so the existing joint conversion serves both generations, and
`extract_ragdoll` takes that path when a scene authors no discrete
constraints. Every offset was recovered from the vanilla corpus (no SDK is
consulted) and is pinned by an independent check: body mass equals capsule
volume times the density stored beside the inverse mass (18 of 18 human
bodies, 1e-7); a joint's pivot names the same world point from both bodies
(783 of 850 corpus joints to 1 cm, every one of the human, supermutant,
feral-ghoul, brahmin, mole-rat, dog, radscorpion, yaoguai, mirelurk-king and
radstag joints; the exceptions are turrets, the vertibird, Liberty Prime's
furniture and a few robot parts, whose joints meet at a rest pose other than
the stored bind pose); and a body's bind pose equals its host bone's rest
world transform for 59 of 75 skeletons (the rest get a bone-local offset).
Bodies are named by the skeleton's `bhkNPCollisionObject.body_id`. All 75
vanilla ragdoll blobs decode, every one of the 986 bodies has a shape (922
capsules and spheres, 58 convex polytopes, 6 dynamic compounds) and every one
of the 850 joints decodes (#6067).

Shapes added by #6067, each pinned by an independent check. A polytope keeps
its vertices (`u16` count and relative offset at `+48`, 16-byte rows) and the
convex radius at `+20`; the face planes and index lists after them are
redundant. Its layout is pinned by the 8-vertex boxes, whose Steiner volume
(the box grown by the radius, edges and corners rounded) times the density
reproduces the authored mass to 2% on every box — the mirelurk hunter's 3 cm
box instead authors the box grown as a cuboid, `(a+2r)(b+2r)(c+2r)`. The
organic hulls (20–252 vertices) have no closed-form volume and are not pinned.
A compound keeps its instances (`hkArray` at `+96`, 128-byte rows: rotation
columns at `+0/+16/+32`, translation `+48`, scale `+64`, child pointer `+80`)
with leaf children only; the rotation is stored as **columns** (a child point
`v` sits at `columns·v + translation`), which the compound data object's
stored child bounds settle (the turret workshop's two rotated children match to
the convex radius under columns and miss by 0.1–0.2 under rows). Compound
masses are not volume-checked. The convex radius is dropped on import, as is a
breakable joint's threshold: a rapier hull is sharp and the joint stays
unbreakable.

**Joint pivots on off-bone bodies (open, #6067).** 67 of the 850 joints name
two different world points at the stored bind pose (783 agree). 39 of the 67
agree when a body that sits off its host bone (`is_t`) takes its pivot in the
**bone's** frame rather than its own — the deathclaw's four disagreeing joints
all hang off its offset COM body and share one displacement vector whose length
is that body's bone offset (0.2044 m) — but the same reading makes some of
the other joints worse (vertibird, Liberty Prime's furniture), so it is not one
rule. The 28 left after it: Liberty Prime's furniture (13), vertibird (3),
power armor (6, three files), eyebot (2), createabot (2), mirelurk queen (1),
bloatfly (1). The importer still reads every pivot in the body's own frame; the
reading above is evidence, not wired behaviour (no live ragdoll check exists
for FO4, #6070).

**Friction, restitution and damping (#6066).** Each body takes them from the
blob's material row (`hknpRagdollData+16`, 80 B) and motion-property row
(`+32`, 64 B), both indexed by the body's *motion* index (`+12` of its row).
The material holds three Havok halves (the upper 16 bits of an `f32`) at
`+18/+20/+22`: dynamic friction, static friction, restitution; the motion
property holds linear then angular damping as whole `f32`s at `+24/+28`.
Pinned by varying evidence over the 986 vanilla bodies: the two friction halves
are equal on every body; the body a skeleton leaves at the authoring defaults
(88 in the corpus) reads exactly the Skyrim `bhkRigidBody` defaults — friction
0.3, restitution 0.8 — and 84 of them carry damping 0.1 / 0.05 in the linear,
angular order, none the reverse; and left/right mirror bones carry identical
rows in 570 of 574 pairs under the motion index (514 of 566 when the same
rows are read by body index, a probe over all decoded bodies). The human rig is friction 0.6, restitution 0.2, damping 0.4 / 0.6 — not
the Skyrim humanoid's 0.3 / 0.8 / 0.1 / 0.05 the importer used before; those
remain the fallback for a blob with no readable rows. The upper half of the
material row's third word (`0.0498` on every body) is not identified and not
read.

### Active (motorised) ragdoll — **data captured, simulation deferred**

Motor CInfo is parsed and skipped (slice 1). A future slice drives motors for
get-up / hit-react / partial-ragdoll. Needs the joint-limit fidelity work first.

---

## 6. Rollout order

1. ~~Ragdoll source boundary — FNV~~ — done (M41.x slice 1).
2. ~~Ragdoll source boundary — Oblivion + Skyrim~~ — done (2026-06-14): Oblivion
   `#NI_BS_LTE_16#` decode landed; Skyrim version gate pinned by test (rides the
   FO3+ path, ×69.99 scale). FO3 already rode the FNV path.
3. Joint-limit fidelity — map Havok cone + 2-plane onto Rapier limits more
   faithfully; add soft limits (`ContactConfig`).
4. Death / hit-react AI triggers — replace the console trigger with gameplay
   (the activation path is already trigger-agnostic).
5. Active ragdoll — drive the captured motors.
6. FO4+ `BhkSystemBinary` payload decode — the container and its object table
   are done (#3809) and the skeleton ragdoll payload (`bhkRagdollSystem`,
   #5015) is decoded and threaded; what is left is
   `hknpCompressedMeshShapeData`'s field layout (precombined geometry), the
   joint-pivot frame on off-bone bodies and the undecoded material/motion rows.

Each step ships independently behind `cargo test`; none touches the Vulkan
render-pass / pipeline (writeback rides existing skinning).

---

## 7. Tooling

- `crates/nif/tests/ragdoll_import.rs` — real-data (`#[ignore]`) cross-game thread
  test (Oblivion / FNV / Skyrim); FNV is the measured 18-body / 17-joint
  reference, the others assert structural invariants and print actuals.
- `crates/nif/src/blocks/collision/bhk_constraint_tests.rs` — byte-exact per-era
  constraint decode (Oblivion / FNV / Skyrim, bare + malleable-wrapped).
- `docs/smoke-tests/m41-ragdoll.sh` — GPU end-to-end (parse → thread → build →
  activate) on FNV Doc Mitchell; the visual payoff (the actor crumpling) is watched
  live.
- `ragdoll <entity_id>` debug-server command (`byro-dbg` attach) — activate a
  ragdoll on a live actor.
