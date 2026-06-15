**Severity**: MEDIUM · **Dimension**: 9 — NIFAL NaN/Inf boundary (UB facet) / physics-solver input
**Location**: `crates/nif/src/import/collision.rs:291-305` (`extract_ragdoll` body fields) + `:377-401` (`ragdoll_joint` / `limited_hinge_joint`); consumed by `byroredux/src/ragdoll.rs:190-202` → `crates/physics/src/ragdoll.rs:99-127` (`build_ragdoll` → `RigidBodyBuilder::position`) and `:245-264` (`ragdoll_writeback_system`, no finite guard)
**Source**: `docs/audits/AUDIT_SAFETY_2026-06-14.md` (SAFE-D9-NEW-04)

## Description
The collision **shape** extraction in this same file has finite guards on every radius / half-extent / center / vertex (`collision.rs:461-571`, the #1409 fix). The ragdoll **body** and **joint** extraction does **not**: `body.translation`, `body.rotation`, `body.mass`, and the joint scalars (`cone_max`, `twist_min`/`twist_max`, hinge `min_angle`/`max_angle`) are read as raw Havok floats and forwarded with no `is_finite()` check. They flow into `RagdollBodySpec.translation/rotation` → `iso_from_trs` (`crates/physics/src/convert.rs:47`, no guard) → `RigidBodyBuilder::dynamic().position(...)`, and the joint limits into `GenericJointBuilder::limits(JointAxis::AngX, [tmin, tmax])` (`crates/physics/src/ragdoll.rs:250-252`).

## Evidence
- `collision.rs:293` `mass: body.mass`, `:299-304` `translation: havok_to_engine(...)` / `rotation: havok_quat_to_engine(...)` — no `finite()` (contrast the guarded shape path two functions below).
- `collision.rs:385-389` `cone_max: r.cone_max_angle`, `twist_min/twist_max` — raw, ungated.
- Partial existing defenses (not full coverage): `frame_rot` (`ragdoll.rs:286-295`) falls back to identity for degenerate **axis** vectors (so a NaN twist/plane axis is contained), and `b.mass.max(1e-3)` survives a NaN mass (Rust `f32::max` returns the non-NaN operand). But a **NaN translation** has no fallback — it seeds the body position directly — and **NaN joint limits** `[NaN, NaN]` are handed to the solver. `ragdoll_writeback_system` (`byroredux/src/ragdoll.rs:256-264`) copies `body_pose` straight into `GlobalTransform.translation/rotation` with no `is_finite()` check, and that `GlobalTransform` feeds the bone palette → GPU skinning.

## Impact
A non-finite ragdoll body pose or joint limit from a corrupt / truncated Havok CInfo decode (the per-game seam PHYSAL warns is fragile) either seeds a NaN Rapier body or destabilizes the solver, and the un-guarded writeback propagates the resulting NaN pose into `GlobalTransform` → bone matrices → GPU skinned vertices (NaN-on-GPU = UB; NaN pixels stick through SVGF/TAA history). Trigger requires malformed ragdoll content + an active ragdoll, consistent with the MEDIUM precedent of the raw-NIF-scalar finite-guard class (#1411 CLOSED, #1434 OPEN).

## Related
#1409 (CLOSED — the shape-side finite guards in the *same file* that this path was not extended to); #1434 / #1382 (NIFAL scalar finite-guard class); PHYSAL spec `docs/engine/physal.md`.

## Suggested Fix
Apply the existing `finite(_)` / `finite_vec(_)` helpers (already defined at `collision.rs:468-475`) to the ragdoll body mass / translation / rotation and to the joint limit angles at the extract boundary — drop a body or joint whose CInfo is non-finite (mirroring the shape path's `?`-on-`None` drop). Belt-and-suspenders: add an `is_finite()` skip in `ragdoll_writeback_system` before writing `GlobalTransform`.

## Completeness Checks
- [ ] **SIBLING**: The guarded shape path in the same file is the template — body, joint, and writeback must all match it
- [ ] **TESTS**: A regression test pins this fix (a non-finite ragdoll body/joint CInfo is dropped; writeback never writes a NaN `GlobalTransform`)
