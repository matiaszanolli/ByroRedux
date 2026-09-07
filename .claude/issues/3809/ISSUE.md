# #3809 — EX-14/15 item C4: FO4 precombine collision — Havok BhkSystemBinary decoder (research spike)

State: OPEN
Labels: enhancement, legacy-compat, nif, game:fo4, terrain-exterior, physics

Split from #2369 (EX-14/15 item C4) per that issue's own plan doc,
[`docs/engine/exterior-readiness-plan.md`](../../docs/engine/exterior-readiness-plan.md#ex-1415-ground-covertrees-persistent-refs-fo4-spatial-data),
which explicitly says: "re-scope as its own research spike... not 'land
independently and sooner.'"

## Status (verified 2026-08-31): NOT DONE — corrected premise, real blocker

The plan doc's original framing of this item as "smaller, better-scoped
than previs" was **investigated and found wrong** (2026-08-23) against
real `Fallout4 - MeshesExtra.ba2` data (159,866 files):

- **Naming was wrong**: the sibling collision file is
  `<cell_formid:08x>_physics.nif` (4,484 of them in `MeshesExtra.ba2`
  alone), NOT `_precomb.nif` — that name appears nowhere in the archive.
- **Block types were wrong** about being directly extractable: sampled 16
  real `_physics.nif` files (consistent across all 16) — every one is
  exactly `NiNode` + `NiExtraData` + `bhkNPCollisionObject` +
  `bhkPhysicsSystem`. `bhkPhysicsSystem` decodes to `BhkSystemBinary`
  (`crates/nif/src/blocks/collision/collision_object.rs:121-151`): a
  **raw undecoded Havok-serialised (HKX-like) byte blob**, explicitly
  documented in that module as "store the raw bytes... hand off to a
  Havok parser later." There is no convex-hull/rigid-body data
  extractable with the NIF crate's *existing* parsers the way the
  original framing assumed.

This is the **same blob that already blocks general FO4+ physics/ragdoll
work (PHYSAL)** — not an independent, smaller problem. It needs a Havok
NP-physics binary format decoder — greenfield format work.

## Scope
1. Research spike: crack the `BhkSystemBinary` blob layout against real
   `_physics.nif` samples (candidate cross-reference: public Havok SDK
   documentation of the serialized binary tag format, if legally usable;
   otherwise pure corpus-derived byte analysis, matching this project's
   established NIF-format-recovery method).
2. Once cracked: extract rigid-body/collision-shape data from precombine
   `_physics.nif` files into the existing collision pipeline.
3. Regression tests pinning the decoded layout against real FO4 samples.

## Related
#2369 (parent, split). Coordinates with the general PHYSAL FO4+ Havok
blocker (same underlying format gap) — check for an existing PHYSAL
tracking issue before duplicating scope.
