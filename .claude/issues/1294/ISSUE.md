Surfaced 2026-05-28 in the second Cydonia render (post-#1292 geometry fix).

## Symptom

With #1292 closed, Cydonia loads **93 547 entities and 7 253 unique meshes**. But \`rapier_bodies=1\` — only the player kinematic body, zero static colliders. The character spawns at (-37.9, 29.0, 19.2) and free-falls from frame 0:

```
[M28.5] frame 0:   body Y 29.0  → 27.5  (Δ -1.526), v -45.8,   grounded=false, rapier_bodies=1
[M28.5] frame 60:  body Y -2547 → -2614 (Δ -66.667), v -2000,  grounded=false, rapier_bodies=1
[M28.5] frame 540: body Y -34547 ...
```

Compare to FNV Atomic Wrangler (the prior render test on 2026-05-28):
```
M28.5 static collider AABB: x [-47.8, 5093.2], y [2.6, 14936.0], z [-4693.2, 165.0] (946 fixed colliders); character at (3242.4, 13962.0, -2733.1)
```

946 fixed colliders on FNV vs 0 on Cydonia.

## Likely root cause

The `synthesize_static_trimesh` fallback at [byroredux/src/cell_loader/spawn.rs:1107-1121](byroredux/src/cell_loader/spawn.rs#L1107-L1121) is the path that creates trimesh colliders from render geometry when the NIF didn't ship a usable bhk shape. Gates:

```rust
if collisions.is_empty()
    && final_layer == RenderLayer::Architecture
    && mesh.skin.is_none()
    && !mesh.is_decal
    && !mesh.alpha_test
    && mesh.positions.len() >= 3
    && mesh.indices.len() >= 3
```

For Cydonia content post-#1292:
- `collisions.is_empty()` — TRUE (we don't extract SF \`bhkPhysicsSystem\` shapes; they parse but aren't routed)
- `final_layer == Architecture` — **likely FALSE**. The REFR-layer classifier in `cell_loader/spawn.rs` checks the base record type (STAT / MSTT / etc.) but SF may use a different mapping.
- `mesh.skin.is_none()` — TRUE (set-dressing is unskinned)
- `!mesh.is_decal && !mesh.alpha_test` — TRUE for most
- `positions.len() >= 3 && indices.len() >= 3` — TRUE post-#1292

The most likely culprit is the **render-layer classifier failing to tag SF REFRs as Architecture**. Diagnose by:
1. Add a per-REFR debug log of `final_layer` during cell load
2. Confirm whether 7 253 meshes all skip the collider path due to wrong layer

Alternative: SF may legitimately ship usable bhk shapes that we DO extract, but they fail to insert as Rapier colliders. Trace `bhkPhysicsSystem` → Rapier path for SF specifically.

## Why this matters

Without static colliders the player free-falls indefinitely from any interior cell spawn. The visible Cydonia render reaches GPU but the player can't stand on anything to actually look around. The fly camera (Escape to capture mouse) is the temporary workaround.

The 75 → 93 547 entity improvement from #1292 unlocked geometry RENDERING; this issue unlocks geometry COLLISION which is what makes the cell USABLE.

## Suggested fix order

1. **Spike**: add `log::debug!` of `final_layer` per REFR during Cydonia load; quantify what percentage hits Architecture vs other layers.
2. If most REFRs are misclassified: fix the SF REFR → RenderLayer mapping in spawn.rs.
3. If REFRs are correctly Architecture but collisions.is_empty() is FALSE: trace why SF bhk extraction is leaving non-empty but unusable collision data.
4. Regression test: \`byroredux-debug-server\` resource snapshot should expose \`rapier_bodies\` count for any cell-load test to gate against \`> 1\` on populated interiors.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: confirm the same gate works on FO76 content (similar BSGeometry pattern + may share the collision issue if FO76 hasn't been runtime-tested with the post-#1292 geometry path)
- [ ] **TESTS**: per-cell rapier_bodies-count assertion in audit-runtime baseline TSVs

## References

- Parent fix: [#1292](https://github.com/matiaszanolli/ByroRedux/issues/1292) (commit 9aa69c68)
- Spawn fallback path: [byroredux/src/cell_loader/spawn.rs:1107-1121](byroredux/src/cell_loader/spawn.rs#L1107-L1121)
- Comparison FNV log: \`docs/audits/sf-first-render/cydonia-2026-05-28.engine.log\` line 35400 vicinity (pre-fix free-fall, demonstrates same symptom from a different cause)
