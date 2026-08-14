# PHYS-D3-05

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2880

---

Found by `/audit-physics` Dimension 3 (ECS Sync). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/physics/src/sync.rs:1-14`; `docs/engine/physics.md:88-141`

## Trigger Conditions
n/a — doc rot. Matters because the phase's *position* is the correctness property (forces must be applied before the step integrates them).

## Description
Two independent inaccuracies in the authoritative description of the physics tick.

1. **The phase count is wrong.** The `sync.rs` module doc opens *"Walks four phases"* and enumerates 1-4, and `docs/engine/physics.md:92` says *"It's structured as four phases"* and likewise lists 1-4. The live tick has **five** steps: `crate::water::apply_buoyancy` runs as phase 2.5 at `sync.rs:129-134`, between the kinematic push and the step, and its `BYRO_PROFILE` bracket is a first-class labelled phase (`sync.rs:155`, `buoyancy=`). Its ordering is load-bearing — moving it after the step makes lift lag a frame and reads as a water bug. `grep -i buoyan docs/engine/physics.md` returns nothing. (`docs/engine/watal.md` documents the buoyancy sink itself; the *phase order* is documented nowhere.)

2. **The "loose-NIF viewer opt-out" premise is false.** `docs/engine/physics.md:90-91` states the system *"early-returns if no `PhysicsWorld` resource is present (the loose-NIF viewer opt-out)"*. The early return exists (`sync.rs:96-99`) and is correct, but the binary inserts `PhysicsWorld` **unconditionally** at `byroredux/src/boot.rs:451` — there is no loose-NIF opt-out in the shipping engine, only in test fixtures.

## Impact
Documentation-only, but it is the doc a future change to phase order would be checked against — and the false opt-out premise actively hid a live code path: it is what makes **PHYS-D3-02** reachable from `cargo run -- mesh.nif`.

## Suggested Fix
Renumber to "four phases plus a 2.5 buoyancy hook" in both the module doc and `docs/engine/physics.md`, stating why 2.5 must precede phase 3. Correct the opt-out sentence to "test fixtures and embedders that omit the resource; the shipping binary always inserts it (`boot.rs`)".

## Related
- PHYS-D3-02 (the live path the stale premise concealed)
