# #3792: PHYS-D4-2026-08-30-01: #3330 was closed on its bhkHinge third only — bhkPrismatic and breakable-wrapped edges still drop at the canonical boundary, fragmenting the Protectron ragdoll into 4 free-falling components

**Labels**: bug, nif-parser, medium, game:fnv, game:fo3, nifal, physics
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_PHYSICS_2026-08-30.md` — PHYS-D4-2026-08-30-01 (MEDIUM), independently re-derived as `AUDIT_REGRESSION_2026-08-30.md` REG-2026-08-30-04 and `AUDIT_LEGACY_COMPAT_2026-08-30.md` D4-01
**Dimension**: 4 — PHYSAL ragdoll articulation
**Location**:
- `crates/nif/src/import/collision/ragdoll.rs:142-155` — the `BhkBreakableConstraint` arm
- `crates/nif/src/import/collision/ragdoll.rs:193-220` — the `BhkConstraintData::Other` arm
- `crates/nif/src/blocks/collision/constraints.rs` — `BhkConstraint::parse`

## Description

**#3330 was closed on its `bhkHinge` third only.** Its title and evidence name three drop classes across three FNV creature skeletons; the fix commit (`1ccf1abe`) decoded one of them.

Premise re-verified at HEAD (`64f64480`) by symbol, **not inherited from the closed issue**:

- `BhkConstraint::parse` (`constraints.rs:391-398`) decodes only `Ragdoll` and `LimitedHinge` — plus the malleable wrapper's inner types 7/2, and, since #3330, `LimitedHingeCInfo::parse_hinge_fo3` for type 1. Every other type falls into the `other => { … }` arm.
- `bhkPrismaticConstraint`, `bhkBallAndSocketConstraint` and `bhkStiffSpringConstraint` therefore still arrive as `BhkConstraintData::Other` and are dropped with a `warn!` (`ragdoll.rs:211-215`, whose message literally reads *"bhkPrismatic / bhkStiffSpring not yet mapped to a canonical joint"*).
- `BhkBreakableConstraint` still fails the `downcast_ref::<BhkConstraint>()` (`ragdoll.rs:142`) and is dropped with its own `warn!`, because its wrapped CInfo geometry is `stream.skip`ped at parse time.

The #3330 fix commit **rewrote the source comment to say so itself** (`ragdoll.rs:204-212`):

> *"What remains reaching here on vanilla FNV is `creatures\protectron\skeleton.nif`'s two `bhkPrismaticConstraint` edges, which need a canonical prismatic joint kind that does not exist yet."*

## Evidence

#3330's own corpus evidence named three fragmenting FNV creature skeletons and attributed them precisely:

| Skeleton | Cause | State |
|---|---|---|
| `sentryturret` | `bhkHingeConstraint` | **FIXED** — now 1 component, 3/3 constraints |
| `minisentryturret` | `bhkHingeConstraint` | **FIXED** — now 1 component, 3/3 constraints |
| `creatures\protectron\skeleton.nif` | 2× `bhkPrismatic` + 1× breakable | **NOT FIXED** — 12 authored edges → 9 surfaced, **4 connected components** |

The three severed bodies are `Bip01 Head`, `Bip01 Head Dome` and `Bip01 Spine Brain`, each becoming an independent free-falling multibody.

Corpus context from the same run: **58 of 61 FNV skeletons surface 100% of authored constraints as one connected component** (incl. `_male\skeleton.nif` 17/17 and deathclaw 31/31). Only `protectron` drops. `build_ragdoll`'s forest `warn!` (`crates/physics/src/ragdoll.rs:290-302`) fires, so it is diagnosable from the log — the visual break is unchanged.

## Impact

A destroyed Protectron's head, head dome and spine-brain each become an independent free-falling multibody — exactly the visible break #3330 documented. Blast radius is 1 creature skeleton (down from 3), on FNV and FO3.

Per `_audit-severity.md`, a translatable block dropped at the canonical boundary is **MEDIUM minimum**.

**Trigger**: any FNV/FO3 cell containing a Protectron whose ragdoll activates (death, or the `ragdoll` console command). Content: `creatures\protectron\skeleton.nif` from `Fallout - Meshes.bsa`.

## Tracking gap — this is why a successor is being filed

**#3330, #1539 and #1850 are all CLOSED**, and #3330 was closed on a partial fix with **no successor**. The residual is currently tracked only by a source comment. Three independent audit dimensions this cycle (physics D4, regression, legacy-compat D4) re-derived it separately, which is the cost of that gap.

The audit recommends a successor rather than a reopen, because the two remaining halves need genuinely different work.

## Suggested Fix

Two distinct pieces of work:

**(a) A canonical prismatic joint kind.** `ImportedJointKind::Prismatic` → `RagdollJointSpec::Prismatic` → a Rapier `GenericJoint` leaving the authored linear axis free. This closes the 2 prismatic edges.

**(b) Retain `BhkBreakableConstraint`'s wrapped CInfo at parse time** (`crates/nif/src/blocks/collision/constraints.rs` — the geometry is currently `stream.skip`ped), so the inner Ragdoll/LimitedHinge joint can be rebuilt and the downcast in `ragdoll.rs:142` has something to find. This is #1850's own deferred note.

## Related

- #3330 (CLOSED — partial), #1539 (CLOSED), #1850 (CLOSED — its deferred note is half (b))

## Completeness Checks
- [ ] **SIBLING**: `bhkBallAndSocketConstraint` and `bhkStiffSpringConstraint` reach the same `Other` arm — check whether either has vanilla occupancy before deciding they stay unmapped
- [ ] **CANONICAL-BOUNDARY**: The new joint kind lands at the NIFAL parser→canonical boundary; per-game logic stays in the CInfo decode, which PHYSAL doctrine names as the **only** permitted per-game seam. See `/audit-nifal` and `docs/engine/physal.md`.
- [ ] **TESTS**: A regression test pins `creatures\protectron\skeleton.nif` at 12/12 constraints and **1** connected component — mirroring the `bhk_constraint_tests.rs:214-283` FO3/Oblivion hinge fixtures that guard the fixed third
