# #3959 #3960 #3961 #3962 — `/audit-physics` 2026-09-06 follow-ups

All four from `docs/audits/AUDIT_PHYSICS_2026-09-06.md` (base `229306ce`).
Snapshot as filed; GitHub is authoritative for live state.

## #3959 — PHYS-D1-2026-09-06-01 (HIGH, `physics`, `game:fo4`/`fo76`/`starfield`)
`synthesize_packed_havok_proxy` pre-bakes `ref_scale`, so FO4+/Starfield
packed-Havok proxies get an `XSCL²` collider — the third site of #3064/#3065,
closed on the first two only.

- `byroredux/src/cell_loader/spawn.rs:101-105` (stale rationale), `:227`
  (application), `:277-291` (parented ghost)
- Consumer: `crates/physics/src/sync.rs:907` → `convert.rs:246-251`
- No-op at `XSCL == 1.0`, which is why it survived four passes.

## #3960 — PHYS-D3-2026-09-06-01 (HIGH, `physics`, `ecs`)
Phase 1's fresh-`GlobalTransform` precondition is enforced only by scheduler
stage order, and `register_newcomers_and_refresh_queries` — a second entry into
Phase 1 that runs outside the scheduler — walks around it. NIF-node colliders
register at the world origin, permanently (registration is one-shot).

- `crates/physics/src/sync.rs:200-219`, `:807-866`/`:880-940`
- Callers: `byroredux/src/scene.rs`, `byroredux/src/systems/character.rs`
  (and `byroredux/src/commands/view.rs`, not named in the issue)
- Producer: `byroredux/src/scene/nif_loader.rs` seeds `GlobalTransform::IDENTITY`
  eight lines after computing the correct `rest_pose`.

## #3961 — PHYS-D3-2026-09-06-02 (MEDIUM, `documentation`, `doc-rot`)
The scale contract is stated backwards on `collision_shape_to_parts`'s own
rustdoc (`crates/physics/src/convert.rs:138-141`) and in the doc it cites
(`docs/engine/physics.md:424-428`). Both of its per-producer census entries are
wrong. `docs/engine/physal.md:105-110` holds the correct wording.

Lands together with #3959 — it is the contract statement that fix is checked
against.

## #3962 — PHYS-D4-2026-09-06-01 (MEDIUM, `physics`)
`seed_joint_from_body_poses` dispatches on `ndofs()`, so #3792's new prismatic
joints are seeded with a rotation angle on their linear axis.

- `crates/physics/src/ragdoll.rs:623-652`, called from `:392`
- `LimitedHinge` and `Prismatic` both lock 5 axes ⇒ both `ndofs() == 1`
- `apply_displacement` walks linear axes first ⇒ `coords[LinX] = angular.x`
- Zero `Prismatic` test coverage in the crate; 156/156 green with the defect
  present. Known content: Protectron skeleton (FNV/FO3), 2 of 12 joints.
