# #6328: PHYS-D3-2026-10-21-04: Stale "FO4 ragdolls do not activate yet" in the death teardown

**Labels**: low,documentation,physics,doc-rot,game:fo4

**Source**: docs/audits/AUDIT_PHYSICS_2026-10-21.md · **Severity**: LOW · **Dimension**: Ragdoll & Constraint Seam (doc rot) · **Location**: `byroredux/src/combat.rs:1051-1057`

- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam (doc rot)
- **Location**: `byroredux/src/combat.rs:1051-1057`
- **Status**: NEW (made false by `e1bd40bcf` / `7c6e6c3e1`)
- **Description**: The #5861 comment justifies removing `ObjectAnimationGraph` with "FO4 ragdolls do not activate yet, so nothing overrides the restored pose and the corpse stands and idles". FO4 decoded ragdolls now activate at death, and `docs/smoke-tests/fo4-ragdoll.sh` gates them. The removal is still right, because the graph would keep driving the skeleton under the writeback. The stated reason is wrong.
- **Suggested Fix**: Say the graph is removed because, while it lives, its clips fight the ragdoll writeback on every rig.

**Publish note**: the same stale sentence also appears at `byroredux/src/asset_provider/actor_graph.rs:162` and `byroredux/src/systems/graph_locomotion.rs:446` ("FO4 ragdolls do not activate yet"); TD3-2026-10-21-01 assigns those code comments to this issue. Fix all three sites together. The status-doc siblings (`docs/feature-matrix.md`, `ROADMAP.md`) are TD3-2026-10-21-01.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)

_Source: `docs/audits/AUDIT_PHYSICS_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
