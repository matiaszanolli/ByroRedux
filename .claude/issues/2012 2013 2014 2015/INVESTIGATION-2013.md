# #2013 investigation — TES-family character grounding failure

## Summary

Code-review-only investigation (no live Vulkan device / bench harness available
in this session). Several plausible root-cause hypotheses were checked against
the source and, where available, the authoritative `nif.xml` reference at
`/mnt/data/src/reference/nifxml/nif.xml`. **No provable, code-confirmable bug
was found.** Per this project's established discipline against speculative
physics/Vulkan fixes (see `feedback_speculative_vulkan_fixes` — "don't ship
changes when failure modes are invisible to `cargo test`; verify or don't
ship"), no code change was made for this issue. It remains open.

## Hypotheses checked and ruled out

1. **Z-up→Y-up coordinate conversion inverts triangle winding, flipping
   collision-mesh normals.** `crates/nif/src/import/coord.rs::zup_point_to_yup`
   maps `(x, y, z) → (x, z, -y)`. As a 3×3 matrix this has determinant **+1**
   — a proper rotation (specifically a 90° rotation about X), not a
   reflection. A proper rotation preserves triangle winding and outward-facing
   normals for every mesh, universally, regardless of source game. Ruled out:
   this can't explain a TES-only symptom (and would have broken Fallout-family
   collision too, which grounds correctly).

2. **`bhkCompressedMeshShape` (Skyrim+'s dominant floor/architecture shape)
   is unhandled, so Skyrim collision silently resolves to nothing.**
   Checked `crates/nif/src/import/collision/shape.rs::resolve_shape` —
   `BhkCompressedMeshShape` IS handled (dispatches to `resolve_compressed_mesh`
   at shape.rs:281-285). Ruled out.

3. **`resolve_compressed_mesh`'s vertex scale-by-`havok_scale` is wrong for
   Skyrim, displacing the floor far from the render mesh.** `nif.xml`'s
   `bhkCompressedMeshShapeData` doc doesn't state units explicitly, but the
   `hk`-prefixed `AABB: hkAabb` field and the shape's Havok-quantized chunk
   format (`error`-scaled `u16` vertices) are consistent with genuine
   Havok-unit authoring — the same category `resolve_packed_mesh` (also
   `havok_scale`-multiplied) already handles correctly for
   `bhkPackedNiTriStripsShape`. No documented evidence this differs for
   compressed-mesh data. Not ruled out with certainty, but no evidence of a
   bug either — would need real Skyrim SE collision data to verify vertex
   magnitudes against the render mesh's AABB.

4. **The #1832 zero-mass "Dynamic"-per-enum → Static reclassification
   (`crates/nif/src/import/collision/mod.rs:284-311`, commit `ae083d69`)
   doesn't cover the actual code path these games' floor/architecture takes.**
   Checked: `extract_from_classic` is documented as "the dominant path for
   Oblivion / FO3 / FNV / Skyrim LE / SSE" and is the ONLY path handling
   `BhkCollisionObject` → `BhkRigidBody` chains — there's no separate
   per-shape-type dispatch that would bypass this reclassification for
   `bhkCompressedMeshShape` specifically. The fix applies uniformly to all
   four games' rigid bodies. **This is the most important finding**: the
   fix's own comment (mod.rs:291-297) states it is "the root cause of the
   TES-family (Oblivion/Skyrim) 'character never grounds' bug" and describes
   the exact symptom #2013 reports — yet #2013's evidence (gathered at the
   *current* HEAD, `c3e09bb5`, with this exact code present) shows the bug
   still reproducing on both TES-family cells. **This is a genuine, unresolved
   discrepancy between the code's own claim and observed behavior** that a
   future investigation should treat as the primary lead: either the
   reclassification condition (`motion_type == MotionType::Dynamic && mass
   <= 0.0`) doesn't match what these specific cells' floor bodies actually
   carry (raw `motionType`/`mass` bytes worth re-verifying against a live
   parse of `WhiterunDragonsreach`/`ICMarketDistrictTheGildedCarafe`), or the
   fix works for *some* architecture pieces but not the specific one(s) under
   the player's spawn point.

## Candidate leads not yet checkable without live data/tooling

- **Door-teleport spawn to an unloaded exterior** (issue's lead #1): interior
  spawn point is "the cell's first door's own placement" — confirmed at
  `byroredux/src/cell_loader/load.rs:27-32`, applied uniformly across all
  games (not TES-specific), so this mechanism itself isn't the differentiator
  — but *which* door a specific cell's first-door-in-file-order resolves to,
  and whether that door is an interior→exterior teleport link for
  `WhiterunDragonsreach` specifically, requires inspecting that cell's actual
  REFR data or a live parse — not verifiable from source alone.
- **Oblivion's distinct "sticks at Y≈324 but `grounded` never flips true"
  sub-symptom** (vs. Skyrim's true infinite fall) suggests a different
  mechanism for Oblivion specifically — a contact IS being found (vertical
  motion halts) but Rapier's `grounded` heuristic (contact-normal-vs-up
  angle) evaluates false. This would point at a wrong-signed or
  wrong-magnitude contact normal for Oblivion's specific floor shape type
  (likely `bhkNiTriStripsShape`, since #1744's doc notes Oblivion/FO3/FNV
  share that shape for architecture) — but FO3/FNV use the *same* shape type
  successfully, so if this is real, the divergence must be in the specific
  authored data (scale, degenerate triangles, or a Havok material/friction
  value) for Oblivion's test cell, not the shared code path. Unverifiable
  without live content.

## Recommendation

Do not attempt a speculative fix. The highest-value next step is empirical:
load `WhiterunDragonsreach` and `ICMarketDistrictTheGildedCarafe` with
`--bench-hold`, attach `byro-dbg`, and directly inspect (a) the raw
`motionType`/`mass` bytes the floor architecture's `BhkRigidBody` actually
carries at those specific cells (to confirm or refute finding #4 above), and
(b) whether any collider AABB overlaps the spawn XZ position at all (to
confirm or refute the door-teleport-to-void lead). Both require a live
Vulkan device and real game data neither of which are available in this
session.
