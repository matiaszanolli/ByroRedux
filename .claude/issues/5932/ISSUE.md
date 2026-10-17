# PHYS-D1-2026-10-16-01: A collider authored on the NIF root node of an animated prop is paired with the root, which is never spawned, and is silently dropped

Labels: high,physics,bug,game:skyrim
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: HIGH
- **Dimension**: Shape Translation (collider producer, cell loader)
- **Location**: `byroredux/src/cell_loader/graph_nodes.rs:132-155` (`pair_collisions`), `:268-270` (`spawn_nodes` skips `parent.is_none()`), `byroredux/src/cell_loader/spawn.rs:1428` (`spawn_node_collisions`), `:1481-1483` (`spawn_collision_shapes` skip)
- **Status**: NEW (regression introduced by fa1473e25, 2026-10-14; the detached collider spawned before it)
- **Description**: `pair_collisions` pairs each flat collision with the first node that carries a body and stands at the collision's pose. Its candidate set is every node with `collision.is_some()`, including the root. `GraphNodes::build` requires an identity root, so a root-hosted body's pose (identity) matches the root and `collision_nodes[i] = Some(root)`. Then (a) `spawn_collision_shapes` skips every collision whose `collision_nodes` entry is `Some`, and (b) `spawn_node_collisions` needs `spawned.node_entity(root)`, but `spawn_nodes` only creates entities for nodes with a parent (the root IS the placement root) and `entities` never holds the root, so it hits `else { continue }`. The collider is spawned by neither path. `collisions` is non-empty, so `missing_collision_fallback` does not engage either. The mesh side avoids the same trap explicitly ("when it hangs under one below the root"); the collision side lacks that exclusion. The only test (`a_nodes_own_body_rides_the_node_as_a_keyframed_child`) puts the body on a child node and builds the graph with `GraphNodes::for_test`, bypassing `pair_collisions`.
- **Evidence**: census probe over Skyrim SE `Meshes0/1.bsa` (parse + `import_nif_scene` + `import_nif_with_collision`, same conditions as `build`): 1199 NIFs carry `BSBehaviorGraphExtraData`; 411 have collision only on child nodes, 41 have it on the ROOT node, and all 41 pass `build`'s mesh-alignment and identity-root conditions, so `graph_nodes` is `Some` for all of them. Examples: `meshes\dungeons\nordic\levers\pullchain\genpullchain01.nif` and `norpullchain01.nif`, `meshes\traps\beartrap\beartrap01.nif`, `trapdwefirepillar01.nif`, `trapmagiccaster01.nif`, `dweobservatoryarmillary01.nif`, `dwebrentrance01.nif` (14 colliders), `dwekagrumezgauntlet.nif` (9), `dlc01sebf_room.nif`, `seruinstemplepuzzle01.nif`, `dragonmounddirtcap.nif`, the four `magicstone*01.nif`, `dragonbones*.nif`, `smelter01.nif`, `eyeofmagnus01.nif` (57).
- **Impact**: those props lose their root body entirely, and any child body at identity pose pairs with the root too. Players walk through pull-chain mounts, trap bases, ruin pieces and room shells; an unrelated physics-driven behaviour (hit/trap events keyed on these bodies, `ApplyHavokImpulse`) finds no body. No log line announces it (the debug log in `spawn_node_collisions` is after the `continue`).
- **Trigger Conditions**: any placed reference whose NIF has a `BSBehaviorGraphExtraData` root and a `bhkCollisionObject` on the root NiNode, with an identity root transform.
- **Related**: fa1473e25, `graph_nodes::build` mesh-vs-collision asymmetry, #5686 / 696da4f47 (same area, no overlap).
- **Suggested Fix**: restrict `pair_collisions` to nodes with a parent (`nodes[index].parent.is_some()`), exactly like the mesh pairing, so root-hosted bodies stay on the flat detached spawn; add a `pair_collisions`-level test (root + child, body on root) and a spawn test asserting one detached collider. Alternatively map the root to `placement_root` in `SpawnedNodes::node_entity`, but then the root body would be re-keyframed onto the placement root, which is not what the flat spawn did.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
