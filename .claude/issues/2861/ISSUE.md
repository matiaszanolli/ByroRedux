# PHYS-D1-02

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2861

---

Found by `/audit-physics` Dimension 1 (Shape Translation). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/ragdoll.rs:177-185` vs `crates/physics/src/sync.rs:621-630`

## Trigger Conditions
Any activated ragdoll (`ragdoll <id>` console trigger, or the future death/hit-react trigger) that contacts world geometry at speed, or two ragdolls contacting each other. Fires on every game whose ragdolls thread (Oblivion / FO3 / FNV / Skyrim).

## Description
`ContactConfig::default_contact_skin_bu` (1.0 BU ~ 1.4 cm) is documented as the *"per-collider contact skin ... wide enough to keep TriMesh seams from leaking the kinematic player through"* (`config.rs:64-69`). `register_newcomers` applies it to **every** part it emits, regardless of shape kind. `build_ragdoll` receives the same `&ContactConfig` — it reads `cfg.ragdoll_extra_angular_damping` from it two lines earlier (`ragdoll.rs:161`) — but its `ColliderBuilder` chain omits `.contact_skin(...)` entirely, so every ragdoll collider is built with Rapier's default skin of `0.0`.

```rust
// crates/physics/src/ragdoll.rs:178 — no .contact_skin()
let col = ColliderBuilder::new(shape)
    .position(iso).friction(...).restitution(...).mass(part_mass).build();

// crates/physics/src/sync.rs:624 — has it
let collider = ColliderBuilder::new(shape)
    .position(iso).friction(...).restitution(...).mass(part_mass)
    .contact_skin(contact_skin).build();
```

`grep -rn "contact_skin" crates byroredux` returns exactly `sync.rs:621/629` + `config.rs` — the ragdoll site is the only unskinned production path. Nothing in the crate or the docs states this is deliberate; `config.rs:1-11` enumerates the unification sites and simply never lists the ragdoll builder.

## Impact
Rapier's skin is **additive between the two colliders in a pair** (*"a small gap ... equal to the sum of their skin"*, `rapier3d-0.22.0/src/geometry/collider.rs:1002-1008`), so:
- a ragdoll limb against skinned static world geometry gets **half** the intended margin (1.0 BU instead of 2.0)
- two ragdolls against each other get **zero**

That is exactly the "unskinned collider adjacent to a skinned one" seam the config was created to eliminate. Self-collision within one ragdoll is already suppressed by interaction groups (#2338), so the exposure is ragdoll-vs-world tunnelling through TriMesh seams and ragdoll-vs-ragdoll interpenetration.

## Suggested Fix
Add `.contact_skin(cfg.default_contact_skin_bu.max(0.0))` to the `build_ragdoll` collider chain. If zero skin is intentional for reduced-coordinate multibody stability, instead give `ContactConfig` an explicit `ragdoll_contact_skin_bu` field and say so in the module doc — so the divergence is a decision rather than an omission.

## Related
- #2338 (CLOSED — ragdoll interaction groups)
- `crates/physics/src/config.rs:1-11` module doc's site enumeration
