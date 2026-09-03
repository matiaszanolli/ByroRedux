# #3792 — PHYS-D4-2026-08-30-01: #3330 was closed on its bhkHinge third only — bhkPrismatic and breakable-wrapped edges still drop at the canonical boundary, fragmenting the Protectron ragdoll into 4 free-falling components

**Severity**: MEDIUM
**Location**: `crates/nif/src/import/collision/ragdoll.rs`, `crates/nif/src/blocks/collision/constraints.rs`
**Source**: `docs/audits/AUDIT_PHYSICS_2026-08-30.md` (PHYS-D4-2026-08-30-01), independently
re-derived as REG-2026-08-30-04 and LC D4-01.

#3330 closed on its `bhkHinge` third only. `creatures\protectron\skeleton.nif` still
fragmented: 12 authored constraint edges → 9 surfaced → 4 connected components (`Bip01 Head`,
`Bip01 Head Dome`, `Bip01 Spine Brain` each free-falling independently), caused by two
undecoded classes: bare `bhkPrismaticConstraint` (no canonical joint kind existed), and
`bhkBreakableConstraint`'s wrapped CInfo geometry being `stream.skip`ped at parse time (#1850's
deferred note).

## Suggested Fix (two pieces)

(a) A canonical prismatic joint kind: `ImportedJointKind::Prismatic` → `RagdollJointSpec::Prismatic`
→ a Rapier `GenericJoint` leaving the authored linear axis free.
(b) Retain `BhkBreakableConstraint`'s wrapped CInfo at parse time so the inner joint can be
rebuilt and the `ragdoll.rs:142` downcast has something to find.

## Fix implemented

**Parser** (`crates/nif/src/blocks/collision/constraints.rs`):
- `PrismaticCInfo` struct + `parse_fo3`/`parse_oblivion`, field layout verified against
  `nif.xml`'s `bhkPrismaticConstraintCInfo` (`until="20.0.0.5"` vs `since="20.2.0.7"` — two
  genuinely different field orders, matching the existing Ragdoll/LimitedHinge precedent).
  `BhkConstraintData::Prismatic(PrismaticCInfo)` variant. Wired into `BhkConstraint::parse`'s
  bare-type dispatch (both eras) and the malleable-wrapped inner dispatch (both eras).
- `BhkBreakableConstraint` grew a `data: BhkConstraintData` field. `parse()` now decodes the
  wrapped payload for the four types a canonical joint exists for (Hinge/LimitedHinge/
  Prismatic/Ragdoll = 1/2/6/7) using the exact same per-era field-order parsers a bare
  `BhkConstraint` uses, instead of a byte-count skip — byte consumption is unchanged (verified
  against the existing `wrapped_payload_size`/`fnv_motor_prefix_size` tables), so
  `threshold`/`remove_when_broken` stay reachable exactly as before. BallAndSocket(0)/
  StiffSpring(8) still skip into `Other` (no canonical joint kind yet — explicitly deferred,
  see Completeness Checks below); Malleable(13) is unchanged (nested dispatch, `block_size`
  recovery).

**NIFAL** (`crates/nif/src/import/types.rs`, `crates/nif/src/import/collision/ragdoll.rs`):
- `ImportedJointKind::Prismatic { axis_a, perp_a, pivot_a, axis_b, perp_b, pivot_b,
  min_distance, max_distance }`, mirroring `LimitedHinge`'s shape (`Sliding` → axis,
  `Rotation` → perp reference, matching the `Perp Axis In A1`/`B1` role).
- `prismatic_joint()` conversion fn, sibling of `ragdoll_joint`/`limited_hinge_joint` (same
  non-finite drop, #1534).
- `extract_ragdoll`'s `BhkConstraint` dispatch gained a `Prismatic` arm.
- `extract_ragdoll`'s `BhkBreakableConstraint` arm (previously an unconditional drop-and-warn)
  now tries `try_breakable_joint` first — the same resolve/validate/build steps a bare
  constraint uses — and only falls back to the #1850 drop diagnostic when the wrapped type has
  no canonical joint kind, an endpoint doesn't resolve, it's a self-loop, or the decode is
  non-finite.

**PHYSAL** (`crates/physics/src/ragdoll.rs`, `byroredux/src/ragdoll.rs`):
- `RagdollJointSpec::Prismatic`, `scaled_pivots` treats `min_distance`/`max_distance` as linear
  (scaled like a pivot) not angular.
- `build_joint`'s new arm: nif.xml's own description — "all three rotation axes and the
  remaining two translation axes are fixed" — becomes `prismatic_locked()`
  (`LIN_Y|LIN_Z|ANG_X|ANG_Y|ANG_Z`), leaving `JointAxis::LinX` free-but-limited to
  `[min_distance, max_distance]`, mirroring exactly how `LimitedHinge` leaves `AngX` free.
  Same flip-negates-and-swaps-the-limit treatment as `LimitedHinge`'s `min_angle`/`max_angle`.
- `joint_from_imported` (byroredux) threads the new variant through with no re-derivation.

## Verification against real game data

Ran the new `#[ignore]`d `fnv_protectron_skeleton_is_one_connected_component` test
(`crates/nif/tests/ragdoll_import.rs`) against the mounted `Fallout - Meshes.bsa`:

```
FalloutNV ragdoll: 13 bodies, 12 joints (5 Ragdoll + 5 LimitedHinge + 2 Prismatic)
Protectron: 13 bodies, 12 joints, 1 connected component(s)
```

**12/12 constraints surfaced, 1 connected component** — exactly the issue's own acceptance
criterion. The three pre-existing humanoid-skeleton real-data tests (Oblivion/FNV/Skyrim SE
`_male` skeletons, 18 bodies / 17 joints each) still pass unchanged, confirming zero regression.

## Completeness Checks

- [x] **SIBLING**: `bhkBallAndSocketConstraint` / `bhkStiffSpringConstraint` still reach `Other`
      (no canonical joint kind exists for either) — **deliberately not resolved here**, matching
      the issue's own checklist framing ("check whether either has vanilla occupancy before
      deciding they stay unmapped"). No occupancy census was run for these two; left open for a
      future issue if warranted.
- [x] **CANONICAL-BOUNDARY**: `Prismatic` lands at the NIFAL parser→canonical boundary
      (`MaterialInfo`-equivalent: `ImportedJointKind`); the per-game seam stays confined to
      `constraints.rs`'s `parse_fo3`/`parse_oblivion`, never re-derived downstream.
- [x] **TESTS**: byte-exact parser tests (FO3+/Oblivion Prismatic field order, `BhkBreakableConstraint`
      decode for all 4 canonical types + BallAndSocket-stays-Other), synthetic `extract_ragdoll`
      unit tests (bare Prismatic, breakable-wrapped Ragdoll now surfaces, breakable-wrapped
      BallAndSocket still drops), a connected-components helper + its own CI-runnable pin, and
      the real-data Protectron regression test verified live above.

Full workspace: `cargo test --no-fail-fast` 7031 passing, 0 failing.
