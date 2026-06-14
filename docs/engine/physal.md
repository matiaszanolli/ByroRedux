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
| **Raw / `Imported*`** | A faithful decode of the Havok articulation — the constraint graph, per-body mass/damping/shape, joint geometry — converted to engine units (Y-up, `havok_scale`). May carry era-specific quirks. **Allowed to be messy.** | `crates/nif/src/blocks/collision/` (wire) + `crates/nif/src/import/collision.rs` (`ImportedRagdoll`) | Decode only; never the engine's source of truth. |
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
`import/collision.rs::resolve_shape`, NIFAL's "collision" category). PHYSAL does
not re-decode shapes — it reuses the canonical ones and adds the *articulation*
(graph + joints + simulation) NIFAL stops short of. Clean dependency, zero
duplication.

---

## 3. Ragdolls — the reference realisation

### Source boundary — per-game constraint decode

The whole per-game seam is the typed decode of two constraint CInfos
(`crates/nif/src/blocks/collision/constraints.rs`). The importer
(`ragdoll_joint` / `limited_hinge_joint`) reads only the **common subset** of
fields (twist/plane/pivot + angle limits for ragdoll; axis/pivot + limits for
hinge), so era-only fields (FO3+ motors, FO3+ `Perp Axis In B1`) are decoded-or-
zeroed and never reach the canonical spec. One `RagdollCInfo` /
`LimitedHingeCInfo` therefore feeds every game:

| Game | Discriminator | Constraint layout | State |
|---|---|---|---|
| **Oblivion / Morrowind** | NIF ≤ 20.0.0.5 (`#NI_BS_LTE_16#`) | Ragdoll 6×Vec4 + 6×f32 (pivots-first, **no motors**); LimitedHinge 7×Vec4 + 3×f32 (**no Perp B1**) | **decoded (2026-06-14)** |
| **FO3 / FNV** | NIF 20.2.0.7, bsver ≤ 34 (`!#NI_BS_LTE_16#`) | Ragdoll 8×Vec4 + 6×f32 + motor; LimitedHinge 8×Vec4 + 3×f32 + motor; the dominant FNV form is a `bhkMalleableConstraint` wrapping a Ragdoll | **decoded — slice 1 reference** |
| **Skyrim LE/SE** | NIF 20.2.0.7, bsver 83–127 | identical FO3+ layout; gated by NIF **version**, not bsver. `havok_scale` ×69.99 applied at import via `havok_scale_for(header)` | **decoded; version-gate pinned by test, real-data validation pending** |
| **FO4 / FO76 / Starfield** | `BhkNPCollisionObject` → `BhkSystemBinary` | Havok-serialised binary blob — the constraint graph is inside the blob, not as discrete `bhkRigidBody.constraints` | **blocked on a blob decoder (multi-day reverse-engineering); documented limitation, not a leak** |

Sources of truth (no-guessing): `/mnt/data/src/reference/nifxml/nif.xml`
(`bhkRagdollConstraintCInfo` / `bhkLimitedHingeConstraintCInfo`, both version
branches) cross-checked against the sibling `BhkBreakableConstraint` byte tables in
the same file. Every decoder asserts exact stream advancement (byte-level tests in
`blocks/collision/bhk_constraint_tests.rs`).

### Extract — articulation graph (`crates/nif/src/import/collision.rs`)

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
axes — good enough to switch an actor from bind-pose to a plausible ragdoll;
limit-fidelity refinement (and motors, captured but unused) are follow-ups.

---

## 4. "Better than Havok" levers

Centralised in `crates/physics/src/config.rs::ContactConfig`, so the
"reimplement it properly" knobs are one resource, not scattered constants:

- **Multibody joints** (not impulse joints) — the structural win: no inter-link
  separation, which is most of what makes the original ragdolls read as clunky.
- `ragdoll_extra_angular_damping` — extra per-body angular damping on top of the
  authored value (inert at the `0.0` default); the biggest "less floppy than
  Havok" dial.
- Solver iteration count (ragdolls want more than clutter for limit stability) and
  joint-limit stiffness/damping (soft limits vs Havok's hard clamp) — staged as
  the limits get refined.
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

### Phantoms / trigger volumes — **parked (needs a `TriggerVolume` ECS path)**

`BhkPCollisionObject` phantoms are parsed but want a trigger-volume consumer, not a
rigid body — deferred, mirroring the NIFAL collision note.

### FO4+ packed Havok — **blocked (blob decoder)**

`BhkNPCollisionObject → BhkSystemBinary` holds the whole physics system
(bodies + constraints) as a serialised Havok binary. Decoding it is a separate
multi-day project; until then FO4/FO76/Starfield ragdolls don't thread (static
collision already falls back to a synthesised trimesh — see
[`physics.md`](physics.md)). Documented limitation, **not** a silent leak.

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
6. FO4+ `BhkSystemBinary` decoder — unblocks the packed-Havok games (large,
   independent).

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
