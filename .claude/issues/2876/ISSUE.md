# PHYS-D7-05

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2876

---

Found by `/audit-physics` Dimension 7 (Queries & Diagnostics). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW
**Location**: `crates/physics/src/sync.rs:397` (the `pub fn`); `crates/physics/src/lib.rs:34-35` (re-export); `byroredux/src/scene.rs:1120-1128` (its **only** call site); `byroredux/src/commands/mod.rs:52-107` (the full console registry)

## Trigger Conditions
Any "I fell through the floor" / "there is no collision here" investigation that is *not* the frame-0 door spawn — mid-cell holes, post-transition positions, `--player` override spawns, `spawn_on_camera_ground` spawns, or any position the player walks to.

## Description
The census is public, re-exported from the crate root, and gated behind a single `if floor_probe_failed` inside `setup_scene`'s door-teleport branch. Grepping the whole workspace finds no other caller:

```
crates/physics/src/sync.rs:397   pub fn dump_spawn_collider_census(...)   <- definition
crates/physics/src/lib.rs:34     re-export
byroredux/src/scene.rs:1122      sole call site
```

`byroredux/src/commands/mod.rs`'s `build_command_registry` registers 50+ commands (`tex.*`, `mesh.*`, `water.contacts`, `light.*`, `mat.*`, `ragdoll`, `cond`, `setav`, `time.*`, ...) and **none** of them touch `PhysicsWorld`'s query surface: `colliders_near_xz`, `static_colliders_aabb`, `cast_ray_down`, `cast_capsule_down*`, `body_count` and `awake_counts` have **zero** console exposure. Grep of `crates/debug-server/src/`, `crates/debug-protocol/src/` and `tools/byro-dbg/src/` for physics/collider/census terms returns one incidental doc-comment hit and nothing functional.

There is therefore no way to answer "what collision is under *this* point" while the engine is running — which is the situation the operator is always in, since the failure is noticed by falling, not by reading frame-0 logs.

Worse: the sibling ragdoll/faller diagnostic (`dump_awake_fallers`, `sync.rs:237`) is one-shot per **process** (`AWAKE_FALLERS_DUMPED.swap`, `:252`) and env-gated, so **both** physics diagnostics are effectively boot-time-only.

## Impact
The diagnostic exists but is unusable for the workflow it was built for. `--bench-hold` + `byro-dbg` (the documented investigation workflow in `CLAUDE.md`) cannot reach it.

This also blocks closing the remaining open question on the door-threshold spawn gap: this audit established the *mechanism* (PHYS-D5-01 / D5-02 / D5-03) but could not determine which dominates a specific in-game report, because that needs a live run with exactly this telemetry.

**Exact precedent**: #518 (CLOSED) — *"tex.missing / tex.loaded / mesh.cache / mesh.info unreachable via byro-dbg"* — established that a diagnostic without a console entry point is a defect in this repo, not a nice-to-have.

## Suggested Fix
Register a `phys.census <x> <z> [radius]` command in `byroredux/src/commands/scene.rs` (defaulting XZ to the player/camera position) plus a `phys.stats` surfacing `body_count` / `awake_counts` / `static_colliders_aabb`. Both are pure reads of an already-`pub` API; `water.contacts` in `byroredux/src/commands/water.rs` is the existing template for a physics-reading command.

Worth fixing together with PHYS-D7-03 and PHYS-D7-04 so the exposed command is worth having.

## Related
- #518 (CLOSED precedent)
- PHYS-D7-03, PHYS-D7-04 (the census's content defects)
- PHYS-D5-01 / D5-02 / D5-03 (the defect class this diagnostic serves)
