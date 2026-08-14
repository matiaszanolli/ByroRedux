# PHYS-D4-02

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2882

---

Found by `/audit-physics` Dimension 4 (Ragdoll Articulation). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/nif/src/import/collision/ragdoll.rs:39-44`; `byroredux/src/scene/nif_loader.rs:1159-1172`; `byroredux/src/commands/scene.rs:671-676`

## Trigger Conditions
Loading an FO4 / FO76 / Starfield humanoid or creature skeleton, then running `ragdoll <id>` on the actor.

## Description
The census helper `summarize_collision_authoring` exists **precisely** so the loader can tell "nothing authored" from "authored but opaque" — and the *collider* path uses it correctly (`byroredux/src/cell_loader/spawn.rs:84`, with `packed_collision_fallbacks` / `unresolved_packed_collision` counters). The *ragdoll* path does not.

`extract_ragdoll` bails at its first gate (`has_constraint_authoring` -> `return None`) with **no log**, because on packed-Havok games the constraint graph lives inside `BhkSystemBinary` and no `BhkConstraint` / `BhkBreakableConstraint` block exists to find. `scene/nif_loader.rs` only logs on the *success* branch ("Attached RagdollTemplate (N bodies)"), so nothing is emitted at all. The console then reports `ragdoll: entity N has no RagdollTemplate` — **byte-identical to the message for a rock**.

## Evidence
`grep -rn "summarize_collision_authoring" byroredux crates` -> only `cell_loader/partial.rs:102`, `cell_loader/references/import.rs:103`, plus examples/tests. `scene/nif_loader.rs` (the sole `template_from_imported` caller, `:1161`) never computes or consults it. The early return at `import/collision/ragdoll.rs:42-44` carries a comment explaining why it is *quiet* for architecture, but that rationale does not extend to skeleton NIFs.

## Impact
Diagnosability only — the simulation is unaffected and `docs/engine/physal.md` §3/§5 correctly document the limitation on paper. But at runtime the doc's promise *"documented limitation, **not** a silent leak"* is **not kept**: an engineer testing FO4 ragdolls gets the same output as for an unrigged mesh, with no signal that the blocker is the undecoded blob. Same telemetry class as #1539 / #1718 / #1850 / #2339, all of which were filed and fixed.

## Suggested Fix
In `scene/nif_loader.rs`, when `imported.ragdoll.is_none()`, consult `summarize_collision_authoring(&scene)` (already parsed) and `log::info!` once per NIF when `needs_packed_havok_fallback()`: *"skeleton '<label>': N packed-Havok collision objects authored, ragdoll articulation is inside an undecoded BhkSystemBinary (PHYSAL rollout step 6)"*. Optionally surface the same state to the `ragdoll` command so its error distinguishes blocked-from-absent.

## Related
- #2339 (the sibling silent-drop-site sweep in the same function — already fixed, see the audit's issue-hygiene note)
- #2355 / `PackedAabbProxy` (the collider path that *does* report it)
- `docs/engine/physal.md` §5 "FO4+ packed Havok — blocked"
